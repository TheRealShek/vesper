use crate::events::ChannelSendExt;
use libadwaita::gtk::{self};
use libadwaita::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

type RefreshCb = Rc<dyn Fn()>;

#[derive(Clone)]
pub struct FilterController {
    pub filter_model: gtk::FilterListModel,
    pub sort_list_model: gtk::SortListModel,
    filter: gtk::CustomFilter,
    selected_tags: Rc<RefCell<Vec<String>>>,
    match_all: Rc<RefCell<bool>>,
    search_query: Rc<RefCell<String>>,
    active_filter_pill: gtk::Button,
    match_mode_box: gtk::Box,
    tag_list_box: gtk::ListBox,
    tag_names: Rc<RefCell<Vec<String>>>,
    send_query: RefreshCb,
}

pub struct FilterControllerParams {
    pub list_store: gtk::gio::ListStore,
    pub selected_tags: Rc<RefCell<Vec<String>>>,
    pub match_all: Rc<RefCell<bool>>,
    pub search_query: Rc<RefCell<String>>,
    pub search_entry: gtk::SearchEntry,
    pub tag_list_box: gtk::ListBox,
    pub tag_names: Rc<RefCell<Vec<String>>>,
    pub match_any_radio: gtk::CheckButton,
    pub match_all_radio: gtk::CheckButton,
    pub match_mode_box: gtk::Box,
    pub active_filter_pill: gtk::Button,
    pub no_results_clear_btn: gtk::Button,
    pub sort_radios: Vec<gtk::CheckButton>,
    pub initial_sort: String,
    pub app_tx: tokio::sync::mpsc::Sender<crate::events::AppEvent>,
}

impl FilterController {
    pub fn new(params: FilterControllerParams) -> Self {
        let filter = crate::ui::filter_sort::create_filter(
            params.selected_tags.clone(),
            params.match_all.clone(),
            params.search_query.clone(),
        );
        let filter_model =
            gtk::FilterListModel::new(Some(params.list_store.clone()), Some(filter.clone()));

        let initial_idx = sort_model_list()
            .iter()
            .position(|&s| s == params.initial_sort)
            .unwrap_or(0) as u32;
        let active_sort_idx = Rc::new(RefCell::new(initial_idx));
        let sorter = crate::ui::filter_sort::create_sorter(
            active_sort_idx.clone(),
            params.search_query.clone(),
        );
        let sort_list_model =
            gtk::SortListModel::new(Some(filter_model.clone()), Some(sorter.clone()));

        let send_query = build_query_dispatcher(
            params.selected_tags.clone(),
            params.match_all.clone(),
            params.search_query.clone(),
            active_sort_idx.clone(),
            params.app_tx,
        );

        let controller = Self {
            filter_model,
            sort_list_model,
            filter,
            selected_tags: params.selected_tags,
            match_all: params.match_all,
            search_query: params.search_query,
            active_filter_pill: params.active_filter_pill,
            match_mode_box: params.match_mode_box,
            tag_list_box: params.tag_list_box,
            tag_names: params.tag_names,
            send_query: send_query.clone(),
        };

        controller.connect_sort_radios(
            params.sort_radios,
            active_sort_idx,
            sorter,
            send_query.clone(),
        );
        controller.connect_match_mode(
            params.match_any_radio,
            params.match_all_radio,
            send_query.clone(),
        );
        controller.connect_tag_list(send_query.clone());
        controller.connect_search_entry(params.search_entry.clone(), send_query.clone());
        controller.connect_clear_buttons(
            params.search_entry,
            params.no_results_clear_btn,
            send_query,
        );

        controller
    }

    pub fn refresh(&self) {
        self.filter.changed(gtk::FilterChange::Different);
        update_filter_ui(
            &self.active_filter_pill,
            &self.selected_tags,
            &self.match_mode_box,
            &self.search_query,
        );
        (self.send_query)();
    }

