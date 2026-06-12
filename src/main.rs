pub mod config;
mod events;
mod db;
mod index;
mod scan;
mod ui;
mod thumbnail;
pub mod state;

use libadwaita as adw;
use libadwaita::prelude::*;
use libadwaita::{glib, Application};

fn main() -> glib::ExitCode {
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Failed to create tokio runtime: {}", e);
            return glib::ExitCode::FAILURE;
        }
    };
    let _guard = rt.enter();

    let app = Application::builder()
        .application_id("com.github.vesper.gallery")
        .build();

    app.connect_activate(move |app| {
        let db_path_res = std::env::current_dir().map(|d| d.join(crate::config::DB_NAME));
        let db_res = db_path_res.and_then(|p| crate::db::Database::open(&p).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
        let state_res = std::panic::catch_unwind(|| crate::state::AppState::load());
        
        match (db_res, state_res) {
            (Ok(db), Ok(state)) => {
                let db_arc = std::sync::Arc::new(std::sync::Mutex::new(db));
                let state_arc = std::sync::Arc::new(std::sync::Mutex::new(state));
                ui::build_ui(app, db_arc, state_arc);
            }
            _ => {
                let dialog = adw::MessageDialog::builder()
                    .heading("Unexpected Error")
                    .body("An unexpected error occurred. The application will close.")
                    .build();
                dialog.add_response("close", "Close");
                let app_clone = app.clone();
                dialog.connect_response(None, move |_, _| {
                    app_clone.quit();
                });
                dialog.present();
            }
        }
    });

    let ret = app.run();
    rt.shutdown_background();
    ret
}
