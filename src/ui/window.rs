//! Top-level window assembly and the backend→UI event loop.
//!
//! Assembles the widget tree of Architecture §9 bottom-up and drives it from the
//! core `UiEvent` stream. Every backend result carries a generation; the UI
//! applies a result only when its generation is current and ignores superseded
//! ones (Arch §5). Sidebar collapse, banners, and the scroll anchor are wired to
//! in-memory state per the §10 state→UI mapping.

use crate::events::{ChannelSendExt, UiEvent};
use libadwaita as adw;
use libadwaita::gtk::{self, glib};
use libadwaita::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use crate::ui::header::{HeaderControls, SORT_ORDER_LABELS};

/// `session_state` key holding the persisted sidebar-collapsed flag. Kept out of
/// `UiState` (state.rs is authoritative and untouched here) but still durable.
const SIDEBAR_COLLAPSED_KEY: &str = "sidebar_collapsed";

/// Visible media-to-media gap between grid rows: the two 6px `.media-cell`
/// margins in `style.css` sum to the constant 12px gutter (Visual §3).
const GRID_ROW_SPACING: i32 = 12;

/// A-6 startup anchor waiting for the restored query's results, plus the active
/// identity filters used for its context hash.
type PendingAnchor = (crate::state::ScrollAnchor, Vec<crate::state::TagFilter>);

/// A late-bound, replaceable zero-argument callback cell (e.g. the grid-refresh
/// and Settings-refresh hooks registered after their widgets exist).
type RefreshCell = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

/// Grid cell edge in pixels for each of the five thumbnail sizes (index 0..=4).
/// Sized to match the mockup's proportions (Medium ≈ 4–5 columns on a ~1240px
/// grid area).
fn cell_width_for_zoom(zoom: i32) -> i32 {
    match zoom {
        0 => 150,
        1 => 210,
        2 => 280,
        3 => 360,
        4 => 460,
        _ => 280,
    }
}

fn active_sort_order(sort_radios: &[gtk::CheckButton]) -> String {
    sort_radios
        .iter()
        .position(|r| r.is_active())
        .and_then(|i| SORT_ORDER_LABELS.get(i).copied())
        .unwrap_or(SORT_ORDER_LABELS[0])
        .to_string()
}

