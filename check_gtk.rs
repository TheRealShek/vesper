use libadwaita::gtk::{self, prelude::*};
fn check(grid: &gtk::GridView) {
    let _ = grid.get_active_item();
}
