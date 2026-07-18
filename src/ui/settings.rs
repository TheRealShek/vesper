//! Settings (an allowed dialog exception, Vision §2): a preferences window with
//! the fixed navigation Source Roots, Ignore Rules, Appearance, Thumbnails,
//! Advanced, About Vesper (Product §9 — no "Metadata" page in v1). The ignore
//! model is a pattern list; the presentation never changes matching semantics.

use crate::events::AppEvent;
use crate::events::ChannelSendExt;
use crate::state::BackendState;
use libadwaita as adw;
use libadwaita::gtk::{self, glib, prelude::*};
use libadwaita::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

type RefreshCb = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

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

pub fn show(
    parent: &impl IsA<gtk::Window>,
    app_state: std::sync::Arc<std::sync::Mutex<crate::state::AppState>>,
    app_tx: tokio::sync::mpsc::Sender<AppEvent>,
    source_roots: Rc<RefCell<Vec<(i64, String)>>>,
    refresh_cb: RefreshCb,
    status_cb: StatusCb,
) {
    // Every opening reads the current saved backend state.
    let backend_state: BackendState = match app_state.lock() {
        Ok(state) => state.backend.clone(),
        Err(_) => return,
    };

    let window = adw::PreferencesWindow::builder()
        .transient_for(parent)
        .modal(true)
        .title("Settings")
        .default_width(720)
        .default_height(640)
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

    window.add(&build_source_roots_page(
        &window,
        &app_state,
        &app_tx,
        &source_roots,
        &refresh_cb,
        &source_status,
        &backend_state,
    ));
    window.add(&build_ignore_rules_page(
        &app_state,
        &app_tx,
        &backend_state,
    ));
    window.add(&build_placeholder_page(
        "Appearance",
        "preferences-desktop-appearance-symbolic",
        "Vesper follows your system light/dark preference and falls back to dark.",
    ));
    window.add(&build_placeholder_page(
        "Thumbnails",
        "view-grid-symbolic",
        "Vesper stores one 256px grid thumbnail per item under your cache directory.",
    ));
    window.add(&build_advanced_page(&app_tx, &maintenance_status));
    window.add(&build_about_page());

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

fn build_placeholder_page(title: &str, icon: &str, body: &str) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title(title)
        .icon_name(icon)
        .build();
    let group = adw::PreferencesGroup::builder().title(title).build();
    group.add(
        &gtk::Label::builder()
            .label(body)
            .wrap(true)
            .xalign(0.0)
            .css_classes(["dim-label"])
            .build(),
    );
    page.add(&group);
    page
}

