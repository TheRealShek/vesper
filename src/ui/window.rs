use libadwaita as adw;
use libadwaita::prelude::*;
use libadwaita::gtk::{self, prelude::*, glib};
use std::sync::{Arc, Mutex};
use std::cell::RefCell;
use std::rc::Rc;

pub enum UiEvent {
    ThumbnailReady(i64, String, Option<i64>),
    ScanCompleted,
    DataFetched {
        tags: Vec<crate::db::TagWithCount>,
        media: Vec<(crate::db::MediaRow, String)>,
        has_roots: bool,
    },
    FatalError(String),
}

pub fn build(app: &adw::Application, db: Arc<Mutex<crate::db::Database>>, app_state: Arc<Mutex<crate::state::AppState>>) {
    // Load CSS
    let provider = gtk::CssProvider::new();
    provider.load_from_data(include_str!("style.css"));
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().unwrap(),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    // Shared state
    let selected_tags = Rc::new(RefCell::new(Vec::<String>::new()));
    let match_all = Rc::new(RefCell::new(false));
    let search_query = Rc::new(RefCell::new(String::new()));
    let tag_names: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let has_roots_state = Rc::new(RefCell::new(false));

    // UI Elements
    let root_stack = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::Crossfade)
        .build();

    let split_view = adw::OverlaySplitView::builder()
        .min_sidebar_width(200.0)
        .sidebar_width_fraction(0.2)
        .show_sidebar(false)
        .build();

    let stack = gtk::Stack::new();

    // 1. Sidebar setup
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
    tag_overlay.set_child(Some(&tag_list_box));
    tag_overlay.add_overlay(&no_tags_label);

    let tag_search_entry_clone = tag_search_entry.clone();
    tag_list_box.set_filter_func(move |row| {
        let text = tag_search_entry_clone.text().to_lowercase();
        if text.is_empty() { return true; }
        if let Some(lbl) = row.child().and_downcast::<gtk::Label>() {
            return lbl.text().to_lowercase().contains(&text);
        }
        true
    });

    tag_search_entry.connect_search_changed({
        let tag_list_box = tag_list_box.clone();
        move |_| {
            tag_list_box.invalidate_filter();
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
    let is_and = app_state.lock().unwrap().tag_filter_mode == "AND";
    let match_switch = gtk::Switch::builder().active(is_and).valign(gtk::Align::Center).build();
    *match_all.borrow_mut() = is_and;
    match_mode_box.append(&match_label);
    match_mode_box.append(&match_switch);
    sidebar_box.append(&match_mode_box);
    sidebar_toolbar.set_content(Some(&sidebar_box));
    split_view.set_sidebar(Some(&sidebar_toolbar));

    // 2. Main Content Top Bar
    let content_toolbar = adw::ToolbarView::new();
    let content_header = adw::HeaderBar::new();

    let toggle_sidebar_btn = gtk::ToggleButton::builder()
        .icon_name("sidebar-show-symbolic")
        .tooltip_text("Toggle Sidebar")
        .active(false)
        .visible(false)
        .build();
    let split_view_clone = split_view.clone();
    toggle_sidebar_btn.connect_toggled(move |btn| {
        split_view_clone.set_show_sidebar(btn.is_active());
    });
    split_view.bind_property("show-sidebar", &toggle_sidebar_btn, "active")
        .sync_create()
        .bidirectional()
        .build();
    content_header.pack_start(&toggle_sidebar_btn);

    let filter_indicator = gtk::Label::new(None);
    filter_indicator.add_css_class("dim-label");
    content_header.pack_start(&filter_indicator);
    
    let clear_all_filters_btn = gtk::Button::builder()
        .label("Clear filters")
        .visible(false)
        .build();
    content_header.pack_start(&clear_all_filters_btn);

    let search_entry = gtk::SearchEntry::builder()
        .placeholder_text("Search media...")
        .width_request(250)
        .visible(false)
        .build();
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
    
    let initial_sort = app_state.lock().unwrap().sort_order.clone();
    if let Some(pos) = sort_model_list.iter().position(|&s| s == initial_sort) {
        sort_dropdown.set_selected(pos as u32);
    }

    // Zoom slider
    let initial_zoom = app_state.lock().unwrap().zoom_level;
    let zoom_adj = gtk::Adjustment::new(initial_zoom, 0.0, 4.0, 1.0, 1.0, 0.0);
    let zoom_slider = gtk::Scale::builder()
        .orientation(gtk::Orientation::Horizontal)
        .adjustment(&zoom_adj)
        .draw_value(false)
        .valign(gtk::Align::Center)
        .width_request(120)
        .build();
        
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
        
    content_header.pack_end(&settings_btn);
    content_header.pack_end(&sort_dropdown);
    content_header.pack_end(&zoom_box);

    // Channels for thumbnail pipeline
    let (thumb_tx, thumb_rx) = tokio::sync::mpsc::unbounded_channel::<crate::thumbnail::ThumbnailRequest>();
    let (ui_tx, mut ui_rx) = tokio::sync::mpsc::unbounded_channel::<UiEvent>();

    let app_state_settings = app_state.clone();
    let db_settings = db.clone();
    let ui_tx_settings = ui_tx.clone();
    settings_btn.connect_clicked(move |btn| {
        if let Some(parent) = btn.root().and_downcast::<gtk::Window>() {
            crate::ui::settings::show(
                &parent,
                app_state_settings.clone(),
                db_settings.clone(),
                ui_tx_settings.clone(),
            );
        }
    });
    
    crate::thumbnail::start_thumbnail_worker(db.clone(), thumb_rx, ui_tx.clone());

    let list_store = gtk::gio::ListStore::new::<crate::ui::model::MediaItem>();
    
    // Initial fetch offloaded to background
    let db_fetch = db.clone();
    let ui_tx_fetch = ui_tx.clone();
    tokio::task::spawn_blocking(move || {
        if let Ok(db_g) = db_fetch.lock() {
            let tags = db_g.get_all_tags_with_counts().unwrap_or_default();
            let media = db_g.get_all_media_with_tags().unwrap_or_default();
            let has_roots = !db_g.list_source_roots().unwrap_or_default().is_empty();
            let _ = ui_tx_fetch.send(UiEvent::DataFetched { tags, media, has_roots });
        }
    });

    // Handle thumbnail ready events
    let list_store_clone = list_store.clone();
    let db_clone_ui = db.clone();
    let thumb_tx_ui = thumb_tx.clone();
    let ui_tx_loop = ui_tx.clone();
    let tag_names_ui = tag_names.clone();
    let tag_list_box_ui = tag_list_box.clone();
    let has_roots_state_ui = has_roots_state.clone();
    let stack_ui = stack.clone();
    let match_mode_box_ui = match_mode_box.clone();
    let no_tags_label_ui = no_tags_label.clone();
    let toggle_sidebar_btn_ui = toggle_sidebar_btn.clone();
    let search_entry_ui = search_entry.clone();
    let sort_dropdown_ui = sort_dropdown.clone();
    let zoom_box_ui = zoom_box.clone();
    let sidebar_toolbar_ui = sidebar_toolbar.clone();
    let split_view_ui = split_view.clone();
    let root_stack_ui = root_stack.clone();
    let app_for_fatal = app.clone();

    glib::MainContext::default().spawn_local(async move {
        while let Some(event) = ui_rx.recv().await {
            match event {
                UiEvent::ThumbnailReady(media_id, thumb_path, duration) => {
                    let n = list_store_clone.n_items();
                    for i in 0..n {
                        if let Some(obj) = list_store_clone.item(i) {
                            let item = obj.downcast_ref::<crate::ui::model::MediaItem>().unwrap();
                            let id: i64 = item.property("id");
                            if id == media_id {
                                item.set_property("thumbnail-path", &thumb_path);
                                if let Some(d) = duration {
                                    item.set_property("duration-secs", d);
                                }
                                break;
                            }
                        }
                    }
                }
                UiEvent::ScanCompleted => {
                    let db_fetch = db_clone_ui.clone();
                    let ui_tx_fetch = ui_tx_loop.clone();
                    tokio::task::spawn_blocking(move || {
                        if let Ok(db_g) = db_fetch.lock() {
                            let tags = db_g.get_all_tags_with_counts().unwrap_or_default();
                            let media = db_g.get_all_media_with_tags().unwrap_or_default();
                            let has_roots = !db_g.list_source_roots().unwrap_or_default().is_empty();
                            let _ = ui_tx_fetch.send(UiEvent::DataFetched { tags, media, has_roots });
                        }
                    });
                }
                UiEvent::DataFetched { tags, media, has_roots } => {
                    *has_roots_state_ui.borrow_mut() = has_roots;
                    
                    // Update tags
                    while let Some(child) = tag_list_box_ui.first_child() {
                        tag_list_box_ui.remove(&child);
                    }
                    let mut new_names = Vec::new();
                    for tag in &tags {
                        new_names.push(tag.name.clone());
                        let label_text = format!("{} ({})", tag.name, tag.file_count);
                        let row = gtk::Label::builder()
                            .label(&label_text)
                            .xalign(0.0)
                            .margin_start(32)
                            .margin_end(12)
                            .margin_top(8)
                            .margin_bottom(8)
                            .build();
                        tag_list_box_ui.append(&row);
                    }
                    *tag_names_ui.borrow_mut() = new_names;
                    
                    // Update visibility
                    if has_roots {
                        root_stack_ui.set_visible_child_name("main");
                    } else {
                        root_stack_ui.set_visible_child_name("empty");
                    }
                    
                    sidebar_toolbar_ui.set_visible(has_roots);
                    toggle_sidebar_btn_ui.set_visible(has_roots);
                    search_entry_ui.set_visible(has_roots);
                    sort_dropdown_ui.set_visible(has_roots);
                    zoom_box_ui.set_visible(has_roots);
                    
                    let is_empty = tags.is_empty();
                    match_mode_box_ui.set_visible(!is_empty && has_roots);
                    no_tags_label_ui.set_visible(is_empty);
                    
                    if has_roots {
                        split_view_ui.set_show_sidebar(true);
                    }
                    
                    // Update media
                    list_store_clone.remove_all();
                    for (row, mtags) in media {
                        let item = crate::ui::model::MediaItem::new(
                            row.id, 
                            &row.path,
                            &row.filename,
                            &mtags,
                            row.thumbnail_path.as_deref().unwrap_or(""),
                            row.duration_secs.unwrap_or(-1),
                            matches!(row.media_type, crate::events::MediaType::Video),
                            row.size_bytes,
                            row.created_at,
                            row.modified_at
                        );
                        list_store_clone.append(&item);
                        
                        if row.thumbnail_path.is_none() || row.thumbnail_path.as_ref().unwrap().is_empty() {
                            let _ = thumb_tx_ui.send(crate::thumbnail::ThumbnailRequest {
                                media_id: row.id,
                                path: std::path::PathBuf::from(&row.path),
                                media_type: row.media_type,
                            });
                        }
                    }
                    
                    // Update stack visibility
                    if !has_roots {
                        stack_ui.set_visible_child_name("no-roots");
                    } else if list_store_clone.n_items() == 0 {
                        stack_ui.set_visible_child_name("no-results");
                    } else {
                        stack_ui.set_visible_child_name("grid");
                    }
                }
                UiEvent::FatalError(msg) => {
                    eprintln!("Fatal error: {}", msg);
                    let dialog = adw::MessageDialog::builder()
                        .heading("Unexpected Error")
                        .body("An unexpected error occurred. The application will close.")
                        .build();
                    dialog.add_response("close", "Close");
                    let app_clone = app_for_fatal.clone();
                    dialog.connect_response(None, move |_, _| {
                        if let Some(win) = app_clone.active_window() {
                            win.close();
                        }
                        app_clone.quit();
                        std::process::exit(1);
                    });
                    
                    if let Some(win) = app_for_fatal.active_window() {
                        dialog.set_transient_for(Some(&win));
                        dialog.present();
                    } else {
                        dialog.present();
                    }
                }
            }
        }
    });

    let filter = gtk::CustomFilter::new({
        let selected_tags = selected_tags.clone();
        let match_all = match_all.clone();
        let search_query = search_query.clone();
        move |item| {
            let media_item = item.downcast_ref::<crate::ui::model::MediaItem>().unwrap();
            
            let selected = selected_tags.borrow();
            let item_tags_str: String = media_item.property("tags");
            let item_tags: Vec<&str> = item_tags_str.split(',').collect();

            if !selected.is_empty() {
                if *match_all.borrow() {
                    if !selected.iter().all(|t| item_tags.contains(&t.as_str())) { return false; }
                } else {
                    if !selected.iter().any(|t| item_tags.contains(&t.as_str())) { return false; }
                }
            }

            let query = search_query.borrow();
            if !query.is_empty() {
                let filename: String = media_item.property("filename");
                let path: String = media_item.property("path");
                let q = query.as_str();
                if !filename.to_lowercase().contains(q) &&
                   !path.to_lowercase().contains(q) &&
                   !item_tags.iter().any(|t| t.to_lowercase().contains(q)) {
                    return false;
                }
            }

            true
        }
    });

    let filter_model = gtk::FilterListModel::new(Some(list_store.clone()), Some(filter.clone()));
    
    let active_sort_idx = Rc::new(RefCell::new(sort_dropdown.selected()));
    let sorter = gtk::CustomSorter::new({
        let active_sort_idx = active_sort_idx.clone();
        move |item1, item2| {
            let m1 = item1.downcast_ref::<crate::ui::model::MediaItem>().unwrap();
            let m2 = item2.downcast_ref::<crate::ui::model::MediaItem>().unwrap();
            
            let idx = *active_sort_idx.borrow();
            
            let cmp = match idx {
                0 => { // Date modified (newest first)
                    let t1: i64 = m1.property("modified-at");
                    let t2: i64 = m2.property("modified-at");
                    t1.cmp(&t2).reverse()
                }
                1 => { // Date modified (oldest first)
                    let t1: i64 = m1.property("modified-at");
                    let t2: i64 = m2.property("modified-at");
                    t1.cmp(&t2)
                }
                2 => { // Date created (newest first)
                    let t1: i64 = m1.property("created-at");
                    let t2: i64 = m2.property("created-at");
                    t1.cmp(&t2).reverse()
                }
                3 => { // Date created (oldest first)
                    let t1: i64 = m1.property("created-at");
                    let t2: i64 = m2.property("created-at");
                    t1.cmp(&t2)
                }
                4 => { // Filename (A → Z)
                    let f1: String = m1.property("filename");
                    let f2: String = m2.property("filename");
                    f1.to_lowercase().cmp(&f2.to_lowercase())
                }
                5 => { // Filename (Z → A)
                    let f1: String = m1.property("filename");
                    let f2: String = m2.property("filename");
                    f1.to_lowercase().cmp(&f2.to_lowercase()).reverse()
                }
                6 => { // File size (largest first)
                    let s1: i64 = m1.property("size-bytes");
                    let s2: i64 = m2.property("size-bytes");
                    s1.cmp(&s2).reverse()
                }
                7 => { // File size (smallest first)
                    let s1: i64 = m1.property("size-bytes");
                    let s2: i64 = m2.property("size-bytes");
                    s1.cmp(&s2)
                }
                _ => std::cmp::Ordering::Equal,
            };
            
            match cmp {
                std::cmp::Ordering::Less => gtk::Ordering::Smaller,
                std::cmp::Ordering::Greater => gtk::Ordering::Larger,
                std::cmp::Ordering::Equal => gtk::Ordering::Equal,
            }
        }
    });

    let sort_list_model = gtk::SortListModel::new(Some(filter_model.clone()), Some(sorter.clone()));
    let selection_model = gtk::MultiSelection::new(Some(sort_list_model.clone()));
    
    sort_dropdown.connect_selected_notify({
        let sorter = sorter.clone();
        let active_sort_idx = active_sort_idx.clone();
        move |dropdown| {
            *active_sort_idx.borrow_mut() = dropdown.selected();
            sorter.changed(gtk::SorterChange::Different);
        }
    });

    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(move |_factory, list_item| {
        let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
        
        let overlay = gtk::Overlay::builder()
            .css_classes(["card"])
            .hexpand(true)
            .vexpand(true)
            .build();
        
        let picture = gtk::Picture::builder()
            .content_fit(gtk::ContentFit::Cover)
            .hexpand(true)
            .vexpand(true)
            .visible(false)
            .build();
        overlay.set_child(Some(&picture));
        
        let placeholder = gtk::Image::builder()
            .icon_name("image-x-generic-symbolic")
            .pixel_size(48)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .vexpand(true)
            .build();
        overlay.add_overlay(&placeholder);
        
        let checkmark = gtk::Image::builder()
            .icon_name("object-select-symbolic")
            .css_classes(["check-icon"])
            .halign(gtk::Align::Start)
            .valign(gtk::Align::Start)
            .margin_start(8)
            .margin_top(8)
            .build();
        overlay.add_overlay(&checkmark);
        
        let hover_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .css_classes(["hover-overlay"])
            .valign(gtk::Align::End)
            .spacing(4)
            .build();
            
        let type_icon = gtk::Image::builder().icon_name("image-x-generic-symbolic").build();
        let filename_label = gtk::Label::builder()
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .halign(gtk::Align::Start)
            .hexpand(true)
            .build();
        hover_box.append(&type_icon);
        hover_box.append(&filename_label);
        overlay.add_overlay(&hover_box);
        
        let duration_badge = gtk::Label::builder()
            .css_classes(["duration-badge"])
            .halign(gtk::Align::End)
            .valign(gtk::Align::End)
            .margin_end(8)
            .margin_bottom(8)
            .visible(false)
            .build();
        overlay.add_overlay(&duration_badge);
        
        unsafe {
            overlay.set_data("picture", picture);
            overlay.set_data("placeholder", placeholder);
            overlay.set_data("type_icon", type_icon);
            overlay.set_data("filename_label", filename_label);
            overlay.set_data("duration_badge", duration_badge);
        }
        
        let aspect_frame = gtk::AspectFrame::builder()
            .xalign(0.5)
            .yalign(0.5)
            .ratio(1.0)
            .obey_child(false)
            .child(&overlay)
            .build();
            
        list_item.set_child(Some(&aspect_frame));
    });

    factory.connect_bind(move |_factory, list_item| {
        let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
        let media_item = list_item.item().and_downcast::<crate::ui::model::MediaItem>().unwrap();
        let aspect_frame = list_item.child().and_downcast::<gtk::AspectFrame>().unwrap();
        let overlay = aspect_frame.child().and_downcast::<gtk::Overlay>().unwrap();
        
        let picture = unsafe { overlay.steal_data::<gtk::Picture>("picture").unwrap() };
        let placeholder = unsafe { overlay.steal_data::<gtk::Image>("placeholder").unwrap() };
        let type_icon = unsafe { overlay.steal_data::<gtk::Image>("type_icon").unwrap() };
        let filename_label = unsafe { overlay.steal_data::<gtk::Label>("filename_label").unwrap() };
        let duration_badge = unsafe { overlay.steal_data::<gtk::Label>("duration_badge").unwrap() };
        
        let filename: String = media_item.property("filename");
        filename_label.set_text(&filename);
        
        let is_video: bool = media_item.property("is-video");
        let d: i64 = media_item.property("duration-secs");
        if is_video {
            type_icon.set_icon_name(Some("video-x-generic-symbolic"));
            if d >= 0 {
                let secs = d % 60;
                let mins = (d / 60) % 60;
                let hours = d / 3600;
                if hours > 0 {
                    duration_badge.set_text(&format!("{}:{:02}:{:02}", hours, mins, secs));
                } else {
                    duration_badge.set_text(&format!("{}:{:02}", mins, secs));
                }
            } else {
                duration_badge.set_text("");
            }
            duration_badge.set_visible(true);
        } else {
            type_icon.set_icon_name(Some("image-x-generic-symbolic"));
            duration_badge.set_visible(false);
        }
        
        let id2 = media_item.connect_notify_local(Some("duration-secs"), {
            let dbg = duration_badge.clone();
            move |item, _| {
                let d: i64 = item.property("duration-secs");
                if d >= 0 {
                    let secs = d % 60;
                    let mins = (d / 60) % 60;
                    let hours = d / 3600;
                    if hours > 0 {
                        dbg.set_text(&format!("{}:{:02}:{:02}", hours, mins, secs));
                    } else {
                        dbg.set_text(&format!("{}:{:02}", mins, secs));
                    }
                } else {
                    dbg.set_text("");
                }
            }
        });
        
        let id1 = media_item.connect_notify_local(Some("thumbnail-path"), {
            let pic = picture.clone();
            let plc = placeholder.clone();
            move |item, _| {
                let thumb_path: String = item.property("thumbnail-path");
                if thumb_path.is_empty() {
                    pic.set_visible(false);
                    plc.set_visible(true);
                } else {
                    pic.set_filename(Some(&thumb_path));
                    pic.set_visible(true);
                    plc.set_visible(false);
                }
            }
        });
        
        let thumb_path: String = media_item.property("thumbnail-path");
        if thumb_path.is_empty() {
            picture.set_visible(false);
            placeholder.set_visible(true);
        } else {
            picture.set_filename(Some(&thumb_path));
            picture.set_visible(true);
            placeholder.set_visible(false);
        }
        
        unsafe {
            list_item.set_data("sig_id", id1);
            list_item.set_data("sig_duration_id", id2);
            overlay.set_data("picture", picture);
            overlay.set_data("placeholder", placeholder);
            overlay.set_data("type_icon", type_icon);
            overlay.set_data("filename_label", filename_label);
            overlay.set_data("duration_badge", duration_badge);
        }
    });

    factory.connect_unbind(move |_factory, list_item| {
        let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
        if let Some(media_item) = list_item.item().and_downcast::<crate::ui::model::MediaItem>() {
            let sig_id: Option<glib::SignalHandlerId> = unsafe { list_item.steal_data("sig_id") };
            if let Some(id) = sig_id {
                media_item.disconnect(id);
            }
            let sig_duration_id: Option<glib::SignalHandlerId> = unsafe { list_item.steal_data("sig_duration_id") };
            if let Some(id) = sig_duration_id {
                media_item.disconnect(id);
            }
        }
    });

    let grid_view = gtk::GridView::builder()
        .model(&selection_model)
        .factory(&factory)
        .max_columns(30)
        .min_columns(1)
        .enable_rubberband(true)
        .build();

    let grid_provider = gtk::CssProvider::new();
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().unwrap(),
        &grid_provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    zoom_slider.connect_value_changed({
        let grid_provider = grid_provider.clone();
        move |scale| {
            let val = scale.value().round() as i32;
            let width = match val {
                0 => 100,
                1 => 140,
                2 => 180,
                3 => 240,
                4 => 320,
                _ => 180,
            };
            let css = format!("gridview child {{ min-width: {}px; min-height: {}px; }}", width, width);
            grid_provider.load_from_data(&css);
        }
    });
    
    zoom_slider.emit_by_name::<()>("value-changed", &[]);

    let scrolled_grid = gtk::ScrolledWindow::builder()
        .child(&grid_view)
        .vexpand(true)
        .hexpand(true)
        .build();
        
    let grid_overlay = gtk::Overlay::new();
    grid_overlay.set_child(Some(&scrolled_grid));
    
    stack.add_named(&grid_overlay, Some("grid"));
    
    grid_view.set_single_click_activate(true);


    // 4. Empty states
    let empty_state_title = gtk::Label::builder()
        .label("Vesper")
        .css_classes(["title-1"])
        .halign(gtk::Align::Center)
        .build();

    let empty_state_desc = gtk::Label::builder()
        .label("Browse your media by folder")
        .css_classes(["dim-label", "body"])
        .halign(gtk::Align::Center)
        .build();

    let add_dir_btn = gtk::Button::builder()
        .label("Add Source Directory")
        .halign(gtk::Align::Center)
        .css_classes(["suggested-action", "desktop-button"])
        .width_request(200)
        .margin_top(16)
        .build();
        
    let no_roots_page = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .vexpand(true)
        .spacing(8)
        .margin_bottom(80) // Push up by ~40px for optical centering
        .build();
        
    no_roots_page.append(&empty_state_title);
    no_roots_page.append(&empty_state_desc);
    no_roots_page.append(&add_dir_btn);

    let empty_state_view = adw::ToolbarView::new();
    let empty_header = adw::HeaderBar::new();
    
    // Hide the redundant top-bar title in the empty state
    let empty_title_widget = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    empty_header.set_title_widget(Some(&empty_title_widget));
    
    let empty_settings_btn = gtk::Button::builder()
        .icon_name("preferences-system-symbolic")
        .tooltip_text("Settings")
        .css_classes(["flat"])
        .valign(gtk::Align::Center)
        .build();
    let app_state_e = app_state.clone();
    let db_e = db.clone();
    let ui_tx_e = ui_tx.clone();
    empty_settings_btn.connect_clicked(move |btn| {
        if let Some(parent) = btn.root().and_downcast::<gtk::Window>() {
            crate::ui::settings::show(&parent, app_state_e.clone(), db_e.clone(), ui_tx_e.clone());
        }
    });
    empty_header.pack_end(&empty_settings_btn);
    empty_state_view.add_top_bar(&empty_header);
    empty_state_view.set_content(Some(&no_roots_page));
    
    root_stack.add_named(&empty_state_view, Some("empty"));
    root_stack.add_named(&split_view, Some("main"));
    
    // Wire up Add Source Directory
    let app_state_add = app_state.clone();
    add_dir_btn.connect_clicked({
        let db = db.clone();
        let ui_tx = ui_tx.clone();
        let app_state_c = app_state_add.clone();
        
        move |btn| {
            let dialog = gtk::FileDialog::new();
            let db = db.clone();
            let ui_tx = ui_tx.clone();
            let app_state_inner = app_state_c.clone();
            let parent_win = btn.root().and_downcast::<gtk::Window>();
            
            dialog.select_folder(parent_win.as_ref(), None::<&libadwaita::gtk::gio::Cancellable>, move |res| {
                if let Ok(file) = res {
                    if let Some(path) = file.path() {
                        let path_str = match path.to_str() {
                            Some(s) => s.to_string(),
                            None => return,
                        };
                        let db_guard = match db.lock() {
                            Ok(g) => g,
                            Err(_) => {
                                let _ = ui_tx.send(UiEvent::FatalError("Database lock poisoned".to_string()));
                                return;
                            }
                        };
                        let _ = db_guard.add_source_root(&path_str);
                        drop(db_guard);
                        
                        let (root_as_tag, global_rules) = match app_state_inner.lock() {
                            Ok(s) => (s.root_as_tag, s.global_ignore_rules.clone()),
                            Err(_) => (false, vec![]),
                        };
                        
                        let db_clone = db.clone();
                        let ui_tx_clone = ui_tx.clone();
                        tokio::spawn(async move {
                            if let Ok(_) = crate::scan::run_scan(path.to_path_buf(), db_clone, global_rules, root_as_tag).await {
                                let _ = ui_tx_clone.send(UiEvent::ScanCompleted);
                            }
                        });
                    }
                }
            });
        }
    });

    let no_results_page = adw::StatusPage::builder()
        .title("No Media Found")
        .description("No files match the current filters.")
        .icon_name("edit-find-symbolic")
        .build();
    let no_res_clear_btn = gtk::Button::builder().label("Clear All Filters").halign(gtk::Align::Center).build();
    no_results_page.set_child(Some(&no_res_clear_btn));
    stack.add_named(&no_results_page, Some("no-results"));

    content_toolbar.add_top_bar(&content_header);
    
    let main_overlay = gtk::Overlay::builder().build();
    main_overlay.set_child(Some(&stack));

    let viewer = crate::ui::viewer::Viewer::new(filter_model.clone(), selection_model.clone(), scrolled_grid.clone());
    main_overlay.add_overlay(&viewer.dim_bg);
    main_overlay.add_overlay(&viewer.overlay);
    
    // Selection Action Bar
    let action_bar_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .css_classes(["action-bar"])
        .halign(gtk::Align::Center)
        .valign(gtk::Align::End)
        .margin_bottom(24)
        .spacing(12)
        .build();
        
    let sel_count_label = gtk::Label::builder().css_classes(["title-4"]).margin_start(8).margin_end(8).build();
    let open_loc_btn = gtk::Button::builder().label("Open file location").build();
    let copy_path_btn = gtk::Button::builder().label("Copy path(s)").build();
    let deselect_btn = gtk::Button::builder().label("Deselect all").css_classes(["destructive-action"]).build();
    
    action_bar_box.append(&sel_count_label);
    action_bar_box.append(&open_loc_btn);
    action_bar_box.append(&copy_path_btn);
    action_bar_box.append(&deselect_btn);
    
    let action_bar_revealer = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideUp)
        .child(&action_bar_box)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::End)
        .build();
        
    main_overlay.add_overlay(&action_bar_revealer);

    let sel_model_for_deselect = selection_model.clone();
    deselect_btn.connect_clicked(move |_| {
        sel_model_for_deselect.unselect_all();
    });
    
    let sel_model_for_copy = selection_model.clone();
    let filter_model_for_copy = filter_model.clone();
    copy_path_btn.connect_clicked(move |_| {
        let bitset = sel_model_for_copy.selection();
        let mut paths = Vec::new();
        let max = if bitset.is_empty() { 0 } else { bitset.maximum() };
        for i in 0..max + 1 {
            if bitset.contains(i) {
                if let Some(item) = filter_model_for_copy.item(i) {
                    if let Ok(media) = item.downcast::<crate::ui::model::MediaItem>() {
                        paths.push(media.property::<String>("path"));
                    }
                }
            }
        }
        if let Some(display) = gtk::gdk::Display::default() {
            display.clipboard().set_text(&paths.join("\n"));
        }
    });
    
    let sel_model_for_open = selection_model.clone();
    let filter_model_for_open = filter_model.clone();
    open_loc_btn.connect_clicked(move |_| {
        let bitset = sel_model_for_open.selection();
        let mut paths = Vec::new();
        let max = if bitset.is_empty() { 0 } else { bitset.maximum() };
        for i in 0..max + 1 {
            if bitset.contains(i) {
                if let Some(item) = filter_model_for_open.item(i) {
                    if let Ok(media) = item.downcast::<crate::ui::model::MediaItem>() {
                        paths.push(media.property::<String>("path"));
                    }
                }
            }
        }
        if let Some(first_path) = paths.first() {
            if let Some(parent) = std::path::Path::new(first_path).parent() {
                if let Ok(uri) = glib::filename_to_uri(parent, None) {
                    let _ = gtk::gio::AppInfo::launch_default_for_uri(&uri, None::<&gtk::gio::AppLaunchContext>);
                }
            }
        }
    });

    let sel_model_for_change = selection_model.clone();
    selection_model.connect_selection_changed(move |_, _, _| {
        let count = sel_model_for_change.selection().size();
        if count > 0 {
            sel_count_label.set_text(&format!("{} selected", count));
            action_bar_revealer.set_reveal_child(true);
        } else {
            action_bar_revealer.set_reveal_child(false);
        }
    });

    let viewer_for_activate = viewer.clone();
    let sel_model_for_activate = selection_model.clone();
    grid_view.connect_activate(move |_, pos| {
        sel_model_for_activate.unselect_all();
        viewer_for_activate.open(pos);
    });

    let key_ctrl = gtk::EventControllerKey::new();
    let sel_model_for_key = selection_model.clone();
    key_ctrl.connect_key_pressed(move |_, keyval, _, state| {
        if keyval == gtk::gdk::Key::Escape {
            sel_model_for_key.unselect_all();
            return glib::Propagation::Stop;
        }
        if (keyval == gtk::gdk::Key::a || keyval == gtk::gdk::Key::A) && state.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
            sel_model_for_key.select_all();
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    grid_view.add_controller(key_ctrl);

    content_toolbar.set_content(Some(&main_overlay));
    split_view.set_content(Some(&content_toolbar));

    // 5. Connecting logic
    
    // Function to update UI based on filter state
    let update_filter_ui = {
        let filter_indicator = filter_indicator.clone();
        let clear_all_filters_btn = clear_all_filters_btn.clone();
        let clear_tags_btn = clear_tags_btn.clone();
        let selected_tags = selected_tags.clone();
        let search_query = search_query.clone();
        
        move || {
            let tags_count = selected_tags.borrow().len();
            let query = search_query.borrow();
            let query_len = query.len();
            
            clear_tags_btn.set_visible(tags_count > 0);
            
            let has_filters = tags_count > 0 || query_len > 0;
            clear_all_filters_btn.set_visible(has_filters);
            
            let mut parts = Vec::new();
            if tags_count > 0 {
                parts.push(format!("{} tags", tags_count));
            }
            if query_len > 0 {
                parts.push(format!("Search: '{}'", query));
            }
            filter_indicator.set_text(&parts.join(" | "));
        }
    };

    match_switch.connect_active_notify({
        let match_all = match_all.clone();
        let filter = filter.clone();
        move |switch| {
            *match_all.borrow_mut() = switch.is_active();
            filter.changed(gtk::FilterChange::Different);
        }
    });

    tag_list_box.connect_selected_rows_changed({
        let selected_tags = selected_tags.clone();
        let filter = filter.clone();
        let tag_names = tag_names.clone();
        let update_filter_ui = update_filter_ui.clone();
        move |list_box| {
            let mut new_selection = Vec::new();
            let current_names = tag_names.borrow();
            for row in list_box.selected_rows() {
                if let Some(name) = current_names.get(row.index() as usize) {
                    new_selection.push(name.clone());
                }
            }
            // match_mode_box visibility is handled in DataFetched based on tag availability
            *selected_tags.borrow_mut() = new_selection;
            filter.changed(gtk::FilterChange::Different);
            update_filter_ui();
        }
    });

    search_entry.connect_search_changed({
        let search_query = search_query.clone();
        let filter = filter.clone();
        let update_filter_ui = update_filter_ui.clone();
        move |entry| {
            *search_query.borrow_mut() = entry.text().to_string().to_lowercase();
            filter.changed(gtk::FilterChange::Different);
            update_filter_ui();
        }
    });

    // Clear buttons handlers
    let clear_all_action = {
        let tag_list_box = tag_list_box.clone();
        let search_entry = search_entry.clone();
        move || {
            tag_list_box.unselect_all();
            search_entry.set_text("");
        }
    };

    clear_tags_btn.connect_clicked({
        let tag_list_box = tag_list_box.clone();
        move |_| tag_list_box.unselect_all()
    });
    
    clear_all_filters_btn.connect_clicked({
        let clear_all = clear_all_action.clone();
        move |_| clear_all()
    });
    no_res_clear_btn.connect_clicked({
        let clear_all = clear_all_action.clone();
        move |_| clear_all()
    });

    // Stack visibility update based on items
    let stack_for_items_changed = stack.clone();
    let has_roots_for_items = has_roots_state.clone();
    filter_model.connect_items_changed(move |model, _, _, _| {
        let has_roots = *has_roots_for_items.borrow();
        if !has_roots {
            // Handled by root_stack
        } else if model.n_items() == 0 {
            stack_for_items_changed.set_visible_child_name("no-results");
        } else {
            stack_for_items_changed.set_visible_child_name("grid");
        }
    });

    // Initial stack state trigger
    stack.set_visible_child_name("grid");

    // 6. Main window and shortcuts
    let state_guard = app_state.lock().unwrap();
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Vesper")
        .default_width(state_guard.window_width)
        .default_height(state_guard.window_height)
        .maximized(state_guard.window_maximized)
        .content(&root_stack)
        .build();
        
    let initial_has_roots = *has_roots_state.borrow();
    if initial_has_roots {
        root_stack.set_visible_child_name("main");
    } else {
        root_stack.set_visible_child_name("empty");
    }
    
    split_view.set_show_sidebar(!state_guard.sidebar_collapsed);
    toggle_sidebar_btn.set_active(!state_guard.sidebar_collapsed);
    drop(state_guard);

    let app_state_close = app_state.clone();
    let zoom_slider_close = zoom_slider.clone();
    let sort_dropdown_close = sort_dropdown.clone();
    let match_switch_close = match_switch.clone();
    let split_view_close = split_view.clone();
    
    window.connect_close_request(move |win| {
        if let Ok(mut state) = app_state_close.lock() {
            state.window_width = win.width();
            state.window_height = win.height();
            state.window_maximized = win.is_maximized();
            state.zoom_level = zoom_slider_close.value();
            state.sidebar_collapsed = !split_view_close.shows_sidebar();
            state.tag_filter_mode = if match_switch_close.is_active() { "AND".to_string() } else { "OR".to_string() };
            
            if let Some(selected_item) = sort_dropdown_close.selected_item() {
                if let Ok(str_obj) = selected_item.downcast::<gtk::StringObject>() {
                    state.sort_order = str_obj.string().to_string();
                }
            }
            
            let _ = state.save();
        }
        glib::Propagation::Proceed
    });

    let shortcut_controller = gtk::ShortcutController::new();
    let trigger = gtk::ShortcutTrigger::parse_string("<Ctrl>b").unwrap();
    let action = gtk::CallbackAction::new({
        let split_view = split_view.clone();
        move |_, _| {
            split_view.set_show_sidebar(!split_view.shows_sidebar());
            glib::Propagation::Stop
        }
    });
    let shortcut = gtk::Shortcut::new(Some(trigger), Some(action));
    shortcut_controller.add_shortcut(shortcut);
    window.add_controller(shortcut_controller);
    
    let key_controller = gtk::EventControllerKey::new();
    let viewer_clone = viewer.clone();
    key_controller.connect_key_pressed(move |_, keyval, _, _| {
        if viewer_clone.is_open() {
            match keyval {
                gtk::gdk::Key::Escape => { viewer_clone.close(); return glib::Propagation::Stop; }
                gtk::gdk::Key::Left => { viewer_clone.prev(); return glib::Propagation::Stop; }
                gtk::gdk::Key::Right => { viewer_clone.next(); return glib::Propagation::Stop; }
                _ => {}
            }
        }
        glib::Propagation::Proceed
    });
    window.add_controller(key_controller);

    window.present();
}
