use libadwaita as adw;
use libadwaita::gtk::{self};
use libadwaita::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// All widget handles the caller needs from the top bar.
pub struct HeaderWidgets {
    pub header_bar: adw::HeaderBar,
    pub search_entry: gtk::SearchEntry,
    pub zoom_slider: gtk::Scale,
    pub clear_filters_button: gtk::Button,
    pub sort_menu_btn: gtk::MenuButton,
    pub sort_radios: Vec<gtk::CheckButton>,
    pub settings_btn: gtk::Button,
    pub scan_error_button: gtk::Button,
    // Holds a transient backend-warning message (non-scan). Scan-error paths are
    // no longer cached here — the button reads them from the scan_errors table (A-4).
    pub backend_warning: Rc<RefCell<Option<String>>>,
}

/// Build the top bar and its child widgets.
pub fn build(ui_state: &crate::state::UiState) -> HeaderWidgets {
    let header_bar = adw::HeaderBar::new();

    let scan_error_button = gtk::Button::builder()
        .css_classes(["flat"])
        .halign(gtk::Align::Start)
        .valign(gtk::Align::End)
        .margin_start(16)
        .margin_bottom(16)
        .visible(false)
        .build();
    let backend_warning = Rc::new(RefCell::new(None::<String>));

    // Uses set_visible instead of opacity because opacity leaves an empty gap in the GTK layout, while set_visible triggers proper reflow.
    let clear_filters_button = gtk::Button::builder()
        .visible(false)
        .valign(gtk::Align::Center)
        .build();
    clear_filters_button
        .update_property(&[gtk::accessible::Property::Label("Clear active filters")]);

    // Always visible per spec rule: search must never be hidden behind an icon toggle once a source root is configured.
    let search_entry = gtk::SearchEntry::builder()
        .placeholder_text("Search media...")
        .width_request(280)
        .hexpand(true)
        .valign(gtk::Align::Center)
        .sensitive(false)
        .build();
    search_entry.update_property(&[
        gtk::accessible::Property::Label("Search media"),
        gtk::accessible::Property::Description("Search is unavailable until media is added."),
    ]);
    search_entry.set_tooltip_text(Some("Search is unavailable until media is added."));

    let sort_model_list = [
        "Date modified (newest first)",
        "Date modified (oldest first)",
        "Date added (newest first)",
        "Date added (oldest first)",
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
        let radio = gtk::CheckButton::builder().label(*sort_opt).build();
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

    // Sort options live in a popover rather than the main header to keep the UI uncluttered since sorting is an infrequent operation.
    let sort_popover = gtk::Popover::builder().child(&sort_box).build();

    let sort_menu_btn = gtk::MenuButton::builder()
        .label("Sort")
        .always_show_arrow(true)
        .tooltip_text("Sort media")
        .popover(&sort_popover)
        .valign(gtk::Align::Center)
        .sensitive(false)
        .build();
    sort_menu_btn.update_property(&[gtk::accessible::Property::Description(
        "Sorting is unavailable until media is added.",
    )]);
    sort_menu_btn.set_tooltip_text(Some("Sorting is unavailable until media is added."));

    // Zoom slider
    let initial_zoom = ui_state.zoom_level;
    let zoom_adj = gtk::Adjustment::new(initial_zoom, 0.0, 4.0, 1.0, 1.0, 0.0);
    let zoom_slider = gtk::Scale::builder()
        .orientation(gtk::Orientation::Horizontal)
        .adjustment(&zoom_adj)
        .draw_value(false)
        .round_digits(0)
        .valign(gtk::Align::Center)
        .width_request(96)
        .tooltip_text("Thumbnail sizing is unavailable until media is added.")
        .sensitive(false)
        .build();
    for value in 0..=4 {
        zoom_slider.add_mark(value.into(), gtk::PositionType::Bottom, None);
    }
    update_zoom_accessibility(&zoom_slider);
    zoom_slider.connect_value_changed(update_zoom_accessibility);

    let settings_btn = gtk::Button::builder()
        .icon_name("preferences-system-symbolic")
        .tooltip_text("Settings")
        .css_classes(["flat"])
        .valign(gtk::Align::Center)
        .build();
    settings_btn.update_property(&[gtk::accessible::Property::Label("Settings")]);

    let controls_group = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .valign(gtk::Align::Center)
        .build();
    controls_group.append(&clear_filters_button);
    controls_group.append(&zoom_slider);
    controls_group.append(&sort_menu_btn);
    controls_group.append(&settings_btn);

    let window_title = adw::WindowTitle::builder().title("Vesper").build();
    let search_clamp = adw::Clamp::builder()
        .maximum_size(360)
        .tightening_threshold(280)
        .child(&search_entry)
        .build();

    header_bar.pack_start(&window_title);
    header_bar.set_title_widget(Some(&search_clamp));
    header_bar.pack_end(&controls_group);

    HeaderWidgets {
        header_bar,
        search_entry,
        zoom_slider,
        clear_filters_button,
        sort_menu_btn,
        sort_radios,
        settings_btn,
        scan_error_button,
        backend_warning,
    }
}

fn update_zoom_accessibility(scale: &gtk::Scale) {
    let size = zoom_size_name(scale.value());
    let description = if scale.is_sensitive() {
        format!("Thumbnail size: {size}")
    } else {
        "Thumbnail sizing is unavailable until media is added.".to_string()
    };
    scale.set_tooltip_text(Some(&description));
    scale.update_property(&[
        gtk::accessible::Property::Label("Thumbnail size"),
        gtk::accessible::Property::ValueText(size),
        gtk::accessible::Property::Description(&description),
    ]);
}

pub fn set_media_controls_available(
    search_entry: &gtk::SearchEntry,
    sort_menu_btn: &gtk::MenuButton,
    zoom_slider: &gtk::Scale,
    available: bool,
) {
    search_entry.set_sensitive(available);
    sort_menu_btn.set_sensitive(available);
    zoom_slider.set_sensitive(available);

    let search_description = if available {
        "Search media"
    } else {
        "Search is unavailable until media is added."
    };
    search_entry.set_tooltip_text(Some(search_description));
    search_entry.update_property(&[gtk::accessible::Property::Description(search_description)]);

    let sort_description = if available {
        "Sort media"
    } else {
        "Sorting is unavailable until media is added."
    };
    sort_menu_btn.set_tooltip_text(Some(sort_description));
    sort_menu_btn.update_property(&[gtk::accessible::Property::Description(sort_description)]);
    update_zoom_accessibility(zoom_slider);
}

fn zoom_size_name(value: f64) -> &'static str {
    match value.round() as i32 {
        0 => "XS",
        1 => "S",
        2 => "M",
        3 => "L",
        4 => "XL",
        _ if value < 0.0 => "XS",
        _ => "XL",
    }
}