#[allow(clippy::too_many_arguments)]
fn build_source_roots_page(
    window: &adw::PreferencesWindow,
    app_state: &std::sync::Arc<std::sync::Mutex<crate::state::AppState>>,
    app_tx: &tokio::sync::mpsc::Sender<AppEvent>,
    source_roots: &Rc<RefCell<Vec<(i64, String)>>>,
    refresh_cb: &RefreshCb,
    source_status: &gtk::Label,
    backend_state: &BackendState,
) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title("Source Roots")
        .icon_name("folder-symbolic")
        .build();

    let group = adw::PreferencesGroup::builder()
        .title("Source Roots")
        .description("Folders and drives that Vesper monitors for media.")
        .build();

    // Header actions: Add Source Root.
    let add_btn = gtk::Button::builder()
        .label("Add Source Root")
        .css_classes(["flat"])
        .build();
    add_btn.connect_clicked({
        let app_tx = app_tx.clone();
        let window = window.clone();
        move |_| pick_source_root(&window, &app_tx)
    });
    group.set_header_suffix(Some(&add_btn));

    let roots_list = gtk::ListBox::builder()
        .css_classes(["boxed-list"])
        .selection_mode(gtk::SelectionMode::None)
        .build();
    group.add(&roots_list);
    group.add(source_status);
    group.add(
        &gtk::Label::builder()
            .label("Changes are applied automatically. You can add, remove, or reorder roots at any time.")
            .css_classes(["dim-label", "caption"])
            .wrap(true)
            .xalign(0.0)
            .margin_top(8)
            .build(),
    );
    page.add(&group);

    // Root-as-tag toggle lives under Source Roots (Product §9).
    let tag_group = adw::PreferencesGroup::builder()
        .title("Tag Behavior")
        .build();
    let root_tag_row = adw::ActionRow::builder()
        .title("Use source root name as a tag")
        .subtitle("Include the top-level folder name itself as a tag.")
        .build();
    let root_tag_switch = gtk::Switch::builder()
        .valign(gtk::Align::Center)
        .active(backend_state.root_as_tag)
        .build();
    root_tag_switch.connect_active_notify({
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
    root_tag_row.add_suffix(&root_tag_switch);
    tag_group.add(&root_tag_row);
    page.add(&tag_group);

    // Populate roots and keep the list in sync via the refresh callback.
    let refresh_closure: Rc<dyn Fn()> = Rc::new({
        let roots_list = roots_list.clone();
        let source_roots = source_roots.clone();
        let app_tx = app_tx.clone();
        let window = window.clone();
        move || {
            while let Some(child) = roots_list.first_child() {
                roots_list.remove(&child);
            }
            let roots = source_roots.borrow();
            if roots.is_empty() {
                roots_list.append(
                    &adw::ActionRow::builder()
                        .title("No source roots configured")
                        .css_classes(["dim-label"])
                        .build(),
                );
            }
            for (id, path) in roots.iter() {
                let row = adw::ActionRow::builder().title(path).build();
                let remove_btn = gtk::Button::builder()
                    .label("Remove")
                    .valign(gtk::Align::Center)
                    .css_classes(["flat"])
                    .build();
                remove_btn.connect_clicked({
                    let app_tx = app_tx.clone();
                    let window = window.clone();
                    let path = path.clone();
                    let root_id = *id;
                    move |_| confirm_remove_root(&window, &app_tx, root_id, &path)
                });
                row.add_suffix(&remove_btn);
                roots_list.append(&row);
            }
        }
    });
    *refresh_cb.borrow_mut() = Some(refresh_closure.clone());
    refresh_closure();

    page
}

fn build_ignore_rules_page(
    app_state: &std::sync::Arc<std::sync::Mutex<crate::state::AppState>>,
    app_tx: &tokio::sync::mpsc::Sender<AppEvent>,
    backend_state: &BackendState,
) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title("Ignore Rules")
        .icon_name("action-unavailable-symbolic")
        .build();
    let group = adw::PreferencesGroup::builder()
        .title("Ignore Rules")
        .description("Patterns for files and folders that Vesper should skip.")
        .build();

    // The pattern list, one per line (Arch §2). Presented as an editable text
    // area; the underlying model is the line list, evaluated globally first.
    let text_buffer = gtk::TextBuffer::new(None);
    text_buffer.set_text(&backend_state.global_ignore_rules.join("\n"));
    let text_view = gtk::TextView::builder()
        .buffer(&text_buffer)
        .monospace(true)
        .css_classes(["monospace"])
        .wrap_mode(gtk::WrapMode::None)
        .left_margin(8)
        .right_margin(8)
        .top_margin(8)
        .bottom_margin(8)
        .build();
    text_view.update_property(&[gtk::accessible::Property::Label("Ignore rule patterns")]);
    let scrolled = gtk::ScrolledWindow::builder()
        .child(&text_view)
        .min_content_height(180)
        .css_classes(["card"])
        .build();
    group.add(&scrolled);

    let validation_error = gtk::Label::builder()
        .css_classes(["error"])
        .halign(gtk::Align::Start)
        .selectable(true)
        .wrap(true)
        .visible(false)
        .build();
    group.add(&validation_error);

    let saved_rules = Rc::new(RefCell::new(backend_state.global_ignore_rules.clone()));

    // Header actions: Add Rule (focus a new line) + Reset to Defaults.
    let actions = gtk::Box::builder().spacing(8).build();
    let add_rule_btn = gtk::Button::builder()
        .label("Add Rule")
        .css_classes(["flat"])
        .build();
    let reset_btn = gtk::Button::builder()
        .label("Reset to Defaults")
        .css_classes(["flat"])
        .build();
    actions.append(&add_rule_btn);
    actions.append(&reset_btn);
    group.set_header_suffix(Some(&actions));

    add_rule_btn.connect_clicked({
        let text_buffer = text_buffer.clone();
        let text_view = text_view.clone();
        move |_| {
            let mut end = text_buffer.end_iter();
            if text_buffer.char_count() > 0 {
                text_buffer.insert(&mut end, "\n");
            }
            text_view.grab_focus();
        }
    });
    // "Reset to Defaults" is explicit and user-initiated only (Arch §2).
    reset_btn.connect_clicked({
        let text_buffer = text_buffer.clone();
        move |_| text_buffer.set_text(&default_rules_text())
    });

    let apply_btn = gtk::Button::builder()
        .label("Apply")
        .css_classes(["suggested-action"])
        .halign(gtk::Align::Start)
        .sensitive(false)
        .margin_top(8)
        .build();
    group.add(&apply_btn);
    group.add(
        &gtk::Label::builder()
            .label("Patterns are matched using glob syntax. Use * for wildcards and / to match folders.")
            .css_classes(["dim-label", "caption"])
            .wrap(true)
            .xalign(0.0)
            .margin_top(4)
            .build(),
    );

    text_buffer.connect_changed({
        let saved_rules = saved_rules.clone();
        let apply_btn = apply_btn.clone();
        let validation_error = validation_error.clone();
        move |buffer| {
            let text = buffer_text(buffer);
            validation_error.set_visible(false);
            apply_btn.set_sensitive(text != saved_rules.borrow().join("\n"));
        }
    });

    apply_btn.connect_clicked({
        let text_buffer = text_buffer.clone();
        let validation_error = validation_error.clone();
        let apply_btn = apply_btn.clone();
        let saved_rules = saved_rules.clone();
        let app_state = app_state.clone();
        let app_tx = app_tx.clone();
        move |_| {
            let text = buffer_text(&text_buffer);
            let rules = match validated_ignore_rules(&text) {
                Ok(rules) => rules,
                Err(message) => {
                    validation_error.set_label(&message);
                    validation_error.set_visible(true);
                    return;
                }
            };
            validation_error.set_visible(false);
            *saved_rules.borrow_mut() = rules.clone();
            // Field-scoped merge into the current backend state.
            let mut state = match app_state.lock() {
                Ok(state) => state.backend.clone(),
                Err(_) => return,
            };
            state.global_ignore_rules = rules;
            app_tx.send_critical(AppEvent::UpdateSettings(state));
            app_tx.send_critical(AppEvent::RescanRoots);
            apply_btn.set_sensitive(false);
        }
    });

    page.add(&group);
    page
}

