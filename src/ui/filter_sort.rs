use libadwaita::gtk::{self};
use libadwaita::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Create the media filter that applies tag selection and search query.
pub fn create_filter(
    selected_tags: Rc<RefCell<Vec<String>>>,
    match_all: Rc<RefCell<bool>>,
    search_query: Rc<RefCell<String>>,
) -> gtk::CustomFilter {
    gtk::CustomFilter::new(move |item| {
        let Some(media_item) = item.downcast_ref::<crate::ui::model::MediaItem>() else {
            return false;
        };

        let selected = selected_tags.borrow();
        let item_tags_str: String = media_item.property("tags");
        let item_tags: Vec<&str> = item_tags_str.split(',').collect();

        if !selected.is_empty() {
            if *match_all.borrow() {
                if !selected.iter().all(|t| item_tags.contains(&t.as_str())) {
                    return false;
                }
            } else {
                if !selected.iter().any(|t| item_tags.contains(&t.as_str())) {
                    return false;
                }
            }
        }

        let query = search_query.borrow();
        if !query.is_empty() {
            let filename: String = media_item.property("filename");
            let path: String = media_item.property("path");
            let filename_stem = std::path::Path::new(&filename)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&filename);
            let q = query.to_lowercase();
            if !filename_stem.to_lowercase().contains(&q)
                && !path.to_lowercase().contains(&q)
                && !item_tags.iter().any(|t| t.to_lowercase().contains(&q))
            {
                return false;
            }
        }

        true
    })
}

/// Create the media sorter that handles search-rank priority and all 8 sort options.
pub fn create_sorter(
    active_sort_idx: Rc<RefCell<u32>>,
    search_query: Rc<RefCell<String>>,
) -> gtk::CustomSorter {
    gtk::CustomSorter::new(move |item1, item2| {
        let (Some(m1), Some(m2)) = (
            item1.downcast_ref::<crate::ui::model::MediaItem>(),
            item2.downcast_ref::<crate::ui::model::MediaItem>(),
        ) else {
            return gtk::Ordering::Equal;
        };

        let query = search_query.borrow();
        if !query.is_empty() {
            let q = query.to_lowercase();

            let get_rank = |m: &crate::ui::model::MediaItem| -> u8 {
                let filename: String = m.property("filename");
                let fl = filename.to_lowercase();
                let stem = std::path::Path::new(&filename)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if fl == q || stem == q {
                    return 1;
                }

                let tags: String = m.property("tags");
                if tags.split(',').any(|t| t.to_lowercase().contains(&q)) {
                    return 2;
                }

                let path: String = m.property("path");
                if fl.contains(&q) || path.to_lowercase().contains(&q) {
                    return 3;
                }

                4
            };

            let r1 = get_rank(m1);
            let r2 = get_rank(m2);

            if r1 != r2 {
                return if r1 < r2 {
                    gtk::Ordering::Smaller
                } else {
                    gtk::Ordering::Larger
                };
            }
        }

        let idx = *active_sort_idx.borrow();

        let cmp = match idx {
            0 => {
                // Date modified (newest first)
                let t1: i64 = m1.property("modified-at");
                let t2: i64 = m2.property("modified-at");
                t1.cmp(&t2).reverse()
            }
            1 => {
                // Date modified (oldest first)
                let t1: i64 = m1.property("modified-at");
                let t2: i64 = m2.property("modified-at");
                t1.cmp(&t2)
            }
            2 => {
                // Date created (newest first)
                let t1: i64 = m1.property("created-at");
                let t2: i64 = m2.property("created-at");
                t1.cmp(&t2).reverse()
            }
            3 => {
                // Date created (oldest first)
                let t1: i64 = m1.property("created-at");
                let t2: i64 = m2.property("created-at");
                t1.cmp(&t2)
            }
            4 => {
                // Filename (A → Z)
                let f1: String = m1.property("filename");
                let f2: String = m2.property("filename");
                f1.to_lowercase().cmp(&f2.to_lowercase())
            }
            5 => {
                // Filename (Z → A)
                let f1: String = m1.property("filename");
                let f2: String = m2.property("filename");
                f1.to_lowercase().cmp(&f2.to_lowercase()).reverse()
            }
            6 => {
                // File size (largest first)
                let s1: i64 = m1.property("size-bytes");
                let s2: i64 = m2.property("size-bytes");
                s1.cmp(&s2).reverse()
            }
            7 => {
                // File size (smallest first)
                let s1: i64 = m1.property("size-bytes");
                let s2: i64 = m2.property("size-bytes");
                s1.cmp(&s2)
            }
            _ => std::cmp::Ordering::Equal,
        };

        match cmp {
            std::cmp::Ordering::Less => gtk::Ordering::Smaller,
            std::cmp::Ordering::Greater => gtk::Ordering::Larger,
            std::cmp::Ordering::Equal => gtk::Ordering::Equal,
        }
    })
}
