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
    let window = adw::PreferencesWindow::builder()
        .transient_for(parent)
        .modal(true)
        .title("Preferences")
        .default_width(650)
        .default_height(600)
        .build();

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

    let refresh_roots: Rc<RefCell<Option<Box<dyn Fn()>>>> = Rc::new(RefCell::new(None));

    let refresh_roots_clone = refresh_roots.clone();
    let roots_list_clone = roots_list.clone();
    let db_clone_refresh = db.clone();
    let ui_tx_refresh = ui_tx.clone();
    
    *refresh_roots.borrow_mut() = Some(Box::new(move || {
        while let Some(child) = roots_list_clone.first_child() {
            roots_list_clone.remove(&child);
        }
        if let Ok(guard) = db_clone_refresh.lock() {
            if let Ok(roots) = guard.list_source_roots() {
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
    }));

    if let Some(cb) = refresh_roots.borrow().as_ref() {
        cb();
    }

    let add_root_btn = gtk::Button::builder()
        .label("Add Directory...")
        .margin_top(12)
        .halign(gtk::Align::Start)
        .build();
        
    let window_clone = window.clone();
    let db_add = db.clone();
    let ui_tx_add = ui_tx.clone();
    let refresh_add = refresh_roots.clone();
    
    add_root_btn.connect_clicked(move |_| {
        let dialog = gtk::FileDialog::new();
        let db_cb = db_add.clone();
        let ui_tx_cb = ui_tx_add.clone();
        let ref_cb = refresh_add.clone();
        
        dialog.select_folder(Some(&window_clone), None::<&libadwaita::gtk::gio::Cancellable>, move |res| {
            if let Ok(file) = res {
                if let Some(path) = file.path() {
                    let path_str = path.to_str().unwrap().to_string();
                    let db_guard = db_cb.lock().unwrap();
                    let _ = db_guard.add_source_root(&path_str);
                    drop(db_guard);
                    
                    let db_c = db_cb.clone();
                    let ui_c = ui_tx_cb.clone();
                    tokio::spawn(async move {
                        // TODO: need to fetch active global ignores if we want to honor them during this manual scan.
                        if let Ok(_) = crate::scan::run_scan(path.to_path_buf(), db_c, vec![]).await {
                            let _ = ui_c.send(UiEvent::ScanCompleted);
                        }
                    });
                    
                    if let Some(cb) = ref_cb.borrow().as_ref() {
                        cb();
                    }
                }
            }
        });
    });
    roots_group.add(&add_root_btn);


    // 2. Ignore Rules Group
    let ignore_group = adw::PreferencesGroup::builder()
        .title("Ignore Rules")
        .description("Global patterns for files and directories to ignore across all source roots. One per line. Uses .gitignore syntax.")
        .build();
    page.add(&ignore_group);

    let text_buffer = gtk::TextBuffer::new(None);
    {
        let state = app_state.lock().unwrap();
        text_buffer.set_text(&state.global_ignore_rules.join("\n"));
    }
    
    let text_view = gtk::TextView::builder()
        .buffer(&text_buffer)
        .monospace(true)
        .wrap_mode(gtk::WrapMode::None)
        .left_margin(8)
        .right_margin(8)
        .top_margin(8)
        .bottom_margin(8)
        .build();
        
    let scrolled_text = gtk::ScrolledWindow::builder()
        .child(&text_view)
        .min_content_height(150)
        .css_classes(["frame"])
        .build();
        
    ignore_group.add(&scrolled_text);
    
    let apply_ignore_btn = gtk::Button::builder()
        .label("Apply Rules")
        .margin_top(12)
        .halign(gtk::Align::End)
        .build();
        
    let app_state_ignore = app_state.clone();
    let text_buffer_clone = text_buffer.clone();
    let ui_tx_ignore = ui_tx.clone();
    let db_ignore = db.clone();
    
    apply_ignore_btn.connect_clicked(move |_| {
        let start = text_buffer_clone.start_iter();
        let end = text_buffer_clone.end_iter();
        let text = text_buffer_clone.text(&start, &end, true).to_string();
        
        let rules: Vec<String> = text.lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
            
        if let Ok(mut state) = app_state_ignore.lock() {
            state.global_ignore_rules = rules.clone();
            let _ = state.save();
        }
        
        // Trigger rescan for all roots to apply new ignore rules
        let db_c = db_ignore.clone();
        let ui_c = ui_tx_ignore.clone();
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
    
    ignore_group.add(&apply_ignore_btn);


    // 3. Preferences Group
    let prefs_group = adw::PreferencesGroup::builder()
        .title("General Preferences")
        .build();
    page.add(&prefs_group);
    
    let root_tag_row = adw::ActionRow::builder()
        .title("Use Source Root as Tag")
        .subtitle("Include the top-level directory name itself as a tag.")
        .build();
        
    let root_tag_switch = gtk::Switch::builder()
        .valign(gtk::Align::Center)
        .active(app_state.lock().unwrap().root_as_tag)
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
