//! The grid-area header bar (Arch §9): a sidebar-restore toggle (shown only
//! while the sidebar is collapsed), the search entry, and the trailing Sort /
//! thumbnail-size / selection / primary-menu controls plus window controls.

use libadwaita as adw;
use libadwaita::gtk::{self};
use libadwaita::prelude::*;

/// The five thumbnail sizes (Product §2 / Vision §6) with their `Ctrl+N`
/// shortcuts, in index order 0..=4.
pub const THUMB_SIZE_LABELS: [&str; 5] = ["Extra Small", "Small", "Medium", "Large", "Extra Large"];

/// Sort options in the order the radios are built; the strings match the
/// persisted `sort_order` values in [`crate::state::UiState`].
pub const SORT_ORDER_LABELS: [&str; 8] = [
    "Date modified (newest first)",
    "Date modified (oldest first)",
    "Date added (newest first)",
    "Date added (oldest first)",
    "Filename (A → Z)",
    "Filename (Z → A)",
    "File size (largest first)",
    "File size (smallest first)",
];

/// All widget handles the caller needs from the header.
pub struct HeaderWidgets {
    pub header_bar: adw::HeaderBar,
    pub sidebar_toggle: gtk::Button,
    pub search_entry: gtk::SearchEntry,
    pub clear_filters_button: gtk::Button,
    pub sort_menu_btn: gtk::MenuButton,
    pub sort_radios: Vec<gtk::CheckButton>,
    pub thumb_size_btn: gtk::MenuButton,
    /// The five thumbnail-size option buttons, index 0..=4.
    pub thumb_size_buttons: Vec<gtk::Button>,
    /// The check icon shown on the active thumbnail-size row, index 0..=4.
    pub thumb_size_checks: Vec<gtk::Image>,
    pub select_button: gtk::ToggleButton,
    pub primary_settings_btn: gtk::Button,
    pub primary_shortcuts_btn: gtk::Button,
    pub primary_about_btn: gtk::Button,
}

