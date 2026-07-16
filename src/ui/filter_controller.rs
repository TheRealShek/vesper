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
    selected_tags: Rc<RefCell<Vec<crate::state::TagFilter>>>,
    match_all: Rc<RefCell<bool>>,
    search_query: Rc<RefCell<String>>,
    clear_filters_button: gtk::Button,
    match_mode_box: gtk::Box,
    tag_list_box: gtk::ListBox,
    tags: Rc<RefCell<Vec<crate::events::UiTag>>>,
    send_query: RefreshCb,
}

pub struct FilterControllerParams {
    pub list_store: gtk::gio::ListStore,
    pub selected_tags: Rc<RefCell<Vec<crate::state::TagFilter>>>,
    pub match_all: Rc<RefCell<bool>>,
    pub search_query: Rc<RefCell<String>>,
    pub search_entry: gtk::SearchEntry,
    pub tag_list_box: gtk::ListBox,
    pub tags: Rc<RefCell<Vec<crate::events::UiTag>>>,
    pub match_any_radio: gtk::CheckButton,
    pub match_all_radio: gtk::CheckButton,
    pub match_mode_box: gtk::Box,
    pub clear_filters_button: gtk::Button,
    pub no_results_clear_btn: gtk::Button,
    pub sort_radios: Vec<gtk::CheckButton>,
    pub initial_sort: String,
    pub app_tx: tokio::sync::mpsc::Sender<crate::events::AppEvent>,
    /// Shared monotonic query-generation counter (B-2). Every dispatched query
    /// takes the next generation so the UI can discard superseded results.
    pub query_generation: Rc<RefCell<crate::events::QueryGeneration>>,
}

impl FilterController {
    pub fn new(params: FilterControllerParams) -> Self {
        let filter = crate::ui::filter_sort::create_filter(params.search_query.clone());
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
            params.query_generation.clone(),
        );

        let controller = Self {
            filter_model,
            sort_list_model,
            filter,
            selected_tags: params.selected_tags,
            match_all: params.match_all,
            search_query: params.search_query,
            clear_filters_button: params.clear_filters_button,
            match_mode_box: params.match_mode_box,
            tag_list_box: params.tag_list_box,
            tags: params.tags,
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
            &self.clear_filters_button,
            &self.selected_tags,
            &self.match_mode_box,
            &self.search_query,
        );
        (self.send_query)();
    }

    pub fn apply_restored_state(&self, active_tags: &[crate::state::TagFilter]) {
        if active_tags.is_empty() {
            return;
        }

        let mut current_selected = self.selected_tags.borrow_mut();
        for (i, tag) in self.tags.borrow().iter().enumerate() {
            let filter = tag_filter(tag);
            if active_tags.contains(&filter)
                && let Some(row) = self.tag_list_box.row_at_index(i as i32)
            {
                row.add_css_class("active");
            }
            if active_tags.contains(&filter) && !current_selected.contains(&filter) {
                current_selected.push(filter);
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
            let tags = self.tags.clone();
            let clear_filters_button = self.clear_filters_button.clone();
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
                if let Some(tag) = tags.borrow().get(index) {
                    let filter = tag_filter(tag);
                    if row.has_css_class("active") {
                        if !new_selection.contains(&filter) {
                            new_selection.push(filter);
                        }
                    } else {
                        new_selection.retain(|selected| selected != &filter);
                    }
                }

                *selected_tags.borrow_mut() = new_selection;
                filter.changed(gtk::FilterChange::Different);
                update_filter_ui(
                    &clear_filters_button,
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
            let clear_filters_button = self.clear_filters_button.clone();
            let selected_tags = self.selected_tags.clone();
            let match_mode_box = self.match_mode_box.clone();
            move |entry| {
                *search_query.borrow_mut() = entry.text().to_string().to_lowercase();
                filter.changed(gtk::FilterChange::Different);
                update_filter_ui(
                    &clear_filters_button,
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
            let clear_filters_button = self.clear_filters_button.clone();
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
                    &clear_filters_button,
                    &selected_tags,
                    &match_mode_box,
                    &search_query,
                );
                send_query();
            }
        });

        self.clear_filters_button.connect_clicked({
            let clear_all = clear_all_action.clone();
            move |_| clear_all()
        });
        no_results_clear_btn.connect_clicked(move |_| clear_all_action());
    }
}

fn build_query_dispatcher(
    selected_tags: Rc<RefCell<Vec<crate::state::TagFilter>>>,
    match_all: Rc<RefCell<bool>>,
    search_query: Rc<RefCell<String>>,
    active_sort_idx: Rc<RefCell<u32>>,
    app_tx: tokio::sync::mpsc::Sender<crate::events::AppEvent>,
    query_generation: Rc<RefCell<crate::events::QueryGeneration>>,
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
                2 => crate::events::SortOrder::DateAddedDesc,
                3 => crate::events::SortOrder::DateAddedAsc,
                4 => crate::events::SortOrder::FilenameAsc,
                5 => crate::events::SortOrder::FilenameDesc,
                6 => crate::events::SortOrder::FileSizeDesc,
                _ => crate::events::SortOrder::FileSizeAsc,
            },
        };
        let generation = query_generation.borrow_mut().next();
        app_tx.send_critical(crate::events::AppEvent::QueryMedia(q, generation));
    })
}

fn update_filter_ui(
    clear_filters_button: &gtk::Button,
    selected_tags: &Rc<RefCell<Vec<crate::state::TagFilter>>>,
    match_mode_box: &gtk::Box,
    search_query: &Rc<RefCell<String>>,
) {
    let active_count = selected_tags.borrow().len();
    let has_tags = active_count > 0;
    let has_search = !search_query.borrow().is_empty();

    clear_filters_button.set_visible(has_tags || has_search);
    match_mode_box.set_visible(has_tags);

    if has_tags || has_search {
        let filter_count = active_count + usize::from(has_search);
        clear_filters_button.set_label(&format!("Clear filters ({filter_count})"));

        let description = match (active_count, has_search) {
            (0, false) => unreachable!(),
            (0, true) => "Clear search".to_string(),
            (1, false) => "Clear one tag filter".to_string(),
            (1, true) => "Clear one tag filter and search".to_string(),
            (count, false) => format!("Clear {count} tag filters"),
            (count, true) => format!("Clear {count} tag filters and search"),
        };
        clear_filters_button
            .update_property(&[gtk::accessible::Property::Description(&description)]);
    }
}

pub(crate) fn tag_filter(tag: &crate::events::UiTag) -> crate::state::TagFilter {
    crate::state::TagFilter {
        source_root_id: tag.source_root_id,
        relative_folder_path: tag.relative_folder_path.clone(),
        display_name: tag.display_name.clone(),
    }
}

fn sort_model_list() -> [&'static str; 8] {
    [
        "Date modified (newest first)",
        "Date modified (oldest first)",
        "Date added (newest first)",
        "Date added (oldest first)",
        "Filename (A → Z)",
        "Filename (Z → A)",
        "File size (largest first)",
        "File size (smallest first)",
    ]
}
