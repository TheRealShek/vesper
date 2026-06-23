use crate::events::ChannelSendExt;
use libadwaita as adw;
use libadwaita::gtk::{self, glib};
use libadwaita::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

type RefreshCb = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

pub enum UiEvent {
    ThumbnailReady(i64, String, Option<i64>),
    ScanCompleted(usize, Vec<String>),
    ScanStarted,
    ScanProgress(usize),
    DataFetched {
        tags: Vec<crate::events::UiTag>,
        media: Vec<crate::events::UiMediaItem>,
        roots: Vec<crate::events::UiSourceRoot>,
        has_roots: bool,
    },
    RootsOffline(usize),
    #[allow(dead_code)]
    FatalError(String),
    ViewerClosed(u32),
    MediaAdded(crate::events::UiMediaItem),
    MediaRemoved(String),
    TagsUpdated(Vec<crate::events::UiTag>),
    QueryResult(Vec<crate::events::UiMediaItem>, u32),
}

pub fn build(
    app: &adw::Application,
    app_tx: tokio::sync::mpsc::Sender<crate::events::AppEvent>,
    ui_tx: tokio::sync::mpsc::Sender<UiEvent>,
    mut ui_rx: tokio::sync::mpsc::Receiver<UiEvent>,
    thumb_tx: tokio::sync::mpsc::Sender<crate::thumbnail::ThumbnailRequest>,
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
        app_state.lock().map(|s| s.ui.clone()).unwrap_or_default(),
    ));
    let selected_tags = Rc::new(RefCell::new(Vec::<String>::new()));
    let match_all = Rc::new(RefCell::new(false));
    let search_query = Rc::new(RefCell::new(String::new()));
    let has_roots_state = Rc::new(RefCell::new(false));
    let source_roots_state: Rc<RefCell<Vec<(i64, String)>>> = Rc::new(RefCell::new(Vec::new()));
    let settings_refresh_cb: RefreshCb = Rc::new(RefCell::new(None));
    let grid_refresh_cb: RefreshCb = Rc::new(RefCell::new(None));
    let filter_controller_ref: Rc<RefCell<Option<crate::ui::filter_controller::FilterController>>> =
        Rc::new(RefCell::new(None));

    // UI Elements
    let root_stack = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::Crossfade)
        .css_classes(["grid-area"])
        .vexpand(true)
        .hexpand(true)
        .build();

    let stack = gtk::Stack::new();

    let main_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);

    // 1. Sidebar setup
    let sidebar_widgets = crate::ui::sidebar::build(&ui_state.borrow(), match_all.clone());
    let sidebar_root = sidebar_widgets.root;
    let tag_list_box = sidebar_widgets.tag_list_box;
    let tag_names = sidebar_widgets.tag_names;
    let match_any_radio = sidebar_widgets.match_any_radio;
    let match_all_radio = sidebar_widgets.match_all_radio;
    let match_mode_box = sidebar_widgets.match_mode_box;
    let no_tags_label = sidebar_widgets.no_tags_label;
    let roots_list_box = sidebar_widgets.roots_list_box;
    let update_tag_visibility = sidebar_widgets.update_tag_visibility;

    sidebar_root.set_hexpand(false);

    // 2. Main Content Top Bar
    let header_widgets = crate::ui::header::build(&ui_state.borrow());
    let header_bar = header_widgets.header_bar;

    let search_entry = header_widgets.search_entry;
    let zoom_slider = header_widgets.zoom_slider;
    let zoom_box = header_widgets.zoom_box;
    let active_filter_pill = header_widgets.active_filter_pill;
    let sort_menu_btn = header_widgets.sort_menu_btn;
    let sort_radios = header_widgets.sort_radios;
    let settings_btn = header_widgets.settings_btn;
    let offline_banner = header_widgets.offline_banner;
    let scan_error_button = header_widgets.scan_error_button;
    let scan_error_paths = header_widgets.scan_error_paths;

    let scan_indicator_banner = adw::Banner::builder()
        .title("Indexing media… 0 files found")
        .revealed(false)
        .build();

    settings_btn.set_tooltip_text(Some("Settings"));
    sort_menu_btn.set_tooltip_text(Some("Sort by"));
    zoom_slider.update_property(&[gtk::accessible::Property::Label("Zoom level")]);

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
    let no_res_clear_btn = gtk::Button::builder()
        .label("Clear All Filters")
        .halign(gtk::Align::Center)
        .build();

    // Initial fetch offloaded to background
    app_tx.send_critical(crate::events::AppEvent::FetchData);

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
    let grid_refresh_cb_ui = grid_refresh_cb.clone();
    let filter_controller_ref_ui = filter_controller_ref.clone();
    let stack_ui = stack.clone();

    let selected_tags_ui = selected_tags.clone();
    let no_tags_label_ui = no_tags_label.clone();
    let sort_menu_btn_ui = sort_menu_btn.clone();
    let update_tag_visibility_ui = update_tag_visibility.clone();
    let zoom_box_ui = zoom_box.clone();
    let sidebar_root_ui = sidebar_root.clone();
    let root_stack_ui = root_stack.clone();
    let roots_list_box_ui = roots_list_box.clone();
    let offline_banner_ui = offline_banner.clone();
    let scan_error_button_ui = scan_error_button.clone();
    let scan_error_paths_ui = scan_error_paths.clone();
    let search_entry_ui = search_entry.clone();
    let app_for_fatal = app.clone();
    let main_box_ui = main_box.clone();
    let scan_indicator_banner_ui = scan_indicator_banner.clone();

    let mut is_first_fetch = true;
    glib::MainContext::default().spawn_local(async move {
        while let Some(event) = ui_rx.recv().await {
            match event {
                UiEvent::ThumbnailReady(media_id, thumb_path, duration) => {
                    let n = list_store_clone.n_items();
                    for i in 0..n {
                        if let Some(obj) = list_store_clone.item(i)
                            && let Some(item) = obj.downcast_ref::<crate::ui::model::MediaItem>()
                        {
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
                UiEvent::ScanStarted => {
                    scan_indicator_banner_ui.set_title("Indexing media… 0 files found");
                    scan_indicator_banner_ui.set_revealed(true);
                }
                UiEvent::ScanProgress(count) => {
                    scan_indicator_banner_ui
                        .set_title(&format!("Indexing media… {} files found", count));
                }
                UiEvent::ScanCompleted(count, paths) => {
                    scan_indicator_banner_ui.set_revealed(false);
                    if count > 0 {
                        scan_error_button_ui
                            .set_label(&format!("{} file(s) could not be read.", count));
                        scan_error_button_ui.set_visible(true);
                        *scan_error_paths_ui.borrow_mut() = paths;
                    } else {
                        scan_error_button_ui.set_visible(false);
                        scan_error_paths_ui.borrow_mut().clear();
                    }
                    // DB is the source of truth for grid slices; fetching fresh ensures UI perfectly matches post-scan state without complex local recalculations.
                    app_tx_loop.send_critical(crate::events::AppEvent::FetchData);
                }
                UiEvent::TagsUpdated(tags) => {
                    while let Some(child) = tag_list_box_ui.first_child() {
                        tag_list_box_ui.remove(&child);
                    }
                    let mut new_names = Vec::new();
                    let mut sorted_tags = tags.clone();
                    sorted_tags.sort_by_key(|b| std::cmp::Reverse(b.file_count));
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

                    let current_selected = selected_tags_ui.borrow().clone();
                    for (i, tag) in sorted_tags.iter().enumerate() {
                        if current_selected.contains(&tag.name)
                            && let Some(row) = tag_list_box_ui.row_at_index(i as i32)
                        {
                            row.add_css_class("active");
                        }
                    }

                    let is_empty = tags.is_empty();
                    no_tags_label_ui.set_visible(is_empty);
                }
                UiEvent::MediaAdded(item_data) => {
                    let mut found = false;
                    for i in 0..list_store_clone.n_items() {
                        if let Some(obj) = list_store_clone.item(i)
                            && let Some(item) = obj.downcast_ref::<crate::ui::model::MediaItem>()
                        {
                            let path: String = item.property("path");
                            if path == item_data.path {
                                item.set_property("filename", &item_data.filename);
                                item.set_property("tags", &item_data.tags);
                                item.set_property("thumbnail-path", &item_data.thumbnail_path);
                                item.set_property("duration-secs", item_data.duration_secs);
                                item.set_property(
                                    "is-video",
                                    matches!(item_data.media_type, crate::events::MediaType::Video),
                                );
                                item.set_property("size-bytes", item_data.size_bytes);
                                if let Some(c) = item_data.created_at {
                                    item.set_property("created-at", c);
                                }
                                item.set_property("modified-at", item_data.modified_at);
                                item.set_property("is-offline", item_data.is_offline);
                                found = true;
                                break;
                            }
                        }
                    }
                    if !found {
                        let item = crate::ui::model::MediaItem::from(item_data.clone());
                        list_store_clone.append(&item);
                    }

                    if item_data.thumbnail_path.is_empty() {
                        thumb_tx_ui.send_log(crate::thumbnail::ThumbnailRequest {
                            media_id: item_data.id,
                            path: std::path::PathBuf::from(&item_data.path),
                            media_type: item_data.media_type,
                            modified_at: item_data.modified_at,
                        });
                    }

                    if list_store_clone.n_items() == 0 {
                        stack_ui.set_visible_child_name("no-results");
                    } else {
                        stack_ui.set_visible_child_name("grid");
                    }
                }
                UiEvent::MediaRemoved(path_str) => {
                    for i in 0..list_store_clone.n_items() {
                        if let Some(obj) = list_store_clone.item(i)
                            && let Some(item) = obj.downcast_ref::<crate::ui::model::MediaItem>()
                        {
                            let path: String = item.property("path");
                            if path == path_str {
                                list_store_clone.remove(i);
                                break;
                            }
                        }
                    }
                    if list_store_clone.n_items() == 0 {
                        stack_ui.set_visible_child_name("no-results");
                    }
                }
                UiEvent::QueryResult(media, _total) => {
                    list_store_clone.remove_all();
                    for item_data in media {
                        let item = crate::ui::model::MediaItem::from(item_data.clone());
                        list_store_clone.append(&item);

                        if item_data.thumbnail_path.is_empty() {
                            thumb_tx_ui.send_log(crate::thumbnail::ThumbnailRequest {
                                media_id: item_data.id,
                                path: std::path::PathBuf::from(&item_data.path),
                                media_type: item_data.media_type,
                                modified_at: item_data.modified_at,
                            });
                        }
                    }
                    if list_store_clone.n_items() == 0 {
                        stack_ui.set_visible_child_name("no-results");
                    } else {
                        stack_ui.set_visible_child_name("grid");
                    }
                }
                UiEvent::DataFetched {
                    tags,
                    media,
                    roots,
                    has_roots,
                } => {
                    *has_roots_state_ui.borrow_mut() = has_roots;

                    let mut roots_for_state = Vec::new();

                    while let Some(child) = roots_list_box_ui.first_child() {
                        roots_list_box_ui.remove(&child);
                    }
                    for root in roots {
                        roots_for_state.push((root.id, root.path.clone()));

                        let row_box = gtk::Box::builder()
                            .orientation(gtk::Orientation::Horizontal)
                            .spacing(8)
                            .build();

                        let icon = gtk::Image::builder().icon_name("folder-symbolic").build();

                        let label = gtk::Label::builder()
                            .label(&root.name)
                            .halign(gtk::Align::Start)
                            .ellipsize(gtk::pango::EllipsizeMode::End)
                            .hexpand(true)
                            .build();

                        row_box.append(&icon);
                        row_box.append(&label);

                        let list_box_row = gtk::ListBoxRow::builder().child(&row_box).build();

                        if !root.is_available {
                            list_box_row.add_css_class("offline");
                            icon.add_css_class("dim-label");
                            label.add_css_class("dim-label");
                            let offline_icon = gtk::Image::builder()
                                .icon_name("network-offline-symbolic")
                                .build();
                            row_box.append(&offline_icon);
                            list_box_row.set_tooltip_text(Some("Offline"));
                            list_box_row.update_property(&[
                                gtk::accessible::Property::Description("Offline"),
                            ]);
                        }

                        roots_list_box_ui.append(&list_box_row);
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
                    sorted_tags.sort_by_key(|b| std::cmp::Reverse(b.file_count));
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
                            if let Some(controller) = filter_controller_ref_ui.borrow().as_ref() {
                                controller.apply_restored_state(&tags, &active_tags);
                            } else if let Some(cb) = grid_refresh_cb_ui.borrow().as_ref() {
                                cb();
                            }
                        }

                        if scroll_pos > 0
                            && let (Some(grid), Some(vadj)) = (
                                grid_view_ref_ui.borrow().as_ref(),
                                vadj_ref_ui.borrow().as_ref(),
                            )
                        {
                            let grid_clone = grid.clone();
                            let vadj_clone = vadj.clone();
                            let ui_state_clone = ui_state_ui.clone();
                            // Queued instead of immediate to prevent layout thrashing while GTK computes container bounds during resize/init.
                            glib::idle_add_local_once(move || {
                                let zoom = ui_state_clone.borrow().zoom_level.round() as i32;
                                let width = match zoom {
                                    0 => 100,
                                    1 => 140,
                                    2 => 180,
                                    3 => 240,
                                    4 => 320,
                                    _ => 180,
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

                    // Update visibility
                    if has_roots {
                        root_stack_ui.set_visible_child_name("main");
                        if sidebar_root_ui.parent().is_none() {
                            main_box_ui.prepend(&sidebar_root_ui);
                        }
                    } else {
                        root_stack_ui.set_visible_child_name("empty");
                    }

                    sort_menu_btn_ui.set_visible(has_roots);
                    zoom_box_ui.set_visible(has_roots);
                    search_entry_ui.set_visible(true);

                    let is_empty = tags.is_empty();
                    no_tags_label_ui.set_visible(is_empty);

                    // Update media
                    list_store_clone.remove_all();
                    for item_data in media {
                        let item = crate::ui::model::MediaItem::from(item_data.clone());
                        list_store_clone.append(&item);

                        if item_data.thumbnail_path.is_empty() {
                            thumb_tx_ui.send_log(crate::thumbnail::ThumbnailRequest {
                                media_id: item_data.id,
                                path: std::path::PathBuf::from(&item_data.path),
                                media_type: item_data.media_type,
                                modified_at: item_data.modified_at,
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
                UiEvent::ViewerClosed(index) => {
                    if let Some(grid) = grid_view_ref_ui.borrow().as_ref() {
                        let grid_clone = grid.clone();
                        glib::idle_add_local_once(move || {
                            grid_clone.scroll_to(index, gtk::ListScrollFlags::FOCUS, None);
                            grid_clone.grab_focus();
                        });
                    }
                }
            }
        }
    });

    let filter_controller = crate::ui::filter_controller::FilterController::new(
        crate::ui::filter_controller::FilterControllerParams {
            list_store: list_store.clone(),
            selected_tags: selected_tags.clone(),
            match_all: match_all.clone(),
            search_query: search_query.clone(),
            search_entry: search_entry.clone(),
            tag_list_box: tag_list_box.clone(),
            tag_names: tag_names.clone(),
            match_any_radio: match_any_radio.clone(),
            match_all_radio: match_all_radio.clone(),
            match_mode_box: match_mode_box.clone(),
            active_filter_pill: active_filter_pill.clone(),
            no_results_clear_btn: no_res_clear_btn.clone(),
            sort_radios: sort_radios.clone(),
            initial_sort: ui_state.borrow().sort_order.clone(),
            app_tx: app_tx.clone(),
        },
    );
    let filter_controller_for_refresh = filter_controller.clone();
    *grid_refresh_cb.borrow_mut() = Some(Rc::new(move || filter_controller_for_refresh.refresh()));
    *filter_controller_ref.borrow_mut() = Some(filter_controller.clone());
    let filter_model = filter_controller.filter_model.clone();
    let sort_list_model = filter_controller.sort_list_model.clone();
    let selection_model = gtk::MultiSelection::new(Some(sort_list_model.clone()));

    let viewer_ref: Rc<RefCell<Option<Rc<crate::ui::viewer::Viewer>>>> =
        Rc::new(RefCell::new(None));
    let selection_anchor: Rc<RefCell<Option<u32>>> = Rc::new(RefCell::new(None));
    let selection_history: Rc<RefCell<Vec<u32>>> = Rc::new(RefCell::new(Vec::new()));
    let factory = crate::ui::grid_cell::create_factory(
        viewer_ref.clone(),
        selection_model.clone(),
        selection_anchor.clone(),
        selection_history.clone(),
    );
    // gtk::GridView provides viewport virtualization; rendering all cells at once scales poorly beyond a few hundred widgets.
    // The factory uses cell reuse pooling because allocating new GTK widgets for every item is too slow.
    let grid_view = gtk::GridView::builder()
        .model(&selection_model)
        .factory(&factory)
        .max_columns(30)
        .min_columns(2)
        .enable_rubberband(false)
        // 8px margin exactly matches the grid's 8px border-spacing rhythm
        // and provides the absolute minimum clearance required to prevent
        // the card's 12px blur radius box-shadow from clipping at y=0.
        .margin_top(8)
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
            let css = format!(
                "gridview child {{ min-width: {}px; min-height: {}px; }}",
                width, width
            );
            grid_provider.load_from_string(&css);
        }
    });

    zoom_slider.emit_by_name::<()>("value-changed", &[]);

    let scrolled_grid = gtk::ScrolledWindow::builder()
        .child(&grid_view)
        .vexpand(true)
        .hexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
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
        // Debounced to prevent thrashing UI state and excessive config writes during rapid scroll/resize.
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

    let empty_state_view = no_roots_page;

    root_stack.add_named(&empty_state_view, Some("empty"));

    // Wire up Add Source Directory

    let app_tx_add = app_tx.clone();
    add_dir_btn.connect_clicked({
        let app_tx_c = app_tx_add.clone();

        move |btn| {
            let dialog = gtk::FileDialog::new();
            let app_tx_inner = app_tx_c.clone();
            let parent_win = btn.root().and_downcast::<gtk::Window>();

            dialog.select_folder(
                parent_win.as_ref(),
                None::<&libadwaita::gtk::gio::Cancellable>,
                move |res| {
                    if let Ok(file) = res
                        && let Some(path) = file.path()
                    {
                        let path_str = match path.to_str() {
                            Some(s) => s.to_string(),
                            None => return,
                        };
                        app_tx_inner
                            .send_critical(crate::events::AppEvent::AddSourceRoot(path_str));
                    }
                },
            );
        }
    });

    let no_results_page = adw::StatusPage::builder()
        .title("No Results")
        .description("Try a different search or tag combination.")
        .icon_name("edit-find-symbolic")
        .build();
    no_results_page.set_child(Some(&no_res_clear_btn));
    stack.add_named(&no_results_page, Some("no-results"));

    let main_overlay = gtk::Overlay::builder().build();
    main_overlay.set_child(Some(&stack));

    let viewer = crate::ui::viewer::Viewer::new(filter_model.clone(), ui_tx.clone());
    *viewer_ref.borrow_mut() = Some(viewer.clone());
    main_overlay.add_overlay(&viewer.dim_bg);
    main_overlay.add_overlay(&viewer.overlay);

    let selection_bar = crate::ui::selection_bar::SelectionBar::new(
        selection_model.clone(),
        filter_model.clone(),
        selection_anchor.clone(),
        selection_history.clone(),
    );
    main_overlay.add_overlay(&selection_bar.revealer);
    main_overlay.add_overlay(&scan_error_button);

    let viewer_for_activate = viewer.clone();
    grid_view.connect_activate(move |_, pos| {
        viewer_for_activate.open(pos);
    });

    selection_bar.install_grid_keyboard_handler(&grid_view, &search_entry, viewer.clone());

    main_overlay.set_hexpand(true);
    main_overlay.set_vexpand(true);
    root_stack.add_named(&main_overlay, Some("main"));

    let grid_toolbar_view = adw::ToolbarView::builder().content(&root_stack).build();

    grid_toolbar_view.add_top_bar(&header_bar);
    grid_toolbar_view.add_top_bar(&offline_banner);
    grid_toolbar_view.add_top_bar(&scan_indicator_banner);

    grid_toolbar_view.set_hexpand(true);

    main_box.append(&grid_toolbar_view);

    // 5. Connecting logic

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
    let (w, h, max) = {
        let s = ui_state.borrow();
        (s.window_width, s.window_height, s.window_maximized)
    };
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Vesper")
        .default_width(w)
        .default_height(h)
        .maximized(max)
        .content(&main_box)
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
        if sidebar_root.parent().is_none() {
            main_box.prepend(&sidebar_root);
        }
    } else {
        root_stack.set_visible_child_name("empty");
    }

    let app_state_close = app_state.clone();

    let zoom_slider_close = zoom_slider.clone();
    let sort_radios_close = sort_radios.clone();
    let match_all_radio_close = match_all_radio.clone();

    let selected_tags_close = selected_tags.clone();
    let ui_state_close = ui_state.clone();

    window.connect_close_request(move |win| {
        if let Ok(mut state) = app_state_close.lock() {
            state.ui.window_width = win.width();
            state.ui.window_height = win.height();
            state.ui.window_maximized = win.is_maximized();
            state.ui.zoom_level = zoom_slider_close.value();
            state.ui.scroll_position = ui_state_close.borrow().scroll_position;
            state.ui.tag_filter_mode = if match_all_radio_close.is_active() {
                "AND".to_string()
            } else {
                "OR".to_string()
            };

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
            for (i, radio) in sort_radios_close.iter().enumerate() {
                if radio.is_active() {
                    state.ui.sort_order = sort_model_list[i].to_string();
                    break;
                }
            }

            state.ui.active_tags = selected_tags_close.borrow().clone();

            let _ = state.save();
        }
        glib::Propagation::Proceed
    });

    let key_controller = gtk::EventControllerKey::new();
    let viewer_clone = viewer.clone();
    let window_clone = window.clone();
    key_controller.connect_key_pressed(move |_, keyval, _, state| {
        if keyval == gtk::gdk::Key::F1
            || (keyval == gtk::gdk::Key::question
                && state.contains(gtk::gdk::ModifierType::CONTROL_MASK))
        {
            crate::ui::shortcuts::show_shortcuts_window(&window_clone);
            return glib::Propagation::Stop;
        }

        // Viewer shortcuts guard
        if !viewer_clone.is_open() {
            return glib::Propagation::Proceed;
        }

        if keyval == gtk::gdk::Key::Escape {
            viewer_clone.close();
            return glib::Propagation::Stop;
        }

        glib::Propagation::Proceed
    });
    window.add_controller(key_controller);

    window.present();
}
