pub mod app_loop;
pub mod concurrency;
pub mod live_update;
pub mod liveness;
pub mod watcher;

use crate::backend::concurrency::BackendConcurrency;
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
    coord: Arc<BackendConcurrency>,
) {
    let (debouncer_tx, debouncer_rx) = std::sync::mpsc::channel();
    watcher::start_watcher(debouncer_rx, app_tx.clone());

    // The liveness worker owns filesystem probing and the watcher, decoupled
    // from UI hydration (B-2). The app loop triggers it via LivenessCommand.
    let (liveness_tx, liveness_rx) = tokio::sync::mpsc::channel(32);
    liveness::start(
        db.clone(),
        ui_tx.clone(),
        app_tx.clone(),
        liveness_rx,
        debouncer_tx,
    );

    app_loop::start(app_rx, app_tx, ui_tx, db, state, liveness_tx, coord);
}