fn ordered_media_ids(model: &impl IsA<gtk::gio::ListModel>) -> Vec<i64> {
    (0..model.n_items())
        .filter_map(|i| model.item(i).and_downcast::<crate::ui::model::MediaItem>())
        .map(|item| item.property::<i64>("id"))
        .collect()
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum BannerPriority {
    Critical,
    Offline,
    Indexing,
    #[default]
    None,
}

#[derive(Default)]
struct BannerState {
    critical: bool,
    offline: bool,
    indexing: bool,
}

fn banner_priority(state: &BannerState) -> BannerPriority {
    if state.critical {
        BannerPriority::Critical
    } else if state.offline {
        BannerPriority::Offline
    } else if state.indexing {
        BannerPriority::Indexing
    } else {
        BannerPriority::None
    }
}

fn update_status_banner_stack(stack: &gtk::Stack, state: &BannerState) {
    match banner_priority(state) {
        BannerPriority::None => stack.set_visible(false),
        BannerPriority::Critical => {
            stack.set_visible_child_name("critical");
            stack.set_visible(true);
        }
        BannerPriority::Offline => {
            stack.set_visible_child_name("offline");
            stack.set_visible(true);
        }
        BannerPriority::Indexing => {
            stack.set_visible_child_name("indexing");
            stack.set_visible(true);
        }
    }
}

/// A status banner row: warning glyph + message + optional Details/close.
struct Banner {
    root: gtk::Box,
    label: gtk::Label,
}

fn build_banner(state_class: &str, details: bool, closable: bool) -> Banner {
    let root = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .css_classes(["status-banner", state_class])
        .spacing(12)
        .build();
    root.append(&gtk::Image::from_icon_name("dialog-warning-symbolic"));
    let label = gtk::Label::builder()
        .halign(gtk::Align::Start)
        .hexpand(true)
        .wrap(true)
        .xalign(0.0)
        .build();
    root.append(&label);
    if details {
        let details_content = gtk::Box::builder().spacing(4).build();
        details_content.append(&gtk::Label::new(Some("Details")));
        details_content.append(&gtk::Image::from_icon_name("go-next-symbolic"));
        root.append(
            &gtk::Button::builder()
                .child(&details_content)
                .css_classes(["flat", "details"])
                .build(),
        );
    }
    if closable {
        let close = gtk::Button::builder()
            .icon_name("window-close-symbolic")
            .css_classes(["flat"])
            .build();
        close.update_property(&[gtk::accessible::Property::Label("Dismiss")]);
        let root_clone = root.clone();
        close.connect_clicked(move |_| root_clone.set_visible(false));
        root.append(&close);
    }
    Banner { root, label }
}

pub fn build(
    app: &adw::Application,
    app_tx: tokio::sync::mpsc::Sender<crate::events::AppEvent>,
    ui_tx: tokio::sync::mpsc::Sender<UiEvent>,
    ui_rx: tokio::sync::mpsc::Receiver<UiEvent>,
    thumb_tx: tokio::sync::mpsc::Sender<crate::thumbnail::ThumbnailRequest>,
    app_state: Arc<Mutex<crate::state::AppState>>,
    db: Arc<crate::db::Database>,
) {
    let _ = ui_tx; // backend-owned sender; the UI drives itself via callbacks.

    let provider = gtk::CssProvider::new();
    provider.load_from_string(include_str!("style.css"));
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    // ── Shared UI state ────────────────────────────────────────────────────
    let ui_state = Rc::new(RefCell::new(
        app_state.lock().map(|s| s.ui.clone()).unwrap_or_default(),
    ));
    let selected_tags = Rc::new(RefCell::new(Vec::<crate::state::TagFilter>::new()));
    let match_all = Rc::new(RefCell::new(false));
    let search_query = Rc::new(RefCell::new(String::new()));
    let has_roots_state = Rc::new(RefCell::new(false));
    let source_roots_state: Rc<RefCell<Vec<(i64, String)>>> = Rc::new(RefCell::new(Vec::new()));
    // Refresh callback (roots list) and status callback are registered by
    // Settings while open and cleared on close.
    let settings_refresh: RefreshCell = Rc::new(RefCell::new(None));
    let settings_status_cb: crate::ui::settings::StatusCb = Rc::new(RefCell::new(None));
    let grid_refresh_cb: RefreshCell = Rc::new(RefCell::new(None));
    let suspended_filters: Rc<RefCell<Vec<crate::state::TagFilter>>> =
        Rc::new(RefCell::new(Vec::new()));
    let query_generation = Rc::new(RefCell::new(crate::ui::model::QueryGeneration::default()));
    let hydration_generation = Rc::new(RefCell::new(0u64));
    let selection_mode = Rc::new(RefCell::new(false));

    // ── Sidebar ────────────────────────────────────────────────────────────
    let sidebar_widgets = crate::ui::sidebar::build(&ui_state.borrow(), match_all.clone());
    let sidebar_root = sidebar_widgets.root;
    let tag_list_box = sidebar_widgets.tag_list_box;
    let tags = sidebar_widgets.tags;
    let match_any_radio = sidebar_widgets.match_any_radio;
    let match_all_radio = sidebar_widgets.match_all_radio;
    let match_mode_box = sidebar_widgets.match_mode_box;
    let no_tags_label = sidebar_widgets.no_tags_label;
    let tips_box = sidebar_widgets.tips_box;
    let add_source_root_button = sidebar_widgets.add_source_root_button;
    let open_settings_button = sidebar_widgets.open_settings_button;
    let collapse_button = sidebar_widgets.collapse_button;

    // GTK4 CSS has no `max-width`, so a fixed 220px width is enforced with a
    // width_request on a non-expanding panel (still "fixed 220px, never
    // resizable, no GtkPaned" per Arch §9 — just a different GTK mechanism than
    // the CSS min==max the spec assumed).
    sidebar_root.set_hexpand(false);
    sidebar_root.set_size_request(220, -1);
    let sidebar_revealer = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideRight)
        .transition_duration(200)
        .hexpand(false)
        .child(&sidebar_root)
        .build();

    // ── Header ───────────────────────────────────────────────────────────
    let hw = crate::ui::header::build(&ui_state.borrow());
    let header_bar = hw.header_bar;
    let sidebar_toggle = hw.sidebar_toggle;
    let search_entry = hw.search_entry;
    let clear_filters_button = hw.clear_filters_button;
    let sort_menu_btn = hw.sort_menu_btn;
    let sort_radios = hw.sort_radios;
    let thumb_size_btn = hw.thumb_size_btn;
    let thumb_size_buttons = hw.thumb_size_buttons;
    let thumb_size_checks = hw.thumb_size_checks;
    let select_button = hw.select_button;
    let primary_settings_btn = hw.primary_settings_btn;
    let primary_shortcuts_btn = hw.primary_shortcuts_btn;
    let primary_about_btn = hw.primary_about_btn;

    let header_controls = HeaderControls {
        search_entry: search_entry.clone(),
        sort_menu_btn: sort_menu_btn.clone(),
        thumb_size_btn: thumb_size_btn.clone(),
        select_button: select_button.clone(),
    };

    // ── Sidebar collapse persistence (session_state, not UiState) ──────────
    let initial_collapsed = db
        .get_session_state(SIDEBAR_COLLAPSED_KEY)
        .ok()
        .flatten()
        .map(|v| v == "true")
        .unwrap_or(false);
    sidebar_revealer.set_reveal_child(!initial_collapsed);
    sidebar_toggle.set_visible(initial_collapsed);

    let set_collapsed: Rc<dyn Fn(bool)> = {
        let sidebar_revealer = sidebar_revealer.clone();
        let sidebar_toggle = sidebar_toggle.clone();
        let db = db.clone();
        Rc::new(move |collapsed: bool| {
            sidebar_revealer.set_reveal_child(!collapsed);
            sidebar_toggle.set_visible(collapsed);
            let _ = db.set_session_state(
                SIDEBAR_COLLAPSED_KEY,
                if collapsed { "true" } else { "false" },
            );
        })
    };
    collapse_button.connect_clicked({
        let set_collapsed = set_collapsed.clone();
        move |_| set_collapsed(true)
    });
    sidebar_toggle.connect_clicked({
        let set_collapsed = set_collapsed.clone();
        move |_| set_collapsed(false)
    });

    // ── Status banners ─────────────────────────────────────────────────────
    let critical_banner = build_banner("critical", true, false);
    let offline_banner = build_banner("offline", true, true);
    let indexing_banner = build_banner("", false, false);
    indexing_banner
        .label
        .set_text("Indexing media… 0 files found");
    let status_banner_stack = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::Crossfade)
        .visible(false)
        .build();
    status_banner_stack.add_named(&critical_banner.root, Some("critical"));
    status_banner_stack.add_named(&offline_banner.root, Some("offline"));
    status_banner_stack.add_named(&indexing_banner.root, Some("indexing"));
    let banner_state = Rc::new(RefCell::new(BannerState::default()));

    // ── Model + thumbnail memory cache ─────────────────────────────────────
    let list_store = gtk::gio::ListStore::new::<crate::ui::model::MediaItem>();
    let thumbnail_memory_cache = Rc::new(RefCell::new(
        crate::ui::grid_cell::ThumbnailMemoryCache::new(),
    ));

    // ── Scan-issue indicator (bottom-left of the grid) ─────────────────────
    let scan_error_button = gtk::Button::builder()
        .css_classes(["scan-error-button"])
        .halign(gtk::Align::Start)
        .valign(gtk::Align::End)
        .visible(false)
        .build();
    let scan_error_content = gtk::Box::builder().spacing(6).build();
    scan_error_content.append(&gtk::Image::from_icon_name("dialog-warning-symbolic"));
    scan_error_content.append(&gtk::Label::new(Some("Some files could not be scanned")));
    scan_error_button.set_child(Some(&scan_error_content));
    scan_error_button.update_property(&[gtk::accessible::Property::Label(
        "Some files could not be scanned",
    )]);
    let scan_error_paths: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let backend_warning: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

    let pending_anchor: Rc<RefCell<Option<PendingAnchor>>> = Rc::new(RefCell::new(None));
    let grid_view_ref: Rc<RefCell<Option<gtk::GridView>>> = Rc::new(RefCell::new(None));
    let vadj_ref: Rc<RefCell<Option<gtk::Adjustment>>> = Rc::new(RefCell::new(None));

    // ── Grid group header + bottom status line ─────────────────────────────
    let group_title = gtk::Label::builder()
        .label("Library")
        .css_classes(["grid-group-title"])
        .halign(gtk::Align::Start)
        .build();
    let group_count = gtk::Label::builder()
        .label("0 items")
        .css_classes(["grid-group-count"])
        .halign(gtk::Align::Start)
        .build();
    let group_header = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .margin_top(12)
        .margin_start(12)
        .margin_end(12)
        .margin_bottom(4)
        .build();
    group_header.append(&group_title);
    group_header.append(&group_count);

    let status_line = gtk::Label::builder()
        .css_classes(["grid-status-line"])
        .halign(gtk::Align::Start)
        .build();

    // ── Selection model + factory + grid ───────────────────────────────────
    let selection_anchor: Rc<RefCell<Option<u32>>> = Rc::new(RefCell::new(None));
    let selection_history: Rc<RefCell<Vec<u32>>> = Rc::new(RefCell::new(Vec::new()));
    let focused_position: Rc<RefCell<Option<u32>>> = Rc::new(RefCell::new(None));
    let viewer_ref: Rc<RefCell<Option<Rc<crate::ui::viewer::Viewer>>>> =
        Rc::new(RefCell::new(None));

    let fc = crate::ui::filter_controller::FilterController::new(
        crate::ui::filter_controller::FilterControllerParams {
            list_store: list_store.clone(),
            selected_tags: selected_tags.clone(),
            match_all: match_all.clone(),
            search_query: search_query.clone(),
            search_entry: search_entry.clone(),
            tag_list_box: tag_list_box.clone(),
            tags: tags.clone(),
            match_any_radio: match_any_radio.clone(),
            match_all_radio: match_all_radio.clone(),
            match_mode_box: match_mode_box.clone(),
            clear_filters_button: clear_filters_button.clone(),
            no_results_clear_search_btn: {
                // Wired to the no-results page button, created below; use a proxy.
                gtk::Button::new()
            },
            sort_radios: sort_radios.clone(),
            initial_sort: ui_state.borrow().sort_order.clone(),
            app_tx: app_tx.clone(),
            query_generation: query_generation.clone(),
        },
    );
    *grid_refresh_cb.borrow_mut() = Some(Rc::new({
        let fc = fc.clone();
        move || fc.refresh()
    }));
    let sort_list_model = fc.sort_list_model.clone();
    let selection_model = gtk::MultiSelection::new(Some(sort_list_model.clone()));

    let factory = crate::ui::grid_cell::create_factory(
        viewer_ref.clone(),
        selection_model.clone(),
        selection_anchor.clone(),
        selection_history.clone(),
        selection_mode.clone(),
        app_tx.clone(),
        thumbnail_memory_cache.clone(),
        thumb_tx,
        focused_position.clone(),
    );
    let grid_view = gtk::GridView::builder()
        .model(&selection_model)
        .factory(&factory)
        .max_columns(30)
        .min_columns(2)
        .enable_rubberband(false)
        .single_click_activate(false)
        // Outer padding = 6px grid margin + 6px cell margin = 12px (Visual §3).
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(6)
        .margin_end(6)
        .build();
    *grid_view_ref.borrow_mut() = Some(grid_view.clone());

    // Thumbnail-size CSS provider (per-size cell measurements).
    let grid_provider = gtk::CssProvider::new();
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &grid_provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
    let apply_thumb_size: Rc<dyn Fn(usize)> = {
        let grid_provider = grid_provider.clone();
        let ui_state = ui_state.clone();
        let checks = thumb_size_checks.clone();
        Rc::new(move |idx: usize| {
            let idx = idx.min(4);
            ui_state.borrow_mut().zoom_level = idx as f64;
            let width = cell_width_for_zoom(idx as i32);
            grid_provider.load_from_string(&format!(
                "gridview child {{ min-width: {width}px; min-height: {width}px; }}"
            ));
            for (i, check) in checks.iter().enumerate() {
                check.set_opacity(if i == idx { 1.0 } else { 0.0 });
            }
        })
    };
    // Read the persisted size into a local first so the borrow is released
    // before `apply_thumb_size` takes a mutable borrow of `ui_state`.
    let initial_thumb_idx = (ui_state.borrow().zoom_level.round() as i32).clamp(0, 4) as usize;
    apply_thumb_size(initial_thumb_idx);
    for (i, button) in thumb_size_buttons.iter().enumerate() {
        let apply_thumb_size = apply_thumb_size.clone();
        let popover = thumb_size_btn.popover();
        button.connect_clicked(move |_| {
            apply_thumb_size(i);
            if let Some(popover) = &popover {
                popover.popdown();
            }
        });
    }

    let scrolled_grid = gtk::ScrolledWindow::builder()
        .child(&grid_view)
        .vexpand(true)
        .hexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    let vadj = scrolled_grid.vadjustment();
    *vadj_ref.borrow_mut() = Some(vadj.clone());
    install_scroll_anchor_capture(
        &vadj,
        &grid_view,
        &ui_state,
        &sort_radios,
        &match_all_radio,
        &selected_tags,
    );

    let grid_overlay = gtk::Overlay::new();
    grid_overlay.set_child(Some(&scrolled_grid));
    grid_overlay.add_overlay(&scan_error_button);

    let grid_page = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .build();
    grid_page.append(&group_header);
    grid_page.append(&grid_overlay);

    // ── Empty & no-results pages ───────────────────────────────────────────
    let (empty_page, empty_add_btn) = build_empty_page();
    let (no_results_page, no_results_clear_btn, review_tags_btn) = build_no_results_page();

    let root_stack = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::Crossfade)
        .transition_duration(200)
        .css_classes(["grid-area"])
        .vexpand(true)
        .hexpand(true)
        .build();
    root_stack.add_named(&empty_page, Some("empty"));
    root_stack.add_named(&no_results_page, Some("no-results"));
    root_stack.add_named(&grid_page, Some("grid"));
    root_stack.set_visible_child_name("empty");

    // Wire the no-results "Clear search" to the filter controller by proxying
    // its click to empty the search entry (Product §4 — search only).
    no_results_clear_btn.connect_clicked({
        let search_entry = search_entry.clone();
        move |_| search_entry.set_text("")
    });
    review_tags_btn.connect_clicked({
        let tag_list_box = tag_list_box.clone();
        move |_| {
            tag_list_box.grab_focus();
            if let Some(first) = tag_list_box.row_at_index(0) {
                first.grab_focus();
            }
        }
    });

    // ── Viewer ─────────────────────────────────────────────────────────────
    let viewer = crate::ui::viewer::Viewer::new(sort_list_model.clone());
    *viewer_ref.borrow_mut() = Some(viewer.clone());
    viewer.set_on_close({
        let grid_view = grid_view.clone();
        Rc::new(move |index: u32| {
            if index >= grid_view.model().map_or(0, |m| m.n_items()) {
                return;
            }
            let grid = grid_view.clone();
            glib::idle_add_local_once(move || {
                grid.scroll_to(index, gtk::ListScrollFlags::FOCUS, None);
                grid.grab_focus();
                let grid2 = grid.clone();
                glib::idle_add_local_once(move || {
                    if let Some(cell) = grid2.focus_child() {
                        cell.add_css_class("viewer-origin");
                        glib::timeout_add_local_once(
                            std::time::Duration::from_millis(900),
                            move || cell.remove_css_class("viewer-origin"),
                        );
                    }
                });
            });
        })
    });

    // ── Selection: bar + mode wiring ───────────────────────────────────────
    let exit_selection: crate::ui::selection_bar::ExitSelection = {
        let select_button = select_button.clone();
        let selection_mode = selection_mode.clone();
        Rc::new(move || {
            *selection_mode.borrow_mut() = false;
            select_button.set_active(false);
        })
    };
    let selection_bar = crate::ui::selection_bar::SelectionBar::new(
        selection_model.clone(),
        sort_list_model.clone(),
        selection_anchor.clone(),
        selection_history.clone(),
        selection_mode.clone(),
        exit_selection.clone(),
    );
    grid_overlay.add_overlay(&selection_bar.revealer);
    selection_bar.install_grid_keyboard_handler(
        &grid_view,
        &search_entry,
        viewer.clone(),
        focused_position.clone(),
        exit_selection,
    );

    // Header select toggle drives selection mode.
    select_button.connect_toggled({
        let selection_mode = selection_mode.clone();
        let revealer = selection_bar.revealer.clone();
        let selection_model = selection_model.clone();
        move |btn| {
            let active = btn.is_active();
            *selection_mode.borrow_mut() = active;
            if active {
                revealer.set_reveal_child(true);
            } else {
                selection_model.unselect_all();
                revealer.set_reveal_child(false);
            }
        }
    });

    // Bottom status line, recomputed from the model + selection (Product §6).
    let update_status_line: Rc<dyn Fn()> = {
        let status_line = status_line.clone();
        let group_count = group_count.clone();
        let sort_list_model = sort_list_model.clone();
        let selection_model = selection_model.clone();
        let selection_mode = selection_mode.clone();
        Rc::new(move || {
            let total = sort_list_model.n_items();
            let selected = selection_model.selection().size();
            group_count.set_text(&format!("{total} items"));
            let text = if *selection_mode.borrow() && selected > 0 {
                format!("{selected} items selected")
            } else if selected > 0 {
                format!("{total} items, {selected} selected")
            } else {
                format!("{total} items")
            };
            status_line.set_text(&text);
        })
    };
    selection_model.connect_selection_changed({
        let update_status_line = update_status_line.clone();
        move |_, _, _| update_status_line()
    });
    sort_list_model.connect_items_changed({
        let update_status_line = update_status_line.clone();
        move |_, _, _, _| update_status_line()
    });

    // Double-click / Enter opens the viewer and clears any selection (Arch §9).
    grid_view.connect_activate({
        let viewer = viewer.clone();
        let selection_model = selection_model.clone();
        let selection_history = selection_history.clone();
        let selection_anchor = selection_anchor.clone();
        move |_, pos| {
            if selection_model.selection().size() > 0 {
                selection_model.unselect_all();
                selection_history.borrow_mut().clear();
                *selection_anchor.borrow_mut() = None;
            }
            viewer.open(pos);
        }
    });

    // ── Assemble the shell ─────────────────────────────────────────────────
    let grid_toolbar_view = adw::ToolbarView::builder()
        .css_classes(["grid-area"])
        .content(&root_stack)
        .hexpand(true)
        .build();
    grid_toolbar_view.add_top_bar(&header_bar);
    grid_toolbar_view.add_top_bar(&status_banner_stack);
    grid_toolbar_view.add_bottom_bar(&status_line);

    let main_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    main_box.set_hexpand(true);
    main_box.set_vexpand(true);
    main_box.append(&sidebar_revealer);
    main_box.append(&grid_toolbar_view);

    let app_overlay = gtk::Overlay::builder()
        .child(&main_box)
        .hexpand(true)
        .vexpand(true)
        .build();
    app_overlay.add_overlay(&viewer.overlay);
    // GtkOverlay allocates an overlay child its natural size, which for the
    // viewer is only the media's width — leaving the grid visible beside it.
    // Force the viewer to match the full app-overlay size while it is visible so
    // it covers the sidebar, header, and grid (Arch §9).
    {
        let app_overlay_ref = app_overlay.clone();
        viewer.overlay.add_tick_callback(move |w, _| {
            if w.get_visible() {
                let (pw, ph) = (app_overlay_ref.width(), app_overlay_ref.height());
                if pw > 0 && ph > 0 && (w.width_request() != pw || w.height_request() != ph) {
                    w.set_size_request(pw, ph);
                }
            }
            glib::ControlFlow::Continue
        });
    }

    // ── Add Source Root / Settings / menu wiring ───────────────────────────
    let pick_folder: Rc<dyn Fn(&gtk::Widget)> = {
        let app_tx = app_tx.clone();
        Rc::new(move |widget: &gtk::Widget| {
            let dialog = gtk::FileDialog::new();
            let parent = widget.root().and_downcast::<gtk::Window>();
            let app_tx = app_tx.clone();
            dialog.select_folder(
                parent.as_ref(),
                None::<&gtk::gio::Cancellable>,
                move |res| {
                    if let Ok(file) = res
                        && let Some(path) = file.path()
                        && let Some(s) = path.to_str()
                    {
                        app_tx.send_critical(crate::events::AppEvent::AddSourceRoot(s.to_string()));
                    }
                },
            );
        })
    };
    add_source_root_button.connect_clicked({
        let pick_folder = pick_folder.clone();
        move |btn| pick_folder(btn.upcast_ref::<gtk::Widget>())
    });
    empty_add_btn.connect_clicked({
        let pick_folder = pick_folder.clone();
        move |btn| pick_folder(btn.upcast_ref::<gtk::Widget>())
    });

    let open_settings: Rc<dyn Fn(&gtk::Widget)> = {
        let app_state = app_state.clone();
        let app_tx = app_tx.clone();
        let source_roots_state = source_roots_state.clone();
        let settings_refresh = settings_refresh.clone();
        let settings_status_cb = settings_status_cb.clone();
        Rc::new(move |widget: &gtk::Widget| {
            if let Some(parent) = widget.root().and_downcast::<gtk::Window>() {
                crate::ui::settings::show(
                    &parent,
                    app_state.clone(),
                    app_tx.clone(),
                    source_roots_state.clone(),
                    settings_refresh.clone(),
                    settings_status_cb.clone(),
                );
            }
        })
    };
    open_settings_button.connect_clicked({
        let open_settings = open_settings.clone();
        move |btn| open_settings(btn.upcast_ref::<gtk::Widget>())
    });
    primary_settings_btn.connect_clicked({
        let open_settings = open_settings.clone();
        move |btn| open_settings(btn.upcast_ref::<gtk::Widget>())
    });

    // ── Window ─────────────────────────────────────────────────────────────
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
        .content(&app_overlay)
        .build();

    primary_shortcuts_btn.connect_clicked({
        let window = window.clone();
        move |_| crate::ui::shortcuts::show_shortcuts_window(&window)
    });
    primary_about_btn.connect_clicked({
        let open_settings = open_settings.clone();
        let window = window.clone();
        move |_| open_settings(window.upcast_ref::<gtk::Widget>())
    });

    // Scan-issue popover.
    scan_error_button.connect_clicked({
        let backend_warning = backend_warning.clone();
        let scan_error_paths = scan_error_paths.clone();
        let app_tx = app_tx.clone();
        move |button| {
            app_tx.send_log(crate::events::AppEvent::FetchScanErrors);
            show_scan_error_popover(button, &backend_warning, &scan_error_paths);
        }
    });

    install_window_keys(
        &window,
        viewer.clone(),
        search_entry.clone(),
        apply_thumb_size.clone(),
    );

    install_close_persistence(
        &window,
        app_state.clone(),
        db.clone(),
        ui_state.clone(),
        sort_radios.clone(),
        match_all_radio.clone(),
        selected_tags.clone(),
        suspended_filters.clone(),
    );

    // ── Backend → UI event loop ────────────────────────────────────────────
    let ctx = EventLoopCtx {
        list_store: list_store.clone(),
        thumbnail_memory_cache,
        tag_list_box,
        tags,
        no_tags_label,
        tips_box,
        selected_tags,
        suspended_filters,
        source_roots_state,
        settings_refresh,
        settings_status_cb,
        grid_refresh_cb,
        query_generation,
        hydration_generation,
        ui_state,
        pending_anchor,
        grid_view_ref,
        vadj_ref,
        root_stack,
        has_roots_state,
        group_title,
        update_status_line,
        header_controls,
        scan_error_button,
        scan_error_paths,
        backend_warning,
        offline_banner_label: offline_banner.label,
        critical_banner_label: critical_banner.label,
        indexing_banner_label: indexing_banner.label,
        status_banner_stack,
        banner_state,
        app_tx: app_tx.clone(),
        app_for_fatal: app.clone(),
    };
    spawn_event_loop(ui_rx, ctx);

    app_tx.send_critical(crate::events::AppEvent::FetchData);
    window.present();
}

