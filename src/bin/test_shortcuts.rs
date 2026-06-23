use gtk::{
    Application, ApplicationWindow, ShortcutsGroup, ShortcutsSection, ShortcutsShortcut,
    ShortcutsWindow, prelude::*,
};
use libadwaita::gtk;

fn main() {
    let app = Application::builder()
        .application_id("org.test.shortcuts")
        .build();
    app.connect_activate(|app| {
        let win = ApplicationWindow::new(app);

        let shortcuts_win = ShortcutsWindow::builder()
            .transient_for(&win)
            .modal(true)
            .build();

        let nav_section = ShortcutsSection::builder()
            .title("Navigation")
            .visible(true)
            .build();

        let nav_group = ShortcutsGroup::builder()
            .title("Selection")
            .visible(true)
            .build();

        nav_group.append(
            &ShortcutsShortcut::builder()
                .title("Toggle selection")
                .subtitle("Ctrl+Click")
                .shortcut_type(gtk::ShortcutType::Gesture)
                .visible(true)
                .build(),
        );

        nav_section.append(&nav_group);
        shortcuts_win.set_child(Some(&nav_section));
    });
}