/// Build the header bar and its child widgets.
pub fn build(ui_state: &crate::state::UiState) -> HeaderWidgets {
    let header_bar = adw::HeaderBar::new();

    // Start: sidebar restore toggle, visible only while the sidebar is
    // collapsed (Arch §10). The caller flips its visibility.
    let sidebar_toggle = gtk::Button::builder()
        .icon_name("sidebar-show-symbolic")
        .css_classes(["flat", "sidebar-toggle"])
        .tooltip_text("Show sidebar")
        .visible(false)
        .build();
    sidebar_toggle.update_property(&[gtk::accessible::Property::Label("Show sidebar")]);
    header_bar.pack_start(&sidebar_toggle);

    // Title widget: the search entry. Placeholder "Search Vesper" with a `/`
    // focus hint (Product §2). Search clears on every launch (Arch §8).
    let search_entry = gtk::SearchEntry::builder()
        .placeholder_text("Search Vesper")
        .width_request(320)
        .valign(gtk::Align::Center)
        .sensitive(false)
        .build();
    search_entry.set_tooltip_text(Some("Search  (press / to focus)"));
    search_entry.update_property(&[gtk::accessible::Property::Label("Search Vesper")]);
    let search_clamp = adw::Clamp::builder()
        .maximum_size(420)
        .tightening_threshold(320)
        .child(&search_entry)
        .build();
    header_bar.set_title_widget(Some(&search_clamp));

    // ── Trailing controls ─────────────────────────────────────────────────
    let clear_filters_button = gtk::Button::builder()
        .visible(false)
        .valign(gtk::Align::Center)
        .build();
    clear_filters_button
        .update_property(&[gtk::accessible::Property::Label("Clear active filters")]);

    // Sort popover with the eight order radios.
    let sort_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .margin_top(8)
        .margin_bottom(8)
        .margin_start(8)
        .margin_end(8)
        .build();
    let mut sort_radios = Vec::new();
    let mut prev_radio: Option<gtk::CheckButton> = None;
    for label in SORT_ORDER_LABELS {
        let radio = gtk::CheckButton::builder().label(label).build();
        if let Some(prev) = &prev_radio {
            radio.set_group(Some(prev));
        }
        if label == ui_state.sort_order {
            radio.set_active(true);
        }
        sort_box.append(&radio);
        prev_radio = Some(radio.clone());
        sort_radios.push(radio);
    }
    let sort_popover = gtk::Popover::builder().child(&sort_box).build();
    let sort_menu_btn = gtk::MenuButton::builder()
        .label("Sort: Date")
        .always_show_arrow(true)
        .tooltip_text("Sort")
        .popover(&sort_popover)
        .valign(gtk::Align::Center)
        .sensitive(false)
        .build();

    // Thumbnail-size popover: exactly five options with Ctrl+1..5 hints.
    let thumb_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .margin_top(8)
        .margin_bottom(8)
        .margin_start(8)
        .margin_end(8)
        .build();
    thumb_box.append(
        &gtk::Label::builder()
            .label("Thumbnail size")
            .css_classes(["heading"])
            .halign(gtk::Align::Start)
            .margin_bottom(4)
            .build(),
    );
    let active_size = (ui_state.zoom_level.round() as i32).clamp(0, 4) as usize;
    let mut thumb_size_buttons = Vec::new();
    let mut thumb_size_checks = Vec::new();
    for (i, label) in THUMB_SIZE_LABELS.iter().enumerate() {
        let row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .build();
        let check = gtk::Image::builder()
            .icon_name("object-select-symbolic")
            .build();
        check.set_opacity(if i == active_size { 1.0 } else { 0.0 });
        let name = gtk::Label::builder()
            .label(*label)
            .halign(gtk::Align::Start)
            .hexpand(true)
            .build();
        let shortcut = gtk::Label::builder()
            .label(format!("Ctrl+{}", i + 1))
            .css_classes(["dim-label", "caption", "numeric"])
            .build();
        row.append(&check);
        row.append(&name);
        row.append(&shortcut);
        let button = gtk::Button::builder()
            .css_classes(["flat"])
            .child(&row)
            .build();
        button.update_property(&[gtk::accessible::Property::Label(label)]);
        thumb_box.append(&button);
        thumb_size_buttons.push(button);
        thumb_size_checks.push(check);
    }
    let thumb_popover = gtk::Popover::builder().child(&thumb_box).build();
    let thumb_size_btn = gtk::MenuButton::builder()
        .icon_name("view-grid-symbolic")
        .tooltip_text("Thumbnail size")
        .popover(&thumb_popover)
        .valign(gtk::Align::Center)
        .sensitive(false)
        .build();
    thumb_size_btn.update_property(&[gtk::accessible::Property::Label("Thumbnail size")]);

    // Selection toggle.
    let select_button = gtk::ToggleButton::builder()
        .icon_name("selection-mode-symbolic")
        .css_classes(["flat"])
        .tooltip_text("Select")
        .valign(gtk::Align::Center)
        .sensitive(false)
        .build();
    select_button.update_property(&[gtk::accessible::Property::Label("Select items")]);

    // Primary menu: Settings / Keyboard Shortcuts / About Vesper.
    let menu_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(6)
        .margin_end(6)
        .build();
    let primary_settings_btn = menu_row(&menu_box, "Settings");
    let primary_shortcuts_btn = menu_row(&menu_box, "Keyboard Shortcuts");
    let primary_about_btn = menu_row(&menu_box, "About Vesper");
    let primary_popover = gtk::Popover::builder().child(&menu_box).build();
    let primary_menu_button = gtk::MenuButton::builder()
        .icon_name("view-more-symbolic")
        .tooltip_text("Main menu")
        .popover(&primary_popover)
        .valign(gtk::Align::Center)
        .build();
    primary_menu_button.update_property(&[gtk::accessible::Property::Label("Main menu")]);
    // Close the popover when an item is chosen.
    for btn in [
        &primary_settings_btn,
        &primary_shortcuts_btn,
        &primary_about_btn,
    ] {
        let popover = primary_popover.clone();
        btn.connect_clicked(move |_| popover.popdown());
    }

    // pack_end reverses order, so pack right-to-left to read L→R as:
    // clear-filters · Sort · thumbnail-size · select · primary-menu.
    header_bar.pack_end(&primary_menu_button);
    header_bar.pack_end(&select_button);
    header_bar.pack_end(&thumb_size_btn);
    header_bar.pack_end(&sort_menu_btn);
    header_bar.pack_end(&clear_filters_button);

    HeaderWidgets {
        header_bar,
        sidebar_toggle,
        search_entry,
        clear_filters_button,
        sort_menu_btn,
        sort_radios,
        thumb_size_btn,
        thumb_size_buttons,
        thumb_size_checks,
        select_button,
        primary_settings_btn,
        primary_shortcuts_btn,
        primary_about_btn,
    }
}

fn menu_row(parent: &gtk::Box, label: &str) -> gtk::Button {
    let button = gtk::Button::builder()
        .css_classes(["flat"])
        .child(
            &gtk::Label::builder()
                .label(label)
                .halign(gtk::Align::Start)
                .hexpand(true)
                .build(),
        )
        .build();
    button.update_property(&[gtk::accessible::Property::Label(label)]);
    parent.append(&button);
    button
}

/// Enables or disables the media-dependent controls (search, sort, thumbnail
/// size, selection) until at least one media item exists.
pub fn set_media_controls_available(widgets: &HeaderControls, available: bool) {
    widgets.search_entry.set_sensitive(available);
    widgets.sort_menu_btn.set_sensitive(available);
    widgets.thumb_size_btn.set_sensitive(available);
    widgets.select_button.set_sensitive(available);
}

/// The subset of header widgets toggled by [`set_media_controls_available`].
pub struct HeaderControls {
    pub search_entry: gtk::SearchEntry,
    pub sort_menu_btn: gtk::MenuButton,
    pub thumb_size_btn: gtk::MenuButton,
    pub select_button: gtk::ToggleButton,
}