/// The set of handles the backend→UI event loop mutates. Grouped into a struct
/// to keep the loop signature manageable.
struct EventLoopCtx {
    list_store: gtk::gio::ListStore,
    thumbnail_memory_cache: Rc<RefCell<crate::ui::grid_cell::ThumbnailMemoryCache>>,
    tag_list_box: gtk::ListBox,
    tags: Rc<RefCell<Vec<crate::events::UiTag>>>,
    no_tags_label: gtk::Label,
    tips_box: gtk::Box,
    selected_tags: Rc<RefCell<Vec<crate::state::TagFilter>>>,
    suspended_filters: Rc<RefCell<Vec<crate::state::TagFilter>>>,
    source_roots_state: Rc<RefCell<Vec<(i64, String)>>>,
    settings_refresh: RefreshCell,
    settings_status_cb: crate::ui::settings::StatusCb,
    grid_refresh_cb: RefreshCell,
    query_generation: Rc<RefCell<crate::ui::model::QueryGeneration>>,
    hydration_generation: Rc<RefCell<u64>>,
    ui_state: Rc<RefCell<crate::state::UiState>>,
    pending_anchor: Rc<RefCell<Option<PendingAnchor>>>,
    grid_view_ref: Rc<RefCell<Option<gtk::GridView>>>,
    vadj_ref: Rc<RefCell<Option<gtk::Adjustment>>>,
    root_stack: gtk::Stack,
    has_roots_state: Rc<RefCell<bool>>,
    group_title: gtk::Label,
    update_status_line: Rc<dyn Fn()>,
    header_controls: HeaderControls,
    scan_error_button: gtk::Button,
    scan_error_paths: Rc<RefCell<Vec<String>>>,
    backend_warning: Rc<RefCell<Option<String>>>,
    offline_banner_label: gtk::Label,
    critical_banner_label: gtk::Label,
    indexing_banner_label: gtk::Label,
    status_banner_stack: gtk::Stack,
    banner_state: Rc<RefCell<BannerState>>,
    app_tx: tokio::sync::mpsc::Sender<crate::events::AppEvent>,
    app_for_fatal: adw::Application,
}

