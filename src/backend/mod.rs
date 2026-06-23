pub mod app_loop;
pub mod live_update;
pub mod watcher;

use crate::db::Database;
use crate::events::AppEvent;
use crate::state::AppState;
use crate::ui::window::UiEvent;
use std::sync::{Arc, Mutex};

pub fn start_backend(
    app_rx: tokio::sync::mpsc::Receiver<AppEvent>,
    app_tx: tokio::sync::mpsc::Sender<AppEvent>,
    ui_tx: tokio::sync::mpsc::Sender<UiEvent>,
    db: Arc<Database>,
    state: Arc<Mutex<AppState>>,
) {
    let (debouncer_tx, debouncer_rx) = std::sync::mpsc::channel();
    watcher::start_watcher(debouncer_rx, app_tx.clone());
    app_loop::start(app_rx, app_tx, ui_tx, db, state, debouncer_tx);
}
