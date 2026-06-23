pub mod backend;
pub mod config;
mod db;
mod events;
mod index;
mod scan;
pub mod state;
mod thumbnail;
mod ui;

use crate::events::AppEvent;
use crate::ui::window::UiEvent;
use libadwaita as adw;
use libadwaita::prelude::*;
use libadwaita::{Application, glib, gtk};
use std::sync::{Arc, Mutex};

fn main() -> glib::ExitCode {
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Failed to create tokio runtime: {}", e);
            return glib::ExitCode::FAILURE;
        }
    };
    let _guard = rt.enter();

    std::panic::set_hook(Box::new(move |info| {
        eprintln!("Panic occurred: {:?}", info);
        glib::MainContext::default().invoke(move || {
            let dialog = adw::MessageDialog::builder()
                .heading("Unexpected Error")
                .body("An unexpected error occurred. The application will close.")
                .build();
            if let Some(app) = gtk::gio::Application::default().and_downcast::<gtk::Application>()
                && let Some(win) = app.active_window()
            {
                dialog.set_transient_for(Some(&win));
            }
            dialog.add_response("close", "Close");
            dialog.connect_response(None, move |_, _| {
                std::process::exit(1);
            });
            dialog.present();
        });
    }));

    let app = Application::builder()
        .application_id("io.github.TheRealShek.vesper")
        .build();

    // Bounded to prevent memory exhaustion during filesystem event storms.
    let (app_tx, app_rx) = tokio::sync::mpsc::channel::<AppEvent>(1024);
    let (ui_tx, ui_rx) = tokio::sync::mpsc::channel::<UiEvent>(256);
    let (thumb_tx, thumb_rx) =
        tokio::sync::mpsc::channel::<crate::thumbnail::ThumbnailRequest>(128);

    let db_res = dirs::data_dir()
        .ok_or_else(|| std::io::Error::other("Could not determine user data directory"))
        .and_then(|data_dir| {
            let vesper_dir = data_dir.join("vesper");
            std::fs::create_dir_all(&vesper_dir)?;
            let db_path = vesper_dir.join(crate::config::DB_NAME);
            crate::db::Database::open(&db_path).map_err(|e| std::io::Error::other(e.to_string()))
        });
    let state_res = std::panic::catch_unwind(crate::state::AppState::load);

    let (db_arc, state_arc) = match (db_res, state_res) {
        (Ok(db), Ok(state)) => (Arc::new(db), Arc::new(Mutex::new(state))),
        _ => {
            eprintln!("Failed to load database or state");
            app.connect_activate(move |app| {
                let dialog = adw::MessageDialog::builder()
                    .heading("Unexpected Error")
                    .body("An unexpected error occurred. The application will close.")
                    .build();
                dialog.add_response("close", "Close");
                let app_clone = app.clone();
                dialog.connect_response(None, move |_, _| {
                    app_clone.quit();
                    std::process::exit(1);
                });
                dialog.present();
            });
            return app.run();
        }
    };

    // Start Thumbnail Worker
    crate::thumbnail::start_thumbnail_worker(db_arc.clone(), thumb_rx, ui_tx.clone());

    // Backend Loop
    crate::backend::start_backend(
        app_rx,
        app_tx.clone(),
        ui_tx.clone(),
        db_arc.clone(),
        state_arc.clone(),
    );

    let ui_rx_cell = std::rc::Rc::new(std::cell::RefCell::new(Some(ui_rx)));

    app.connect_activate(move |app| {
        let rx = ui_rx_cell.borrow_mut().take();
        if let Some(rx) = rx {
            ui::build_ui(
                app,
                app_tx.clone(),
                ui_tx.clone(),
                rx,
                thumb_tx.clone(),
                state_arc.clone(),
            );
        }
    });

    let ret = app.run();
    rt.shutdown_background();
    ret
}
