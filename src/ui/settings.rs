use libadwaita as adw;
use libadwaita::gtk::{self, prelude::*};
use libadwaita::prelude::*;
use std::sync::{Arc, Mutex};
use std::rc::Rc;
use std::cell::RefCell;
use crate::state::AppState;
use crate::db::Database;
use crate::ui::window::UiEvent;

pub fn show(
    parent: &gtk::Window,
    app_state: Arc<Mutex<AppState>>,
    db: Arc<Mutex<Database>>,
    ui_tx: tokio::sync::mpsc::UnboundedSender<UiEvent>,
) {
    let window = adw::Window::builder()
        .transient_for(parent)
        .modal(true)
        .title("Settings")
        .default_width(650)
        .default_height(600)
        .build();

    let toolbar_view = adw::ToolbarView::new();
    let header_bar = adw::HeaderBar::builder()
        .show_start_title_buttons(false)
        .show_end_title_buttons(false)
        .build();
        
    let title = gtk::Label::builder()
        .label("Settings")
        .css_classes(["title"])
        .build();
    header_bar.set_title_widget(Some(&title));
    
    let close_btn = gtk::Button::builder()
        .icon_name("window-close-symbolic")
        .css_classes(["flat"])
        .valign(gtk::Align::Center)
        .build();
        
    let win_clone_close = window.clone();
    close_btn.connect_clicked(move |_| {
        win_clone_close.close();
    });
    
    header_bar.pack_end(&close_btn);
    toolbar_view.add_top_bar(&header_bar);

    let page = adw::PreferencesPage::new();
    let scrolled = gtk::ScrolledWindow::builder()
        .child(&page)
        .build();
        
    toolbar_view.set_content(Some(&scrolled));
    window.set_content(Some(&toolbar_view));

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

    let refresh_roots: Rc<RefCell<Option<Box<dyn Fn()>>>> = Rc::new(RefCell::new(None));

    let refresh_roots_clone = refresh_roots.clone();
    let roots_list_clone = roots_list.clone();
    let db_clone_refresh = db.clone();
    let ui_tx_refresh = ui_tx.clone();

    let window_clone = window.clone();
    let db_add = db.clone();
    let ui_tx_add = ui_tx.clone();
    let refresh_add = refresh_roots.clone();

    *refresh_roots.borrow_mut() = Some(Box::new(move || {
        while let Some(child) = roots_list_clone.first_child() {
            roots_list_clone.remove(&child);
        }
        if let Ok(guard) = db_clone_refresh.lock() {
            if let Ok(roots) = guard.list_source_roots() {
                if roots.is_empty() {
                    let empty_row = adw::ActionRow::builder()
                        .title("No directories configured")
                        .css_classes(["dim-label"])
                        .build();
                    roots_list_clone.append(&empty_row);
                } else {
                    for root in roots {
                    let row = adw::ActionRow::builder()
                        .title(&root.path)
                        .build();
                    
                    let remove_btn = gtk::Button::builder()
                        .icon_name("user-trash-symbolic")
                        .valign(gtk::Align::Center)
                        .css_classes(["flat", "destructive-action"])
                        .build();
                        
                    let db_remove = db_clone_refresh.clone();
                    let ui_tx_remove = ui_tx_refresh.clone();
                    let root_id = root.id;
                    
                    let refresh_cb = refresh_roots_clone.clone();
                    
                    remove_btn.connect_clicked(move |_| {
                        if let Ok(g) = db_remove.lock() {
                            let _ = g.remove_source_root(root_id);
                        }
                        let _ = ui_tx_remove.send(UiEvent::ScanCompleted);
                        if let Some(cb) = refresh_cb.borrow().as_ref() {
                            cb();
                        }
                    });
                    
                    row.add_suffix(&remove_btn);
                    roots_list_clone.append(&row);
                }
                }
            }
        }
        
        let add_root_row = adw::ActionRow::builder()
            .title("Add Directory...")
            .activatable(true)
            .build();
            
        let add_icon = gtk::Image::from_icon_name("list-add-symbolic");
        add_root_row.add_prefix(&add_icon);
        
        let dialog_parent = window_clone.clone();
        let db_cb = db_add.clone();
        let ui_tx_cb = ui_tx_add.clone();
        let ref_cb = refresh_add.clone();
        
        add_root_row.connect_activated(move |_| {
            let dialog = gtk::FileDialog::new();
            let db_c = db_cb.clone();
            let ui_c = ui_tx_cb.clone();
            let r_cb = ref_cb.clone();
            
            dialog.select_folder(Some(&dialog_parent), None::<&libadwaita::gtk::gio::Cancellable>, move |res| {
                if let Ok(file) = res {
                    if let Some(path) = file.path() {
                        let path_str = match path.to_str() {
                            Some(s) => s.to_string(),
                            None => {
                                eprintln!("Invalid UTF-8 in selected path");
                                return;
                            }
                        };
                        let db_guard = match db_c.lock() {
                            Ok(g) => g,
                            Err(_) => {
                                let _ = ui_c.send(UiEvent::FatalError("Database lock poisoned".to_string()));
                                return;
                            }
                        };
                        let _ = db_guard.add_source_root(&path_str);
                        drop(db_guard);
                        
                        let db_c2 = db_c.clone();
                        let ui_c2 = ui_c.clone();
                        tokio::spawn(async move {
                            if let Ok(_) = crate::scan::run_scan(path.to_path_buf(), db_c2, vec![]).await {
                                let _ = ui_c2.send(UiEvent::ScanCompleted);
                            }
                        });
                        
                        if let Some(cb) = r_cb.borrow().as_ref() {
                            cb();
                        }
                    }
                }
            });
        });
        
        roots_list_clone.append(&add_root_row);
    }));

    if let Some(cb) = refresh_roots.borrow().as_ref() {
        cb();
    }


    // 2. Ignore Rules Group
    let ignore_group = adw::PreferencesGroup::builder()
        .title("Ignore Rules")
        .description("Global patterns for files and directories to ignore across all source roots. One per line. Uses .gitignore syntax.")
        .build();
    page.add(&ignore_group);

    let text_buffer = gtk::TextBuffer::new(None);
    {
        let state = match app_state.lock() {
            Ok(s) => s,
            Err(_) => {
                let _ = ui_tx.send(UiEvent::FatalError("State lock poisoned".to_string()));
                return;
            }
        };
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
        
    let scrolled_text = gtk::ScrolledWindow::builder()
        .child(&text_view)
        .min_content_height(150)
        .css_classes(["card"])
        .build();
        
    ignore_group.add(&scrolled_text);
    
    let app_state_ignore = app_state.clone();
    text_buffer.connect_changed(move |buffer| {
        let start = buffer.start_iter();
        let end = buffer.end_iter();
        let text = buffer.text(&start, &end, true).to_string();
        
        let rules: Vec<String> = text.lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
            
        if let Ok(mut state) = app_state_ignore.lock() {
            state.global_ignore_rules = rules;
            let _ = state.save();
        }
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
        .active(match app_state.lock() {
            Ok(s) => s.root_as_tag,
            Err(_) => {
                let _ = ui_tx.send(UiEvent::FatalError("State lock poisoned".to_string()));
                return;
            }
        })
        .build();
        
    let app_state_prefs = app_state.clone();
    let ui_tx_prefs = ui_tx.clone();
    let db_prefs = db.clone();
    
    root_tag_switch.connect_active_notify(move |switch| {
        let is_active = switch.is_active();
        let mut rules = vec![];
        if let Ok(mut state) = app_state_prefs.lock() {
            state.root_as_tag = is_active;
            rules = state.global_ignore_rules.clone();
            let _ = state.save();
        }
        
        // Trigger rescan because tag generation changed
        let db_c = db_prefs.clone();
        let ui_c = ui_tx_prefs.clone();
        tokio::spawn(async move {
            let roots = {
                if let Ok(g) = db_c.lock() {
                    g.list_source_roots().unwrap_or_default()
                } else {
                    vec![]
                }
            };
            for root in roots {
                let path = std::path::PathBuf::from(root.path);
                let _ = crate::scan::run_scan(path, db_c.clone(), rules.clone()).await;
            }
            let _ = ui_c.send(UiEvent::ScanCompleted);
        });
    });
        
    root_tag_row.add_suffix(&root_tag_switch);
    prefs_group.add(&root_tag_row);

    window.present();
}
