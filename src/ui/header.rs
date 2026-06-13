use libadwaita as adw;
use libadwaita::prelude::*;
use libadwaita::gtk::{self};
use std::cell::RefCell;
use std::rc::Rc;

/// All widget handles the caller needs from the top bar.
pub struct HeaderWidgets {
    pub header_bar: adw::HeaderBar,
    pub search_entry: gtk::SearchEntry,
    pub zoom_slider: gtk::Scale,
    pub zoom_box: gtk::Box,
    pub active_filter_pill: gtk::Button,
    pub sort_menu_btn: gtk::MenuButton,
    pub sort_radios: Vec<gtk::CheckButton>,
    pub settings_btn: gtk::Button,
    pub offline_banner: adw::Banner,
    pub scan_error_button: gtk::Button,
    pub scan_error_paths: Rc<RefCell<Vec<String>>>,
}

/// Build the top bar and its child widgets.
pub fn build(
    ui_state: &crate::state::UiState,
) -> HeaderWidgets {
    let header_bar = adw::HeaderBar::new();

    let offline_banner = adw::Banner::builder()
        .revealed(false)
        .build();
    let scan_error_button = gtk::Button::builder()
        .css_classes(["osd", "pill"])
        .halign(gtk::Align::Start)
        .valign(gtk::Align::End)
        .margin_start(16)
        .margin_bottom(16)
        .visible(false)
        .build();
    let scan_error_paths = Rc::new(RefCell::new(Vec::<String>::new()));

    let active_filter_pill = gtk::Button::builder()
        .css_classes(["pill", "suggested-action"])
        .visible(false)
        .valign(gtk::Align::Center)
        .margin_start(8)
        .build();
    active_filter_pill.update_property(&[gtk::accessible::Property::Label("Clear active filters")]);

    let search_entry = gtk::SearchEntry::builder()
        .placeholder_text("Search media...")
        .hexpand(true)
        .valign(gtk::Align::Center)
        .build();
    search_entry.update_property(&[gtk::accessible::Property::Label("Search media")]);

    let sort_model_list = [
        "Date modified (newest first)",
        "Date modified (oldest first)",
        "Date created (newest first)",
        "Date created (oldest first)",
        "Filename (A → Z)",
        "Filename (Z → A)",
        "File size (largest first)",
        "File size (smallest first)",
    ];
    
    let sort_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .margin_start(12)
        .margin_end(12)
        .margin_top(12)
        .margin_bottom(12)
        .build();
        
    let initial_sort = &ui_state.sort_order;
    let mut sort_radios = Vec::new();
    let mut prev_radio: Option<gtk::CheckButton> = None;
    
    for sort_opt in &sort_model_list {
        let radio = gtk::CheckButton::builder()
            .label(*sort_opt)
            .build();
        if let Some(prev) = &prev_radio {
            radio.set_group(Some(prev));
        }
        if sort_opt == initial_sort {
            radio.set_active(true);
        }
        sort_box.append(&radio);
        prev_radio = Some(radio.clone());
        sort_radios.push(radio);
    }

    let sort_popover = gtk::Popover::builder()
        .child(&sort_box)
        .build();

    let sort_menu_btn = gtk::MenuButton::builder()
        .icon_name("view-more-symbolic")
        .tooltip_text("Sort by")
        .popover(&sort_popover)
        .valign(gtk::Align::Center)
        .margin_start(6)
        .margin_end(6)
        .visible(false)
        .build();

    // Zoom slider
    let initial_zoom = ui_state.zoom_level;
    let zoom_adj = gtk::Adjustment::new(initial_zoom, 0.0, 4.0, 1.0, 1.0, 0.0);
    let zoom_slider = gtk::Scale::builder()
        .orientation(gtk::Orientation::Horizontal)
        .adjustment(&zoom_adj)
        .draw_value(false)
        .valign(gtk::Align::Center)
        .width_request(120)
        .build();
    zoom_slider.update_property(&[gtk::accessible::Property::Label("Zoom level")]);

    let zoom_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .margin_start(6)
        .margin_end(6)
        .valign(gtk::Align::Center)
        .tooltip_text("Grid Zoom Size")
        .visible(false)
        .build();

    zoom_box.append(&gtk::Image::from_icon_name("zoom-out-symbolic"));
    zoom_box.append(&zoom_slider);
    zoom_box.append(&gtk::Image::from_icon_name("zoom-in-symbolic"));

    let settings_btn = gtk::Button::builder()
        .icon_name("preferences-system-symbolic")
        .tooltip_text("Settings")
        .css_classes(["flat"])
        .valign(gtk::Align::Center)
        .build();
    settings_btn.update_property(&[gtk::accessible::Property::Label("Settings")]);

    header_bar.pack_end(&settings_btn);
    header_bar.pack_end(&sort_menu_btn);
    header_bar.pack_end(&zoom_box);
    header_bar.pack_end(&active_filter_pill);
    header_bar.pack_end(&search_entry);

    HeaderWidgets {
        header_bar,
        search_entry,
        zoom_slider,
        zoom_box,
        active_filter_pill,
        sort_menu_btn,
        sort_radios,
        settings_btn,
        offline_banner,
        scan_error_button,
        scan_error_paths,
    }
}
