use anyhow::Result;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use crate::events::MediaType;
use crate::db::Database;
use std::hash::{Hash, Hasher};

pub struct ThumbnailRequest {
    pub media_id: i64,
    pub path: PathBuf,
    pub media_type: MediaType,
}

pub fn start_thumbnail_worker(
    db: Arc<Mutex<Database>>,
    mut rx: mpsc::UnboundedReceiver<ThumbnailRequest>,
    ui_sender: tokio::sync::mpsc::UnboundedSender<crate::ui::window::UiEvent>,
) {
    let cache_dir = dirs::cache_dir()
        .unwrap_or_else(|| std::env::temp_dir())
        .join("vesper")
        .join("thumbnails");
    let _ = std::fs::create_dir_all(&cache_dir);

    tokio::task::spawn_blocking(move || {
        while let Some(req) = rx.blocking_recv() {
            let (thumb_path, duration) = match generate_thumbnail(&req.path, &req.media_type, &cache_dir) {
                Ok(res) => res,
                Err(_) => continue, // Silently ignore failures per spec
            };

            if let Some(path_str) = thumb_path.to_str() {
                let db_guard = match db.lock() {
                    Ok(g) => g,
                    Err(_) => {
                        let _ = ui_sender.send(crate::ui::window::UiEvent::FatalError("Database lock poisoned in thumbnail worker".to_string()));
                        break;
                    }
                };
                if let Ok(_) = db_guard.set_thumbnail_and_duration(req.media_id, path_str, duration) {
                    let _ = ui_sender.send(crate::ui::window::UiEvent::ThumbnailReady(req.media_id, path_str.to_string(), duration));
                }
            }
        }
    });
}

fn generate_thumbnail(
    media_path: &Path,
    media_type: &MediaType,
    cache_dir: &Path,
) -> Result<(PathBuf, Option<i64>)> {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    media_path.hash(&mut hasher);
    let hash = hasher.finish();
    
    let thumb_name = format!("{:016x}.jpg", hash);
    let thumb_path = cache_dir.join(thumb_name);
    
    let mut duration_secs = None;
    if *media_type == MediaType::Video {
        let ffprobe_status = Command::new("ffprobe")
            .args(["-v", "error", "-show_entries", "format=duration", "-of", "default=noprint_wrappers=1:nokey=1"])
            .arg(media_path)
            .output();
            
        if let Ok(out) = ffprobe_status {
            let s = String::from_utf8_lossy(&out.stdout);
            if let Ok(f) = s.trim().parse::<f64>() {
                duration_secs = Some(f.round() as i64);
            }
        }
    }
    
    if thumb_path.exists() {
        let mut stale = false;
        if let (Ok(src_meta), Ok(thumb_meta)) = (std::fs::metadata(media_path), std::fs::metadata(&thumb_path)) {
            if let (Ok(src_mtime), Ok(thumb_mtime)) = (src_meta.modified(), thumb_meta.modified()) {
                if src_mtime > thumb_mtime {
                    stale = true;
                }
            }
        }
        if !stale {
            return Ok((thumb_path, duration_secs));
        }
    }

    match media_type {
        MediaType::Image => {
            let img = image::open(media_path)?;
            let resized = img.resize_to_fill(256, 256, image::imageops::FilterType::Triangle);
            resized.save_with_format(&thumb_path, image::ImageFormat::Jpeg)?;
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

            let status = Command::new("ffmpeg")
                .args([
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
                .status()?;
                
            if !status.success() {
                anyhow::bail!("ffmpeg failed");
            }
        }
    }
    
    Ok((thumb_path, duration_secs))
}
