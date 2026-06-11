mod events;
mod db;
mod index;

use libadwaita::prelude::*;
use libadwaita::{glib, Application, ApplicationWindow};

fn main() -> glib::ExitCode {
    let app = Application::builder()
        .application_id("com.github.vesper.gallery")
        .build();

    app.connect_activate(|app| {
        let window = ApplicationWindow::builder()
            .application(app)
            .title("Vesper")
            .default_width(800)
            .default_height(600)
            .build();

        window.present();
    });

    app.run()
}
