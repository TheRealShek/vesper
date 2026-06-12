use gtk4 as gtk;
use gtk::prelude::*;

fn main() {
    let btn = gtk::Button::new();
    btn.update_property(&[gtk::accessible::Property::Label("Test")]);
}
