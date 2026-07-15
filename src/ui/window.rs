use crate::events::ChannelSendExt;
use libadwaita as adw;
use libadwaita::gtk::{self, glib};
use libadwaita::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

type RefreshCb = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

/// Vertical spacing between grid rows in pixels; must match the `.gridview`
/// `border-spacing` in `style.css`.
const GRID_ROW_SPACING: i32 = 16;

/// Sort labels in the order the sort radios are built (see `header.rs`).
const SORT_ORDER_LABELS: [&str; 8] = [
    "Date modified (newest first)",
    "Date modified (oldest first)",
    "Date created (newest first)",
    "Date created (oldest first)",
    "Filename (A → Z)",
    "Filename (Z → A)",
    "File size (largest first)",
    "File size (smallest first)",
];

/// Grid cell width in pixels for a rounded zoom level; mirrors the widths used
/// to build the grid CSS in the zoom handler.
fn cell_width_for_zoom(zoom: i32) -> i32 {
    match zoom {
        0 => 100,
        1 => 140,
        2 => 180,
        3 => 240,
        4 => 320,
        _ => 180,
    }
}

/// The currently selected sort order, read from the sort radio group.
fn active_sort_order(sort_radios: &[gtk::CheckButton]) -> String {
    sort_radios
        .iter()
        .position(|r| r.is_active())
        .and_then(|i| SORT_ORDER_LABELS.get(i).copied())
        .unwrap_or(SORT_ORDER_LABELS[0])
        .to_string()
}

/// Media ids currently on display, in display order — used to resolve a stored
/// [`crate::state::ScrollAnchor`] against the live (filtered/sorted) result set.
fn ordered_media_ids(model: &impl IsA<gtk::gio::ListModel>) -> Vec<i64> {
    (0..model.n_items())
        .filter_map(|i| model.item(i).and_downcast::<crate::ui::model::MediaItem>())
        .map(|item| item.property::<i64>("id"))
        .collect()
}

