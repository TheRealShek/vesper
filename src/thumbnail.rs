use crate::events::ChannelSendExt;
use anyhow::Result;

use crate::db::Database;
use crate::events::MediaType;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

pub struct ThumbnailRequest {
    pub media_id: i64,
    pub path: PathBuf,
    pub media_type: MediaType,
    pub modified_at: i64,
}

pub fn start_thumbnail_worker(
    db: Arc<Mutex<Database>>,
    rx: mpsc::Receiver<ThumbnailRequest>,
    ui_sender: tokio::sync::mpsc::Sender<crate::ui::window::UiEvent>,
) {
    let cache_dir = dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("vesper")
        .join("thumbnails");
    let _ = std::fs::create_dir_all(&cache_dir);

    let rx_shared = Arc::new(tokio::sync::Mutex::new(rx));
    let num_workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(4);

    for _ in 0..num_workers {
        let rx_clone = rx_shared.clone();
        let db_clone = db.clone();
        let ui_sender_clone = ui_sender.clone();
        let cache_dir_clone = cache_dir.clone();

        tokio::spawn(async move {
            loop {
                let req = {
                    let mut guard = rx_clone.lock().await;
                    guard.recv().await
                };

                let req = match req {
                    Some(r) => r,
                    None => break, // Channel closed
                };

                let (thumb_path, duration) =
                    match generate_thumbnail(&req.path, &req.media_type, &cache_dir_clone).await {
                        Ok(res) => res,
                        Err(e) => {
                            eprintln!("Thumbnail failed for {:?}: {}", req.path, e);
                            continue; // Silently ignore failures per spec
                        }
                    };

                if let Some(path_str) = thumb_path.to_str() {
                    let db_guard = match db_clone.lock() {
                        Ok(g) => g,
                        Err(_) => {
                            let _ =
                                ui_sender_clone.send_log(crate::ui::window::UiEvent::FatalError(
                                    "Database lock poisoned in thumbnail worker".to_string(),
                                ));
                            break;
                        }
                    };
                    let src_path_str = req.path.to_string_lossy();
                    if let Ok(_) = db_guard.set_thumbnail_and_duration(
                        req.media_id,
                        &src_path_str,
                        req.modified_at,
                        path_str,
                        duration,
                    ) {
                        let _ =
                            ui_sender_clone.send_log(crate::ui::window::UiEvent::ThumbnailReady(
                                req.media_id,
                                path_str.to_string(),
                                duration,
                            ));
                    }
                }
            }
        });
    }
}

async fn generate_thumbnail(
    media_path: &Path,
    media_type: &MediaType,
    cache_dir: &Path,
) -> Result<(PathBuf, Option<i64>)> {
    let meta = std::fs::metadata(media_path)?;
    let modified_time = meta
        .modified()
        .and_then(|t| {
            t.duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
        })
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let file_size = meta.len();

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    media_path.hash(&mut hasher);
    modified_time.hash(&mut hasher);
    file_size.hash(&mut hasher);
    let hash = hasher.finish();

    let thumb_name = format!("{:016x}.jpg", hash);
    let thumb_path = cache_dir.join(thumb_name);

    let mut duration_secs = None;
    if *media_type == MediaType::Video {
        let mut cmd = tokio::process::Command::new("ffprobe");
        cmd.args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(media_path)
        .kill_on_drop(true);

        if let Ok(Ok(out)) =
            tokio::time::timeout(std::time::Duration::from_secs(30), cmd.output()).await
        {
            let s = String::from_utf8_lossy(&out.stdout);
            if let Ok(f) = s.trim().parse::<f64>() {
                duration_secs = Some(f.round() as i64);
            }
        } else {
            eprintln!("ffprobe timed out or failed for {:?}", media_path);
        }
    }

    if thumb_path.exists() {
        let mut stale = false;
        if let (Ok(src_meta), Ok(thumb_meta)) = (
            std::fs::metadata(media_path),
            std::fs::metadata(&thumb_path),
        ) && let (Ok(src_mtime), Ok(thumb_mtime)) = (src_meta.modified(), thumb_meta.modified())
            && src_mtime > thumb_mtime
        {
            stale = true;
        }
        if !stale {
            return Ok((thumb_path, duration_secs));
        }
    }

    match media_type {
        MediaType::Image => {
            let path_clone = media_path.to_path_buf();
            let thumb_path_clone = thumb_path.clone();
            tokio::task::spawn_blocking(move || -> Result<()> {
                let img = image::open(&path_clone)?;
                let resized = img.resize_to_fill(256, 256, image::imageops::FilterType::Triangle);
                resized.save_with_format(&thumb_path_clone, image::ImageFormat::Jpeg)?;
                Ok(())
            })
            .await??;
        }
        MediaType::Video => {
            let media_path_str = match media_path.to_str() {
                Some(s) => s,
                None => anyhow::bail!("Invalid UTF-8 in media path"),
            };
            let thumb_path_str = match thumb_path.to_str() {
                Some(s) => s,
                None => anyhow::bail!("Invalid UTF-8 in thumbnail path"),
            };

            let mut cmd = tokio::process::Command::new("ffmpeg");
            cmd.args([
                "-y",
                "-i",
                media_path_str,
                "-vf",
                "thumbnail,scale=256:256:force_original_aspect_ratio=increase,crop=256:256",
                "-frames:v",
                "1",
                "-q:v",
                "5",
                thumb_path_str,
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);

            let status_res =
                tokio::time::timeout(std::time::Duration::from_secs(30), cmd.status()).await;
            match status_res {
                Ok(Ok(status)) if status.success() => {}
                Ok(Ok(_)) => anyhow::bail!("ffmpeg failed"),
                Ok(Err(e)) => anyhow::bail!("ffmpeg error: {}", e),
                Err(_) => {
                    eprintln!("ffmpeg timed out for {:?}", media_path);
                    anyhow::bail!("ffmpeg timed out");
                }
            }
        }
    }

    Ok((thumb_path, duration_secs))
}
