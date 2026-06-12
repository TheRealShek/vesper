fn main() {
    let (tx, rx) = std::sync::mpsc::channel();
    let mut debouncer = notify_debouncer_mini::new_debouncer(std::time::Duration::from_millis(500), tx).unwrap();
    debouncer.watcher().watch(std::path::Path::new("."), notify_debouncer_mini::notify::RecursiveMode::Recursive).unwrap();
}
