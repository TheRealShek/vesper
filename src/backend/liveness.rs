//! Independent liveness / watcher worker (B-2 sub-step b).
//!
//! Root-liveness probing, `notify` watcher setup/teardown, and
//! `set_source_root_available` writes are *not* UI-hydration concerns. They used
//! to run inline in the `FetchData` handler, coupling every hydration read to
//! filesystem I/O, watcher reconfiguration, and database writes (ARCH-004).
//!
//! This worker owns all of that. It reconciles on demand when it receives a
//! [`LivenessCommand::Probe`]; hydration triggers it rather than doing the work
//! itself. When a probe changes any root's availability, the worker asks the UI
//! to re-hydrate so the corrected offline state is reflected.

use crate::db::Database;
use crate::events::{AppEvent, ChannelSendExt, UiEvent};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use notify_debouncer_mini::notify::{RecursiveMode, Watcher};

/// A request to the liveness worker.
pub enum LivenessCommand {
    /// Reconcile now: probe every root's availability, sync watchers to the live
    /// online-root set, persist availability changes, and publish the offline
    /// count. Triggered by hydration and by add/remove-root flows.
    Probe,
    /// Probe one root and report whether it is currently online. Used by
    /// delete-event gating (B-4): a watcher delete must not purge media records
    /// when the owning root has gone offline. This reuses the same
    /// reconciliation the worker already performs — so a vanished root is marked
    /// offline as a side effect — rather than opening a second probing path.
    ProbeRoot {
        root_id: i64,
        respond: tokio::sync::oneshot::Sender<bool>,
    },
}

/// The single filesystem-liveness predicate for a source root: it exists, is a
/// directory, and is readable. Shared by full reconciliation and single-root
/// probes so there is exactly one definition of "online" (B-4).
pub fn is_root_available(path: &std::path::Path) -> bool {
    path.exists() && path.is_dir() && std::fs::read_dir(path).is_ok()
}

/// Starts the liveness worker on the Tokio runtime. It owns the `notify`
/// debouncer (built from `debouncer_tx`) for the process lifetime.
pub fn start(
    db: Arc<Database>,
    ui_tx: tokio::sync::mpsc::Sender<UiEvent>,
    app_tx: tokio::sync::mpsc::Sender<AppEvent>,
    mut cmd_rx: tokio::sync::mpsc::Receiver<LivenessCommand>,
    debouncer_tx: std::sync::mpsc::Sender<notify_debouncer_mini::DebounceEventResult>,
) {
    tokio::spawn(async move {
        let mut debouncer = match notify_debouncer_mini::new_debouncer(
            std::time::Duration::from_millis(crate::config::FS_DEBOUNCE_MS),
            debouncer_tx,
        ) {
            Ok(d) => d,
            Err(e) => {
                tracing::error!(error = %e, "failed to create filesystem debouncer");
                std::process::exit(1);
            }
        };
        let mut watched_roots: HashSet<PathBuf> = HashSet::new();

        // B-2: liveness schedules itself. Hydration is a pure database read and
        // never triggers probing; this periodic tick (first tick immediate)
        // keeps root availability current independently of UI activity.
        let mut probe_interval = tokio::time::interval(std::time::Duration::from_secs(
            crate::config::LIVENESS_PROBE_INTERVAL_SECS,
        ));
        probe_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            let cmd = tokio::select! {
                _ = probe_interval.tick() => {
                    reconcile(
                        &db,
                        &ui_tx,
                        &app_tx,
                        debouncer.watcher(),
                        &mut watched_roots,
                    );
                    continue;
                }
                cmd = cmd_rx.recv() => match cmd {
                    Some(cmd) => cmd,
                    None => break,
                },
            };
            match cmd {
                LivenessCommand::Probe => {
                    reconcile(
                        &db,
                        &ui_tx,
                        &app_tx,
                        debouncer.watcher(),
                        &mut watched_roots,
                    );
                }
                LivenessCommand::ProbeRoot { root_id, respond } => {
                    // Reuse the full reconciliation so watcher teardown, the
                    // availability write-back (marking a vanished root offline),
                    // the offline-count publish, and re-hydration all stay
                    // consistent — then read this root's freshly-persisted
                    // availability back to answer the caller.
                    reconcile(
                        &db,
                        &ui_tx,
                        &app_tx,
                        debouncer.watcher(),
                        &mut watched_roots,
                    );
                    let online = db
                        .list_source_roots()
                        .unwrap_or_default()
                        .iter()
                        .find(|r| r.id == root_id)
                        .map(|r| r.is_available)
                        .unwrap_or(false);
                    let _ = respond.send(online);
                }
            }
        }
    });
}

/// One liveness-reconciliation pass, extracted verbatim in intent from the
/// former inline `FetchData` block.
///
/// Probes each source root, watches newly-online roots, unwatches roots that no
/// longer exist, and persists any availability change. Publishes the offline
/// count to the status banner, and — when availability changed — requests a
/// re-hydration so the grid reflects the corrected offline state.
fn reconcile<W: Watcher + ?Sized>(
    db: &Database,
    ui_tx: &tokio::sync::mpsc::Sender<UiEvent>,
    app_tx: &tokio::sync::mpsc::Sender<AppEvent>,
    watcher: &mut W,
    watched_roots: &mut HashSet<PathBuf>,
) {
    let roots = db.list_source_roots().unwrap_or_default();
    let current: HashSet<PathBuf> = roots.iter().map(|r| PathBuf::from(&r.path)).collect();

    let mut offline_count = 0usize;
    let mut availability_changed = false;

    for root in &roots {
        let path = std::path::Path::new(&root.path);
        let is_avail = is_root_available(path);

        if is_avail {
            let path_buf = path.to_path_buf();
            if !watched_roots.contains(&path_buf) {
                if let Err(e) = watcher.watch(path, RecursiveMode::Recursive) {
                    tracing::warn!(
                        root = %crate::logging::redact_path(path),
                        error = %e,
                        "watcher failed to watch source root"
                    );
                    ui_tx.send_critical(UiEvent::BackendWarning(format!(
                        "Live updates disabled for {}: {}",
                        path.display(),
                        e
                    )));
                } else {
                    watched_roots.insert(path_buf);
                }
            }
        } else {
            offline_count += 1;
        }

        if root.is_available != is_avail {
            availability_changed = true;
            crate::logging::root_availability_changed(path, is_avail);
            let _ = db.set_source_root_available(root.id, is_avail);
        }
    }

    // Stop watching roots that have been removed from the library.
    let removed: Vec<PathBuf> = watched_roots.difference(&current).cloned().collect();
    for path in removed {
        if let Err(e) = watcher.unwatch(&path) {
            tracing::warn!(
                root = %crate::logging::redact_path(&path),
                error = %e,
                "watcher failed to unwatch source root"
            );
        }
        watched_roots.remove(&path);
    }

    ui_tx.send_critical(UiEvent::RootsOffline(offline_count));

    // A hydration read cannot see freshly-probed availability until it is written
    // back, so ask the UI to re-hydrate once — this converges after one extra
    // cycle and does not loop while availability is stable.
    if availability_changed {
        app_tx.send_critical(AppEvent::FetchData);
    }
}
