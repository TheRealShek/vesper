//! Settings (an allowed dialog exception, Vision §2): a custom two-pane window
//! (Product §9 / mockup 06) — a left navigation with an info card and a right
//! scrollable content of card sections and column tables. Navigation is fixed:
//! Source Roots, Ignore Rules, Appearance, Thumbnails, Advanced, About Vesper
//! (no "Metadata" page in v1). The ignore model is a pattern list; the table is
//! a presentation of that list and never changes matching semantics.

use crate::events::AppEvent;
use crate::events::ChannelSendExt;
use crate::state::BackendState;
use libadwaita as adw;
use libadwaita::gtk::{self, glib, prelude::*};
use libadwaita::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

type RefreshCb = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

/// A zero-argument action bound to a row's overflow-menu entry.
type MenuAction = Rc<dyn Fn()>;

#[derive(Debug, Clone, Copy)]
pub enum StatusArea {
    Source,
    Maintenance,
}

pub type StatusCb = Rc<RefCell<Option<Rc<dyn Fn(StatusArea, String)>>>>;

#[derive(Debug, Clone, Copy)]
enum MaintenanceAction {
    Rescan,
    RegenerateThumbnails,
    RebuildIndex,
}

fn maintenance_event(action: MaintenanceAction) -> AppEvent {
    match action {
        MaintenanceAction::Rescan => AppEvent::RescanRoots,
        MaintenanceAction::RegenerateThumbnails => AppEvent::RegenerateThumbnails,
        MaintenanceAction::RebuildIndex => AppEvent::RebuildLibraryIndex,
    }
}

fn remove_source_event(root_id: i64) -> AppEvent {
    AppEvent::RemoveSourceRoot(root_id)
}

fn ignore_rules_from_text(text: &str) -> Vec<String> {
    text.lines().map(str::to_string).collect()
}

fn validated_ignore_rules(text: &str) -> Result<Vec<String>, String> {
    let rules = ignore_rules_from_text(text);
    crate::index::ignore_rules::validate_global_patterns(&rules)
        .map(|_| rules)
        .map_err(|errors| format_ignore_validation_errors(&errors))
}

fn default_rules_text() -> String {
    crate::index::ignore_rules::DEFAULT_GLOBAL_PATTERNS.join("\n")
}

