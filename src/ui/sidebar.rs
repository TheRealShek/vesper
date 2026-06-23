use libadwaita::gtk::{self};
use libadwaita::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// All widget handles the caller needs from the sidebar.
pub struct SidebarWidgets {
    pub root: gtk::Box,
    pub tag_list_box: gtk::ListBox,
    pub tag_names: Rc<RefCell<Vec<String>>>,
    pub match_any_radio: gtk::CheckButton,
    pub match_all_radio: gtk::CheckButton,
    pub match_mode_box: gtk::Box,
    pub no_tags_label: gtk::Label,
    pub roots_list_box: gtk::ListBox,
    pub update_tag_visibility: Rc<dyn Fn()>,
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
    // ├── roots_frame (gtk::Frame)
    // │   └── roots_list_box (gtk::Box, vertical)
    // └── gtk::Separator (horizontal)

    // Width is constrained in CSS because GTK can ignore width_request under certain expand conditions, whereas CSS min-width/max-width is strictly enforced.
    let sidebar_root = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .css_classes(["sidebar-panel", "background"])
        .vexpand(true)
        .build();

    let tags_header = gtk::Label::builder()
        .label("TAGS")
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

    let update_tag_visibility: Rc<dyn Fn()> = {
        let tag_list_box = tag_list_box.clone();
        let show_more_btn = show_more_btn.clone();
        let tag_search_entry = tag_search_entry.clone();
        let show_all_tags = show_all_tags.clone();
        Rc::new(move || {
            let text = tag_search_entry.text().to_lowercase();
            let show_all = *show_all_tags.borrow();
            let mut total_matches = 0;

            let mut child = tag_list_box.first_child();
            while let Some(row) = child {
                let mut matches = true;
                if !text.is_empty()
                    && let Some(r) = row.downcast_ref::<gtk::ListBoxRow>()
                    && let Some(lbl) = r.child().and_downcast::<gtk::Label>()
                {
                    matches = lbl.text().to_lowercase().contains(&text);
                }

                if matches {
                    total_matches += 1;
                    if total_matches <= 30 || show_all {
                        row.set_visible(true);
                    } else {
                        row.set_visible(false);
                    }
                } else {
                    row.set_visible(false);
                }
                child = row.next_sibling();
            }

            if total_matches > 30 {
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
        .label("SOURCES")
        .css_classes(["dim-label", "caption"])
        .halign(gtk::Align::Start)
        .margin_start(12)
        .margin_bottom(8)
        .build();
    let roots_list_box = gtk::ListBox::builder()
        .css_classes(["navigation-sidebar"])
        .selection_mode(gtk::SelectionMode::None)
        .build();

    let roots_frame = gtk::Frame::builder()
        .css_classes(["card", "sources-card"])
        .margin_start(12)
        .margin_end(12)
        .margin_bottom(12)
        .child(&roots_list_box)
        .build();

    sidebar_root.append(&roots_header);
    sidebar_root.append(&roots_frame);

    let tag_names: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));

    SidebarWidgets {
        root: sidebar_root,
        tag_list_box,
        tag_names,
        match_any_radio,
        match_all_radio,
        match_mode_box,
        no_tags_label,
        roots_list_box,
        update_tag_visibility,
    }
}
