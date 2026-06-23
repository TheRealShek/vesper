use libadwaita::gtk::{self, prelude::*};

pub fn show_shortcuts_window(parent: &libadwaita::ApplicationWindow) {
    let shortcuts_win = gtk::ShortcutsWindow::builder()
        .transient_for(parent)
        .modal(true)
        .build();

    let main_section = gtk::ShortcutsSection::builder()
        .title("Shortcuts")
        .visible(true)
        .max_height(10)
        .build();

    // Navigation
    let nav_group = gtk::ShortcutsGroup::builder()
        .title("Navigation")
        .visible(true)
        .build();

    nav_group.append(
        &gtk::ShortcutsShortcut::builder()
            .title("Move focus")
            .accelerator("Up Down Left Right")
            .visible(true)
            .build(),
    );

    nav_group.append(
        &gtk::ShortcutsShortcut::builder()
            .title("Open selected item")
            .accelerator("Return")
            .visible(true)
            .build(),
    );

    nav_group.append(
        &gtk::ShortcutsShortcut::builder()
            .title("Close viewer / clear selection")
            .accelerator("Escape")
            .visible(true)
            .build(),
    );

    main_section.append(&nav_group);

    // Selection
    let sel_group = gtk::ShortcutsGroup::builder()
        .title("Selection")
        .visible(true)
        .build();

    sel_group.append(
        &gtk::ShortcutsShortcut::builder()
            .title("Toggle selection")
            .subtitle("Ctrl+Click")
            .shortcut_type(gtk::ShortcutType::Gesture)
            .visible(true)
            .build(),
    );

    sel_group.append(
        &gtk::ShortcutsShortcut::builder()
            .title("Range selection")
            .subtitle("Shift+Click")
            .shortcut_type(gtk::ShortcutType::Gesture)
            .visible(true)
            .build(),
    );

    sel_group.append(
        &gtk::ShortcutsShortcut::builder()
            .title("Select all")
            .accelerator("<Control>a")
            .visible(true)
            .build(),
    );

    main_section.append(&sel_group);

    // Viewer
    let viewer_group = gtk::ShortcutsGroup::builder()
        .title("Viewer")
        .visible(true)
        .build();

    viewer_group.append(
        &gtk::ShortcutsShortcut::builder()
            .title("Toggle info panel")
            .accelerator("i")
            .visible(true)
            .build(),
    );

    viewer_group.append(
        &gtk::ShortcutsShortcut::builder()
            .title("Previous / Next item")
            .accelerator("Left Right")
            .visible(true)
            .build(),
    );

    viewer_group.append(
        &gtk::ShortcutsShortcut::builder()
            .title("Toggle fullscreen")
            .accelerator("f")
            .visible(true)
            .build(),
    );

    viewer_group.append(
        &gtk::ShortcutsShortcut::builder()
            .title("Play / Pause video")
            .accelerator("space")
            .visible(true)
            .build(),
    );

    viewer_group.append(
        &gtk::ShortcutsShortcut::builder()
            .title("Close viewer")
            .accelerator("Escape")
            .visible(true)
            .build(),
    );

    main_section.append(&viewer_group);

    shortcuts_win.set_child(Some(&main_section));
    shortcuts_win.present();
}
