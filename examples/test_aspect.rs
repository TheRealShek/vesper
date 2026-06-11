use libadwaita::gtk::{self, prelude::*};
fn main() {
    let af = gtk::AspectFrame::builder().xalign(0.5).yalign(0.5).ratio(1.0).obey_child(false).build();
}
