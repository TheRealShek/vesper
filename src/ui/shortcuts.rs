use libadwaita::gtk::{self, prelude::*};

/// One shortcut row: a human title paired with either key accelerators or a
/// pointer-gesture description.
enum Binding {
    /// Space-separated accelerators in GTK notation (e.g. "<Control>a",
    /// "Up Down Left Right"). Each token renders as its own key label.
    Keys(&'static str),
    /// A pointer gesture shown as plain text (e.g. "Ctrl+Click").
    Gesture(&'static str),
}

/// Present the keyboard-shortcuts help.
///
/// This is a hand-rolled window rather than `gtk::ShortcutsWindow`: the latter
/// is deprecated as of GTK 4.18 and, on GTK 4.22, tears down its internal
/// search bar unsafely — closing it emits `GTK_IS_SEARCH_BAR` /
/// `GTK_IS_EDITABLE` criticals and a use-after-free that can crash the process.
/// Building the window from stable widgets avoids that path entirely while
/// keeping the same content and a native accelerator rendering via
/// `gtk::ShortcutLabel`.
pub fn show_shortcuts_window(parent: &libadwaita::ApplicationWindow) {
    let sections: [(&str, &[(&str, Binding)]); 3] = [
        (
            "Navigation",
            &[
                ("Move focus", Binding::Keys("Up Down Left Right")),
                ("Open selected item", Binding::Keys("Return")),
                ("Close viewer / clear selection", Binding::Keys("Escape")),
            ],
        ),
        (
            "Selection",
            &[
                ("Toggle selection", Binding::Gesture("Ctrl+Click")),
                ("Range selection", Binding::Gesture("Shift+Click")),
                ("Select all", Binding::Keys("<Control>a")),
            ],
        ),
        (
            "Viewer",
            &[
                ("Toggle info panel", Binding::Keys("i")),
                ("Previous / Next item", Binding::Keys("Left Right")),
                ("Toggle fullscreen", Binding::Keys("f")),
                ("Play / Pause video", Binding::Keys("space")),
                ("Close viewer", Binding::Keys("Escape")),
            ],
        ),
    ];

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(24)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();

    for (title, rows) in sections {
        let group = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .build();

        group.append(
            &gtk::Label::builder()
                .label(title)
                .halign(gtk::Align::Start)
                .css_classes(["heading"])
                .build(),
        );

        for (row_title, binding) in rows {
            let row = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(16)
                .build();

            row.append(
                &gtk::Label::builder()
                    .label(*row_title)
                    .halign(gtk::Align::Start)
                    .hexpand(true)
                    .xalign(0.0)
                    .build(),
            );

            match binding {
                Binding::Keys(accel) => {
                    let keys = gtk::Box::builder()
                        .orientation(gtk::Orientation::Horizontal)
                        .spacing(6)
                        .halign(gtk::Align::End)
                        .build();
                    for token in accel.split_whitespace() {
                        keys.append(&gtk::ShortcutLabel::builder().accelerator(token).build());
                    }
                    row.append(&keys);
                }
                Binding::Gesture(text) => {
                    row.append(
                        &gtk::Label::builder()
                            .label(*text)
                            .halign(gtk::Align::End)
                            .css_classes(["dim-label"])
                            .build(),
                    );
                }
            }

            group.append(&row);
        }

        content.append(&group);
    }

    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .propagate_natural_height(true)
        .child(&content)
        .build();

    let toolbar = libadwaita::ToolbarView::builder()
        .content(&scrolled)
        .build();
    toolbar.add_top_bar(
        &libadwaita::HeaderBar::builder()
            .title_widget(&libadwaita::WindowTitle::new("Keyboard Shortcuts", ""))
            .build(),
    );

    let window = libadwaita::Window::builder()
        .transient_for(parent)
        .modal(true)
        .title("Keyboard Shortcuts")
        .default_width(420)
        .default_height(560)
        .content(&toolbar)
        .build();

    // Esc closes the help window, matching the previous behavior.
    let key_controller = gtk::EventControllerKey::new();
    let window_close = window.clone();
    key_controller.connect_key_pressed(move |_, keyval, _, _| {
        if keyval == gtk::gdk::Key::Escape {
            window_close.close();
            return gtk::glib::Propagation::Stop;
        }
        gtk::glib::Propagation::Proceed
    });
    window.add_controller(key_controller);

    window.present();
}