fn build_advanced_page(
    app_tx: &tokio::sync::mpsc::Sender<AppEvent>,
    maintenance_status: &gtk::Label,
) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title("Advanced")
        .icon_name("emblem-system-symbolic")
        .build();
    let group = adw::PreferencesGroup::builder()
        .title("Library Maintenance")
        .description("Maintenance runs in the background. Only one operation runs at a time.")
        .build();
    for (label, action) in [
        ("Rescan Library", MaintenanceAction::Rescan),
        (
            "Regenerate Thumbnails",
            MaintenanceAction::RegenerateThumbnails,
        ),
        ("Rebuild Library Index", MaintenanceAction::RebuildIndex),
    ] {
        let row = adw::ActionRow::builder()
            .title(label)
            .activatable(true)
            .build();
        row.connect_activated({
            let app_tx = app_tx.clone();
            move |_| app_tx.send_critical(maintenance_event(action))
        });
        group.add(&row);
    }
    group.add(maintenance_status);
    page.add(&group);
    page
}

fn build_about_page() -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title("About Vesper")
        .icon_name("help-about-symbolic")
        .build();
    let group = adw::PreferencesGroup::builder()
        .title("About Vesper")
        .build();
    group.add(
        &gtk::Label::builder()
            .label("Vesper — a quiet, media-first gallery for Linux. Your folder structure is the organizational system.")
            .wrap(true)
            .xalign(0.0)
            .css_classes(["dim-label"])
            .build(),
    );
    page.add(&group);
    page
}

fn buffer_text(buffer: &gtk::TextBuffer) -> String {
    buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), true)
        .to_string()
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
    window: &adw::PreferencesWindow,
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
        let text = default_rules_text();
        let rules = ignore_rules_from_text(&text);
        for default in crate::index::ignore_rules::DEFAULT_GLOBAL_PATTERNS {
            assert!(rules.iter().any(|rule| rule == default));
        }
        // No hidden-files default (Product §9 / Arch §1).
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
}
