//! Mutually-exclusive library maintenance jobs (B-6).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};

use crate::backend::BackendServices;
use crate::backend::concurrency::BackendConcurrency;
use crate::db::Database;
use crate::events::{AppEvent, ChannelSendExt};
use crate::state::BackendState;
use crate::ui::window::UiEvent;

const ALREADY_RUNNING_STATUS: &str = "Library maintenance is already running";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaintenanceOperation {
    RescanLibrary,
    RegenerateThumbnails,
    RebuildLibraryIndex,
}

/// One process-wide gate for every index-mutating maintenance operation.
pub struct MaintenanceCoordinator {
    running: AtomicBool,
}

impl MaintenanceCoordinator {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            running: AtomicBool::new(false),
        })
    }

    fn try_begin(self: &Arc<Self>) -> Option<MaintenanceGuard> {
        self.running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| MaintenanceGuard {
                coordinator: self.clone(),
            })
    }
}

struct MaintenanceGuard {
    coordinator: Arc<MaintenanceCoordinator>,
}

impl Drop for MaintenanceGuard {
    fn drop(&mut self) {
        self.coordinator.running.store(false, Ordering::Release);
    }
}

/// Starts one callable backend maintenance job, or publishes the required
/// passive status when another job already owns the maintenance gate.
pub fn start_operation(
    operation: MaintenanceOperation,
    db: Arc<Database>,
    backend_state: BackendState,
    ui_tx: tokio::sync::mpsc::Sender<UiEvent>,
    app_tx: tokio::sync::mpsc::Sender<AppEvent>,
    services: Arc<BackendServices>,
) {
    let Some(guard) = try_begin_or_report(&services.maintenance, &ui_tx) else {
        return;
    };

    tokio::spawn(async move {
        let result = match operation {
            MaintenanceOperation::RescanLibrary => {
                run_rescan_library(&db, &backend_state, &ui_tx, &services.concurrency).await
            }
            MaintenanceOperation::RegenerateThumbnails => run_regenerate_thumbnails(
                &db,
                &crate::thumbnail::thumbnail_cache_dir(),
                &ui_tx,
                &services.thumbnail_cache,
            )
            .await
            .map(|_| ()),
            MaintenanceOperation::RebuildLibraryIndex => {
                run_rebuild_library_index(&db, &backend_state, &ui_tx, &services.concurrency)
                    .await
                    .map(|_| ())
            }
        };

        if let Err(error) = result {
            ui_tx.send_critical(UiEvent::BackendWarning(format!(
                "Library maintenance failed: {error}"
            )));
        }
        app_tx.send_critical(AppEvent::FetchData);
        drop(guard);
    });
}

fn try_begin_or_report(
    coordinator: &Arc<MaintenanceCoordinator>,
    ui_tx: &tokio::sync::mpsc::Sender<UiEvent>,
) -> Option<MaintenanceGuard> {
    let guard = coordinator.try_begin();
    if guard.is_none() {
        ui_tx.send_critical(UiEvent::BackendWarning(ALREADY_RUNNING_STATUS.to_string()));
    }
    guard
}

