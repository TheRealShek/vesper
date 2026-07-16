use libadwaita::gtk::{self, glib};
use libadwaita::prelude::*;
use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

const MULTI_FOLDER_TOOLTIP: &str = "Selected files must reside in the same folder.";

pub struct SelectionBar {
    pub revealer: gtk::Revealer,
    selection_model: gtk::MultiSelection,
    selection_anchor: Rc<RefCell<Option<u32>>>,
    selection_history: Rc<RefCell<Vec<u32>>>,
}

impl SelectionBar {
    pub fn new(
        selection_model: gtk::MultiSelection,
        filter_model: gtk::SortListModel,
        selection_anchor: Rc<RefCell<Option<u32>>>,
        selection_history: Rc<RefCell<Vec<u32>>>,
    ) -> Self {
        let action_bar_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .css_classes(["action-bar"])
            .halign(gtk::Align::Center)
            .valign(gtk::Align::End)
            .margin_bottom(24)
            .spacing(12)
            .build();

        let sel_count_label = gtk::Label::builder()
            .css_classes(["title-4"])
            .margin_start(8)
            .margin_end(8)
            .build();
        let open_loc_btn = gtk::Button::builder().label("Open file location").build();
        open_loc_btn.set_tooltip_text(Some("Open containing folder"));
        let copy_path_btn = gtk::Button::builder().label("Copy path(s)").build();
        copy_path_btn.set_tooltip_text(Some("Copy selected paths"));
        let deselect_btn = gtk::Button::builder()
            .label("Deselect all")
            .css_classes(["destructive-action"])
            .build();
        deselect_btn.set_tooltip_text(Some("Deselect all"));

        action_bar_box.append(&sel_count_label);
        action_bar_box.append(&open_loc_btn);
        action_bar_box.append(&copy_path_btn);
        action_bar_box.append(&deselect_btn);

        let revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideUp)
            .child(&action_bar_box)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::End)
            .build();

        deselect_btn.connect_clicked({
            let selection_model = selection_model.clone();
            move |_| {
                selection_model.unselect_all();
            }
        });

        copy_path_btn.connect_clicked({
            let selection_model = selection_model.clone();
            let filter_model = filter_model.clone();
            move |_| {
                let paths = selected_paths(&selection_model, &filter_model);
                if let Some(display) = gtk::gdk::Display::default() {
                    display.clipboard().set_text(&paths.join("\n"));
                }
            }
        });

        open_loc_btn.connect_clicked({
            let selection_model = selection_model.clone();
            let filter_model = filter_model.clone();
            move |_| {
                let paths = selected_paths(&selection_model, &filter_model);
                if let Some(first_path) = paths.first()
                    && let Some(parent) = std::path::Path::new(first_path).parent()
                    && let Ok(uri) = glib::filename_to_uri(parent, None)
                {
                    let _ = gtk::gio::AppInfo::launch_default_for_uri(
                        &uri,
                        None::<&gtk::gio::AppLaunchContext>,
                    );
                }
            }
        });

        selection_model.connect_selection_changed({
            let selection_model = selection_model.clone();
            let filter_model = filter_model.clone();
            let selection_anchor = selection_anchor.clone();
            let selection_history = selection_history.clone();
            let revealer = revealer.clone();
            move |_, _, _| {
                let count = selection_model.selection().size();
                if count > 0 {
                    sel_count_label.set_text(&format!("{} selected", count));
                    revealer.set_reveal_child(true);
                } else {
                    selection_history.borrow_mut().clear();
                    *selection_anchor.borrow_mut() = None;
                    revealer.set_reveal_child(false);
                }

                let paths = selected_paths(&selection_model, &filter_model);
                let can_open = selection_parent_count(&paths) == 1;
                open_loc_btn.set_sensitive(can_open);
                open_loc_btn.set_tooltip_text(Some(if can_open {
                    "Open containing folder"
                } else if count > 0 {
                    MULTI_FOLDER_TOOLTIP
                } else {
                    "Open containing folder"
                }));
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
    ) {
        let key_ctrl = gtk::EventControllerKey::new();
        let selection_model = self.selection_model.clone();
        let selection_anchor = self.selection_anchor.clone();
        let selection_history = self.selection_history.clone();
        let search_entry = search_entry.clone();
        key_ctrl.connect_key_pressed(move |_, keyval, _, state| {
            if keyval == gtk::gdk::Key::Escape {
                if viewer.is_open() {
                    viewer.close();
                    return glib::Propagation::Stop;
                }
                if selection_model.selection().size() > 0 {
                    selection_model.unselect_all();
                    selection_history.borrow_mut().clear();
                    *selection_anchor.borrow_mut() = None;
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
                let total = selection_model.n_items();
                for i in 0..total {
                    history.push(i);
                }
                if let Some(last) = history.last().copied() {
                    *selection_anchor.borrow_mut() = Some(last);
                } else {
                    *selection_anchor.borrow_mut() = None;
                }
                return glib::Propagation::Stop;
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

fn selection_parent_count(paths: &[String]) -> usize {
    paths
        .iter()
        .filter_map(|path| Path::new(path).parent().map(PathBuf::from))
        .collect::<HashSet<_>>()
        .len()
}

fn selected_paths(
    selection_model: &gtk::MultiSelection,
    filter_model: &gtk::SortListModel,
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
            && let Some(item) = filter_model.item(i)
            && let Ok(media) = item.downcast::<crate::ui::model::MediaItem>()
        {
            paths.push(media.property::<String>("path"));
        }
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::selection_parent_count;

    #[test]
    fn open_location_disables_for_multiple_parent_folders() {
        let same_folder = vec![
            "/library/trip/a.jpg".to_string(),
            "/library/trip/b.jpg".to_string(),
        ];
        assert_eq!(selection_parent_count(&same_folder), 1);

        let different_folders = vec![
            "/library/trip/a.jpg".to_string(),
            "/library/family/b.jpg".to_string(),
        ];
        assert_eq!(selection_parent_count(&different_folders), 2);
    }
}