    pub fn apply_restored_state(&self, tags: &[crate::events::UiTag], active_tags: &[String]) {
        if active_tags.is_empty() {
            return;
        }

        let mut current_selected = self.selected_tags.borrow_mut();
        for (i, tag) in tags.iter().enumerate() {
            if active_tags.contains(&tag.display_name)
                && let Some(row) = self.tag_list_box.row_at_index(i as i32)
            {
                row.add_css_class("active");
            }
            if active_tags.contains(&tag.display_name)
                && !current_selected.contains(&tag.display_name)
            {
                current_selected.push(tag.display_name.clone());
            }
        }
        drop(current_selected);

        self.refresh();
    }

    fn connect_sort_radios(
        &self,
        sort_radios: Vec<gtk::CheckButton>,
        active_sort_idx: Rc<RefCell<u32>>,
        sorter: gtk::CustomSorter,
        send_query: RefreshCb,
    ) {
        for (i, radio) in sort_radios.iter().enumerate() {
            let active_sort_idx = active_sort_idx.clone();
            let sorter = sorter.clone();
            let send_query = send_query.clone();
            radio.connect_toggled(move |btn| {
                if btn.is_active() {
                    *active_sort_idx.borrow_mut() = i as u32;
                    sorter.changed(gtk::SorterChange::Different);
                    send_query();
                }
            });
        }
    }

    fn connect_match_mode(
        &self,
        match_any_radio: gtk::CheckButton,
        match_all_radio: gtk::CheckButton,
        send_query: RefreshCb,
    ) {
        match_any_radio.connect_toggled({
            let match_all = self.match_all.clone();
            let filter = self.filter.clone();
            let send_query = send_query.clone();
            move |btn| {
                if btn.is_active() {
                    *match_all.borrow_mut() = false;
                    filter.changed(gtk::FilterChange::Different);
                    send_query();
                }
            }
        });

        match_all_radio.connect_toggled({
            let match_all = self.match_all.clone();
            let filter = self.filter.clone();
            move |btn| {
                if btn.is_active() {
                    *match_all.borrow_mut() = true;
                    filter.changed(gtk::FilterChange::Different);
                    send_query();
                }
            }
        });
    }

    fn connect_tag_list(&self, send_query: RefreshCb) {
        self.tag_list_box.connect_row_activated({
            let selected_tags = self.selected_tags.clone();
            let filter = self.filter.clone();
            let tag_names = self.tag_names.clone();
            let active_filter_pill = self.active_filter_pill.clone();
            let match_mode_box = self.match_mode_box.clone();
            let search_query = self.search_query.clone();
            move |_list_box, row| {
                if row.has_css_class("active") {
                    row.remove_css_class("active");
                } else {
                    row.add_css_class("active");
                }

                let mut new_selection = selected_tags.borrow().clone();
                let index = row.index() as usize;
                if let Some(name) = tag_names.borrow().get(index) {
                    if row.has_css_class("active") {
                        if !new_selection.contains(name) {
                            new_selection.push(name.clone());
                        }
                    } else {
                        new_selection.retain(|t| t != name);
                    }
                }

                *selected_tags.borrow_mut() = new_selection;
                filter.changed(gtk::FilterChange::Different);
                update_filter_ui(
                    &active_filter_pill,
                    &selected_tags,
                    &match_mode_box,
                    &search_query,
                );
                send_query();
            }
        });
    }

    fn connect_search_entry(&self, search_entry: gtk::SearchEntry, send_query: RefreshCb) {
        search_entry.connect_search_changed({
            let search_query = self.search_query.clone();
            let filter = self.filter.clone();
            let active_filter_pill = self.active_filter_pill.clone();
            let selected_tags = self.selected_tags.clone();
            let match_mode_box = self.match_mode_box.clone();
            move |entry| {
                *search_query.borrow_mut() = entry.text().to_string().to_lowercase();
                filter.changed(gtk::FilterChange::Different);
                update_filter_ui(
                    &active_filter_pill,
                    &selected_tags,
                    &match_mode_box,
                    &search_query,
                );
                send_query();
            }
        });
    }