fn spawn_event_loop(mut ui_rx: tokio::sync::mpsc::Receiver<UiEvent>, ctx: EventLoopCtx) {
    let mut is_first_fetch = true;
    glib::MainContext::default().spawn_local(async move {
        while let Some(event) = ui_rx.recv().await {
            match event {
                UiEvent::ThumbnailReady(media_id, thumb_path, duration) => {
                    for i in 0..ctx.list_store.n_items() {
                        if let Some(item) =
                            ctx.list_store.item(i).and_downcast::<crate::ui::model::MediaItem>()
                            && item.property::<i64>("id") == media_id
                        {
                            item.set_property("thumbnail-path", &thumb_path);
                            if let Some(d) = duration {
                                item.set_property("duration-secs", d);
                            }
                            break;
                        }
                    }
                }
                UiEvent::ThumbnailDecoded(decoded) => {
                    let media_id = decoded.media_id;
                    let path = decoded.path.clone();
                    if ctx.thumbnail_memory_cache.borrow_mut().insert(decoded) {
                        for i in 0..ctx.list_store.n_items() {
                            if let Some(item) =
                                ctx.list_store.item(i).and_downcast::<crate::ui::model::MediaItem>()
                                && item.property::<i64>("id") == media_id
                                && item.property::<String>("thumbnail-path") == path
                            {
                                item.notify("thumbnail-path");
                                break;
                            }
                        }
                    }
                }
                UiEvent::ThumbnailsEvicted(media_ids) => {
                    for i in 0..ctx.list_store.n_items() {
                        if let Some(item) =
                            ctx.list_store.item(i).and_downcast::<crate::ui::model::MediaItem>()
                            && media_ids.contains(&item.property::<i64>("id"))
                        {
                            item.set_property("thumbnail-path", "");
                        }
                    }
                }
                UiEvent::ScanStarted => {
                    ctx.indexing_banner_label.set_text("Indexing media… 0 files found");
                    ctx.banner_state.borrow_mut().indexing = true;
                    update_status_banner_stack(&ctx.status_banner_stack, &ctx.banner_state.borrow());
                }
                UiEvent::ScanProgress(count) => {
                    ctx.indexing_banner_label
                        .set_text(&format!("Indexing media… {count} files found"));
                }
                UiEvent::ScanCompleted(_count, _paths) => {
                    ctx.banner_state.borrow_mut().indexing = false;
                    update_status_banner_stack(&ctx.status_banner_stack, &ctx.banner_state.borrow());
                    *ctx.backend_warning.borrow_mut() = None;
                    ctx.app_tx.send_critical(crate::events::AppEvent::FetchScanErrors);
                    ctx.app_tx.send_critical(crate::events::AppEvent::FetchData);
                }
                UiEvent::ScanErrorPaths(paths) => {
                    let count = paths.len();
                    *ctx.scan_error_paths.borrow_mut() = paths;
                    if ctx.backend_warning.borrow().is_none() {
                        ctx.scan_error_button.set_visible(count > 0);
                    }
                }
                UiEvent::BackendWarning(message) => {
                    if let Some(cb) = ctx.settings_status_cb.borrow().as_ref() {
                        cb(crate::ui::settings::StatusArea::Maintenance, message.clone());
                    }
                    ctx.scan_error_button.set_visible(true);
                    *ctx.backend_warning.borrow_mut() = Some(message);
                }
                UiEvent::RecoverableCritical(message) => {
                    let mut state = ctx.banner_state.borrow_mut();
                    state.critical = message.is_some();
                    if let Some(message) = message {
                        ctx.critical_banner_label.set_text(&message);
                    }
                    update_status_banner_stack(&ctx.status_banner_stack, &state);
                }
                UiEvent::SettingsError(message) => {
                    if let Some(cb) = ctx.settings_status_cb.borrow().as_ref() {
                        cb(crate::ui::settings::StatusArea::Source, message);
                    } else {
                        *ctx.backend_warning.borrow_mut() = Some(message);
                        ctx.scan_error_button.set_visible(true);
                    }
                }
                UiEvent::TagsUpdated(tags) => {
                    crate::ui::sidebar::populate_tag_rows(
                        &ctx.tag_list_box,
                        &ctx.tags,
                        &tags,
                        &ctx.source_roots_state.borrow(),
                    );
                    reapply_active_rows(&ctx.tag_list_box, &ctx.tags, &ctx.selected_tags);
                    ctx.no_tags_label.set_visible(tags.is_empty());
                    if let Some(cb) = ctx.grid_refresh_cb.borrow().as_ref() {
                        cb();
                    }
                }
                UiEvent::MediaAdded(_) | UiEvent::MediaRemoved(_) => {
                    if let Some(cb) = ctx.grid_refresh_cb.borrow().as_ref() {
                        cb();
                    }
                }
                UiEvent::QueryChunk { generation, items } => {
                    if !ctx.query_generation.borrow().is_current(generation) {
                        continue;
                    }
                    append_items(&ctx.list_store, items);
                    if ctx.list_store.n_items() > 0 {
                        ctx.root_stack.set_visible_child_name("grid");
                    }
                    try_restore_anchor(&ctx.pending_anchor, &ctx.grid_view_ref, &ctx.vadj_ref, &ctx.ui_state);
                }
                UiEvent::QueryResult(media, _total, generation) => {
                    if !ctx.query_generation.borrow().is_current(generation) {
                        continue;
                    }
                    ctx.list_store.remove_all();
                    append_items(&ctx.list_store, media);
                    let has_items = ctx.list_store.n_items() > 0;
                    if has_items {
                        crate::ui::header::set_media_controls_available(&ctx.header_controls, true);
                        ctx.root_stack.set_visible_child_name("grid");
                    } else if *ctx.has_roots_state.borrow() {
                        ctx.root_stack.set_visible_child_name("no-results");
                    }
                    update_group_title(&ctx.group_title, &ctx.selected_tags.borrow());
                    (ctx.update_status_line)();
                    try_restore_anchor(&ctx.pending_anchor, &ctx.grid_view_ref, &ctx.vadj_ref, &ctx.ui_state);
                }
                UiEvent::MediaChunk { generation, items } => {
                    if generation != *ctx.hydration_generation.borrow() {
                        continue;
                    }
                    append_items(&ctx.list_store, items);
                    if ctx.list_store.n_items() > 0 {
                        crate::ui::header::set_media_controls_available(&ctx.header_controls, true);
                        if *ctx.has_roots_state.borrow() {
                            ctx.root_stack.set_visible_child_name("grid");
                        }
                    }
                }
                UiEvent::DataFetched { tags, media, roots, has_roots, generation } => {
                    *ctx.hydration_generation.borrow_mut() = generation;
                    *ctx.has_roots_state.borrow_mut() = has_roots;

                    let roots_for_state: Vec<(i64, String)> =
                        roots.iter().map(|r| (r.id, r.path.clone())).collect();
                    *ctx.source_roots_state.borrow_mut() = roots_for_state;
                    if let Some(cb) = ctx.settings_refresh.borrow().as_ref() {
                        cb();
                    }

                    crate::ui::sidebar::populate_tag_rows(
                        &ctx.tag_list_box,
                        &ctx.tags,
                        &tags,
                        &ctx.source_roots_state.borrow(),
                    );

                    reconcile_filters(&ctx, &roots, &tags, is_first_fetch);

                    ctx.no_tags_label.set_visible(tags.is_empty());
                    ctx.tips_box.set_visible(!has_roots);

                    if is_first_fetch {
                        is_first_fetch = false;
                        let anchor = ctx.ui_state.borrow().scroll_anchor.clone();
                        if anchor.media_id.is_some() {
                            let active = ctx.selected_tags.borrow().clone();
                            *ctx.pending_anchor.borrow_mut() = Some((anchor, active));
                        }
                        ctx.app_tx.send_critical(crate::events::AppEvent::FetchScanErrors);
                    }

                    ctx.list_store.remove_all();
                    append_items(&ctx.list_store, media);
                    crate::ui::header::set_media_controls_available(
                        &ctx.header_controls,
                        ctx.list_store.n_items() > 0,
                    );
                    if !has_roots {
                        ctx.root_stack.set_visible_child_name("empty");
                    } else if ctx.list_store.n_items() == 0 {
                        ctx.root_stack.set_visible_child_name("no-results");
                    } else {
                        ctx.root_stack.set_visible_child_name("grid");
                    }
                    update_group_title(&ctx.group_title, &ctx.selected_tags.borrow());
                    (ctx.update_status_line)();

                    // Converge on the database's authoritative ordering.
                    if has_roots
                        && let Some(cb) = ctx.grid_refresh_cb.borrow().as_ref()
                    {
                        cb();
                    }
                }
                UiEvent::RootsOffline(count) => {
                    if count > 0 {
                        let mut title = if count == 1 {
                            "1 source root is currently unavailable. Existing indexed items remain browsable.".to_string()
                        } else {
                            format!("{count} source roots are currently unavailable. Existing indexed items remain browsable.")
                        };
                        if !ctx.suspended_filters.borrow().is_empty() {
                            title.push_str(" Filters from offline sources are temporarily unavailable.");
                        }
                        ctx.offline_banner_label.set_text(&title);
                        ctx.banner_state.borrow_mut().offline = true;
                    } else {
                        ctx.banner_state.borrow_mut().offline = false;
                    }
                    update_status_banner_stack(&ctx.status_banner_stack, &ctx.banner_state.borrow());
                }
                UiEvent::FatalError(msg) => {
                    show_fatal_dialog(&ctx.app_for_fatal, &msg);
                }
            }
        }
    });
}

