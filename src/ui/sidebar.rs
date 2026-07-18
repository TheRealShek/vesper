//! The fixed 220px sidebar (Arch §9): brand block + collapse, a flat
//! count-sorted tag list, the AND/OR match-mode control, and the footer with
//! the primary "Add Source Root" button and the settings gear.

use libadwaita::gtk::{self};
use libadwaita::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// All widget handles the caller needs from the sidebar.
pub struct SidebarWidgets {
    pub root: gtk::Box,
    pub tag_list_box: gtk::ListBox,
    pub tags: Rc<RefCell<Vec<crate::events::UiTag>>>,
    pub match_any_radio: gtk::CheckButton,
    pub match_all_radio: gtk::CheckButton,
    pub match_mode_box: gtk::Box,
    pub no_tags_label: gtk::Label,
    pub tips_box: gtk::Box,
    pub add_source_root_button: gtk::Button,
    pub open_settings_button: gtk::Button,
    pub collapse_button: gtk::Button,
}

#[derive(Debug, Clone)]
pub(crate) struct TagRowPresentation {
    pub tag: crate::events::UiTag,
    pub lineage: Option<String>,
}

/// Sorts tag rows by count desc, then case-insensitive name, then exact path
/// identity, and adds collision-only lineage text (Arch §3). A root path is
/// appended only when two equal display paths would otherwise stay ambiguous.
pub(crate) fn prepare_tag_rows(
    tags: &[crate::events::UiTag],
    roots: &[(i64, String)],
) -> Vec<TagRowPresentation> {
    let mut sorted = tags.to_vec();
    sorted.sort_by(|a, b| {
        b.file_count
            .cmp(&a.file_count)
            .then_with(|| {
                a.display_name
                    .to_lowercase()
                    .cmp(&b.display_name.to_lowercase())
            })
            .then_with(|| a.source_root_id.cmp(&b.source_root_id))
            .then_with(|| a.relative_folder_path.cmp(&b.relative_folder_path))
    });

    sorted
        .iter()
        .map(|tag| {
            let duplicate_name = sorted
                .iter()
                .filter(|other| other.display_name == tag.display_name)
                .count()
                > 1;
            let duplicate_path = duplicate_name
                && sorted
                    .iter()
                    .filter(|other| {
                        other.display_name == tag.display_name
                            && other.display_path == tag.display_path
                    })
                    .count()
                    > 1;

            let lineage = duplicate_name.then(|| {
                if duplicate_path {
                    let root = roots
                        .iter()
                        .find(|(id, _)| *id == tag.source_root_id)
                        .map(|(_, path)| path.as_str())
                        .unwrap_or("Unknown source");
                    format!("{} — {root}", tag.display_path)
                } else {
                    tag.display_path.clone()
                }
            });

            TagRowPresentation {
                tag: tag.clone(),
                lineage,
            }
        })
        .collect()
}

pub(crate) fn populate_tag_rows(
    list_box: &gtk::ListBox,
    stored_tags: &Rc<RefCell<Vec<crate::events::UiTag>>>,
    tags: &[crate::events::UiTag],
    roots: &[(i64, String)],
) {
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }

    let presentations = prepare_tag_rows(tags, roots);
    *stored_tags.borrow_mut() = presentations
        .iter()
        .map(|presentation| presentation.tag.clone())
        .collect();

    for presentation in presentations {
        // Leading folder symbolic — the app never invents per-tag icons
        // (Visual §4).
        let icon = gtk::Image::builder()
            .icon_name("folder-symbolic")
            .valign(gtk::Align::Center)
            .build();

        let text_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .valign(gtk::Align::Center)
            .hexpand(true)
            .build();
        let name = gtk::Label::builder()
            .label(&presentation.tag.display_name)
            .css_classes(["tag-name"])
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        text_box.append(&name);
        if let Some(lineage) = &presentation.lineage {
            let secondary = gtk::Label::builder()
                .label(lineage)
                .xalign(0.0)
                .ellipsize(gtk::pango::EllipsizeMode::Middle)
                .css_classes(["caption", "dim-label"])
                .build();
            text_box.append(&secondary);
        }

        let count = gtk::Label::builder()
            .label(presentation.tag.file_count.to_string())
            .css_classes(["tag-count", "numeric"])
            .valign(gtk::Align::Center)
            .build();

        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();
        content.append(&icon);
        content.append(&text_box);
        content.append(&count);

        let row = gtk::ListBoxRow::builder()
            .child(&content)
            .css_classes(["tag-row"])
            .build();
        row.set_tooltip_text(presentation.lineage.as_deref());
        let accessible_label = match &presentation.lineage {
            Some(lineage) => format!(
                "{}, {}, {} files",
                presentation.tag.display_name, lineage, presentation.tag.file_count
            ),
            None => format!(
                "{}, {} files",
                presentation.tag.display_name, presentation.tag.file_count
            ),
        };
        row.update_property(&[gtk::accessible::Property::Label(&accessible_label)]);
        list_box.append(&row);
    }
}

