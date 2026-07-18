pub mod backend;
pub mod config;
mod db;
mod events;
mod index;
mod lock;
pub mod logging;
mod scan;
pub mod state;
mod thumbnail;
mod ui;

use crate::events::{AppEvent, UiEvent};
use libadwaita as adw;
use libadwaita::prelude::*;
use libadwaita::{Application, glib, gtk};
use std::sync::{Arc, Mutex};

fn main() -> glib::ExitCode {
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Failed to create tokio runtime: {e}");
            return glib::ExitCode::FAILURE;
        }
    };
    let _guard = rt.enter();

    std::panic::set_hook(Box::new(move |info| {
        eprintln!("Panic occurred: {info:?}");
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
            dialog.connect_response(None, move |_, _| std::process::exit(1));
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

    // Single-instance library lock (B-1): hold an exclusive OS-level lock for the
    // whole process lifetime so no second write-capable instance shares state.
    let mut _library_lock: Option<crate::lock::LibraryLock> = None;
    if let Ok(ref vesper_dir) = vesper_dir_res {
        let lock_path = vesper_dir.join(crate::config::LOCK_NAME);
        match crate::lock::LibraryLock::acquire(&lock_path) {
            Ok(Some(lock)) => _library_lock = Some(lock),
            Ok(None) => {
                eprintln!("Vesper is already running. Activating the existing window.");
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

    // Structured logging: initialize once the lock is held and before opening the
    // database so schema-migration events are captured.
    let mut _log_guard = None;
    if let Ok(ref vesper_dir) = vesper_dir_res {
        _log_guard = crate::logging::init(vesper_dir);
        tracing::info!("Vesper starting");
    }

    let vesper_dir = match vesper_dir_res {
        Ok(vesper_dir) => vesper_dir,
        Err(e) => {
            eprintln!("Failed to prepare data directory: {e}");
            return run_closing_dialog(app, GENERIC_HEADING, GENERIC_BODY);
        }
    };
    let db_path = vesper_dir.join(crate::config::DB_NAME);
    let db_arc = match crate::db::Database::open(&db_path) {
        Ok(db) => Arc::new(db),
        Err(crate::db::DbError::Migration(msg)) => {
            tracing::error!(error = %msg, "database migration failed");
            return run_migration_recovery_dialog(app, db_path);
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to load database");
            return run_closing_dialog(app, GENERIC_HEADING, GENERIC_BODY);
        }
    };

    // State lives in SQLite. AssertUnwindSafe: the DB is Mutex-guarded and
    // poison-safe, so a panic in load leaves no torn shared state.
    let state_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::state::AppState::load(&db_arc)
    }));
    let state_arc = match state_res {
        Ok(state) => Arc::new(Mutex::new(state)),
        Err(_) => {
            eprintln!("Failed to load state");
            return run_closing_dialog(app, GENERIC_HEADING, GENERIC_BODY);
        }
    };

    let concurrency = crate::backend::concurrency::BackendConcurrency::new();
    let thumbnail_cache = crate::thumbnail::ThumbnailCacheState::new();
    let services = Arc::new(crate::backend::BackendServices {
        concurrency: concurrency.clone(),
        thumbnail_cache: thumbnail_cache.clone(),
        maintenance: crate::backend::maintenance::MaintenanceCoordinator::new(),
    });

    crate::thumbnail::start_thumbnail_worker(
        db_arc.clone(),
        thumb_rx,
        ui_tx.clone(),
        concurrency,
        thumbnail_cache,
    );
    crate::backend::start_backend(
        app_rx,
        app_tx.clone(),
        ui_tx.clone(),
        db_arc.clone(),
        state_arc.clone(),
        services,
    );

    // Follow the system light/dark preference, defaulting to dark (Vision §7).
    app.connect_startup(|_| {
        adw::StyleManager::default().set_color_scheme(adw::ColorScheme::PreferDark);
    });

    let ui_rx_cell = std::rc::Rc::new(std::cell::RefCell::new(Some(ui_rx)));
    let db_for_ui = db_arc.clone();
    app.connect_activate(move |app| {
        if let Some(rx) = ui_rx_cell.borrow_mut().take() {
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
    tracing::info!("Vesper shutting down");
    rt.shutdown_background();
    ret
}

const GENERIC_HEADING: &str = "Unexpected Error";
const GENERIC_BODY: &str = "An unexpected error occurred. The application will close.";

/// Presents a single non-recoverable closing dialog and runs the app so it can
/// be shown, returning the process exit code.
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

/// The recoverable-migration dialog (Implementation §4). Explains that user media
/// on disk is unaffected and offers exactly Rebuild Library Index and Close.
fn run_migration_recovery_dialog(app: Application, db_path: std::path::PathBuf) -> glib::ExitCode {
    app.connect_activate(move |app| {
        let dialog = adw::MessageDialog::builder()
            .heading("Library Index Needs Rebuilding")
            .body(
                "Vesper could not update its library index. Your photos and videos \
                 on disk are unaffected.\n\nYou can rebuild the library index — your \
                 source folders are kept and will be rescanned — or close Vesper.",
            )
            .build();
        dialog.add_response("close", "Close");
        dialog.add_response("rebuild", "Rebuild Library Index");
        dialog.set_default_response(Some("rebuild"));
        dialog.set_close_response("close");

        let app_clone = app.clone();
        let db_path = db_path.clone();
        dialog.connect_response(None, move |_, response| {
            if response != "rebuild" {
                app_clone.quit();
                std::process::exit(1);
            }
            match rebuild_library_index(&db_path) {
                Ok(()) => {
                    tracing::info!("library index rebuilt after migration failure");
                    run_recovery_result_dialog(
                        &app_clone,
                        "Library Index Rebuilt",
                        "The library index was recreated and your source folders were \
                         kept. Start Vesper again to rescan your library.",
                        0,
                    );
                }
                Err(e) => {
                    tracing::error!(error = %e, "library index rebuild failed");
                    run_recovery_result_dialog(
                        &app_clone,
                        "Rebuild Failed",
                        "The library index could not be rebuilt. Your media files on \
                         disk are unaffected.",
                        1,
                    );
                }
            }
        });
        dialog.present();
    });
    app.run()
}

fn run_recovery_result_dialog(app: &Application, heading: &str, body: &str, code: i32) {
    let dialog = adw::MessageDialog::builder()
        .heading(heading)
        .body(body)
        .build();
    dialog.add_response("close", "Close");
    let app_clone = app.clone();
    dialog.connect_response(None, move |_, _| {
        app_clone.quit();
        std::process::exit(code);
    });
    dialog.present();
}

/// Recreates the library index while preserving user media (never touched) and
/// the source-root configuration.
fn rebuild_library_index(db_path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut roots: Vec<(String, String)> = Vec::new();
    if let Ok(conn) =
        rusqlite::Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        && let Ok(mut stmt) = conn.prepare("SELECT path, display_path FROM source_roots")
        && let Ok(rows) = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
    {
        roots = rows.flatten().collect();
    }

    let backup = db_path.with_extension("db.failed");
    std::fs::rename(db_path, &backup)?;

    let db = crate::db::Database::open(db_path)?;
    for (path, display_path) in roots {
        if let Err(e) = db.add_source_root(&path, &display_path) {
            tracing::warn!(error = %e, "could not restore a source root during rebuild");
        }
    }
    Ok(())
}