fn format_ignore_validation_errors(
    errors: &[crate::index::ignore_rules::IgnoreValidationError],
) -> String {
    errors
        .iter()
        .map(|error| match error.line {
            Some(line) => format!("{}, line {line}: {}", error.source, error.message),
            None => format!("{}: {}", error.source, error.message),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The six fixed navigation entries (Product §9): label + symbolic icon.
const NAV_ENTRIES: [(&str, &str); 6] = [
    ("Source Roots", "folder-symbolic"),
    ("Ignore Rules", "action-unavailable-symbolic"),
    ("Appearance", "preferences-desktop-appearance-symbolic"),
    ("Thumbnails", "view-grid-symbolic"),
    ("Advanced", "emblem-system-symbolic"),
    ("About Vesper", "help-about-symbolic"),
];

pub fn show(
    parent: &impl IsA<gtk::Window>,
    app_state: std::sync::Arc<std::sync::Mutex<crate::state::AppState>>,
    app_tx: tokio::sync::mpsc::Sender<AppEvent>,
    source_roots: Rc<RefCell<Vec<(i64, String)>>>,
    refresh_cb: RefreshCb,
    status_cb: StatusCb,
) {
    let backend_state: BackendState = match app_state.lock() {
        Ok(state) => state.backend.clone(),
        Err(_) => return,
    };

    let window = adw::Window::builder()
        .transient_for(parent)
        .modal(true)
        .title("Settings")
        .default_width(1040)
        .default_height(760)
        .build();
    window.connect_close_request({
        let cb = refresh_cb.clone();
        let status_cb = status_cb.clone();
        move |_| {
            *cb.borrow_mut() = None;
            *status_cb.borrow_mut() = None;
            glib::Propagation::Proceed
        }
    });

    let source_status = build_status_label();
    let maintenance_status = build_status_label();

    // ── Right content: one scrollable column of card sections ─────────────
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(24)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();

    let mut sections: Vec<gtk::Widget> = Vec::new();
    let source_section = build_source_roots_section(
        &window,
        &app_state,
        &app_tx,
        &source_roots,
        &refresh_cb,
        &source_status,
        &backend_state,
    );
    let ignore_section = build_ignore_rules_section(&app_state, &app_tx, &backend_state);
    let appearance_section = build_note_section(
        "Appearance",
        "Vesper follows your system light/dark preference and falls back to dark when the system expresses none.",
    );
    let thumbnails_section = build_note_section(
        "Thumbnails",
        "Vesper stores one square 256px grid thumbnail per item under your cache directory (up to 5 GB, least-recently-used eviction).",
    );
    let advanced_section = build_advanced_section(&app_tx, &maintenance_status);
    let about_section = build_note_section(
        "About Vesper",
        "Vesper — a quiet, media-first gallery for Linux. Your existing folder structure is the organizational system. It never moves, edits, or uploads your files.",
    );
    for s in [
        &source_section,
        &ignore_section,
        &appearance_section,
        &thumbnails_section,
        &advanced_section,
        &about_section,
    ] {
        content.append(s);
        sections.push(s.clone().upcast());
    }

    let content_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .hexpand(true)
        .vexpand(true)
        .child(&content)
        .build();

    // ── Left navigation panel ─────────────────────────────────────────────
    let nav_panel = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .css_classes(["settings-nav"])
        .build();
    nav_panel.append(
        &gtk::Label::builder()
            .label("Settings")
            .css_classes(["title-2"])
            .halign(gtk::Align::Start)
            .margin_start(16)
            .margin_top(16)
            .margin_bottom(8)
            .build(),
    );

    let nav_list = gtk::ListBox::builder()
        .css_classes(["navigation-sidebar"])
        .selection_mode(gtk::SelectionMode::Single)
        .margin_start(8)
        .margin_end(8)
        .build();
    for (label, icon) in NAV_ENTRIES {
        let row_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .build();
        row_box.append(&gtk::Image::from_icon_name(icon));
        row_box.append(
            &gtk::Label::builder()
                .label(label)
                .halign(gtk::Align::Start)
                .hexpand(true)
                .build(),
        );
        let row = gtk::ListBoxRow::builder()
            .child(&row_box)
            .css_classes(["settings-nav-row"])
            .build();
        nav_list.append(&row);
    }
    nav_panel.append(&nav_list);

    // Spacer + info card at the bottom of the nav.
    nav_panel.append(&gtk::Box::builder().vexpand(true).build());
    let info_card = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .css_classes(["settings-info-card"])
        .spacing(8)
        .margin_start(12)
        .margin_end(12)
        .margin_bottom(12)
        .build();
    info_card.append(&gtk::Image::from_icon_name("help-about-symbolic"));
    info_card.append(
        &gtk::Label::builder()
            .label("Source roots define where Vesper looks for your media. Ignore rules let you skip files and folders you don't want to include.")
            .css_classes(["caption", "dim-label"])
            .wrap(true)
            .xalign(0.0)
            .build(),
    );
    info_card.append(
        &gtk::Button::builder()
            .label("Learn more")
            .css_classes(["flat", "link"])
            .halign(gtk::Align::Start)
            .build(),
    );
    nav_panel.append(&info_card);

    // Nav click scrolls the content to the matching section and highlights it.
    nav_list.connect_row_selected({
        let content_scroll = content_scroll.clone();
        let content = content.clone();
        let sections = sections.clone();
        move |_, row| {
            let Some(row) = row else { return };
            let idx = row.index();
            let Some(section) = sections.get(idx as usize).cloned() else {
                return;
            };
            // Defer so allocation is settled before measuring the offset.
            let content = content.clone();
            let content_scroll = content_scroll.clone();
            glib::idle_add_local_once(move || {
                if let Some(point) =
                    section.compute_point(&content, &gtk::graphene::Point::new(0.0, 0.0))
                {
                    content_scroll
                        .vadjustment()
                        .set_value((point.y() as f64 - 12.0).max(0.0));
                }
            });
        }
    });
    nav_list.select_row(nav_list.row_at_index(0).as_ref());

    let split = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    split.append(&nav_panel);
    split.append(&content_scroll);

    let toolbar = adw::ToolbarView::builder().content(&split).build();
    toolbar.add_top_bar(
        &adw::HeaderBar::builder()
            .title_widget(&adw::WindowTitle::new("Settings", ""))
            .build(),
    );
    window.set_content(Some(&toolbar));

    *status_cb.borrow_mut() = Some(Rc::new(move |area, message| {
        let label = match area {
            StatusArea::Source => &source_status,
            StatusArea::Maintenance => &maintenance_status,
        };
        label.set_label(&message);
        label.set_visible(true);
    }));

    window.present();
}

fn build_status_label() -> gtk::Label {
    let label = gtk::Label::builder()
        .css_classes(["error"])
        .halign(gtk::Align::Start)
        .wrap(true)
        .selectable(true)
        .visible(false)
        .build();
    label.update_property(&[gtk::accessible::Property::Label("Status")]);
    label
}

/// A titled card section: title + description on the left, optional action
/// buttons on the right, then a body.
fn section_card(
    title: &str,
    description: &str,
    actions: Option<&gtk::Widget>,
) -> (gtk::Box, gtk::Box) {
    let card = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .css_classes(["settings-section"])
        .spacing(12)
        .build();

    let header = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .build();
    let titles = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .hexpand(true)
        .spacing(2)
        .build();
    titles.append(
        &gtk::Label::builder()
            .label(title)
            .css_classes(["title-2"])
            .halign(gtk::Align::Start)
            .build(),
    );
    if !description.is_empty() {
        titles.append(
            &gtk::Label::builder()
                .label(description)
                .css_classes(["caption", "dim-label"])
                .halign(gtk::Align::Start)
                .wrap(true)
                .xalign(0.0)
                .build(),
        );
    }
    header.append(&titles);
    if let Some(actions) = actions {
        actions.set_valign(gtk::Align::Start);
        header.append(actions);
    }
    card.append(&header);

    let body = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(0)
        .build();
    card.append(&body);
    (card, body)
}

fn footer_note(text: &str) -> gtk::Label {
    gtk::Label::builder()
        .label(text)
        .css_classes(["caption", "dim-label"])
        .halign(gtk::Align::Start)
        .wrap(true)
        .xalign(0.0)
        .margin_top(4)
        .build()
}

fn column_header(cols: &[(&str, bool)]) -> gtk::Box {
    // cols: (label, expand). A drag-handle spacer precedes when needed.
    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .css_classes(["settings-col-header"])
        .spacing(12)
        .build();
    for (label, expand) in cols {
        let l = gtk::Label::builder()
            .label(*label)
            .css_classes(["caption", "dim-label"])
            .halign(gtk::Align::Start)
            .hexpand(*expand)
            .build();
        if !*expand {
            l.set_width_request(120);
        }
        row.append(&l);
    }
    row
}

fn overflow_menu(items: &[(&str, MenuAction)]) -> gtk::MenuButton {
    let menu_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(6)
        .margin_end(6)
        .build();
    let popover = gtk::Popover::builder().child(&menu_box).build();
    for (label, action) in items {
        let btn = gtk::Button::builder()
            .css_classes(["flat"])
            .child(
                &gtk::Label::builder()
                    .label(*label)
                    .halign(gtk::Align::Start)
                    .hexpand(true)
                    .build(),
            )
            .build();
        let action = action.clone();
        let popover = popover.clone();
        btn.connect_clicked(move |_| {
            popover.popdown();
            action();
        });
        menu_box.append(&btn);
    }
    gtk::MenuButton::builder()
        .icon_name("view-more-symbolic")
        .css_classes(["flat"])
        .valign(gtk::Align::Center)
        .popover(&popover)
        .build()
}

#[allow(clippy::too_many_arguments)]
fn build_source_roots_section(
    window: &adw::Window,
    app_state: &std::sync::Arc<std::sync::Mutex<crate::state::AppState>>,
    app_tx: &tokio::sync::mpsc::Sender<AppEvent>,
    source_roots: &Rc<RefCell<Vec<(i64, String)>>>,
    refresh_cb: &RefreshCb,
    source_status: &gtk::Label,
    backend_state: &BackendState,
) -> gtk::Box {
    let actions = gtk::Box::builder().spacing(8).build();
    let add_btn = pill_button("list-add-symbolic", "Add Source Root");
    let remove_btn = pill_button("list-remove-symbolic", "Remove");
    actions.append(&add_btn);
    actions.append(&remove_btn);
    add_btn.connect_clicked({
        let app_tx = app_tx.clone();
        let window = window.clone();
        move |_| pick_source_root(&window, &app_tx)
    });

    let (card, body) = section_card(
        "Source Roots",
        "Folders and drives that Vesper monitors for media.",
        Some(actions.upcast_ref::<gtk::Widget>()),
    );

    body.append(&column_header(&[
        ("Location", true),
        ("Type", false),
        ("Items", false),
    ]));

    let rows = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .css_classes(["settings-table"])
        .build();
    body.append(&rows);
    body.append(source_status);
    body.append(&footer_note(
        "Changes are applied automatically. You can add, remove, or reorder roots at any time.",
    ));

    // The last-selected row id (for the header "Remove" button).
    let selected_id: Rc<RefCell<Option<i64>>> = Rc::new(RefCell::new(None));
    remove_btn.connect_clicked({
        let selected_id = selected_id.clone();
        let source_roots = source_roots.clone();
        let app_tx = app_tx.clone();
        let window = window.clone();
        move |_| {
            let id = (*selected_id.borrow())
                .or_else(|| source_roots.borrow().first().map(|(id, _)| *id));
            if let Some(id) = id {
                let path = source_roots
                    .borrow()
                    .iter()
                    .find(|(rid, _)| *rid == id)
                    .map(|(_, p)| p.clone())
                    .unwrap_or_default();
                confirm_remove_root(&window, &app_tx, id, &path);
            }
        }
    });

    let refresh_closure: Rc<dyn Fn()> = Rc::new({
        let rows = rows.clone();
        let source_roots = source_roots.clone();
        let app_tx = app_tx.clone();
        let window = window.clone();
        let selected_id = selected_id.clone();
        move || {
            while let Some(child) = rows.first_child() {
                rows.remove(&child);
            }
            let roots = source_roots.borrow();
            if roots.is_empty() {
                rows.append(
                    &gtk::Label::builder()
                        .label("No source roots configured.")
                        .css_classes(["dim-label"])
                        .halign(gtk::Align::Start)
                        .margin_top(8)
                        .margin_bottom(8)
                        .build(),
                );
            }
            for (id, path) in roots.iter() {
                rows.append(&source_root_row(*id, path, &app_tx, &window, &selected_id));
            }
        }
    });
    *refresh_cb.borrow_mut() = Some(refresh_closure.clone());
    refresh_closure();

    // Root-as-tag toggle (lives under Source Roots, Product §9).
    let tag_row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .css_classes(["source-root-row"])
        .spacing(12)
        .margin_top(8)
        .build();
    let tag_labels = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .hexpand(true)
        .build();
    tag_labels.append(
        &gtk::Label::builder()
            .label("Use source root name as a tag")
            .halign(gtk::Align::Start)
            .build(),
    );
    tag_labels.append(
        &gtk::Label::builder()
            .label("Include the top-level folder name itself as a tag.")
            .css_classes(["caption", "dim-label"])
            .halign(gtk::Align::Start)
            .build(),
    );
    let tag_switch = gtk::Switch::builder()
        .valign(gtk::Align::Center)
        .active(backend_state.root_as_tag)
        .build();
    tag_switch.connect_active_notify({
        let app_state = app_state.clone();
        let app_tx = app_tx.clone();
        move |switch| {
            let mut state = match app_state.lock() {
                Ok(state) => state.backend.clone(),
                Err(_) => return,
            };
            state.root_as_tag = switch.is_active();
            app_tx.send_critical(AppEvent::UpdateSettings(state));
            app_tx.send_critical(AppEvent::RescanRoots);
        }
    });
    tag_row.append(&tag_labels);
    tag_row.append(&tag_switch);
    card.append(&tag_row);

    card
}

fn source_root_row(
    id: i64,
    path: &str,
    app_tx: &tokio::sync::mpsc::Sender<AppEvent>,
    window: &adw::Window,
    selected_id: &Rc<RefCell<Option<i64>>>,
) -> gtk::Box {
    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .css_classes(["source-root-row"])
        .spacing(12)
        .build();

    row.append(&gtk::Image::from_icon_name("folder-symbolic"));

    let location = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .hexpand(true)
        .valign(gtk::Align::Center)
        .build();
    location.append(
        &gtk::Label::builder()
            .label(path)
            .halign(gtk::Align::Start)
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .build(),
    );
    row.append(&location);

    let (type_label, type_icon) = root_type(path);
    let type_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .width_request(120)
        .valign(gtk::Align::Center)
        .build();
    type_box.append(&gtk::Image::from_icon_name(type_icon));
    type_box.append(
        &gtk::Label::builder()
            .label(type_label)
            .css_classes(["caption"])
            .halign(gtk::Align::Start)
            .build(),
    );
    row.append(&type_box);

    // Item count is not exposed per-root by the summary DTO.
    row.append(
        &gtk::Label::builder()
            .label("—")
            .css_classes(["dim-label", "numeric"])
            .width_request(120)
            .halign(gtk::Align::Start)
            .valign(gtk::Align::Center)
            .build(),
    );

    let remove_action: Rc<dyn Fn()> = Rc::new({
        let app_tx = app_tx.clone();
        let window = window.clone();
        let path = path.to_string();
        move || confirm_remove_root(&window, &app_tx, id, &path)
    });
    row.append(&overflow_menu(&[("Remove", remove_action)]));

    // Track the last-hovered/clicked row for the header Remove button.
    let click = gtk::GestureClick::new();
    click.connect_pressed({
        let selected_id = selected_id.clone();
        move |_, _, _, _| *selected_id.borrow_mut() = Some(id)
    });
    row.add_controller(click);

    row
}

fn build_ignore_rules_section(
    app_state: &std::sync::Arc<std::sync::Mutex<crate::state::AppState>>,
    app_tx: &tokio::sync::mpsc::Sender<AppEvent>,
    backend_state: &BackendState,
) -> gtk::Box {
    let actions = gtk::Box::builder().spacing(8).build();
    let add_rule = pill_button("list-add-symbolic", "Add Rule");
    let reset = pill_button("view-refresh-symbolic", "Reset to Defaults");
    actions.append(&add_rule);
    actions.append(&reset);

    let (card, body) = section_card(
        "Ignore Rules",
        "Patterns for files and folders that Vesper should skip.",
        Some(actions.upcast_ref::<gtk::Widget>()),
    );

    body.append(&column_header(&[
        ("Pattern", true),
        ("Description", true),
        ("Examples", false),
    ]));

    let rows = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .css_classes(["settings-table"])
        .build();
    body.append(&rows);

    let validation_error = gtk::Label::builder()
        .css_classes(["error"])
        .halign(gtk::Align::Start)
        .selectable(true)
        .wrap(true)
        .visible(false)
        .build();
    body.append(&validation_error);

    let apply = gtk::Button::builder()
        .label("Apply")
        .css_classes(["suggested-action"])
        .halign(gtk::Align::Start)
        .margin_top(8)
        .sensitive(false)
        .build();
    body.append(&apply);
    body.append(&footer_with_link(
        "Patterns are matched using glob syntax. Use * for wildcards and / to match folders.",
        "View glob reference",
    ));

    let mark_dirty: Rc<dyn Fn()> = {
        let apply = apply.clone();
        let validation_error = validation_error.clone();
        Rc::new(move || {
            apply.set_sensitive(true);
            validation_error.set_visible(false);
        })
    };

    let add_row: Rc<dyn Fn(&str)> = {
        let rows = rows.clone();
        let mark_dirty = mark_dirty.clone();
        Rc::new(move |pattern: &str| {
            rows.append(&ignore_rule_row(pattern, &rows, &mark_dirty));
        })
    };
    for pattern in &backend_state.global_ignore_rules {
        add_row(pattern);
    }

    add_rule.connect_clicked({
        let add_row = add_row.clone();
        let mark_dirty = mark_dirty.clone();
        move |_| {
            add_row("");
            mark_dirty();
        }
    });
    reset.connect_clicked({
        let rows = rows.clone();
        let add_row = add_row.clone();
        let mark_dirty = mark_dirty.clone();
        move |_| {
            while let Some(child) = rows.first_child() {
                rows.remove(&child);
            }
            for pattern in ignore_rules_from_text(&default_rules_text()) {
                add_row(&pattern);
            }
            mark_dirty();
        }
    });

    apply.connect_clicked({
        let rows = rows.clone();
        let validation_error = validation_error.clone();
        let apply = apply.clone();
        let app_state = app_state.clone();
        let app_tx = app_tx.clone();
        move |_| {
            let text = collect_patterns(&rows).join("\n");
            let rules = match validated_ignore_rules(&text) {
                Ok(rules) => rules,
                Err(message) => {
                    validation_error.set_label(&message);
                    validation_error.set_visible(true);
                    return;
                }
            };
            validation_error.set_visible(false);
            let mut state = match app_state.lock() {
                Ok(state) => state.backend.clone(),
                Err(_) => return,
            };
            state.global_ignore_rules = rules;
            app_tx.send_critical(AppEvent::UpdateSettings(state));
            app_tx.send_critical(AppEvent::RescanRoots);
            apply.set_sensitive(false);
        }
    });

    card
}

fn ignore_rule_row(
    pattern: &str,
    rows_container: &gtk::Box,
    mark_dirty: &Rc<dyn Fn()>,
) -> gtk::Box {
    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .css_classes(["ignore-rule-row"])
        .spacing(12)
        .build();

    row.append(
        &gtk::Image::builder()
            .icon_name("list-drag-handle-symbolic")
            .css_classes(["dim-label"])
            .build(),
    );

    let entry = gtk::Entry::builder()
        .text(pattern)
        .hexpand(true)
        .css_classes(["flat"])
        .valign(gtk::Align::Center)
        .build();
    entry.connect_changed({
        let mark_dirty = mark_dirty.clone();
        move |_| mark_dirty()
    });
    row.append(&entry);

    let (description, example) = rule_description(pattern);
    row.append(
        &gtk::Label::builder()
            .label(description)
            .css_classes(["caption", "dim-label"])
            .halign(gtk::Align::Start)
            .hexpand(true)
            .build(),
    );
    row.append(
        &gtk::Label::builder()
            .label(example)
            .css_classes(["caption", "dim-label", "numeric"])
            .halign(gtk::Align::Start)
            .width_request(140)
            .build(),
    );

    let delete_action: Rc<dyn Fn()> = Rc::new({
        let rows_container = rows_container.clone();
        let row_weak = row.downgrade();
        let mark_dirty = mark_dirty.clone();
        move || {
            if let Some(row) = row_weak.upgrade() {
                rows_container.remove(&row);
                mark_dirty();
            }
        }
    });
    row.append(&overflow_menu(&[("Delete", delete_action)]));

    row
}

/// Collects the current pattern list from the ignore-rule rows in order.
fn collect_patterns(rows: &gtk::Box) -> Vec<String> {
    let mut patterns = Vec::new();
    let mut child = rows.first_child();
    while let Some(row) = child {
        if let Some(entry) = row
            .downcast_ref::<gtk::Box>()
            .and_then(|b| b.first_child())
            .and_then(|c| c.next_sibling())
            .and_downcast::<gtk::Entry>()
        {
            patterns.push(entry.text().to_string());
        }
        child = row.next_sibling();
    }
    patterns
}

fn build_advanced_section(
    app_tx: &tokio::sync::mpsc::Sender<AppEvent>,
    maintenance_status: &gtk::Label,
) -> gtk::Box {
    let (card, body) = section_card(
        "Advanced",
        "Library maintenance runs in the background. Only one operation runs at a time.",
        None,
    );
    for (label, action) in [
        ("Rescan Library", MaintenanceAction::Rescan),
        (
            "Regenerate Thumbnails",
            MaintenanceAction::RegenerateThumbnails,
        ),
        ("Rebuild Library Index", MaintenanceAction::RebuildIndex),
    ] {
        let row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .css_classes(["source-root-row"])
            .spacing(12)
            .build();
        row.append(
            &gtk::Label::builder()
                .label(label)
                .halign(gtk::Align::Start)
                .hexpand(true)
                .valign(gtk::Align::Center)
                .build(),
        );
        let run = gtk::Button::builder()
            .label("Run")
            .css_classes(["flat"])
            .build();
        run.connect_clicked({
            let app_tx = app_tx.clone();
            move |_| app_tx.send_critical(maintenance_event(action))
        });
        row.append(&run);
        body.append(&row);
    }
    body.append(maintenance_status);
    card
}

fn build_note_section(title: &str, body_text: &str) -> gtk::Box {
    let (card, body) = section_card(title, "", None);
    body.append(
        &gtk::Label::builder()
            .label(body_text)
            .css_classes(["body", "dim-label"])
            .wrap(true)
            .xalign(0.0)
            .halign(gtk::Align::Start)
            .build(),
    );
    card
}

fn footer_with_link(text: &str, link: &str) -> gtk::Box {
    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .margin_top(4)
        .build();
    row.append(&footer_note(text));
    row.append(
        &gtk::Button::builder()
            .label(link)
            .css_classes(["flat", "link"])
            .halign(gtk::Align::End)
            .hexpand(true)
            .build(),
    );
    row
}

fn pill_button(icon: &str, label: &str) -> gtk::Button {
    let content = gtk::Box::builder().spacing(6).build();
    content.append(&gtk::Image::from_icon_name(icon));
    content.append(&gtk::Label::new(Some(label)));
    let button = gtk::Button::builder()
        .css_classes(["pill"])
        .child(&content)
        .build();
    button.update_property(&[gtk::accessible::Property::Label(label)]);
    button
}

/// Best-effort source-root type from the mount location (the summary DTO carries
/// no device/type field). Glyph conveys the type, per Visual §4.
fn root_type(path: &str) -> (&'static str, &'static str) {
    if path.starts_with("/media/") || path.starts_with("/run/media/") {
        ("USB Drive", "media-removable-symbolic")
    } else if path.starts_with("/mnt/") {
        ("External Drive", "drive-harddisk-symbolic")
    } else if path.starts_with("/net/") {
        ("Network Share", "network-server-symbolic")
    } else {
        ("Internal Drive", "drive-harddisk-symbolic")
    }
}

/// Description + example for a known default ignore pattern; custom patterns get
/// a generic description and echo themselves as the example.
fn rule_description(pattern: &str) -> (&'static str, String) {
    match pattern {
        ".git/" => ("Version control data", ".git/".into()),
        "node_modules/" => ("Node.js dependencies", "node_modules/".into()),
        ".Trash/" => ("Trash folders", ".Trash/".into()),
        ".cache/" => ("Application cache directories", ".cache/".into()),
        "*.tmp" => ("Temporary files", "file.tmp".into()),
        "*.part" => ("Partial downloads", "video.mp4.part".into()),
        ".DS_Store" => ("macOS folder metadata", ".DS_Store".into()),
        "Thumbs.db" => ("Windows thumbnail cache", "Thumbs.db".into()),
        "" => ("", String::new()),
        other => ("Custom rule", other.to_string()),
    }
}

fn pick_source_root(window: &impl IsA<gtk::Window>, app_tx: &tokio::sync::mpsc::Sender<AppEvent>) {
    let dialog = gtk::FileDialog::new();
    let app_tx = app_tx.clone();
    dialog.select_folder(Some(window), None::<&gtk::gio::Cancellable>, move |res| {
        if let Ok(file) = res
            && let Some(path) = file.path()
            && let Some(path_str) = path.to_str()
        {
            app_tx.send_critical(AppEvent::AddSourceRoot(path_str.to_string()));
        }
    });
}

fn confirm_remove_root(
    window: &adw::Window,
    app_tx: &tokio::sync::mpsc::Sender<AppEvent>,
    root_id: i64,
    path: &str,
) {
    let dialog = adw::MessageDialog::builder()
        .transient_for(window)
        .modal(true)
        .heading("Remove source root?")
        .body(format!(
            "Remove {path} from Vesper? Files on disk will not be changed."
        ))
        .build();
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("remove", "Remove");
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");
    dialog.set_response_appearance("remove", adw::ResponseAppearance::Destructive);
    dialog.connect_response(Some("remove"), {
        let app_tx = app_tx.clone();
        move |_, _| app_tx.send_critical(remove_source_event(root_id))
    });
    dialog.present();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignore_validation_feedback_includes_source_line_and_message() {
        let rules = ignore_rules_from_text("*.jpg\n[z-a].tmp\n*.png");
        let errors = crate::index::ignore_rules::validate_global_patterns(&rules).unwrap_err();
        let feedback = format_ignore_validation_errors(&errors);
        assert!(feedback.contains("global rules"));
        assert!(feedback.contains("line 2"));
        assert!(feedback.contains(&errors[0].message));
    }

    #[test]
    fn maintenance_buttons_map_to_events() {
        assert!(matches!(
            maintenance_event(MaintenanceAction::Rescan),
            AppEvent::RescanRoots
        ));
        assert!(matches!(
            maintenance_event(MaintenanceAction::RegenerateThumbnails),
            AppEvent::RegenerateThumbnails
        ));
        assert!(matches!(
            maintenance_event(MaintenanceAction::RebuildIndex),
            AppEvent::RebuildLibraryIndex
        ));
    }

    #[test]
    fn reset_to_defaults_matches_arch_default_set() {
        let rules = ignore_rules_from_text(&default_rules_text());
        for default in crate::index::ignore_rules::DEFAULT_GLOBAL_PATTERNS {
            assert!(rules.iter().any(|rule| rule == default));
        }
        assert!(!rules.iter().any(|rule| rule == ".*"));
    }

    #[test]
    fn ignore_apply_rejects_invalid_rules_as_one_set() {
        let result = validated_ignore_rules("*.valid\n[z-a].tmp");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("global rules, line 2:"));
    }

    #[test]
    fn confirmed_root_removal_uses_stable_root_id() {
        assert!(matches!(
            remove_source_event(42),
            AppEvent::RemoveSourceRoot(42)
        ));
    }

    #[test]
    fn known_default_patterns_have_descriptions() {
        assert_eq!(rule_description("*.tmp").0, "Temporary files");
        assert_eq!(rule_description("node_modules/").0, "Node.js dependencies");
        assert_eq!(rule_description("weird_custom").0, "Custom rule");
    }
}
