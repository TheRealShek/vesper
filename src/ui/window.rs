use libadwaita as adw;
use libadwaita::prelude::*;
use libadwaita::gtk::{self, glib};
use std::sync::{Arc, Mutex};
use std::cell::RefCell;
use std::rc::Rc;

pub enum UiEvent {
    ThumbnailReady(i64, String, Option<i64>),
    ScanCompleted(usize, Vec<String>),
    DataFetched {
        tags: Vec<crate::events::UiTag>,
        media: Vec<crate::events::UiMediaItem>,
        roots: Vec<crate::events::UiSourceRoot>,
        has_roots: bool,
    },
    RootsOffline(usize),
    ShowError(String),
    FatalError(String),
    ViewerClosed(u32),
}

pub fn build(
    app: &adw::Application,
    app_tx: tokio::sync::mpsc::UnboundedSender<crate::events::AppEvent>,
    ui_tx: tokio::sync::mpsc::UnboundedSender<UiEvent>,
    mut ui_rx: tokio::sync::mpsc::UnboundedReceiver<UiEvent>,
    thumb_tx: tokio::sync::mpsc::UnboundedSender<crate::thumbnail::ThumbnailRequest>,
    app_state: Arc<Mutex<crate::state::AppState>>,
) {
    // Load CSS
    let provider = gtk::CssProvider::new();
    provider.load_from_string(include_str!("style.css"));
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    // Shared state
    let ui_state = std::rc::Rc::new(std::cell::RefCell::new(
        app_state.lock().map(|s| s.ui.clone()).unwrap_or_default()
    ));
    let selected_tags = Rc::new(RefCell::new(Vec::<String>::new()));
    let match_all = Rc::new(RefCell::new(false));
    let search_query = Rc::new(RefCell::new(String::new()));
    let has_roots_state = Rc::new(RefCell::new(false));
    let source_roots_state: Rc<RefCell<Vec<(i64, String)>>> = Rc::new(RefCell::new(Vec::new()));
    let settings_refresh_cb: Rc<RefCell<Option<Rc<dyn Fn()>>>> = Rc::new(RefCell::new(None));

    // UI Elements
    let root_stack = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::Crossfade)
        .build();

    let initial_sidebar_width = ui_state.borrow().sidebar_width;
    let last_sidebar_width = Rc::new(std::cell::Cell::new(initial_sidebar_width));
    
    let split_view = gtk::Paned::builder()
        .orientation(gtk::Orientation::Horizontal)
        .position(initial_sidebar_width)
        .wide_handle(true)
        .build();
        
    let ui_state_for_paned = ui_state.clone();
    let paned_debounce_id: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    let last_w_notify = last_sidebar_width.clone();
    
    split_view.connect_position_notify(move |p| {
        let pos = p.position();
        
        ui_state_for_paned.borrow_mut().sidebar_collapsed = pos == 0;
        
        if pos > 0 {
            last_w_notify.set(pos);
            
            let mut debounce_id = paned_debounce_id.borrow_mut();
            if let Some(id) = debounce_id.take() {
                id.remove();
            }
            let ui_state_clone = ui_state_for_paned.clone();
            let debounce_id_clone = paned_debounce_id.clone();
            *debounce_id = Some(glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
                let mut state = ui_state_clone.borrow_mut();
                if pos > 0 {
                    state.sidebar_width = pos;
                }
                *debounce_id_clone.borrow_mut() = None;
                glib::ControlFlow::Break
            }));
        }
    });

    let stack = gtk::Stack::new();

    // 1. Sidebar setup
    let sidebar_widgets = crate::ui::sidebar::build(&ui_state.borrow(), match_all.clone());
    let sidebar_toolbar = sidebar_widgets.toolbar;
    let tag_list_box = sidebar_widgets.tag_list_box;
    let tag_names = sidebar_widgets.tag_names;
    let match_switch = sidebar_widgets.match_switch;
    let match_mode_box = sidebar_widgets.match_mode_box;
    let no_tags_label = sidebar_widgets.no_tags_label;
    let roots_list_box = sidebar_widgets.roots_list_box;
    let update_tag_visibility = sidebar_widgets.update_tag_visibility;

    split_view.set_start_child(Some(&sidebar_toolbar));

    // 2. Main Content Top Bar
    let header_widgets = crate::ui::header::build(&ui_state.borrow(), &split_view, &last_sidebar_width);
    let content_toolbar = header_widgets.toolbar;

    let toggle_sidebar_btn = header_widgets.toggle_sidebar_btn;
    let search_entry = header_widgets.search_entry;
    let sort_dropdown = header_widgets.sort_dropdown;
    let zoom_slider = header_widgets.zoom_slider;
    let zoom_box = header_widgets.zoom_box;
    let filter_indicator = header_widgets.filter_indicator;
    let clear_all_filters_btn = header_widgets.clear_all_filters_btn;
    let settings_btn = header_widgets.settings_btn;
    let offline_banner = header_widgets.offline_banner;
    let scan_error_button = header_widgets.scan_error_button;
    let scan_error_paths = header_widgets.scan_error_paths;


    let backend_state_settings = match app_state.lock() {
        Ok(s) => s.backend.clone(),
        Err(_) => return,
    };
    let app_tx_settings = app_tx.clone();
    let source_roots_settings = source_roots_state.clone();
    let settings_refresh_cb_settings = settings_refresh_cb.clone();
    settings_btn.connect_clicked(move |btn| {
        if let Some(parent) = btn.root().and_downcast::<gtk::Window>() {
            crate::ui::settings::show(
                &parent,
                backend_state_settings.clone(),
                app_tx_settings.clone(),
                source_roots_settings.clone(),
                settings_refresh_cb_settings.clone(),
            );
        }
    });
    
    let list_store = gtk::gio::ListStore::new::<crate::ui::model::MediaItem>();
    
    // Initial fetch offloaded to background
    let _ = app_tx.send(crate::events::AppEvent::FetchData);

    // Handle thumbnail ready events
    let ui_state_ui = ui_state.clone();
    let grid_view_ref: Rc<RefCell<Option<gtk::GridView>>> = Rc::new(RefCell::new(None));
    let vadj_ref: Rc<RefCell<Option<gtk::Adjustment>>> = Rc::new(RefCell::new(None));
    let grid_view_ref_ui = grid_view_ref.clone();
    let vadj_ref_ui = vadj_ref.clone();
    let list_store_clone = list_store.clone();
    let thumb_tx_ui = thumb_tx.clone();
    let app_tx_loop = app_tx.clone();
    let tag_names_ui = tag_names.clone();
    let tag_list_box_ui = tag_list_box.clone();
    let has_roots_state_ui = has_roots_state.clone();
    let source_roots_state_ui = source_roots_state.clone();
    let settings_refresh_cb_ui = settings_refresh_cb.clone();
    let stack_ui = stack.clone();
    let match_mode_box_ui = match_mode_box.clone();
    let no_tags_label_ui = no_tags_label.clone();
    let toggle_sidebar_btn_ui = toggle_sidebar_btn.clone();
    let sort_dropdown_ui = sort_dropdown.clone();
    let update_tag_visibility_ui = update_tag_visibility.clone();
    let zoom_box_ui = zoom_box.clone();
    let sidebar_toolbar_ui = sidebar_toolbar.clone();
    let root_stack_ui = root_stack.clone();
    let roots_list_box_ui = roots_list_box.clone();
    let offline_banner_ui = offline_banner.clone();
    let scan_error_button_ui = scan_error_button.clone();
    let scan_error_paths_ui = scan_error_paths.clone();
    let app_for_fatal = app.clone();

    let mut is_first_fetch = true;
    glib::MainContext::default().spawn_local(async move {
        while let Some(event) = ui_rx.recv().await {
            match event {
                UiEvent::ThumbnailReady(media_id, thumb_path, duration) => {
                    let n = list_store_clone.n_items();
                    for i in 0..n {
                        if let Some(obj) = list_store_clone.item(i) {
                            if let Some(item) = obj.downcast_ref::<crate::ui::model::MediaItem>() {
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
                }
                UiEvent::ScanCompleted(count, paths) => {
                    if count > 0 {
                        scan_error_button_ui.set_label(&format!("{} file(s) could not be read.", count));
                        scan_error_button_ui.set_visible(true);
                        *scan_error_paths_ui.borrow_mut() = paths;
                    } else {
                        scan_error_button_ui.set_visible(false);
                        scan_error_paths_ui.borrow_mut().clear();
                    }
                    let _ = app_tx_loop.send(crate::events::AppEvent::FetchData);
                }
                UiEvent::DataFetched { tags, media, roots, has_roots } => {
                    *has_roots_state_ui.borrow_mut() = has_roots;
                    
                    let mut roots_for_state = Vec::new();
                    
                    while let Some(child) = roots_list_box_ui.first_child() {
                        roots_list_box_ui.remove(&child);
                    }
                    for root in roots {
                        roots_for_state.push((root.id, root.path.clone()));
                        let label = gtk::Label::builder()
                            .label(&root.name)
                            .halign(gtk::Align::Start)
                            .build();
                        if !root.is_available {
                            label.add_css_class("dim-label");
                            label.set_text(&format!("{} (Offline)", root.name));
                        }
                        roots_list_box_ui.append(&label);
                    }
                    *source_roots_state_ui.borrow_mut() = roots_for_state;
                    if let Some(cb) = settings_refresh_cb_ui.borrow().as_ref() {
                        cb();
                    }
                    
                    // Update tags
                    while let Some(child) = tag_list_box_ui.first_child() {
                        tag_list_box_ui.remove(&child);
                    }
                    let mut new_names = Vec::new();
                    let mut sorted_tags = tags.clone();
                    sorted_tags.sort_by(|a, b| b.file_count.cmp(&a.file_count));
                    for tag in &sorted_tags {
                        new_names.push(tag.name.clone());
                        let label_text = format!("{} ({})", tag.name, tag.file_count);
                        let label = gtk::Label::builder()
                            .label(&label_text)
                            .xalign(0.0)
                            .margin_start(16)
                            .margin_end(12)
                            .margin_top(8)
                            .margin_bottom(8)
                            .build();
                        let row = gtk::ListBoxRow::builder()
                            .child(&label)
                            .css_classes(["tag-chip"])
                            .build();
                        tag_list_box_ui.append(&row);
                    }
                    *tag_names_ui.borrow_mut() = new_names;
                    update_tag_visibility_ui();
                    
                    if is_first_fetch {
                        is_first_fetch = false;
                        let active_tags = ui_state_ui.borrow().active_tags.clone();
                        let scroll_pos = ui_state_ui.borrow().scroll_position;
                        
                        if !active_tags.is_empty() {
                            for (i, tag) in tags.iter().enumerate() {
                                if active_tags.contains(&tag.name) {
                                    if let Some(row) = tag_list_box_ui.row_at_index(i as i32) {
                                        row.add_css_class("active");
                                    }
                                }
                            }
                        }
                        
                        if scroll_pos > 0 {
                            if let (Some(grid), Some(vadj)) = (grid_view_ref_ui.borrow().as_ref(), vadj_ref_ui.borrow().as_ref()) {
                                let grid_clone = grid.clone();
                                let vadj_clone = vadj.clone();
                                let ui_state_clone = ui_state_ui.clone();
                                glib::idle_add_local_once(move || {
                                    let zoom = ui_state_clone.borrow().zoom_level.round() as i32;
                                    let width = match zoom {
                                        0 => 100, 1 => 140, 2 => 180, 3 => 240, 4 => 320, _ => 180,
                                    };
                                    let mut grid_w = grid_clone.width();
                                    if grid_w <= 0 {
                                        let window_w = ui_state_clone.borrow().window_width;
                                        grid_w = std::cmp::max(100, window_w - 250);
                                    }
                                    let columns = std::cmp::max(1, grid_w / width);
                                    let row = scroll_pos as i32 / columns;
                                    vadj_clone.set_value((row * width) as f64);
                                });
                            }
                        }
                    }
                    
                    // Update visibility
                    if has_roots {
                        root_stack_ui.set_visible_child_name("main");
                    } else {
                        root_stack_ui.set_visible_child_name("empty");
                    }
                    
                    sidebar_toolbar_ui.set_visible(has_roots);
                    toggle_sidebar_btn_ui.set_visible(has_roots);
                    sort_dropdown_ui.set_visible(has_roots);
                    zoom_box_ui.set_visible(has_roots);
                    
                    let is_empty = tags.is_empty();
                    match_mode_box_ui.set_visible(!is_empty && has_roots);
                    no_tags_label_ui.set_visible(is_empty);
                    
                    // Sidebar visibility is handled by AppState and toggle button.
                    // Update media
                    list_store_clone.remove_all();
                    for item_data in media {
                        let item = crate::ui::model::MediaItem::new(
                            item_data.id, 
                            &item_data.path,
                            &item_data.filename,
                            &item_data.tags,
                            &item_data.thumbnail_path,
                            item_data.duration_secs,
                            matches!(item_data.media_type, crate::events::MediaType::Video),
                            item_data.size_bytes,
                            item_data.created_at,
                            item_data.modified_at,
                            item_data.is_offline
                        );
                        list_store_clone.append(&item);
                        
                        if item_data.thumbnail_path.is_empty() {
                            let _ = thumb_tx_ui.send(crate::thumbnail::ThumbnailRequest {
                                media_id: item_data.id,
                                path: std::path::PathBuf::from(&item_data.path),
                                media_type: item_data.media_type,
                            });
                        }
                    }
                    
                    // Update stack visibility
                    if !has_roots {
                        root_stack_ui.set_visible_child_name("empty");
                    } else {
                        root_stack_ui.set_visible_child_name("main");
                        if list_store_clone.n_items() == 0 {
                            stack_ui.set_visible_child_name("no-results");
                        } else {
                            stack_ui.set_visible_child_name("grid");
                        }
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
                UiEvent::RootsOffline(count) => {
                    if count > 0 {
                        offline_banner_ui.set_title(&format!("{} source root(s) offline.", count));
                        offline_banner_ui.set_revealed(true);
                    } else {
                        offline_banner_ui.set_revealed(false);
                    }
                }
                UiEvent::ShowError(msg) => {
                    let dialog = adw::MessageDialog::builder()
                        .heading("Error")
                        .body(&msg)
                        .build();
                    dialog.add_response("ok", "OK");
                    if let Some(win) = app_for_fatal.active_window() {
                        dialog.set_transient_for(Some(&win));
                    }
                    dialog.present();
                }
                UiEvent::ViewerClosed(index) => {
                    if let Some(grid) = grid_view_ref_ui.borrow().as_ref() {
                        let grid_clone = grid.clone();
                        glib::idle_add_local_once(move || {
                            grid_clone.scroll_to(index, gtk::ListScrollFlags::NONE, None);
                            grid_clone.grab_focus();
                        });
                    }
                }
            }
        }
    });

    let filter = crate::ui::filter_sort::create_filter(selected_tags.clone(), match_all.clone(), search_query.clone());
    let filter_model = gtk::FilterListModel::new(Some(list_store.clone()), Some(filter.clone()));
    
    let active_sort_idx = Rc::new(RefCell::new(sort_dropdown.selected()));
    let sorter = crate::ui::filter_sort::create_sorter(active_sort_idx.clone(), search_query.clone());
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

    let viewer_ref: Rc<RefCell<Option<Rc<crate::ui::viewer::Viewer>>>> = Rc::new(RefCell::new(None));
    let factory = crate::ui::grid_cell::create_factory(viewer_ref.clone());
    let grid_view = gtk::GridView::builder()
        .model(&selection_model)
        .factory(&factory)
        .max_columns(30)
        .min_columns(1)
        .enable_rubberband(true)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
        

        
    *grid_view_ref.borrow_mut() = Some(grid_view.clone());

    let grid_provider = gtk::CssProvider::new();
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &grid_provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    zoom_slider.connect_value_changed({
        let grid_provider = grid_provider.clone();
        let ui_state = ui_state.clone();
        move |scale| {
            ui_state.borrow_mut().zoom_level = scale.value();
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
            grid_provider.load_from_string(&css);
        }
    });
    
    zoom_slider.emit_by_name::<()>("value-changed", &[]);

    let scrolled_grid = gtk::ScrolledWindow::builder()
        .child(&grid_view)
        .vexpand(true)
        .hexpand(true)
        .build();
        
    let vadj = scrolled_grid.vadjustment();
    *vadj_ref.borrow_mut() = Some(vadj.clone());
    let ui_state_scroll = ui_state.clone();
    let grid_view_scroll = grid_view.clone();
    
    let scroll_timeout_id: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    
    vadj.connect_value_changed(move |adj| {
        let val = adj.value();
        let ui_state = ui_state_scroll.clone();
        let grid = grid_view_scroll.clone();
        
        if let Some(id) = scroll_timeout_id.borrow_mut().take() {
            id.remove();
        }
        
        let scroll_timeout_id_clone = scroll_timeout_id.clone();
        let new_id = glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
            let zoom = ui_state.borrow().zoom_level.round() as i32;
            let width = match zoom {
                0 => 100,
                1 => 140,
                2 => 180,
                3 => 240,
                4 => 320,
                _ => 180,
            };
            let columns = std::cmp::max(1, grid.width() / width);
            let row = (val / width as f64) as i32;
            let index = (row * columns) as u32;
            
            if ui_state.borrow().scroll_position != index {
                ui_state.borrow_mut().scroll_position = index;
            }
            *scroll_timeout_id_clone.borrow_mut() = None;
            glib::ControlFlow::Break
        });
        
        *scroll_timeout_id.borrow_mut() = Some(new_id);
    });
        
    let grid_overlay = gtk::Overlay::new();
    grid_overlay.set_child(Some(&scrolled_grid));
    
    stack.add_named(&grid_overlay, Some("grid"));
    
    grid_view.set_single_click_activate(false);


    // 4. Empty states
    let add_dir_btn = gtk::Button::builder()
        .label("Add Source Directory")
        .halign(gtk::Align::Center)
        .css_classes(["suggested-action", "desktop-button"])
        .width_request(200)
        .margin_top(16)
        .build();
        
    let no_roots_page = adw::StatusPage::builder()
        .icon_name("folder-open-symbolic")
        .title("No Media Yet")
        .description("Add a source directory to get started.")
        .build();
    no_roots_page.set_child(Some(&add_dir_btn));

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
    let backend_state_e = match app_state.lock() {
        Ok(s) => s.backend.clone(),
        Err(_) => return,
    };
    let app_tx_e = app_tx.clone();
    let source_roots_e = source_roots_state.clone();
    let settings_refresh_cb_e = settings_refresh_cb.clone();
    empty_settings_btn.connect_clicked(move |btn| {
        if let Some(parent) = btn.root().and_downcast::<gtk::Window>() {
            crate::ui::settings::show(&parent, backend_state_e.clone(), app_tx_e.clone(), source_roots_e.clone(), settings_refresh_cb_e.clone());
        }
    });
    empty_header.pack_end(&empty_settings_btn);
    empty_state_view.add_top_bar(&empty_header);
    empty_state_view.set_content(Some(&no_roots_page));
    
    root_stack.add_named(&empty_state_view, Some("empty"));
    root_stack.add_named(&split_view, Some("main"));
    
    // Wire up Add Source Directory

    let app_tx_add = app_tx.clone();
    add_dir_btn.connect_clicked({
        let app_tx_c = app_tx_add.clone();
        
        move |btn| {
            let dialog = gtk::FileDialog::new();
            let app_tx_inner = app_tx_c.clone();
            let parent_win = btn.root().and_downcast::<gtk::Window>();
            
            dialog.select_folder(parent_win.as_ref(), None::<&libadwaita::gtk::gio::Cancellable>, move |res| {
                if let Ok(file) = res {
                    if let Some(path) = file.path() {
                        let path_str = match path.to_str() {
                            Some(s) => s.to_string(),
                            None => return,
                        };
                        let _ = app_tx_inner.send(crate::events::AppEvent::AddSourceRoot(path_str));
                    }
                }
            });
        }
    });

    let no_results_page = adw::StatusPage::builder()
        .title("No Results")
        .description("Try a different search or tag combination.")
        .icon_name("edit-find-symbolic")
        .build();
    let no_res_clear_btn = gtk::Button::builder().label("Clear All Filters").halign(gtk::Align::Center).build();
    no_results_page.set_child(Some(&no_res_clear_btn));
    stack.add_named(&no_results_page, Some("no-results"));

    
    let main_overlay = gtk::Overlay::builder().build();
    main_overlay.set_child(Some(&stack));

    let viewer = crate::ui::viewer::Viewer::new(filter_model.clone(), selection_model.clone(), scrolled_grid.clone(), ui_tx.clone());
    *viewer_ref.borrow_mut() = Some(viewer.clone());
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
    main_overlay.add_overlay(&scan_error_button);

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
    let search_entry_for_key = search_entry.clone();
    key_ctrl.connect_key_pressed(move |_, keyval, _, state| {
        if keyval == gtk::gdk::Key::Escape {
            if !search_entry_for_key.text().is_empty() {
                search_entry_for_key.set_text("");
                return glib::Propagation::Stop;
            }
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
    content_toolbar.set_width_request(550);
    content_toolbar.set_hexpand(true);
    split_view.set_end_child(Some(&content_toolbar));

    // 5. Connecting logic
    
    // Function to update UI based on filter state
    let update_filter_ui = {
        let filter_indicator = filter_indicator.clone();
        let clear_all_filters_btn = clear_all_filters_btn.clone();
        let selected_tags = selected_tags.clone();
        let search_query = search_query.clone();
        
        move || {
            let tags_count = selected_tags.borrow().len();
            let query = search_query.borrow();
            let query_len = query.len();
            
            let has_filters = tags_count > 0 || query_len > 0;
            clear_all_filters_btn.set_visible(has_filters);
            
            if tags_count > 0 {
                filter_indicator.set_text(&format!("{}", tags_count));
                filter_indicator.set_visible(true);
            } else {
                filter_indicator.set_visible(false);
            }
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

    tag_list_box.connect_row_activated({
        let selected_tags = selected_tags.clone();
        let filter = filter.clone();
        let tag_names = tag_names.clone();
        let update_filter_ui = update_filter_ui.clone();
        move |_list_box, row| {
            if row.has_css_class("active") {
                row.remove_css_class("active");
            } else {
                row.add_css_class("active");
            }
            
            let mut new_selection = selected_tags.borrow().clone();
            let index = row.index() as usize;
            if let Some(name) = tag_names.borrow().get(index) {
                if row.has_css_class("active") {
                    if !new_selection.contains(name) {
                        new_selection.push(name.clone());
                    }
                } else {
                    new_selection.retain(|t| t != name);
                }
            }
            
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
        let selected_tags_for_clear = selected_tags.clone();
        let filter_for_clear = filter.clone();
        let update_filter_ui_for_clear = update_filter_ui.clone();
        
        move || {
            let mut i = 0;
            while let Some(row) = tag_list_box.row_at_index(i) {
                row.remove_css_class("active");
                i += 1;
            }
            search_entry.set_text("");
            selected_tags_for_clear.borrow_mut().clear();
            filter_for_clear.changed(gtk::FilterChange::Different);
            update_filter_ui_for_clear();
        }
    };
    
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
    let (w, h, max, sidebar_collapsed) = { let s = ui_state.borrow(); (s.window_width, s.window_height, s.window_maximized, s.sidebar_collapsed) };
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Vesper")
        .default_width(w)
        .default_height(h)
        .maximized(max)
        .content(&root_stack)
        .build();
        
    let scan_error_paths_for_btn = scan_error_paths.clone();
    let window_for_dialog = window.clone();
    scan_error_button.connect_clicked(move |_| {
        let paths = scan_error_paths_for_btn.borrow();
        let mut display_paths = paths.clone();
        if display_paths.len() > 20 {
            display_paths.truncate(20);
            display_paths.push(format!("...and {} more", paths.len() - 20));
        }
        let dialog = adw::MessageDialog::builder()
            .heading("Files Could Not Be Read")
            .body(display_paths.join("\n"))
            .transient_for(&window_for_dialog)
            .build();
        dialog.add_response("close", "Close");
        dialog.present();
    });
        
    let initial_has_roots = *has_roots_state.borrow();
    if initial_has_roots {
        root_stack.set_visible_child_name("main");
    } else {
        root_stack.set_visible_child_name("empty");
    }
    
    let initial_sidebar_collapsed = sidebar_collapsed;
    if initial_sidebar_collapsed {
        split_view.set_position(0);
        toggle_sidebar_btn.set_active(false);
    } else {
        toggle_sidebar_btn.set_active(true);
    }

    let app_state_close = app_state.clone();

    let zoom_slider_close = zoom_slider.clone();
    let sort_dropdown_close = sort_dropdown.clone();
    let match_switch_close = match_switch.clone();
    let split_view_close = split_view.clone();
    let tag_list_box_close = tag_list_box.clone();
    let tag_names_close = tag_names.clone();
    let ui_state_close = ui_state.clone();
    
    window.connect_close_request(move |win| {
        if let Ok(mut state) = app_state_close.lock() {
            state.ui.window_width = win.width();
            state.ui.window_height = win.height();
            state.ui.window_maximized = win.is_maximized();
            state.ui.zoom_level = zoom_slider_close.value();
            state.ui.sidebar_collapsed = split_view_close.position() == 0;
            state.ui.sidebar_width = ui_state_close.borrow().sidebar_width;
            state.ui.scroll_position = ui_state_close.borrow().scroll_position;
            state.ui.tag_filter_mode = if match_switch_close.is_active() { "AND".to_string() } else { "OR".to_string() };
            
            if let Some(selected_item) = sort_dropdown_close.selected_item() {
                if let Ok(str_obj) = selected_item.downcast::<gtk::StringObject>() {
                    state.ui.sort_order = str_obj.string().to_string();
                }
            }
            
            let tag_names_guard = tag_names_close.borrow();
            let mut active_tags = Vec::new();
            let selected_rows = tag_list_box_close.selected_rows();
            for row in selected_rows {
                if let Some(list_box_row) = row.downcast_ref::<gtk::ListBoxRow>() {
                    let idx = list_box_row.index() as usize;
                    if idx < tag_names_guard.len() {
                        active_tags.push(tag_names_guard[idx].clone());
                    }
                }
            }
            state.ui.active_tags = active_tags;
            
            
            let _ = state.save();
        }
        glib::Propagation::Proceed
    });

    let shortcut_controller = gtk::ShortcutController::new();
    if let Some(trigger) = gtk::ShortcutTrigger::parse_string("<Ctrl>b") {
        let action = gtk::CallbackAction::new({
            let split_view = split_view.clone();
        let last_w_shortcut = last_sidebar_width.clone();
        let toggle_sidebar_btn = toggle_sidebar_btn.clone();
        move |_, _| {
            let pos = split_view.position();
            if pos > 0 {
                last_w_shortcut.set(pos);
                split_view.set_position(0);
                toggle_sidebar_btn.set_active(false);
            } else {
                split_view.set_position(last_w_shortcut.get());
                toggle_sidebar_btn.set_active(true);
            }
            glib::Propagation::Stop
        }
    });
        let shortcut = gtk::Shortcut::new(Some(trigger), Some(action));
        shortcut_controller.add_shortcut(shortcut);
    }
    window.add_controller(shortcut_controller);
    
    let key_controller = gtk::EventControllerKey::new();
    let viewer_clone = viewer.clone();
    key_controller.connect_key_pressed(move |_, keyval, _, _| {
        if viewer_clone.is_open() {
            if keyval == gtk::gdk::Key::Escape {
                viewer_clone.close();
                return glib::Propagation::Stop;
            }
            
            if !viewer_clone.video_controls_have_focus() {
                match keyval {
                    gtk::gdk::Key::Left => { viewer_clone.prev(); return glib::Propagation::Stop; }
                    gtk::gdk::Key::Right => { viewer_clone.next(); return glib::Propagation::Stop; }
                    _ => {}
                }
            }
        }
        glib::Propagation::Proceed
    });
    window.add_controller(key_controller);

    window.present();
}