async fn run_rescan_library(
    db: &Arc<Database>,
    backend_state: &BackendState,
    ui_tx: &tokio::sync::mpsc::Sender<UiEvent>,
    concurrency: &Arc<BackendConcurrency>,
) -> Result<()> {
    let roots = db
        .list_source_roots()
        .context("failed to list source roots")?;
    for root in roots.into_iter().filter(|root| root.is_available) {
        let Some(_full_scan) = concurrency.acquire_full_scan().await else {
            anyhow::bail!("unable to schedule a library rescan");
        };
        match crate::scan::run_scan(
            PathBuf::from(&root.path),
            db.clone(),
            backend_state.global_ignore_rules.clone(),
            backend_state.root_as_tag,
            ui_tx.clone(),
        )
        .await
        {
            Ok(result) => ui_tx.send_critical(UiEvent::ScanCompleted(
                result.failed_paths.len(),
                result.failed_paths,
            )),
            Err(error) => {
                crate::backend::app_loop::report_scan_failure(
                    db,
                    ui_tx,
                    Path::new(&root.path),
                    &error,
                );
            }
        }
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub struct RegenerationReport {
    pub attempted: usize,
    pub succeeded: usize,
}

pub async fn run_regenerate_thumbnails(
    db: &Arc<Database>,
    cache_dir: &Path,
    ui_tx: &tokio::sync::mpsc::Sender<UiEvent>,
    cache_state: &Arc<crate::thumbnail::ThumbnailCacheState>,
) -> Result<RegenerationReport> {
    let media_ids = db
        .list_media_needing_thumbnail_regen()
        .context("failed to enumerate thumbnails needing regeneration")?;
    let attempted = media_ids.len();
    let mut succeeded = 0;

    for media_id in media_ids {
        match crate::thumbnail::regenerate_thumbnail(db, cache_dir, media_id).await {
            Ok((path, duration)) => {
                succeeded += 1;
                ui_tx.send_log(UiEvent::ThumbnailReady(media_id, path, duration));
            }
            Err(error) => {
                tracing::warn!(media_id, %error, "thumbnail regeneration failed");
            }
        }
    }

    let db_for_maintenance = db.clone();
    let cache_dir = cache_dir.to_path_buf();
    let cache_state = cache_state.clone();
    let evicted = tokio::task::spawn_blocking(move || {
        crate::thumbnail::enforce_disk_budget(
            &db_for_maintenance,
            &cache_dir,
            crate::config::THUMBNAIL_DISK_BUDGET_BYTES,
            &cache_state,
        )
    })
    .await
    .context("thumbnail cache maintenance task failed")??;
    if !evicted.is_empty() {
        ui_tx.send_log(UiEvent::ThumbnailsEvicted(evicted));
    }

    Ok(RegenerationReport {
        attempted,
        succeeded,
    })
}

#[derive(Debug, PartialEq, Eq)]
pub struct RebuildReport {
    pub migrations_applied: usize,
    pub roots_scanned: usize,
}

pub async fn run_rebuild_library_index(
    db: &Arc<Database>,
    backend_state: &BackendState,
    ui_tx: &tokio::sync::mpsc::Sender<UiEvent>,
    concurrency: &Arc<BackendConcurrency>,
) -> Result<RebuildReport> {
    // Hold the full-scan permit across both destructive preparation and every
    // root scan, so no initial/rescan job can repopulate rows between the clear
    // and the authoritative rebuild.
    let Some(_full_scan) = concurrency.acquire_full_scan().await else {
        anyhow::bail!("unable to schedule rebuilt library scan");
    };
    let db_for_rebuild = db.clone();
    let migrations_applied =
        tokio::task::spawn_blocking(move || db_for_rebuild.prepare_library_index_rebuild())
            .await
            .context("index rebuild preparation task failed")??;

    let roots = db
        .list_source_roots()
        .context("failed to list preserved source roots")?;
    let online_roots = roots
        .into_iter()
        .filter(|root| root.is_available)
        .collect::<Vec<_>>();
    let roots_scanned = online_roots.len();

    for root in online_roots {
        let result = crate::scan::run_scan(
            PathBuf::from(&root.path),
            db.clone(),
            backend_state.global_ignore_rules.clone(),
            backend_state.root_as_tag,
            ui_tx.clone(),
        )
        .await
        .with_context(|| format!("failed to rebuild source root {}", root.display_path))?;
        ui_tx.send_critical(UiEvent::ScanCompleted(
            result.failed_paths.len(),
            result.failed_paths,
        ));
    }

    Ok(RebuildReport {
        migrations_applied,
        roots_scanned,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::MediaEntry;
    use crate::events::MediaType;

    fn insert_image(db: &Database, root_id: i64, path: &Path, modified_at: i64) -> i64 {
        let path = path.to_string_lossy().to_string();
        let entry = MediaEntry {
            relative_path: Path::new(&path)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string(),
            canonical_identity: path.clone(),
            filename: Path::new(&path)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string(),
            path: path.clone(),
            source_root_id: root_id,
            media_type: MediaType::Image,
            size_bytes: 16,
            created_at: None,
            modified_at,
        };
        db.upsert_media_batch(&[(entry, Vec::new())], 1).unwrap();
        db.get_media_with_tags_by_path(&path).unwrap().unwrap().0.id
    }

    #[tokio::test]
    async fn second_maintenance_operation_is_rejected_with_already_running_status() {
        let coordinator = MaintenanceCoordinator::new();
        let (ui_tx, mut ui_rx) = tokio::sync::mpsc::channel(4);
        let first = try_begin_or_report(&coordinator, &ui_tx).unwrap();

        assert!(try_begin_or_report(&coordinator, &ui_tx).is_none());
        match ui_rx.recv().await.unwrap() {
            UiEvent::BackendWarning(message) => assert_eq!(message, ALREADY_RUNNING_STATUS),
            _ => panic!("expected passive maintenance status"),
        }

        drop(first);
        assert!(try_begin_or_report(&coordinator, &ui_tx).is_some());
    }

    #[tokio::test]
    async fn rebuild_library_index_runs_migrations_then_full_reindex() {
        let dir = tempfile::TempDir::new().unwrap();
        let image_path = dir.path().join("indexed.png");
        image::RgbImage::new(8, 8).save(&image_path).unwrap();
        let db = Arc::new(Database::open_in_memory().unwrap());
        let root_path = dir.path().to_string_lossy().to_string();
        let root_id = db.add_source_root(&root_path, &root_path).unwrap();
        let missing_path = dir.path().join("missing.png");
        insert_image(&db, root_id, &missing_path, 1);
        db.set_setting("preserved", "yes").unwrap();
        db.set_session_state("preserved", "yes").unwrap();
        db.forget_schema_migration_for_test(4).unwrap();
        let (ui_tx, _ui_rx) = tokio::sync::mpsc::channel(32);

        let report = run_rebuild_library_index(
            &db,
            &BackendState::default(),
            &ui_tx,
            &BackendConcurrency::new(),
        )
        .await
        .unwrap();

        assert_eq!(report.migrations_applied, 1);
        assert_eq!(report.roots_scanned, 1);
        assert_eq!(
            db.get_all_paths_for_root(root_id).unwrap(),
            vec![image_path.to_string_lossy().to_string()]
        );
        assert_eq!(db.get_setting("preserved").unwrap().as_deref(), Some("yes"));
        assert_eq!(
            db.get_session_state("preserved").unwrap().as_deref(),
            Some("yes")
        );
    }

    #[tokio::test]
    async fn regenerate_thumbnails_processes_stale_and_failed_rows() {
        let dir = tempfile::TempDir::new().unwrap();
        let cache_dir = dir.path().join("cache");
        std::fs::create_dir_all(&cache_dir).unwrap();
        let stale_path = dir.path().join("stale.png");
        let failed_path = dir.path().join("failed.png");
        image::RgbImage::new(8, 8).save(&stale_path).unwrap();
        image::RgbImage::new(8, 8).save(&failed_path).unwrap();
        let db = Arc::new(Database::open_in_memory().unwrap());
        let root = dir.path().to_string_lossy().to_string();
        let root_id = db.add_source_root(&root, &root).unwrap();
        let stale_id = insert_image(&db, root_id, &stale_path, 1);
        let failed_id = insert_image(&db, root_id, &failed_path, 1);
        db.set_thumbnail_success(stale_id, "old", "/cache/old.jpg", 1, None)
            .unwrap();
        insert_image(&db, root_id, &stale_path, 2);
        db.set_thumbnail_failure(failed_id, "previous failure")
            .unwrap();
        let (ui_tx, _ui_rx) = tokio::sync::mpsc::channel(32);

        let report = run_regenerate_thumbnails(
            &db,
            &cache_dir,
            &ui_tx,
            &crate::thumbnail::ThumbnailCacheState::new(),
        )
        .await
        .unwrap();

        assert_eq!(
            report,
            RegenerationReport {
                attempted: 2,
                succeeded: 2
            }
        );
        for media_id in [stale_id, failed_id] {
            let status = db.get_thumbnail_status(media_id).unwrap().unwrap();
            assert!(!status.stale);
            assert!(status.failure.is_none());
            assert!(Path::new(status.thumbnail_path.as_deref().unwrap()).exists());
        }
    }
}
