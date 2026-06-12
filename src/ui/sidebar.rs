use libadwaita as adw;
use libadwaita::prelude::*;
use libadwaita::gtk::{self};
use std::cell::RefCell;
use std::rc::Rc;

/// All widget handles the caller needs from the sidebar.
pub struct SidebarWidgets {
    pub toolbar: adw::ToolbarView,
    pub tag_list_box: gtk::ListBox,
    pub tag_names: Rc<RefCell<Vec<String>>>,
    pub clear_tags_btn: gtk::Button,
    pub match_switch: gtk::Switch,
    pub match_mode_box: gtk::Box,
    pub no_tags_label: gtk::Label,
    pub roots_list_box: gtk::Box,
    pub update_tag_visibility: Rc<dyn Fn()>,
}

/// Build the complete sidebar widget subtree.
pub fn build(ui_state: &crate::state::UiState, match_all: Rc<RefCell<bool>>) -> SidebarWidgets {
    let sidebar_toolbar = adw::ToolbarView::new();
    let sidebar_header = adw::HeaderBar::builder()
        .show_end_title_buttons(false)
        .show_start_title_buttons(false)
        .build();
    let empty_title = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    sidebar_header.set_title_widget(Some(&empty_title));

    let clear_tags_btn = gtk::Button::builder()
        .label("Clear all")
        .css_classes(["flat"])
        .visible(false)
        .build();
    sidebar_header.pack_end(&clear_tags_btn);
    sidebar_toolbar.add_top_bar(&sidebar_header);

    let sidebar_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .css_classes(["vesper-sidebar"])
        .margin_start(12)
        .build();

    let tag_search_entry = gtk::SearchEntry::builder()
        .placeholder_text("Filter tags...")
        .margin_start(12)
        .margin_end(12)
        .margin_top(12)
        .margin_bottom(6)
        .build();
    tag_search_entry.update_property(&[gtk::accessible::Property::Label("Tag search")]);
    sidebar_box.append(&tag_search_entry);

    let tag_list_box = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::Multiple)
        .css_classes(["navigation-sidebar"])
        .margin_start(8)
        .margin_end(8)
        .build();

    let no_tags_label = gtk::Label::builder()
        .label("No tags available")
        .css_classes(["dim-label"])
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .vexpand(true)
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
                if !text.is_empty() {
                    if let Some(r) = row.downcast_ref::<gtk::ListBoxRow>() {
                        if let Some(lbl) = r.child().and_downcast::<gtk::Label>() {
                            matches = lbl.text().to_lowercase().contains(&text);
                        }
                    }
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

    let scrolled_sidebar = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .child(&tag_overlay)
        .build();
    sidebar_box.append(&scrolled_sidebar);

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
        .label("Match all")
        .tooltip_text("Match all active tags (AND logic)")
        .build();
    let is_and = ui_state.tag_filter_mode == "AND";
    let match_switch = gtk::Switch::builder().active(is_and).valign(gtk::Align::Center).build();
    match_switch.update_property(&[gtk::accessible::Property::Label("Filter mode")]);
    *match_all.borrow_mut() = is_and;
    match_mode_box.append(&match_label);
    match_mode_box.append(&match_switch);
    sidebar_box.append(&match_mode_box);

    let roots_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .margin_start(12)
        .margin_end(12)
        .margin_top(12)
        .margin_bottom(12)
        .build();
    let roots_header = gtk::Label::builder()
        .label("Source Roots")
        .css_classes(["dim-label"])
        .halign(gtk::Align::Start)
        .margin_bottom(8)
        .build();
    let roots_list_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .build();
    roots_box.append(&roots_header);
    roots_box.append(&roots_list_box);
    sidebar_box.append(&gtk::Separator::builder().build());
    sidebar_box.append(&roots_box);

    sidebar_toolbar.set_content(Some(&sidebar_box));
    sidebar_toolbar.set_width_request(180);

    let tag_names: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));

    SidebarWidgets {
        toolbar: sidebar_toolbar,
        tag_list_box,
        tag_names,
        clear_tags_btn,
        match_switch,
        match_mode_box,
        no_tags_label,
        roots_list_box,
        update_tag_visibility,
    }
}
