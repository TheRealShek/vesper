use libadwaita as adw;
use libadwaita::prelude::*;
use libadwaita::gtk::{self};
use std::cell::RefCell;
use std::rc::Rc;

/// All widget handles the caller needs from the top bar.
pub struct HeaderWidgets {
    pub toolbar: adw::ToolbarView,
    pub header_bar: adw::HeaderBar,
    pub toggle_sidebar_btn: gtk::ToggleButton,
    pub search_entry: gtk::SearchEntry,
    pub sort_dropdown: gtk::DropDown,
    pub zoom_slider: gtk::Scale,
    pub zoom_box: gtk::Box,
    pub filter_indicator: gtk::Label,
    pub clear_all_filters_btn: gtk::Button,
    pub settings_btn: gtk::Button,
    pub offline_banner: adw::Banner,
    pub scan_error_button: gtk::Button,
    pub scan_error_paths: Rc<RefCell<Vec<String>>>,
}

/// Build the top bar and its child widgets.
///
/// `split_view` and `last_sidebar_width` are needed for the sidebar toggle button wiring.
pub fn build(
    ui_state: &crate::state::UiState,
    split_view: &gtk::Paned,
    last_sidebar_width: &Rc<std::cell::Cell<i32>>,
) -> HeaderWidgets {
    let content_toolbar = adw::ToolbarView::new();
    let content_header = adw::HeaderBar::new();

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

    content_toolbar.add_top_bar(&content_header);
    content_toolbar.add_top_bar(&offline_banner);

    let toggle_sidebar_btn = gtk::ToggleButton::builder()
        .icon_name("sidebar-show-symbolic")
        .tooltip_text("Toggle Sidebar")
        .active(false)
        .visible(false)
        .build();
    toggle_sidebar_btn.update_property(&[gtk::accessible::Property::Label("Toggle sidebar")]);
    let split_view_clone = split_view.clone();
    let last_w_btn = last_sidebar_width.clone();
    toggle_sidebar_btn.connect_toggled(move |btn| {
        if btn.is_active() {
            split_view_clone.set_position(last_w_btn.get());
        } else {
            let pos = split_view_clone.position();
            if pos > 0 {
                last_w_btn.set(pos);
            }
            split_view_clone.set_position(0);
        }
    });
    let app_title = gtk::Label::builder()
        .label("Vesper")
        .css_classes(["heading"])
        .margin_start(8)
        .margin_end(8)
        .build();
    content_header.pack_start(&app_title);

    content_header.pack_start(&toggle_sidebar_btn);

    let filter_indicator = gtk::Label::new(None);
    filter_indicator.add_css_class("dim-label");
    content_header.pack_start(&filter_indicator);

    let clear_all_filters_btn = gtk::Button::builder()
        .label("Clear filters")
        .visible(false)
        .build();
    clear_all_filters_btn.update_property(&[gtk::accessible::Property::Label("Clear filters")]);
    content_header.pack_start(&clear_all_filters_btn);

    let search_entry = gtk::SearchEntry::builder()
        .placeholder_text("Search media...")
        .width_request(250)
        .build();
    search_entry.update_property(&[gtk::accessible::Property::Label("Search media")]);
    content_header.set_title_widget(Some(&search_entry));

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
    let sort_model = gtk::StringList::new(&sort_model_list);
    let sort_dropdown = gtk::DropDown::builder()
        .model(&sort_model)
        .tooltip_text("Sort by")
        .margin_start(6)
        .margin_end(6)
        .valign(gtk::Align::Center)
        .visible(false)
        .build();
    sort_dropdown.update_property(&[gtk::accessible::Property::Label("Sort order")]);

    let initial_sort = &ui_state.sort_order;
    if let Some(pos) = sort_model_list.iter().position(|&s| s == initial_sort) {
        sort_dropdown.set_selected(pos as u32);
    }

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

    content_header.pack_end(&settings_btn);
    content_header.pack_end(&sort_dropdown);
    content_header.pack_end(&zoom_box);

    HeaderWidgets {
        toolbar: content_toolbar,
        header_bar: content_header,
        toggle_sidebar_btn,
        search_entry,
        sort_dropdown,
        zoom_slider,
        zoom_box,
        filter_indicator,
        clear_all_filters_btn,
        settings_btn,
        offline_banner,
        scan_error_button,
        scan_error_paths,
    }
}