fn append_items(list_store: &gtk::gio::ListStore, items: Vec<crate::events::UiMediaItem>) {
    for item in items {
        list_store.append(&crate::ui::model::MediaItem::from(item));
    }
}

fn reapply_active_rows(
    tag_list_box: &gtk::ListBox,
    tags: &Rc<RefCell<Vec<crate::events::UiTag>>>,
    selected_tags: &Rc<RefCell<Vec<crate::state::TagFilter>>>,
) {
    let selected = selected_tags.borrow().clone();
    for (i, tag) in tags.borrow().iter().enumerate() {
        let filter = crate::ui::filter_controller::tag_filter(tag);
        if let Some(row) = tag_list_box.row_at_index(i as i32) {
            if selected.contains(&filter) {
                row.add_css_class("active");
            } else {
                row.remove_css_class("active");
            }
        }
    }
}

fn reconcile_filters(
    ctx: &EventLoopCtx,
    roots: &[crate::events::UiSourceRoot],
    tags: &[crate::events::UiTag],
    is_first_fetch: bool,
) {
    let roots_map: std::collections::HashMap<i64, crate::state::RootStatus> = roots
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
        .filter(|t| roots_map.get(&t.source_root_id) == Some(&crate::state::RootStatus::Online))
        .map(|t| (t.source_root_id, t.relative_folder_path.clone()))
        .collect();

    let persisted = if is_first_fetch {
        ctx.ui_state.borrow().active_tags.clone()
    } else {
        let mut set = ctx.selected_tags.borrow().clone();
        for filter in ctx.suspended_filters.borrow().iter() {
            if !set.contains(filter) {
                set.push(filter.clone());
            }
        }
        set
    };
    let reconciled = crate::state::reconcile_tag_filters(&persisted, &roots_map, &online_tags);
    let active = reconciled.active.clone();
    *ctx.suspended_filters.borrow_mut() = reconciled.suspended.clone();
    ctx.ui_state.borrow_mut().active_tags = reconciled.to_persist();
    *ctx.selected_tags.borrow_mut() = active;
    reapply_active_rows(&ctx.tag_list_box, &ctx.tags, &ctx.selected_tags);
}

