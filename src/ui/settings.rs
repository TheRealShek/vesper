use crate::events::AppEvent;
use crate::events::ChannelSendExt;
use crate::state::BackendState;
use libadwaita as adw;
use libadwaita::gtk::{self, glib, prelude::*};
use libadwaita::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

type RefreshCb = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

pub fn show(
    parent: &impl IsA<gtk::Window>,
    backend_state: BackendState,
    app_tx: tokio::sync::mpsc::Sender<AppEvent>,
    source_roots: Rc<RefCell<Vec<(i64, String)>>>,
    refresh_cb: RefreshCb,
) {
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
        let app_tx_close = app_tx.clone();
        move |_| {
            *cb.borrow_mut() = None;
            app_tx_close.send_critical(AppEvent::RescanRoots);
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

                remove_btn.connect_clicked(move |_| {
                    // Removed by DB ID rather than path because path canonicalization rules might change or differ, but ID is an absolute DB identity.
                    app_tx_remove.send_critical(AppEvent::RemoveSourceRoot(root_id));
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
    page.add(&ignore_group);

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

    let shared_state = Rc::new(RefCell::new(backend_state.clone()));
    let shared_state_ignore = shared_state.clone();
    let app_tx_ignore = app_tx.clone();
    text_buffer.connect_changed(move |buffer| {
        let start = buffer.start_iter();
        let end = buffer.end_iter();
        let text = buffer.text(&start, &end, true).to_string();

        let rules: Vec<String> = text
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let mut state = shared_state_ignore.borrow_mut();
        state.global_ignore_rules = rules;
        // Sent immediately rather than on dialog close so that any background scans firing while settings are open use consistent rules.
        app_tx_ignore.send_critical(AppEvent::UpdateSettings(state.clone()));
    });

    // 3. Preferences Group
    let prefs_group = adw::PreferencesGroup::builder()
        .title("Preferences")
        .build();
    page.add(&prefs_group);

    let root_tag_row = adw::ActionRow::builder()
        .title("Use Source Root as Tag")
        .subtitle("Include the top-level directory name itself as a tag.")
        .build();

    let root_tag_switch = gtk::Switch::builder()
        .valign(gtk::Align::Center)
        .active(backend_state.root_as_tag)
        .build();
    root_tag_switch.update_property(&[gtk::accessible::Property::Label(
        "Treat root directory as tag",
    )]);

    let shared_state_prefs = shared_state.clone();
    let app_tx_prefs = app_tx.clone();

    root_tag_switch.connect_active_notify(move |switch| {
        let is_active = switch.is_active();
        let mut state = shared_state_prefs.borrow_mut();
        state.root_as_tag = is_active;
        app_tx_prefs.send_critical(AppEvent::UpdateSettings(state.clone()));

        // Trigger rescan because tag generation changed
        app_tx_prefs.send_critical(AppEvent::RescanRoots);
    });

    root_tag_row.add_suffix(&root_tag_switch);
    prefs_group.add(&root_tag_row);

    window.present();
}
