use libadwaita::gtk::{glib, prelude::*};

// Must be a GObject subclass instead of a plain Rust struct because GTK's ListStore and FilterListModel require GObject elements.
glib::wrapper! {
    pub struct MediaItem(ObjectSubclass<imp::MediaItem>);
}

impl From<crate::events::UiMediaItem> for MediaItem {
    fn from(item: crate::events::UiMediaItem) -> Self {
        let created = item.created_at.unwrap_or(0);
        let is_video = matches!(item.media_type, crate::events::MediaType::Video);
        glib::Object::builder()
            .property("id", item.id)
            .property("path", &item.path)
            .property("filename", &item.filename)
            .property("tags", &item.tags)
            .property("thumbnail-path", &item.thumbnail_path)
            .property("duration-secs", item.duration_secs)
            .property("is-video", is_video)
            .property("size-bytes", item.size_bytes)
            .property("created-at", created)
            .property("modified-at", item.modified_at)
            .property("is-offline", item.is_offline)
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
        pub is_video: RefCell<bool>,
        pub size_bytes: RefCell<i64>,
        pub created_at: RefCell<i64>,
        pub modified_at: RefCell<i64>,
        pub is_offline: RefCell<bool>,
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
                    glib::ParamSpecInt64::builder("duration-secs")
                        .minimum(-1)
                        .maximum(i64::MAX)
                        .default_value(-1)
                        .build(),
                    glib::ParamSpecBoolean::builder("is-video")
                        .default_value(false)
                        .build(),
                    glib::ParamSpecInt64::builder("size-bytes")
                        .minimum(0)
                        .maximum(i64::MAX)
                        .default_value(0)
                        .build(),
                    glib::ParamSpecInt64::builder("created-at")
                        .minimum(0)
                        .maximum(i64::MAX)
                        .default_value(0)
                        .build(),
                    glib::ParamSpecInt64::builder("modified-at")
                        .minimum(0)
                        .maximum(i64::MAX)
                        .default_value(0)
                        .build(),
                    glib::ParamSpecBoolean::builder("is-offline")
                        .default_value(false)
                        .build(),
                ]
            })
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "id" => {
                    if let Ok(v) = value.get() {
                        *self.id.borrow_mut() = v;
                    }
                }
                "path" => {
                    if let Ok(v) = value.get() {
                        *self.path.borrow_mut() = v;
                    }
                }
                "filename" => {
                    if let Ok(v) = value.get() {
                        *self.filename.borrow_mut() = v;
                    }
                }
                "tags" => {
                    if let Ok(v) = value.get() {
                        *self.tags.borrow_mut() = v;
                    }
                }
                "thumbnail-path" => {
                    if let Ok(v) = value.get() {
                        *self.thumbnail_path.borrow_mut() = v;
                    }
                }
                "duration-secs" => {
                    if let Ok(v) = value.get() {
                        *self.duration_secs.borrow_mut() = v;
                    }
                }
                "is-video" => {
                    if let Ok(v) = value.get() {
                        *self.is_video.borrow_mut() = v;
                    }
                }
                "size-bytes" => {
                    if let Ok(v) = value.get() {
                        *self.size_bytes.borrow_mut() = v;
                    }
                }
                "created-at" => {
                    if let Ok(v) = value.get() {
                        *self.created_at.borrow_mut() = v;
                    }
                }
                "modified-at" => {
                    if let Ok(v) = value.get() {
                        *self.modified_at.borrow_mut() = v;
                    }
                }
                "is-offline" => {
                    if let Ok(v) = value.get() {
                        *self.is_offline.borrow_mut() = v;
                    }
                }
                _ => (),
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
                "is-video" => self.is_video.borrow().to_value(),
                "size-bytes" => self.size_bytes.borrow().to_value(),
                "created-at" => self.created_at.borrow().to_value(),
                "modified-at" => self.modified_at.borrow().to_value(),
                "is-offline" => self.is_offline.borrow().to_value(),
                _ => glib::Value::from(0i32),
            }
        }
    }
}