fn update_group_title(label: &gtk::Label, selected_tags: &[crate::state::TagFilter]) {
    if selected_tags.is_empty() {
        label.set_text("Library");
    } else {
        let names: Vec<&str> = selected_tags
            .iter()
            .map(|t| t.display_name.as_str())
            .collect();
        label.set_text(&names.join(", "));
    }
}

/// A-6: resolve the pending startup anchor against the freshly published result.
fn try_restore_anchor(
    pending_anchor: &Rc<RefCell<Option<PendingAnchor>>>,
    grid_view_ref: &Rc<RefCell<Option<gtk::GridView>>>,
    vadj_ref: &Rc<RefCell<Option<gtk::Adjustment>>>,
    ui_state: &Rc<RefCell<crate::state::UiState>>,
) {
    if pending_anchor.borrow().is_none() {
        return;
    }
    let (grid, vadj) = {
        let grid = grid_view_ref.borrow();
        let vadj = vadj_ref.borrow();
        match (grid.as_ref(), vadj.as_ref()) {
            (Some(grid), Some(vadj)) => (grid.clone(), vadj.clone()),
            _ => return,
        }
    };
    let pending = pending_anchor.clone();
    let ui_state = ui_state.clone();
    glib::idle_add_local_once(move || {
        let Some((anchor, hash_tags)) = pending.borrow().clone() else {
            return;
        };
        let Some(model) = grid.model() else {
            return;
        };
        let ordered = ordered_media_ids(&model);
        let Some(index) = anchor.resolve(&ordered) else {
            return;
        };
        *pending.borrow_mut() = None;

        let zoom = ui_state.borrow().zoom_level.round() as i32;
        let width = cell_width_for_zoom(zoom);
        let mut grid_w = grid.width();
        if grid_w <= 0 {
            grid_w = std::cmp::max(100, ui_state.borrow().window_width - 250);
        }
        let columns = std::cmp::max(1, (grid_w + GRID_ROW_SPACING) / (width + GRID_ROW_SPACING));
        let row = index as i32 / columns;
        let row_top = (row * (width + GRID_ROW_SPACING)) as f64;

        let current_hash = {
            let s = ui_state.borrow();
            crate::state::ScrollAnchor::context_hash(&s.sort_order, &hash_tags, &s.tag_filter_mode)
        };
        let offset = if current_hash == anchor.context_hash {
            anchor.offset_within_cell
        } else {
            0.0
        };
        vadj.set_value(row_top + offset);
    });
}

