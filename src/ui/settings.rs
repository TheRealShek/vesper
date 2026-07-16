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

fn append_missing_default_rules(text: &str) -> String {
    let mut rules = ignore_rules_from_text(text);
    for default in crate::index::ignore_rules::DEFAULT_GLOBAL_PATTERNS {
        if !rules.iter().any(|rule| rule == default) {
            rules.push((*default).to_string());
        }
    }
    rules.join("\n")
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
    // U-5: every opening reads the *current* saved backend state — never a
    // clone captured at main-window construction — so controls always reflect
    // what is persisted right now.
    let backend_state: BackendState = match app_state.lock() {
        Ok(state) => state.backend.clone(),
        Err(_) => return,
    };

    let window = adw::PreferencesWindow::builder()
        .transient_for(parent)
        .modal(true)
        .search_enabled(true)
        .title("Settings")
        .default_width(650)
        .default_height(600)
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

    let page = adw::PreferencesPage::new();
    window.add(&page);

    // 1. Source Roots Group
    let roots_group = adw::PreferencesGroup::builder()
        .title("Source Directories")
        .description("Folders containing your media. Vesper will watch these for changes.")
        .build();
    page.add(&roots_group);

    let roots_list = gtk::ListBox::builder()
        .css_classes(["boxed-list"])
        .selection_mode(gtk::SelectionMode::None)
        .build();
    roots_group.add(&roots_list);

    let source_status = gtk::Label::builder()
        .css_classes(["error"])
        .halign(gtk::Align::Start)
        .wrap(true)
        .selectable(true)
        .visible(false)
        .build();
    source_status.update_property(&[gtk::accessible::Property::Label("Source directory status")]);
    roots_group.add(&source_status);

    let app_tx_refresh = app_tx.clone();
    let roots_list_clone = roots_list.clone();
    let source_roots_clone = source_roots.clone();

    let window_clone = window.clone();
    let app_tx_add = app_tx.clone();

    let refresh_closure: Rc<dyn Fn()> = Rc::new(move || {
        while let Some(child) = roots_list_clone.first_child() {
            roots_list_clone.remove(&child);
        }
        let roots = source_roots_clone.borrow();
        if roots.is_empty() {
            let empty_row = adw::ActionRow::builder()
                .title("No directories configured")
                .css_classes(["dim-label"])
                .build();
            roots_list_clone.append(&empty_row);
        } else {
            for (id, path) in roots.iter() {
                let row = adw::ActionRow::builder().title(path).build();

                let remove_btn = gtk::Button::builder()
                    .icon_name("user-trash-symbolic")
                    .valign(gtk::Align::Center)
                    .css_classes(["flat", "destructive-action"])
                    .build();
                remove_btn.update_property(&[gtk::accessible::Property::Label("Remove directory")]);

                let app_tx_remove = app_tx_refresh.clone();
                let root_id = *id;

                let root_path = path.clone();
                let confirmation_parent = window_clone.clone();
                remove_btn.connect_clicked(move |_| {
                    let dialog = adw::MessageDialog::builder()
                        .transient_for(&confirmation_parent)
                        .modal(true)
                        .heading("Remove Source Directory?")
                        .body(format!(
                            "Remove {root_path} from Vesper? Files on disk will not be changed."
                        ))
                        .build();
                    dialog.add_response("cancel", "Cancel");
                    dialog.add_response("remove", "Remove");
                    dialog.set_default_response(Some("cancel"));
                    dialog.set_close_response("cancel");
                    dialog.set_response_appearance("remove", adw::ResponseAppearance::Destructive);
                    let app_tx_remove = app_tx_remove.clone();
                    dialog.connect_response(Some("remove"), move |_, _| {
                        // The stable database id, never the mutable row index,
                        // identifies the root confirmed by the user.
                        app_tx_remove.send_critical(remove_source_event(root_id));
                    });
                    dialog.present();
                });

                row.add_suffix(&remove_btn);
                roots_list_clone.append(&row);
            }
        }

        let add_root_row = adw::ActionRow::builder()
            .title("Add Directory...")
            .activatable(true)
            .build();
        add_root_row.update_property(&[gtk::accessible::Property::Label("Add directory")]);

        let add_icon = gtk::Image::from_icon_name("list-add-symbolic");
        add_root_row.add_prefix(&add_icon);

        let dialog_parent = window_clone.clone();
        let app_tx_cb = app_tx_add.clone();

        add_root_row.connect_activated(move |_| {
            let dialog = gtk::FileDialog::new();
            let app_tx_c = app_tx_cb.clone();

            dialog.select_folder(
                Some(&dialog_parent),
                None::<&libadwaita::gtk::gio::Cancellable>,
                move |res| {
                    if let Ok(file) = res
                        && let Some(path) = file.path()
                    {
                        let path_str = match path.to_str() {
                            Some(s) => s.to_string(),
                            None => {
                                eprintln!("Invalid UTF-8 in selected path");
                                return;
                            }
                        };
                        app_tx_c.send_critical(AppEvent::AddSourceRoot(path_str));
                    }
                },
            );
        });

        roots_list_clone.append(&add_root_row);
    });

    *refresh_cb.borrow_mut() = Some(refresh_closure.clone());
    refresh_closure();

    // 2. Ignore Rules Group
    let ignore_group = adw::PreferencesGroup::builder()
        .title("Ignore Rules")
        .description("Global patterns for files and directories to ignore across all source roots. One per line. Uses .gitignore syntax.")
        .build();
    let text_buffer = gtk::TextBuffer::new(None);
    {
        let state = backend_state.clone();
        text_buffer.set_text(&state.global_ignore_rules.join("\n"));
    }

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
    text_view.update_property(&[gtk::accessible::Property::Label("Ignore rules input")]);

    let scrolled_text = gtk::ScrolledWindow::builder()
        .child(&text_view)
        .min_content_height(150)
        .css_classes(["card"])
        .build();

    ignore_group.add(&scrolled_text);

    let validation_error = gtk::Label::builder()
        .css_classes(["error"])
        .halign(gtk::Align::Start)
        .selectable(true)
        .wrap(true)
        .visible(false)
        .build();
    validation_error.update_property(&[gtk::accessible::Property::Label(
        "Ignore rule validation errors",
    )]);
    ignore_group.add(&validation_error);

    let saved_ignore_rules = Rc::new(RefCell::new(backend_state.global_ignore_rules.clone()));
    let apply_ignore_button = gtk::Button::builder()
        .label("Apply Ignore Rules")
        .css_classes(["suggested-action"])
        .halign(gtk::Align::Start)
        .sensitive(false)
        .build();
    let restore_defaults_button = gtk::Button::builder()
        .label("Restore Default Ignore Rules")
        .halign(gtk::Align::Start)
        .build();
    ignore_group.add(&restore_defaults_button);
    ignore_group.add(&apply_ignore_button);

    let saved_ignore_rules_changed = saved_ignore_rules.clone();
    let apply_ignore_button_changed = apply_ignore_button.clone();
    let validation_error_changed = validation_error.clone();
    text_buffer.connect_changed(move |buffer| {
        let start = buffer.start_iter();
        let end = buffer.end_iter();
        let text = buffer.text(&start, &end, true).to_string();
        validation_error_changed.set_visible(false);
        let saved = saved_ignore_rules_changed.borrow().join("\n");
        apply_ignore_button_changed.set_sensitive(text != saved);
    });

    let text_buffer_restore = text_buffer.clone();
    restore_defaults_button.connect_clicked(move |_| {
        let start = text_buffer_restore.start_iter();
        let end = text_buffer_restore.end_iter();
        let text = text_buffer_restore.text(&start, &end, true);
        text_buffer_restore.set_text(&append_missing_default_rules(&text));
    });

    let app_state_apply = app_state.clone();
    let saved_ignore_rules_apply = saved_ignore_rules.clone();
    let text_buffer_apply = text_buffer.clone();
    let validation_error_apply = validation_error.clone();
    let apply_ignore_button_apply = apply_ignore_button.clone();
    let app_tx_apply = app_tx.clone();
    apply_ignore_button.connect_clicked(move |_| {
        let start = text_buffer_apply.start_iter();
        let end = text_buffer_apply.end_iter();
        let text = text_buffer_apply.text(&start, &end, true);
        let rules = match validated_ignore_rules(&text) {
            Ok(rules) => rules,
            Err(message) => {
                validation_error_apply.set_label(&message);
                validation_error_apply.set_visible(true);
                return;
            }
        };

        validation_error_apply.set_visible(false);
        *saved_ignore_rules_apply.borrow_mut() = rules.clone();
        // U-5: merge the edited field into the *current* backend state so this
        // control never clobbers a setting saved by another control or an
        // earlier dialog opening.
        let mut state = match app_state_apply.lock() {
            Ok(state) => state.backend.clone(),
            Err(_) => return,
        };
        state.global_ignore_rules = rules;
        app_tx_apply.send_critical(AppEvent::UpdateSettings(state));
        app_tx_apply.send_critical(AppEvent::RescanRoots);
        apply_ignore_button_apply.set_sensitive(false);
    });

    // 3. Preferences Group
    let prefs_group = adw::PreferencesGroup::builder()
        .title("Tag Behavior")
        .build();
    page.add(&prefs_group);

    let root_tag_row = adw::ActionRow::builder()
        .title("Include source root name as tag")
        .subtitle("Include the top-level directory name itself as a tag.")
        .build();

    let root_tag_switch = gtk::Switch::builder()
        .valign(gtk::Align::Center)
        .active(backend_state.root_as_tag)
        .build();
    root_tag_switch.update_property(&[gtk::accessible::Property::Label(
        "Treat root directory as tag",
    )]);

    let app_state_prefs = app_state.clone();
    let app_tx_prefs = app_tx.clone();

    root_tag_switch.connect_active_notify(move |switch| {
        let is_active = switch.is_active();
        // U-5: field-scoped merge against the current backend state (see the
        // ignore-rules Apply handler).
        let mut state = match app_state_prefs.lock() {
            Ok(state) => state.backend.clone(),
            Err(_) => return,
        };
        state.root_as_tag = is_active;
        app_tx_prefs.send_critical(AppEvent::UpdateSettings(state));

        // Trigger rescan because tag generation changed
        app_tx_prefs.send_critical(AppEvent::RescanRoots);
    });

    root_tag_row.add_suffix(&root_tag_switch);
    prefs_group.add(&root_tag_row);
    page.add(&ignore_group);

    // 4. Library Maintenance Group
    let maintenance_group = adw::PreferencesGroup::builder()
        .title("Library Maintenance")
        .description("Maintenance runs in the background. Only one operation runs at a time.")
        .build();
    page.add(&maintenance_group);

    for (label, action) in [
        ("Rescan Library", MaintenanceAction::Rescan),
        (
            "Regenerate Thumbnails",
            MaintenanceAction::RegenerateThumbnails,
        ),
        ("Rebuild Library Index", MaintenanceAction::RebuildIndex),
    ] {
        let button = gtk::Button::builder()
            .label(label)
            .halign(gtk::Align::Start)
            .build();
        let app_tx = app_tx.clone();
        button.connect_clicked(move |_| {
            app_tx.send_critical(maintenance_event(action));
        });
        maintenance_group.add(&button);
    }

    let maintenance_status = gtk::Label::builder()
        .halign(gtk::Align::Start)
        .wrap(true)
        .selectable(true)
        .visible(false)
        .build();
    maintenance_status.update_property(&[gtk::accessible::Property::Label(
        "Library maintenance status",
    )]);
    maintenance_group.add(&maintenance_status);

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
    fn maintenance_buttons_map_to_b6_events() {
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
    fn restore_defaults_appends_only_missing_patterns() {
        let restored = append_missing_default_rules("*.custom\n.git/\n*.tmp");
        let rules = ignore_rules_from_text(&restored);

        assert_eq!(rules[0], "*.custom");
        assert_eq!(rules.iter().filter(|rule| *rule == ".git/").count(), 1);
        assert_eq!(rules.iter().filter(|rule| *rule == "*.tmp").count(), 1);
        for default in crate::index::ignore_rules::DEFAULT_GLOBAL_PATTERNS {
            assert!(rules.iter().any(|rule| rule == default));
        }
    }

    #[test]
    fn ignore_apply_rejects_invalid_rules_as_one_set() {
        let saved = vec!["*.saved".to_string()];
        let result = validated_ignore_rules("*.valid\n[z-a].tmp");

        assert!(result.is_err());
        assert_eq!(saved, vec!["*.saved".to_string()]);
        let message = result.unwrap_err();
        assert!(message.contains("global rules, line 2:"));
    }

    #[test]
    fn confirmed_root_removal_uses_stable_root_id() {
        assert!(matches!(
            remove_source_event(42),
            AppEvent::RemoveSourceRoot(42)
        ));
    }
}
