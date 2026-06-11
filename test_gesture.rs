use libadwaita::gtk::{self, prelude::*};
fn main() {
    let g = gtk::GestureClick::new();
    let s = g.state(gtk::EventSequence::NONE);
    if s == gtk::EventSequenceState::Denied {}
    g.set_state(gtk::EventSequence::NONE, gtk::EventSequenceState::Claimed);
}