fn install_scroll_anchor_capture(
    vadj: &gtk::Adjustment,
    grid_view: &gtk::GridView,
    ui_state: &Rc<RefCell<crate::state::UiState>>,
    sort_radios: &[gtk::CheckButton],
    match_all_radio: &gtk::CheckButton,
    selected_tags: &Rc<RefCell<Vec<crate::state::TagFilter>>>,
) {
    let ui_state = ui_state.clone();
    let grid_view = grid_view.clone();
    let sort_radios = sort_radios.to_vec();
    let match_all_radio = match_all_radio.clone();
    let selected_tags = selected_tags.clone();
    let scroll_timeout: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));

    vadj.connect_value_changed(move |adj| {
        let val = adj.value();
        let ui_state = ui_state.clone();
        let grid = grid_view.clone();
        let sort_radios = sort_radios.clone();
        let match_all_radio = match_all_radio.clone();
        let selected_tags = selected_tags.clone();
        if let Some(id) = scroll_timeout.borrow_mut().take() {
            id.remove();
        }
        let scroll_timeout_clone = scroll_timeout.clone();
        let new_id = glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
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
            *scroll_timeout_clone.borrow_mut() = None;
            glib::ControlFlow::Break
        });
        *scroll_timeout.borrow_mut() = Some(new_id);
    });
}

