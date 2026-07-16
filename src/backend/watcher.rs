use crate::events::AppEvent;

pub fn start_watcher(
    debouncer_rx: std::sync::mpsc::Receiver<notify_debouncer_mini::DebounceEventResult>,
    app_tx: tokio::sync::mpsc::Sender<AppEvent>,
) {
    std::thread::spawn(move || {
        while let Ok(res) = debouncer_rx.recv() {
            match res {
                Ok(events) => {
                    for event in events {
                        let path = event.path;
                        if path.file_name().and_then(|n| n.to_str()) == Some(".galleryignore") {
                            if let Some(parent) = path.parent() {
                                crate::events::ChannelSendExt::send_critical(
                                    &app_tx,
                                    AppEvent::RescanSubtree(parent.to_path_buf()),
                                );
                            }
                        } else {
                            let kind = if path.exists() {
                                crate::events::ChangeKind::Modified
                            } else {
                                crate::events::ChangeKind::Deleted
                            };
                            crate::events::ChannelSendExt::send_critical(
                                &app_tx,
                                AppEvent::FileChanged(path, kind),
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(error = ?e, "watcher error");
                }
            }
        }
    });
}
