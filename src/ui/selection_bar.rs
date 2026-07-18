//! The grid-scoped selection action bar (`action_bar_revealer`, Arch §9).
//!
//! It shows the selection count and exactly the four allowed batch actions —
//! Open, Reveal in Folder, Copy Path, Clear Selection (Product §6). No action
//! deletes, moves, renames, tags, rates, or collects. Clipboard preparation and
//! file-manager / external launches run off the input path via idle/async work.

use libadwaita::gtk::{self, glib};
use libadwaita::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Callback invoked by "Clear Selection" to leave selection mode entirely
/// (untoggle the header select button, hide the bar).
pub type ExitSelection = Rc<dyn Fn()>;

pub struct SelectionBar {
    pub revealer: gtk::Revealer,
    selection_model: gtk::MultiSelection,
    selection_anchor: Rc<RefCell<Option<u32>>>,
    selection_history: Rc<RefCell<Vec<u32>>>,
}

impl SelectionBar {
    pub fn new(
        selection_model: gtk::MultiSelection,
        sort_list_model: gtk::SortListModel,
        selection_anchor: Rc<RefCell<Option<u32>>>,
        selection_history: Rc<RefCell<Vec<u32>>>,
        selection_mode: Rc<RefCell<bool>>,
        exit_selection: ExitSelection,
    ) -> Self {
        let bar = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .css_classes(["selection-bar"])
            .hexpand(true)
            .valign(gtk::Align::End)
            .spacing(12)
            .build();

        let count_label = gtk::Label::builder()
            .css_classes(["selection-count"])
            .halign(gtk::Align::Start)
            .hexpand(true)
            .build();

        let open_btn = action_button("document-open-symbolic", "Open");
        let reveal_btn = action_button("folder-symbolic", "Reveal in Folder");
        let copy_btn = action_button("edit-copy-symbolic", "Copy Path");
        let clear_btn = action_button("edit-clear-symbolic", "Clear Selection");

        bar.append(&count_label);
        bar.append(&open_btn);
        bar.append(&reveal_btn);
        bar.append(&copy_btn);
        bar.append(&clear_btn);

        let revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideUp)
            .child(&bar)
            .halign(gtk::Align::Fill)
            .valign(gtk::Align::End)
            .build();

        // Open: launch each selected file with its default handler, off-thread.
        open_btn.connect_clicked({
            let selection_model = selection_model.clone();
            let sort_list_model = sort_list_model.clone();
            move |_| {
                for path in selected_paths(&selection_model, &sort_list_model) {
                    launch_uri_for_path(&path, false);
                }
            }
        });

        // Reveal in Folder: open the file manager at the first item's folder.
        reveal_btn.connect_clicked({
            let selection_model = selection_model.clone();
            let sort_list_model = sort_list_model.clone();
            move |_| {
                if let Some(first) = selected_paths(&selection_model, &sort_list_model).first() {
                    launch_uri_for_path(first, true);
                }
            }
        });

        // Copy Path: join the selected paths onto the clipboard from idle work.
        copy_btn.connect_clicked({
            let selection_model = selection_model.clone();
            let sort_list_model = sort_list_model.clone();
            move |_| {
                let paths = selected_paths(&selection_model, &sort_list_model);
                glib::idle_add_local_once(move || {
                    if let Some(display) = gtk::gdk::Display::default() {
                        display.clipboard().set_text(&paths.join("\n"));
                    }
                });
            }
        });

        // Clear Selection: empty the selection and leave selection mode.
        clear_btn.connect_clicked({
            let selection_model = selection_model.clone();
            let exit_selection = exit_selection.clone();
            move |_| {
                selection_model.unselect_all();
                exit_selection();
            }
        });

        selection_model.connect_selection_changed({
            let selection_model = selection_model.clone();
            let selection_anchor = selection_anchor.clone();
            let selection_history = selection_history.clone();
            let selection_mode = selection_mode.clone();
            let revealer = revealer.clone();
            move |_, _, _| {
                let count = selection_model.selection().size();
                count_label.set_text(&format!("Selected {count} items"));
                if count > 0 {
                    revealer.set_reveal_child(true);
                } else {
                    selection_history.borrow_mut().clear();
                    *selection_anchor.borrow_mut() = None;
                    // Keep the bar up while selection mode stays engaged.
                    revealer.set_reveal_child(*selection_mode.borrow());
                }
            }
        });