/// Build the complete sidebar widget subtree.
pub fn build(ui_state: &crate::state::UiState, match_all: Rc<RefCell<bool>>) -> SidebarWidgets {
    let sidebar_root = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .css_classes(["sidebar-panel"])
        .vexpand(true)
        .build();

    // ── Brand block: icon + "Vesper" + subtitle + collapse « ──────────────
    let sidebar_header = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .css_classes(["sidebar-header"])
        .spacing(8)
        .build();

    // Brand mark: moon glyph on an indigo rounded tile (mockup 01).
    let brand_icon = gtk::Image::builder()
        .icon_name("weather-clear-night-symbolic")
        .pixel_size(22)
        .css_classes(["brand-icon"])
        .valign(gtk::Align::Center)
        .build();

    let brand_text = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .hexpand(true)
        .valign(gtk::Align::Center)
        .build();
    brand_text.append(
        &gtk::Label::builder()
            .label("Vesper")
            .css_classes(["title-1", "sidebar-brand"])
            .xalign(0.0)
            .build(),
    );
    brand_text.append(
        &gtk::Label::builder()
            .label("Quiet nocturne media gallery")
            .css_classes(["caption", "dim-label"])
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build(),
    );

    let collapse_button = gtk::Button::builder()
        .label("«")
        .css_classes(["flat", "sidebar-collapse"])
        .valign(gtk::Align::Center)
        .tooltip_text("Collapse sidebar")
        .build();
    collapse_button.update_property(&[gtk::accessible::Property::Label("Collapse sidebar")]);

    sidebar_header.append(&brand_icon);
    sidebar_header.append(&brand_text);
    sidebar_header.append(&collapse_button);
    sidebar_root.append(&sidebar_header);

    // ── Flat, count-sorted tag list ───────────────────────────────────────
    let tag_list_box = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["navigation-sidebar"])
        .margin_start(8)
        .margin_end(8)
        .build();

    // First-run single placeholder row (Arch §9).
    let no_tags_label = gtk::Label::builder()
        .label("No tags available")
        .css_classes(["dim-label"])
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .build();

    let tag_overlay = gtk::Overlay::builder().build();
    tag_overlay.set_child(Some(&tag_list_box));
    tag_overlay.add_overlay(&no_tags_label);

    let scrolled_sidebar = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&tag_overlay)
        .build();
    sidebar_root.append(&scrolled_sidebar);

    // First-run tip hint (Product §3), hidden once media exists.
    let tips_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .css_classes(["sidebar-tip"])
        .spacing(4)
        .visible(false)
        .build();
    tips_box.append(
        &gtk::Label::builder()
            .label("Tips")
            .css_classes(["caption", "dim-label"])
            .xalign(0.0)
            .build(),
    );
    tips_box.append(
        &gtk::Label::builder()
            .label("Add a source folder to start browsing your photos and videos.")
            .css_classes(["caption"])
            .wrap(true)
            .xalign(0.0)
            .build(),
    );
    sidebar_root.append(&tips_box);

    // ── AND/OR match-mode control (visible only with >= 2 tags, Arch §10) ──
    let match_mode_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .css_classes(["match-mode-box"])
        .spacing(12)
        .visible(false)
        .build();
    let match_label = gtk::Label::builder()
        .label("Match:")
        .css_classes(["dim-label"])
        .valign(gtk::Align::Center)
        .build();

    let is_and = ui_state.tag_filter_mode == "AND";
    *match_all.borrow_mut() = is_and;

    let match_any_radio = gtk::CheckButton::builder()
        .label("Any")
        .active(!is_and)
        .build();
    let match_all_radio = gtk::CheckButton::builder()
        .label("All")
        .group(&match_any_radio)
        .active(is_and)
        .build();

    match_mode_box.append(&match_label);
    match_mode_box.append(&match_any_radio);
    match_mode_box.append(&match_all_radio);
    sidebar_root.append(&match_mode_box);

    // ── Footer: primary Add Source Root + settings gear ───────────────────
    let sidebar_footer = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .css_classes(["sidebar-footer"])
        .spacing(8)
        .build();

    let add_source_root_button = gtk::Button::builder()
        .css_classes(["suggested-action"])
        .hexpand(true)
        .build();
    let add_content = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .halign(gtk::Align::Center)
        .spacing(8)
        .build();
    add_content.append(&gtk::Image::from_icon_name("list-add-symbolic"));
    add_content.append(&gtk::Label::new(Some("Add Source Root")));
    add_source_root_button.set_child(Some(&add_content));
    add_source_root_button.update_property(&[gtk::accessible::Property::Label("Add Source Root")]);

    let open_settings_button = gtk::Button::builder()
        .icon_name("preferences-system-symbolic")
        .css_classes(["flat"])
        .tooltip_text("Settings")
        .build();
    open_settings_button.update_property(&[gtk::accessible::Property::Label("Settings")]);

    sidebar_footer.append(&add_source_root_button);
    sidebar_footer.append(&open_settings_button);
    sidebar_root.append(&sidebar_footer);

    SidebarWidgets {
        root: sidebar_root,
        tag_list_box,
        tags: Rc::new(RefCell::new(Vec::new())),
        match_any_radio,
        match_all_radio,
        match_mode_box,
        no_tags_label,
        tips_box,
        add_source_root_button,
        open_settings_button,
        collapse_button,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(
        root: i64,
        path: &str,
        name: &str,
        display_path: &str,
        count: i64,
    ) -> crate::events::UiTag {
        crate::events::UiTag {
            id: root * 100 + count,
            source_root_id: root,
            relative_folder_path: path.to_string(),
            display_name: name.to_string(),
            display_path: display_path.to_string(),
            file_count: count,
        }
    }

    #[test]
    fn duplicate_tag_names_render_with_lineage_disambiguation() {
        let tags = [
            tag(1, "Travel/2023", "2023", "Travel / 2023", 4),
            tag(2, "Archive/2023", "2023", "Archive / 2023", 4),
        ];
        let rows = prepare_tag_rows(&tags, &[]);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].tag.display_name, "2023");
        assert_eq!(rows[0].lineage.as_deref(), Some("Travel / 2023"));
        assert_eq!(rows[1].lineage.as_deref(), Some("Archive / 2023"));
    }

    #[test]
    fn tag_rows_sort_by_count_name_and_path_identity() {
        let tags = [
            tag(1, "Z/2023", "2023", "Z / 2023", 3),
            tag(1, "Travel", "travel", "Travel", 8),
            tag(1, "A/2023", "2023", "A / 2023", 3),
            tag(1, "Archive", "Archive", "Archive", 8),
        ];
        let rows = prepare_tag_rows(&tags, &[]);
        let order: Vec<(&str, i64)> = rows
            .iter()
            .map(|row| (row.tag.display_name.as_str(), row.tag.file_count))
            .collect();
        assert_eq!(
            order,
            [("Archive", 8), ("travel", 8), ("2023", 3), ("2023", 3)]
        );
    }
}
