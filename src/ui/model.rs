use libadwaita::gtk::{self, glib, prelude::*};

glib::wrapper! {
    pub struct MediaItem(ObjectSubclass<imp::MediaItem>);
}

impl MediaItem {
    pub fn new(id: i64, path: &str, filename: &str, tags: &str, thumbnail_path: &str, duration_secs: i64) -> Self {
        glib::Object::builder()
            .property("id", id)
            .property("path", path)
            .property("filename", filename)
            .property("tags", tags)
            .property("thumbnail-path", thumbnail_path)
            .property("duration-secs", duration_secs)
            .build()
    }
}

mod imp {
    use super::*;
    use glib::subclass::prelude::*;
    use std::cell::RefCell;

    #[derive(Default)]
    pub struct MediaItem {
        pub id: RefCell<i64>,
        pub path: RefCell<String>,
        pub filename: RefCell<String>,
        pub tags: RefCell<String>,
        pub thumbnail_path: RefCell<String>,
        pub duration_secs: RefCell<i64>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MediaItem {
        const NAME: &'static str = "VesperMediaItem";
        type Type = super::MediaItem;
    }

    impl ObjectImpl for MediaItem {
        fn properties() -> &'static [glib::ParamSpec] {
            use std::sync::OnceLock;
            static PROPERTIES: OnceLock<Vec<glib::ParamSpec>> = OnceLock::new();
            PROPERTIES.get_or_init(|| {
                vec![
                    glib::ParamSpecInt64::builder("id").build(),
                    glib::ParamSpecString::builder("path").build(),
                    glib::ParamSpecString::builder("filename").build(),
                    glib::ParamSpecString::builder("tags").build(),
                    glib::ParamSpecString::builder("thumbnail-path").build(),
                    glib::ParamSpecInt64::builder("duration-secs").minimum(-1).maximum(i64::MAX).default_value(-1).build(),
                ]
            })
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "id" => *self.id.borrow_mut() = value.get().unwrap(),
                "path" => *self.path.borrow_mut() = value.get().unwrap(),
                "filename" => *self.filename.borrow_mut() = value.get().unwrap(),
                "tags" => *self.tags.borrow_mut() = value.get().unwrap(),
                "thumbnail-path" => *self.thumbnail_path.borrow_mut() = value.get().unwrap(),
                "duration-secs" => *self.duration_secs.borrow_mut() = value.get().unwrap(),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "id" => self.id.borrow().to_value(),
                "path" => self.path.borrow().to_value(),
                "filename" => self.filename.borrow().to_value(),
                "tags" => self.tags.borrow().to_value(),
                "thumbnail-path" => self.thumbnail_path.borrow().to_value(),
                "duration-secs" => self.duration_secs.borrow().to_value(),
                _ => unimplemented!(),
            }
        }
    }
}
