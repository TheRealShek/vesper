pub fn test_scroll(view: &libadwaita::gtk::GridView) {
    view.scroll_to(0, libadwaita::gtk::ListScrollFlags::NONE, None);
}