fn install_window_keys(
    window: &adw::ApplicationWindow,
    viewer: Rc<crate::ui::viewer::Viewer>,
    search_entry: gtk::SearchEntry,
    apply_thumb_size: Rc<dyn Fn(usize)>,
) {
    let key_controller = gtk::EventControllerKey::new();
    let window_clone = window.clone();
    key_controller.connect_key_pressed(move |_, keyval, _, state| {
        // F1 / Ctrl+? → shortcuts help.
        if keyval == gtk::gdk::Key::F1
            || ((keyval == gtk::gdk::Key::question || keyval == gtk::gdk::Key::slash)
                && state.contains(gtk::gdk::ModifierType::CONTROL_MASK))
        {
            crate::ui::shortcuts::show_shortcuts_window(&window_clone);
            return glib::Propagation::Stop;
        }
        // Ctrl+1..5 → thumbnail size.
        if state.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
            let idx = match keyval {
                gtk::gdk::Key::_1 => Some(0),
                gtk::gdk::Key::_2 => Some(1),
                gtk::gdk::Key::_3 => Some(2),
                gtk::gdk::Key::_4 => Some(3),
                gtk::gdk::Key::_5 => Some(4),
                _ => None,
            };
            if let Some(idx) = idx {
                apply_thumb_size(idx);
                return glib::Propagation::Stop;
            }
        }
        // `/` focuses search when the viewer is closed and no text widget has
        // focus.
        if keyval == gtk::gdk::Key::slash
            && !viewer.is_open()
            && !gtk::prelude::GtkWindowExt::focus(&window_clone)
                .map(|w| w.downcast_ref::<gtk::Text>().is_some())
                .unwrap_or(false)
        {
            search_entry.grab_focus();
            return glib::Propagation::Stop;
        }
        if viewer.is_open() && keyval == gtk::gdk::Key::Escape {
            viewer.handle_escape();
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    window.add_controller(key_controller);
}

#[allow(clippy::too_many_arguments)]
fn install_close_persistence(
    window: &adw::ApplicationWindow,
    app_state: Arc<Mutex<crate::state::AppState>>,
    db: Arc<crate::db::Database>,
    ui_state: Rc<RefCell<crate::state::UiState>>,
    sort_radios: Vec<gtk::CheckButton>,
    match_all_radio: gtk::CheckButton,
    selected_tags: Rc<RefCell<Vec<crate::state::TagFilter>>>,
    suspended_filters: Rc<RefCell<Vec<crate::state::TagFilter>>>,
) {
    window.connect_close_request(move |win| {
        if let Ok(mut state) = app_state.lock() {
            state.ui.window_width = win.width();
            state.ui.window_height = win.height();
            state.ui.window_maximized = win.is_maximized();
            state.ui.zoom_level = ui_state.borrow().zoom_level;
            state.ui.scroll_anchor = ui_state.borrow().scroll_anchor.clone();
            state.ui.tag_filter_mode = if match_all_radio.is_active() {
                "AND".to_string()
            } else {
                "OR".to_string()
            };
            state.ui.sort_order = active_sort_order(&sort_radios);
            let mut active_tags = selected_tags.borrow().clone();
            active_tags.extend(suspended_filters.borrow().iter().cloned());
            state.ui.active_tags = active_tags;
            let _ = state.save(&db);
        }
        glib::Propagation::Proceed
    });
}

fn build_empty_page() -> (gtk::Box, gtk::Button) {
    let page = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .css_classes(["empty-state"])
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .spacing(24)
        .build();
    page.append(
        &gtk::Image::builder()
            .icon_name("weather-clear-night-symbolic")
            .pixel_size(96)
            .css_classes(["placeholder-illustration"])
            .build(),
    );
    let text = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .halign(gtk::Align::Center)
        .spacing(8)
        .build();
    text.append(
        &gtk::Label::builder()
            .label("Add folders to start browsing your media.")
            .css_classes(["title-1"])
            .justify(gtk::Justification::Center)
            .build(),
    );
    text.append(
        &gtk::Label::builder()
            .label("Vesper indexes media from your folders so you can browse them beautifully and privately.")
            .css_classes(["body"])
            .wrap(true)
            .max_width_chars(48)
            .justify(gtk::Justification::Center)
            .build(),
    );
    text.append(
        &gtk::Label::builder()
            .label("Nothing is moved or modified.")
            .css_classes(["dim-label", "caption"])
            .build(),
    );
    page.append(&text);

    let add_btn = gtk::Button::builder()
        .css_classes(["suggested-action", "pill"])
        .halign(gtk::Align::Center)
        .build();
    let add_content = gtk::Box::builder()
        .spacing(8)
        .halign(gtk::Align::Center)
        .build();
    add_content.append(&gtk::Image::from_icon_name("list-add-symbolic"));
    add_content.append(&gtk::Label::new(Some("Add Source Root")));
    add_btn.set_child(Some(&add_content));
    add_btn.update_property(&[gtk::accessible::Property::Label("Add Source Root")]);
    page.append(&add_btn);

    page.append(
        &gtk::Button::builder()
            .label("Learn more about source roots")
            .css_classes(["flat", "link"])
            .halign(gtk::Align::Center)
            .build(),
    );
    (page, add_btn)
}

fn build_no_results_page() -> (gtk::Box, gtk::Button, gtk::Button) {
    let page = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .css_classes(["no-results"])
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .spacing(16)
        .build();
    page.append(
        &gtk::Image::builder()
            .icon_name("edit-find-symbolic")
            .pixel_size(96)
            .css_classes(["placeholder-illustration"])
            .build(),
    );
    page.append(
        &gtk::Label::builder()
            .label("No media matches your filters.")
            .css_classes(["title-1"])
            .build(),
    );
    page.append(
        &gtk::Label::builder()
            .label("Try adjusting your search or removing folder tags and filters.")
            .css_classes(["body"])
            .wrap(true)
            .max_width_chars(42)
            .justify(gtk::Justification::Center)
            .build(),
    );
    let clear_btn = gtk::Button::builder()
        .label("Clear search")
        .css_classes(["suggested-action", "pill"])
        .halign(gtk::Align::Center)
        .build();
    page.append(&clear_btn);
    let review_btn = gtk::Button::builder()
        .label("Review folder tags")
        .css_classes(["flat", "link"])
        .halign(gtk::Align::Center)
        .build();
    page.append(&review_btn);
    (page, clear_btn, review_btn)
}

fn show_scan_error_popover(
    button: &gtk::Button,
    backend_warning: &Rc<RefCell<Option<String>>>,
    scan_error_paths: &Rc<RefCell<Vec<String>>>,
) {
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .css_classes(["scan-error-popover"])
        .spacing(8)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    content.append(
        &gtk::Label::builder()
            .label("Some files could not be scanned")
            .css_classes(["heading"])
            .halign(gtk::Align::Start)
            .build(),
    );
    let (body, paths) = if let Some(message) = backend_warning.borrow().clone() {
        (message, Vec::new())
    } else {
        (
            "Vesper couldn't access a few files or folders. They may be offline, unsupported, or require permissions.".to_string(),
            scan_error_paths.borrow().clone(),
        )
    };
    content.append(
        &gtk::Label::builder()
            .label(&body)
            .wrap(true)
            .xalign(0.0)
            .max_width_chars(48)
            .build(),
    );
    if !paths.is_empty() {
        let list = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .build();
        for path in paths {
            list.append(
                &gtk::Label::builder()
                    .label(&path)
                    .selectable(true)
                    .wrap(true)
                    .xalign(0.0)
                    .css_classes(["dim-label", "caption"])
                    .build(),
            );
        }
        content.append(
            &gtk::ScrolledWindow::builder()
                .child(&list)
                .min_content_width(320)
                .max_content_height(240)
                .propagate_natural_height(true)
                .build(),
        );
    }
    let popover = gtk::Popover::builder()
        .autohide(true)
        .child(&content)
        .build();
    popover.set_parent(button);
    popover.connect_closed(|popover| popover.unparent());
    popover.popup();
}

fn show_fatal_dialog(app: &adw::Application, msg: &str) {
    eprintln!("Fatal error: {msg}");
    let dialog = adw::MessageDialog::builder()
        .heading("Unexpected Error")
        .body("An unexpected error occurred. The application will close.")
        .build();
    dialog.add_response("close", "Close");
    let app_clone = app.clone();
    dialog.connect_response(None, move |_, _| {
        if let Some(win) = app_clone.active_window() {
            win.close();
        }
        app_clone.quit();
        std::process::exit(1);
    });
    if let Some(win) = app.active_window() {
        dialog.set_transient_for(Some(&win));
    }
    dialog.present();
}

#[cfg(test)]
mod tests {
    use super::{BannerPriority, BannerState, banner_priority, cell_width_for_zoom};

    #[test]
    fn banner_priority_orders_critical_offline_then_indexing() {
        let mut state = BannerState {
            indexing: true,
            ..BannerState::default()
        };
        assert_eq!(banner_priority(&state), BannerPriority::Indexing);
        state.offline = true;
        assert_eq!(banner_priority(&state), BannerPriority::Offline);
        state.critical = true;
        assert_eq!(banner_priority(&state), BannerPriority::Critical);
    }

    #[test]
    fn five_thumbnail_sizes_increase_monotonically() {
        let widths: Vec<i32> = (0..5).map(cell_width_for_zoom).collect();
        assert!(widths.windows(2).all(|w| w[0] < w[1]));
    }
}