        Self {
            revealer,
            selection_model,
            selection_anchor,
            selection_history,
        }
    }

    pub fn install_grid_keyboard_handler(
        &self,
        grid_view: &gtk::GridView,
        search_entry: &gtk::SearchEntry,
        viewer: Rc<crate::ui::viewer::Viewer>,
        focused_position: Rc<RefCell<Option<u32>>>,
        exit_selection: ExitSelection,
    ) {
        let key_ctrl = gtk::EventControllerKey::new();
        let selection_model = self.selection_model.clone();
        let selection_anchor = self.selection_anchor.clone();
        let selection_history = self.selection_history.clone();
        key_ctrl.connect_key_pressed(move |_, keyval, _, state| {
            if keyval == gtk::gdk::Key::Escape {
                // Fullscreen-exit / viewer-close takes precedence; only then
                // does Escape clear the selection.
                if viewer.handle_escape() {
                    return glib::Propagation::Stop;
                }
                if selection_model.selection().size() > 0 {
                    selection_model.unselect_all();
                    exit_selection();
                    return glib::Propagation::Stop;
                }
                return glib::Propagation::Proceed;
            }
            if (keyval == gtk::gdk::Key::a || keyval == gtk::gdk::Key::A)
                && state.contains(gtk::gdk::ModifierType::CONTROL_MASK)
            {
                selection_model.select_all();
                let mut history = selection_history.borrow_mut();
                history.clear();
                for i in 0..selection_model.n_items() {
                    history.push(i);
                }
                *selection_anchor.borrow_mut() = history.last().copied();
                return glib::Propagation::Stop;
            }

            // Ctrl+Space toggles the focused cell; Shift+Space range-selects
            // from the anchor to it — keyboard parity with modifier-click.
            // Suppressed while the viewer is open (space controls playback).
            if keyval == gtk::gdk::Key::space && !viewer.is_open() {
                let is_ctrl = state.contains(gtk::gdk::ModifierType::CONTROL_MASK);
                let is_shift = state.contains(gtk::gdk::ModifierType::SHIFT_MASK);
                let pos = match *focused_position.borrow() {
                    Some(pos) if pos < selection_model.n_items() => pos,
                    _ => return glib::Propagation::Proceed,
                };
                if is_ctrl {
                    let mut history = selection_history.borrow_mut();
                    if selection_model.is_selected(pos) {
                        selection_model.unselect_item(pos);
                        if let Some(idx) = history.iter().position(|&p| p == pos) {
                            history.remove(idx);
                        }
                        *selection_anchor.borrow_mut() = history.last().copied();
                    } else {
                        selection_model.select_item(pos, false);
                        if let Some(idx) = history.iter().position(|&p| p == pos) {
                            history.remove(idx);
                        }
                        history.push(pos);
                        *selection_anchor.borrow_mut() = Some(pos);
                    }
                    return glib::Propagation::Stop;
                }
                if is_shift {
                    let anchor = match *selection_anchor.borrow() {
                        Some(anchor) if selection_model.is_selected(anchor) => anchor,
                        _ => selection_history.borrow().last().copied().unwrap_or(pos),
                    };
                    let start = std::cmp::min(anchor, pos);
                    let end = std::cmp::max(anchor, pos);
                    selection_model.select_range(start, end - start + 1, false);
                    let mut history = selection_history.borrow_mut();
                    for p in start..=end {
                        if selection_model.is_selected(p) {
                            if let Some(idx) = history.iter().position(|&x| x == p) {
                                history.remove(idx);
                            }
                            history.push(p);
                        }
                    }
                    drop(history);
                    *selection_anchor.borrow_mut() = Some(pos);
                    return glib::Propagation::Stop;
                }
            }

            glib::Propagation::Proceed
        });
        grid_view.add_controller(key_ctrl);

        let search_key_ctrl = gtk::EventControllerKey::new();
        let search_entry_clone = search_entry.clone();
        search_key_ctrl.connect_key_pressed(move |_, keyval, _, _| {
            if keyval == gtk::gdk::Key::Escape {
                search_entry_clone.set_text("");
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        search_entry.add_controller(search_key_ctrl);
    }
}

fn action_button(icon: &str, label: &str) -> gtk::Button {
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .build();
    content.append(&gtk::Image::from_icon_name(icon));
    content.append(&gtk::Label::new(Some(label)));
    let button = gtk::Button::builder()
        .css_classes(["flat"])
        .child(&content)
        .build();
    button.update_property(&[gtk::accessible::Property::Label(label)]);
    button
}

/// Launches the default handler for a path, or its parent folder when `folder`
/// is set (Reveal in Folder). Runs asynchronously so the input path never blocks.
fn launch_uri_for_path(path: &str, folder: bool) {
    let target = if folder {
        std::path::Path::new(path).parent().map(|p| p.to_path_buf())
    } else {
        Some(std::path::PathBuf::from(path))
    };
    if let Some(target) = target
        && let Ok(uri) = glib::filename_to_uri(&target, None)
    {
        gtk::gio::AppInfo::launch_default_for_uri_async(
            &uri,
            None::<&gtk::gio::AppLaunchContext>,
            None::<&gtk::gio::Cancellable>,
            |result| {
                if let Err(error) = result {
                    tracing::warn!(%error, "failed to launch default handler");
                }
            },
        );
    }
}

fn selected_paths(
    selection_model: &gtk::MultiSelection,
    sort_list_model: &gtk::SortListModel,
) -> Vec<String> {
    let bitset = selection_model.selection();
    let mut paths = Vec::new();
    let max = if bitset.is_empty() {
        0
    } else {
        bitset.maximum()
    };
    for i in 0..max + 1 {
        if bitset.contains(i)
            && let Some(item) = sort_list_model.item(i)
            && let Ok(media) = item.downcast::<crate::ui::model::MediaItem>()
        {
            paths.push(media.property::<String>("path"));
        }
    }
    paths
}
