use libadwaita::gtk::{self, prelude::*, glib};
fn test() {
    let dt = glib::DateTime::now_local().unwrap();
    let s = dt.format("%Y-%m-%d").unwrap_or_default().to_string();
}
