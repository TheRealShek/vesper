pub mod backend;
pub mod config;
mod db;
mod events;
mod index;
mod lock;
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

    let vesper_dir_res = dirs::data_dir()
        .ok_or_else(|| std::io::Error::other("Could not determine user data directory"))
        .and_then(|data_dir| {
            let vesper_dir = data_dir.join("vesper");
            std::fs::create_dir_all(&vesper_dir)?;
            Ok(vesper_dir)
        });

    // Single-instance library lock (B-1): acquire an exclusive OS-level lock
    // before opening the database for write access. Held for the whole process
    // lifetime so no second write-capable instance can share the library state.
    let mut _library_lock: Option<crate::lock::LibraryLock> = None;
    if let Ok(ref vesper_dir) = vesper_dir_res {
        let lock_path = vesper_dir.join(crate::config::LOCK_NAME);
        match crate::lock::LibraryLock::acquire(&lock_path) {
            Ok(Some(lock)) => _library_lock = Some(lock),
            Ok(None) => {
                eprintln!("Vesper is already running. Activating the existing window.");
                // Hand off to the primary instance over GTK's D-Bus, if reachable.
                if app.register(None::<&gtk::gio::Cancellable>).is_ok() && app.is_remote() {
                    app.activate();
                }
                return glib::ExitCode::SUCCESS;
            }
            Err(e) => {
                eprintln!("Could not acquire the Vesper library lock: {e}");
                return glib::ExitCode::FAILURE;
            }
        }
    }

    let db_res = match vesper_dir_res {
        Ok(vesper_dir) => {
            let db_path = vesper_dir.join(crate::config::DB_NAME);
            crate::db::Database::open(&db_path)
        }
        Err(e) => {
            eprintln!("Failed to prepare data directory: {e}");
            return run_closing_dialog(app, GENERIC_HEADING, GENERIC_BODY);
        }
    };
    let state_res = std::panic::catch_unwind(crate::state::AppState::load);

    let (db_arc, state_arc) = match (db_res, state_res) {
        (Ok(db), Ok(state)) => (Arc::new(db), Arc::new(Mutex::new(state))),
        (Err(crate::db::DbError::Migration(msg)), _) => {
            // A-1: a recognized migration failure is recoverable (04 §12).
            eprintln!("Database migration failed: {msg}");
            return run_migration_recovery_dialog(app);
        }
        _ => {
            eprintln!("Failed to load database or state");
            return run_closing_dialog(app, GENERIC_HEADING, GENERIC_BODY);
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
    let db_for_ui = db_arc.clone();

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
                db_for_ui.clone(),
            );
        }
    });

    let ret = app.run();
    rt.shutdown_background();
    ret
}

const GENERIC_HEADING: &str = "Unexpected Error";
const GENERIC_BODY: &str = "An unexpected error occurred. The application will close.";

/// Presents a single non-recoverable closing dialog (04 §12) and runs the app
/// so it can be shown, returning the process exit code.
fn run_closing_dialog(
    app: Application,
    heading: &'static str,
    body: &'static str,
) -> glib::ExitCode {
    app.connect_activate(move |app| {
        let dialog = adw::MessageDialog::builder()
            .heading(heading)
            .body(body)
            .build();
        dialog.add_response("close", "Close");
        let app_clone = app.clone();
        dialog.connect_response(None, move |_, _| {
            app_clone.quit();
            std::process::exit(1);
        });
        dialog.present();
    });
    app.run()
}

/// A-1 recovery hook: a recognized migration failure is recoverable per 04 §12
/// and should show a dialog explaining that user media is unaffected, offering
/// "Rebuild Library Index" and "Close" (non-modal Rebuild progress once the main
/// window can open).
///
/// STUB: the Rebuild Library Index maintenance operation (B-6) does not exist
/// yet, so this currently routes to the generic closing dialog. Follow-up: build
/// the two-button recoverable-migration dialog and wire it to Rebuild.
fn run_migration_recovery_dialog(app: Application) -> glib::ExitCode {
    run_closing_dialog(app, GENERIC_HEADING, GENERIC_BODY)
}
