use std::rc::Rc;
use std::cell::RefCell;
use libadwaita::gtk::{self, prelude::*, glib, gio};

pub struct Viewer {
    pub info_revealer: gtk::Revealer,
    info_filename: gtk::Label,
    info_path: gtk::Label,
    info_size: gtk::Label,
    info_dim_dur: gtk::Label,
    info_created: gtk::Label,
    info_modified: gtk::Label,
    info_tags: gtk::Label,
}

fn test() {
    let b = gtk::Label::builder().build();
}