pub enum UiEvent {
    ThumbnailReady(i64, String, Option<i64>),
    ScanCompleted(usize, Vec<String>),
    BackendWarning(String),
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
    db: Arc<crate::db::Database>,
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
    // A-7: identity of every current tag (refreshed on each fetch) so the close
    // handler can map selected display names back to tag identities; plus the
    // filters suspended because their root is offline — retained across the
    // session for re-persistence and to drive the offline banner text.
    let current_tag_identities: Rc<RefCell<Vec<crate::state::TagFilter>>> =
        Rc::new(RefCell::new(Vec::new()));
    let suspended_filters: Rc<RefCell<Vec<crate::state::TagFilter>>> =
        Rc::new(RefCell::new(Vec::new()));

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
    let backend_warning = header_widgets.backend_warning;

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
    let current_tag_identities_ui = current_tag_identities.clone();
    let suspended_filters_ui = suspended_filters.clone();
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
    let backend_warning_ui = backend_warning.clone();
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
                UiEvent::ScanCompleted(count, _paths) => {
                    scan_indicator_banner_ui.set_revealed(false);
                    if count > 0 {
                        scan_error_button_ui
                            .set_label(&format!("{} file(s) could not be read.", count));
                        scan_error_button_ui.set_visible(true);
                        // Scan-error paths live in the scan_errors table now; the
                        // click handler reads them from there (A-4).
                        *backend_warning_ui.borrow_mut() = None;
                    } else {
                        scan_error_button_ui.set_visible(false);
                        *backend_warning_ui.borrow_mut() = None;
                    }
                    // DB is the source of truth for grid slices; fetching fresh ensures UI perfectly matches post-scan state without complex local recalculations.
                    app_tx_loop.send_critical(crate::events::AppEvent::FetchData);
                }
                UiEvent::BackendWarning(message) => {
                    scan_error_button_ui.set_label(&message);
                    scan_error_button_ui.set_visible(true);
                    *backend_warning_ui.borrow_mut() = Some(message);
                }
                UiEvent::TagsUpdated(tags) => {
                    while let Some(child) = tag_list_box_ui.first_child() {
                        tag_list_box_ui.remove(&child);
                    }
                    let mut new_names = Vec::new();
                    let mut sorted_tags = tags.clone();
                    sorted_tags.sort_by_key(|b| std::cmp::Reverse(b.file_count));
                    for tag in &sorted_tags {
                        new_names.push(tag.display_name.clone());
                        let label_text = format!("{} ({})", tag.display_name, tag.file_count);
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
                        if current_selected.contains(&tag.display_name)
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

                    if item_data.thumbnail_path.is_empty() && !item_data.is_offline {
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

                        if item_data.thumbnail_path.is_empty() && !item_data.is_offline {
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
                    // Borrow rather than consume: the availability info in `roots`
                    // is needed again below for A-7 filter reconciliation.
                    for root in &roots {
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
                        new_names.push(tag.display_name.clone());
                        let label_text = format!("{} ({})", tag.display_name, tag.file_count);
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

                    // A-7: keep the full identity list of current tags so the close
                    // handler can map selected display names back to identities.
                    *current_tag_identities_ui.borrow_mut() = tags
                        .iter()
                        .map(|t| crate::state::TagFilter {
                            source_root_id: t.source_root_id,
                            relative_folder_path: t.relative_folder_path.clone(),
                            display_name: t.display_name.clone(),
                        })
                        .collect();

                    if is_first_fetch {
                        is_first_fetch = false;
                        let persisted = ui_state_ui.borrow().active_tags.clone();
                        let anchor = ui_state_ui.borrow().scroll_anchor.clone();

                        // A-7: reconcile persisted identity filters against the live
                        // source roots. Removed-root filters are discarded; offline-
                        // root filters are suspended (hidden but retained); the rest
                        // become the active filter set.
                        let roots_map: std::collections::HashMap<i64, crate::state::RootStatus> =
                            roots
                                .iter()
                                .map(|r| {
                                    let status = if r.is_available {
                                        crate::state::RootStatus::Online
                                    } else {
                                        crate::state::RootStatus::Offline
                                    };
                                    (r.id, status)
                                })
                                .collect();
                        let online_tags: std::collections::HashSet<(i64, String)> = tags
                            .iter()
                            .filter(|t| {
                                roots_map.get(&t.source_root_id)
                                    == Some(&crate::state::RootStatus::Online)
                            })
                            .map(|t| (t.source_root_id, t.relative_folder_path.clone()))
                            .collect();
                        let reconciled = crate::state::reconcile_tag_filters(
                            &persisted,
                            &roots_map,
                            &online_tags,
                        );

                        let active_display_names = reconciled.active_display_names();
                        *suspended_filters_ui.borrow_mut() = reconciled.suspended.clone();
                        // Fold the reconciliation back into in-memory state so a close
                        // with no further edits persists only surviving filters.
                        ui_state_ui.borrow_mut().active_tags = reconciled.to_persist();

                        if !active_display_names.is_empty() {
                            if let Some(controller) = filter_controller_ref_ui.borrow().as_ref() {
                                controller.apply_restored_state(&tags, &active_display_names);
                            } else if let Some(cb) = grid_refresh_cb_ui.borrow().as_ref() {
                                cb();
                            }
                        }

                        if anchor.media_id.is_some()
                            && let (Some(grid), Some(vadj)) = (
                                grid_view_ref_ui.borrow().as_ref(),
                                vadj_ref_ui.borrow().as_ref(),
                            )
                        {
                            let grid_clone = grid.clone();
                            let vadj_clone = vadj.clone();
                            let ui_state_clone = ui_state_ui.clone();
                            // The active display names drive the grid filter, so the
                            // scroll-context hash must be computed from them (matching
                            // how it was captured on save), not from the persisted set
                            // which also includes suspended filters.
                            let hash_tags = active_display_names.clone();
                            // Queued instead of immediate so the sort/filter models and
                            // container bounds are settled before we resolve the anchor
                            // against the current (possibly reordered/filtered) result set.
                            glib::idle_add_local_once(move || {
                                // Resolve the anchor by identity against what is now on
                                // display. A missing item (deleted, filtered out, or on
                                // an offline root) leaves the grid at the top.
                                let Some(model) = grid_clone.model() else {
                                    return;
                                };
                                let ordered = ordered_media_ids(&model);
                                let Some(index) = anchor.resolve(&ordered) else {
                                    return;
                                };

                                let zoom = ui_state_clone.borrow().zoom_level.round() as i32;
                                let width = cell_width_for_zoom(zoom);
                                let mut grid_w = grid_clone.width();
                                if grid_w <= 0 {
                                    let window_w = ui_state_clone.borrow().window_width;
                                    grid_w = std::cmp::max(100, window_w - 250);
                                }
                                let columns = std::cmp::max(
                                    1,
                                    (grid_w + GRID_ROW_SPACING) / (width + GRID_ROW_SPACING),
                                );
                                let row = index as i32 / columns;
                                let row_top = (row * (width + GRID_ROW_SPACING)) as f64;

                                // Apply the saved sub-row offset only when the ordering
                                // context is unchanged; otherwise land at the row top.
                                let current_hash = {
                                    let s = ui_state_clone.borrow();
                                    crate::state::ScrollAnchor::context_hash(
                                        &s.sort_order,
                                        &hash_tags,
                                        &s.tag_filter_mode,
                                    )
                                };
                                let offset = if current_hash == anchor.context_hash {
                                    anchor.offset_within_cell
                                } else {
                                    0.0
                                };
                                vadj_clone.set_value(row_top + offset);
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

                        if item_data.thumbnail_path.is_empty() && !item_data.is_offline {
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
                        // A-7: when a filter is suspended because its root is offline,
                        // the shared offline banner also explains that offline-source
                        // filters are temporarily unavailable (02 §10 / 04 §11).
                        let mut title = format!("{} source root(s) offline.", count);
                        if !suspended_filters_ui.borrow().is_empty() {
                            title.push_str(
                                " Filters from offline sources are temporarily unavailable.",
                            );
                        }
                        offline_banner_ui.set_title(&title);
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
    let sort_radios_scroll = sort_radios.clone();
    let match_all_radio_scroll = match_all_radio.clone();
    let selected_tags_scroll = selected_tags.clone();

    let scroll_timeout_id: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));

    vadj.connect_value_changed(move |adj| {
        let val = adj.value();
        let ui_state = ui_state_scroll.clone();
        let grid = grid_view_scroll.clone();
        let sort_radios = sort_radios_scroll.clone();
        let match_all_radio = match_all_radio_scroll.clone();
        let selected_tags = selected_tags_scroll.clone();

        if let Some(id) = scroll_timeout_id.borrow_mut().take() {
            id.remove();
        }

        let scroll_timeout_id_clone = scroll_timeout_id.clone();
        // Debounced to prevent thrashing UI state and excessive config writes during rapid scroll/resize.
        let new_id = glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
            // Capture a stable anchor (A-6): the identity of the item at the top
            // of the viewport plus its sub-row offset and the ordering context,
            // rather than a raw item index that a later reorder would invalidate.
            let zoom = ui_state.borrow().zoom_level.round() as i32;
            let width = cell_width_for_zoom(zoom);
            let row_height = (width + GRID_ROW_SPACING) as f64;
            let top_row = (val / row_height).floor().max(0.0);
            let offset_within_cell = val - top_row * row_height;

            let grid_w = grid.width().max(1);
            let columns =
                std::cmp::max(1, (grid_w + GRID_ROW_SPACING) / (width + GRID_ROW_SPACING));
            let first_index = top_row as u32 * columns as u32;

            let media_id = grid
                .model()
                .and_then(|m| m.item(first_index))
                .and_downcast::<crate::ui::model::MediaItem>()
                .map(|item| item.property::<i64>("id"));

            let context_hash = crate::state::ScrollAnchor::context_hash(
                &active_sort_order(&sort_radios),
                &selected_tags.borrow(),
                if match_all_radio.is_active() {
                    "AND"
                } else {
                    "OR"
                },
            );

            let anchor = crate::state::ScrollAnchor {
                media_id,
                offset_within_cell,
                context_hash,
            };
            if ui_state.borrow().scroll_anchor != anchor {
                ui_state.borrow_mut().scroll_anchor = anchor;
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

    let viewer = crate::ui::viewer::Viewer::new(sort_list_model.clone(), ui_tx.clone());
    *viewer_ref.borrow_mut() = Some(viewer.clone());
    main_overlay.add_overlay(&viewer.dim_bg);
    main_overlay.add_overlay(&viewer.overlay);

    let selection_bar = crate::ui::selection_bar::SelectionBar::new(
        selection_model.clone(),
        sort_list_model.clone(),
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

    let backend_warning_for_btn = backend_warning.clone();
    let db_for_btn = db.clone();
    let window_for_dialog = window.clone();
    scan_error_button.connect_clicked(move |_| {
        // A transient backend warning takes precedence; otherwise show the
        // outstanding scan errors read live from the scan_errors table (A-4).
        let (heading, body) = if let Some(message) = backend_warning_for_btn.borrow().clone() {
            ("Backend Warning", message)
        } else {
            let paths = db_for_btn.get_scan_error_paths().unwrap_or_default();
            let total = paths.len();
            let mut display_paths = paths;
            if display_paths.len() > 20 {
                display_paths.truncate(20);
                display_paths.push(format!("...and {} more", total - 20));
            }
            ("Files Could Not Be Read", display_paths.join("\n"))
        };
        let dialog = adw::MessageDialog::builder()
            .heading(heading)
            .body(body)
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
    let db_close = db.clone();

    let zoom_slider_close = zoom_slider.clone();
    let sort_radios_close = sort_radios.clone();
    let match_all_radio_close = match_all_radio.clone();

    let selected_tags_close = selected_tags.clone();
    let ui_state_close = ui_state.clone();
    let current_tag_identities_close = current_tag_identities.clone();
    let suspended_filters_close = suspended_filters.clone();

    window.connect_close_request(move |win| {
        if let Ok(mut state) = app_state_close.lock() {
            state.ui.window_width = win.width();
            state.ui.window_height = win.height();
            state.ui.window_maximized = win.is_maximized();
            state.ui.zoom_level = zoom_slider_close.value();
            state.ui.scroll_anchor = ui_state_close.borrow().scroll_anchor.clone();
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

            // A-7: persist identity-qualified filters. Map the currently-active
            // display names back to tag identities, then re-append the filters
            // suspended because their root is offline so they survive to
            // auto-restore once that root returns.
            let identities = current_tag_identities_close.borrow();
            let mut active_tags: Vec<crate::state::TagFilter> = selected_tags_close
                .borrow()
                .iter()
                .filter_map(|name| identities.iter().find(|t| &t.display_name == name).cloned())
                .collect();
            active_tags.extend(suspended_filters_close.borrow().iter().cloned());
            state.ui.active_tags = active_tags;

            let _ = state.save(&db_close);
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
