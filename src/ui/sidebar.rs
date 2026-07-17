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
    pub roots_list_box: gtk::ListBox,
    pub update_tag_visibility: Rc<dyn Fn()>,
}

#[derive(Debug, Clone)]
pub(crate) struct TagRowPresentation {
    pub tag: crate::events::UiTag,
    pub lineage: Option<String>,
}

/// Sorts tag rows and adds collision-only lineage text per the canonical
/// sidebar model. A root path is appended only when two equal display paths
/// would otherwise remain ambiguous.
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
            // U-4: after the case-insensitive name, the tie-break is the exact
            // A-2 path identity — display_path is presentation data and must
            // not participate in ordering.
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
        let text_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .build();
        let name = gtk::Label::builder()
            .label(&presentation.tag.display_name)
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
            .css_classes(["dim-label"])
            .valign(gtk::Align::Center)
            .build();
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .margin_start(9)
            .margin_end(12)
            .margin_top(6)
            .margin_bottom(6)
            .build();
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
    // Widget Hierarchy:
    // sidebar_root (gtk::Box, vertical) [vexpand=true]
    // ├── tags_header (gtk::Label)
    // ├── tag_search_entry (gtk::SearchEntry)
    // ├── scrolled_sidebar (gtk::ScrolledWindow) [vexpand=true]
    // │   └── tag_overlay (gtk::Overlay)
    // │       ├── child: tag_vbox (gtk::Box, vertical)
    // │       │   ├── tag_list_box (gtk::ListBox)
    // │       │   └── show_more_btn (gtk::Button)
    // │       └── overlay: no_tags_label (gtk::Label)
    // ├── match_mode_box (gtk::Box, horizontal)
    // │   ├── match_label
    // │   ├── match_any_radio
    // │   └── match_all_radio
    // ├── gtk::Separator (horizontal)
    // ├── roots_header (gtk::Label)
    // └── roots_list_box (gtk::ListBox, .sources-list — flat, no card frame)

    // Width is constrained in CSS because GTK can ignore width_request under certain expand conditions, whereas CSS min-width/max-width is strictly enforced.
    let sidebar_root = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .css_classes(["sidebar-panel", "background"])
        .vexpand(true)
        .build();

    let tags_header = gtk::Label::builder()
        .label("Tags")
        .css_classes(["dim-label", "caption"])
        .halign(gtk::Align::Start)
        .margin_start(12)
        .margin_top(16)
        .margin_bottom(4)
        .build();
    sidebar_root.append(&tags_header);

    let tag_search_entry = gtk::SearchEntry::builder()
        .placeholder_text("Filter tags...")
        .margin_start(12)
        .margin_end(12)
        .margin_bottom(6)
        .build();
    tag_search_entry.update_property(&[gtk::accessible::Property::Label("Tag search")]);
    sidebar_root.append(&tag_search_entry);

    // Rebuilt from scratch on every TagsUpdated rather than diffed because tag counts change on every file event; diffing adds complexity with no measurable gain.
    let tag_list_box = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["navigation-sidebar"])
        .margin_start(8)
        .margin_end(8)
        .build();

    let no_tags_label = gtk::Label::builder()
        .label("No tags available")
        .css_classes(["dim-label"])
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .build();

    let tag_overlay = gtk::Overlay::builder().build();
    let show_more_btn = gtk::Button::builder()
        .label("Show more")
        .css_classes(["flat"])
        .margin_start(8)
        .margin_end(8)
        .margin_top(4)
        .margin_bottom(4)
        .visible(false)
        .build();

    let tag_vbox = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .build();
    tag_vbox.append(&tag_list_box);
    tag_vbox.append(&show_more_btn);

    tag_overlay.set_child(Some(&tag_vbox));
    tag_overlay.add_overlay(&no_tags_label);

    let show_all_tags = Rc::new(RefCell::new(false));

    let tags: Rc<RefCell<Vec<crate::events::UiTag>>> = Rc::new(RefCell::new(Vec::new()));

    let update_tag_visibility: Rc<dyn Fn()> = {
        let tag_list_box = tag_list_box.clone();
        let show_more_btn = show_more_btn.clone();
        let tag_search_entry = tag_search_entry.clone();
        let show_all_tags = show_all_tags.clone();
        let tags = tags.clone();
        Rc::new(move || {
            let text = tag_search_entry.text().trim().to_lowercase();
            // NEW-7: a non-empty tag query shows every match, bypassing the
            // 30-row collapse without touching the saved session expansion
            // flag; clearing the query reapplies that flag.
            let searching = !text.is_empty();
            let show_all = *show_all_tags.borrow();
            let mut total_matches = 0;

            let mut child = tag_list_box.first_child();
            while let Some(row) = child {
                let mut matches = true;
                if !text.is_empty() {
                    matches = row
                        .downcast_ref::<gtk::ListBoxRow>()
                        .and_then(|row| tags.borrow().get(row.index() as usize).cloned())
                        .is_some_and(|tag| {
                            tag.display_name.to_lowercase().contains(&text)
                                || tag.display_path.to_lowercase().contains(&text)
                        });
                }

                if matches {
                    total_matches += 1;
                    row.set_visible(searching || show_all || total_matches <= 30);
                } else {
                    row.set_visible(false);
                }
                child = row.next_sibling();
            }

            if !searching && total_matches > 30 {
                show_more_btn.set_visible(true);
                show_more_btn.set_label(if show_all { "Show less" } else { "Show more" });
            } else {
                show_more_btn.set_visible(false);
            }
        })
    };

    tag_search_entry.connect_search_changed({
        let update_vis = update_tag_visibility.clone();
        move |_| update_vis()
    });

    show_more_btn.connect_clicked({
        let show_all_tags = show_all_tags.clone();
        let update_vis = update_tag_visibility.clone();
        move |_| {
            let current = *show_all_tags.borrow();
            *show_all_tags.borrow_mut() = !current;
            update_vis();
        }
    });

    // The only widget in the sidebar with vexpand=true so that headers and static elements take their natural height and the tag list fills the rest.
    let scrolled_sidebar = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .child(&tag_overlay)
        .build();
    sidebar_root.append(&scrolled_sidebar);

    let match_mode_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .margin_start(12)
        .margin_end(12)
        .margin_top(12)
        .margin_bottom(12)
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
    let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
    sep.set_margin_top(8);
    sep.set_margin_bottom(8);
    sidebar_root.append(&sep);

    let roots_header = gtk::Label::builder()
        .label("Sources")
        .css_classes(["dim-label", "caption"])
        .halign(gtk::Align::Start)
        .margin_start(12)
        .margin_bottom(8)
        .build();
    // VIS-2 / 05 §5: flat source rows — no card/frame around the source list.
    let roots_list_box = gtk::ListBox::builder()
        .css_classes(["navigation-sidebar", "sources-list"])
        .selection_mode(gtk::SelectionMode::None)
        .margin_start(8)
        .margin_end(8)
        .margin_bottom(12)
        .build();

    sidebar_root.append(&roots_header);
    sidebar_root.append(&roots_list_box);

    SidebarWidgets {
        root: sidebar_root,
        tag_list_box,
        tags,
        match_any_radio,
        match_all_radio,
        match_mode_box,
        no_tags_label,
        roots_list_box,
        update_tag_visibility,
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
        // U-4: equal counts and names tie-break on the exact path identity
        // (source_root_id, relative_folder_path), not on display_path.
        assert_eq!(rows[0].tag.display_name, "2023");
        assert_eq!(rows[0].lineage.as_deref(), Some("Travel / 2023"));
        assert_eq!(rows[1].tag.display_name, "2023");
        assert_eq!(rows[1].lineage.as_deref(), Some("Archive / 2023"));
    }

    #[test]
    fn tag_rows_sort_by_count_name_and_display_path() {
        let tags = [
            tag(1, "Z/2023", "2023", "Z / 2023", 3),
            tag(1, "Travel", "travel", "Travel", 8),
            tag(1, "A/2023", "2023", "A / 2023", 3),
            tag(1, "Archive", "Archive", "Archive", 8),
        ];
        let rows = prepare_tag_rows(&tags, &[]);
        let order: Vec<(&str, &str, i64)> = rows
            .iter()
            .map(|row| {
                (
                    row.tag.display_name.as_str(),
                    row.tag.display_path.as_str(),
                    row.tag.file_count,
                )
            })
            .collect();

        assert_eq!(
            order,
            [
                ("Archive", "Archive", 8),
                ("travel", "Travel", 8),
                ("2023", "A / 2023", 3),
                ("2023", "Z / 2023", 3),
            ]
        );
    }
}