    fn connect_clear_buttons(
        &self,
        search_entry: gtk::SearchEntry,
        no_results_clear_btn: gtk::Button,
        send_query: RefreshCb,
    ) {
        let clear_all_action: RefreshCb = Rc::new({
            let tag_list_box = self.tag_list_box.clone();
            let search_entry = search_entry.clone();
            let selected_tags = self.selected_tags.clone();
            let search_query = self.search_query.clone();
            let filter = self.filter.clone();
            let active_filter_pill = self.active_filter_pill.clone();
            let match_mode_box = self.match_mode_box.clone();
            move || {
                let mut i = 0;
                while let Some(row) = tag_list_box.row_at_index(i) {
                    row.remove_css_class("active");
                    i += 1;
                }
                search_entry.set_text("");
                search_query.borrow_mut().clear();
                selected_tags.borrow_mut().clear();
                filter.changed(gtk::FilterChange::Different);
                update_filter_ui(
                    &active_filter_pill,
                    &selected_tags,
                    &match_mode_box,
                    &search_query,
                );
                send_query();
            }
        });

        self.active_filter_pill.connect_clicked({
            let clear_all = clear_all_action.clone();
            move |_| clear_all()
        });
        no_results_clear_btn.connect_clicked(move |_| clear_all_action());
    }
}

fn build_query_dispatcher(
    selected_tags: Rc<RefCell<Vec<String>>>,
    match_all: Rc<RefCell<bool>>,
    search_query: Rc<RefCell<String>>,
    active_sort_idx: Rc<RefCell<u32>>,
    app_tx: tokio::sync::mpsc::Sender<crate::events::AppEvent>,
) -> RefreshCb {
    Rc::new(move || {
        let q = crate::events::MediaQuery {
            tags: selected_tags.borrow().clone(),
            tag_mode: if *match_all.borrow() {
                crate::events::TagMode::All
            } else {
                crate::events::TagMode::Any
            },
            search: {
                let s = search_query.borrow().clone();
                if s.is_empty() { None } else { Some(s) }
            },
            sort: match *active_sort_idx.borrow() {
                0 => crate::events::SortOrder::DateModifiedDesc,
                1 => crate::events::SortOrder::DateModifiedAsc,
                2 => crate::events::SortOrder::DateCreatedDesc,
                3 => crate::events::SortOrder::DateCreatedAsc,
                4 => crate::events::SortOrder::FilenameAsc,
                5 => crate::events::SortOrder::FilenameDesc,
                6 => crate::events::SortOrder::FileSizeDesc,
                _ => crate::events::SortOrder::FileSizeAsc,
            },
        };
        app_tx.send_critical(crate::events::AppEvent::QueryMedia(q));
    })
}

fn update_filter_ui(
    active_filter_pill: &gtk::Button,
    selected_tags: &Rc<RefCell<Vec<String>>>,
    match_mode_box: &gtk::Box,
    search_query: &Rc<RefCell<String>>,
) {
    let active_count = selected_tags.borrow().len();
    let has_tags = active_count > 0;
    let has_search = !search_query.borrow().is_empty();

    active_filter_pill.set_visible(has_tags || has_search);
    match_mode_box.set_visible(has_tags);

    if has_tags || has_search {
        let label = match (has_tags, has_search) {
            (true, true) => format!("● {} tags + search", active_count),
            (true, false) => format!("● {} tags", active_count),
            (false, true) => "● Search".to_string(),
            (false, false) => unreachable!(),
        };
        active_filter_pill.set_label(&label);
    }
}

fn sort_model_list() -> [&'static str; 8] {
    [
        "Date modified (newest first)",
        "Date modified (oldest first)",
        "Date created (newest first)",
        "Date created (oldest first)",
        "Filename (A → Z)",
        "Filename (Z → A)",
        "File size (largest first)",
        "File size (smallest first)",
    ]
}
