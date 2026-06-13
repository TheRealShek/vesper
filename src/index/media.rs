//! Media type detection from file extensions.
//!
//! Supported types per spec section 4:
//! - Images: JPEG, PNG, GIF, WEBP, TIFF, BMP, HEIC
//! - Videos: MP4, MKV, AVI, MOV, WEBM, FLV, M4V

use std::path::Path;

use crate::events::MediaType;

/// Supported image extensions (lowercase).
const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "webp", "tiff", "tif", "bmp", "heic",
];

/// Supported video extensions (lowercase).
const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mkv", "avi", "mov", "webm", "flv", "m4v"];

/// Classifies a file path as image, video, or unsupported.
///
/// Returns `None` for unsupported or extensionless files.
/// Extension matching is case-insensitive.
pub fn classify(path: &Path) -> Option<MediaType> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
        Some(MediaType::Image)
    } else if VIDEO_EXTENSIONS.contains(&ext.as_str()) {
        Some(MediaType::Video)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn classifies_common_images() {
        assert_eq!(
            classify(&PathBuf::from("photo.jpg")),
            Some(MediaType::Image)
        );
        assert_eq!(
            classify(&PathBuf::from("photo.JPEG")),
            Some(MediaType::Image)
        );
        assert_eq!(
            classify(&PathBuf::from("photo.png")),
            Some(MediaType::Image)
        );
        assert_eq!(
            classify(&PathBuf::from("photo.webp")),
            Some(MediaType::Image)
        );
        assert_eq!(
            classify(&PathBuf::from("photo.heic")),
            Some(MediaType::Image)
        );
        assert_eq!(
            classify(&PathBuf::from("photo.tiff")),
            Some(MediaType::Image)
        );
        assert_eq!(
            classify(&PathBuf::from("photo.tif")),
            Some(MediaType::Image)
        );
        assert_eq!(
            classify(&PathBuf::from("photo.bmp")),
            Some(MediaType::Image)
        );
        assert_eq!(
            classify(&PathBuf::from("photo.gif")),
            Some(MediaType::Image)
        );
    }

    #[test]
    fn classifies_common_videos() {
        assert_eq!(
            classify(&PathBuf::from("video.mp4")),
            Some(MediaType::Video)
        );
        assert_eq!(
            classify(&PathBuf::from("video.MKV")),
            Some(MediaType::Video)
        );
        assert_eq!(
            classify(&PathBuf::from("video.webm")),
            Some(MediaType::Video)
        );
        assert_eq!(
            classify(&PathBuf::from("video.avi")),
            Some(MediaType::Video)
        );
        assert_eq!(
            classify(&PathBuf::from("video.mov")),
            Some(MediaType::Video)
        );
        assert_eq!(
            classify(&PathBuf::from("video.flv")),
            Some(MediaType::Video)
        );
        assert_eq!(
            classify(&PathBuf::from("video.m4v")),
            Some(MediaType::Video)
        );
    }

    #[test]
    fn rejects_unsupported_types() {
        assert_eq!(classify(&PathBuf::from("readme.txt")), None);
        assert_eq!(classify(&PathBuf::from("archive.zip")), None);
        assert_eq!(classify(&PathBuf::from("document.pdf")), None);
        assert_eq!(classify(&PathBuf::from("raw.cr2")), None);
    }

    #[test]
    fn rejects_extensionless_files() {
        assert_eq!(classify(&PathBuf::from("noext")), None);
        assert_eq!(classify(&PathBuf::from(".hidden")), None);
    }
}
