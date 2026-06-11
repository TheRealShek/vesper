use libadwaita::gtk::{self, prelude::*, glib};
fn main() {
    let p = glib::ParamSpecInt64::builder("duration_secs").default_value(-1i64).build();
    println!("Built param spec!");
}
