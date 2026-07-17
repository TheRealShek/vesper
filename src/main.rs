pub mod backend;
pub mod config;
mod db;
mod events;
mod index;
mod lock;
pub mod logging;
mod scan;
pub mod state;
mod thumbnail;

use crate::events::{AppEvent, UiEvent};
use std::sync::{Arc, Mutex};

fn main() {
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("Failed to create Tokio runtime: {error}");
            return;
        }
    };

    if let Err(error) = runtime.block_on(run_headless()) {
        eprintln!("Vesper backend stopped: {error}");
    }
}

async fn run_headless() -> anyhow::Result<()> {
    let vesper_dir = dirs::data_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not determine user data directory"))?
        .join("vesper");
    std::fs::create_dir_all(&vesper_dir)?;

    let lock_path = vesper_dir.join(crate::config::LOCK_NAME);
    let _library_lock = match crate::lock::LibraryLock::acquire(&lock_path)? {
        Some(lock) => lock,
        None => {
            eprintln!("Vesper is already running.");
            return Ok(());
        }
    };

    let _log_guard = crate::logging::init(&vesper_dir);
    tracing::info!("Vesper backend starting without a UI");

    let db_path = vesper_dir.join(crate::config::DB_NAME);
    let db = Arc::new(crate::db::Database::open(&db_path)?);
    let state = Arc::new(Mutex::new(crate::state::AppState::load(&db)));

    let (app_tx, app_rx) = tokio::sync::mpsc::channel::<AppEvent>(1024);
    let (ui_tx, mut ui_rx) = tokio::sync::mpsc::channel::<UiEvent>(256);
    let (thumbnail_tx, thumbnail_rx) =
        tokio::sync::mpsc::channel::<crate::thumbnail::ThumbnailRequest>(128);

    let concurrency = crate::backend::concurrency::BackendConcurrency::new();
    let thumbnail_cache = crate::thumbnail::ThumbnailCacheState::new();
    let services = Arc::new(crate::backend::BackendServices {
        concurrency: concurrency.clone(),
        thumbnail_cache: thumbnail_cache.clone(),
        maintenance: crate::backend::maintenance::MaintenanceCoordinator::new(),
    });

    crate::thumbnail::start_thumbnail_worker(
        db.clone(),
        thumbnail_rx,
        ui_tx.clone(),
        concurrency,
        thumbnail_cache,
    );
    crate::backend::start_backend(app_rx, app_tx.clone(), ui_tx, db, state, services);

    // Keep backend publication non-blocking while the UI is intentionally absent.
    tokio::spawn(async move { while ui_rx.recv().await.is_some() {} });
    app_tx.send(AppEvent::FetchData).await?;

    // TODO: Attach the replacement UI here through the typed event channels.
    tokio::signal::ctrl_c().await?;
    tracing::info!("Vesper backend shutting down");
    drop(thumbnail_tx);
    Ok(())
}
