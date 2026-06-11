mod events;
mod db;
mod index;
mod scan;
mod ui;
mod thumbnail;

use libadwaita::prelude::*;
use libadwaita::{glib, Application};

fn main() -> glib::ExitCode {
    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    let _guard = rt.enter();

    // Open the database
    let db_path = std::env::current_dir().unwrap().join("vesper.db");
    let db = crate::db::Database::open(&db_path).expect("Failed to open database");
    let db = std::sync::Arc::new(std::sync::Mutex::new(db));

    let app = Application::builder()
        .application_id("com.github.vesper.gallery")
        .build();

    let db_clone = db.clone();
    app.connect_activate(move |app| {
        ui::build_ui(app, db_clone.clone());
    });

    let ret = app.run();
    rt.shutdown_background();
    ret
}
