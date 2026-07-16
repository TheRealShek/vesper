//! Media type detection from file extensions.
//!
//! Supported types per spec section 4:
//! - Images: JPEG, PNG, GIF, WEBP, TIFF, BMP, HEIC
//! - Videos: MP4, MKV, AVI, MOV, WEBM, FLV, M4V

use std::path::Path;

use crate::events::MediaType;

/// Supported image extensions (lowercase).
// HEIC is included because the spec explicitly lists it; if the host lacks a decoder, the thumbnailer simply skips it.
pub const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "webp", "tiff", "tif", "bmp", "heic",
];

/// Supported video extensions (lowercase).
pub const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mkv", "avi", "mov", "webm", "flv", "m4v"];

/// Filename suffixes used by browsers, editors, and download tools for
/// in-progress or backup files. Matched against the whole (lowercased)
/// filename, so both dotted extensions (`.crdownload`) and bare suffixes (`~`)
/// are covered (B-3).
pub const TEMP_FILE_SUFFIXES: &[&str] = &[".crdownload", ".partial", ".swp", "~"];

/// Reports whether `path` is a scanner-level temporary/in-progress file.
///
/// This is a **hardcoded scanner-level filter**, deliberately separate from the
/// user-configurable ignore list: such files must never produce a media record
/// or a scan error, regardless of the user's settings, and must be filtered
/// before the live-update stability check even begins (B-3). Matching is on the
/// full filename so backup suffixes like `photo.jpg~` are caught as well as
/// download extensions like `movie.mp4.crdownload`.
pub fn is_temp_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let name = name.to_ascii_lowercase();
    TEMP_FILE_SUFFIXES
        .iter()
        .any(|suffix| name.ends_with(suffix))
}

/// Classifies a file path as image, video, or unsupported.
///
// Returns Option instead of Result because an unsupported extension is not an error, it's just a file the walker should silently ignore.
/// Returns `None` for unsupported or extensionless files.
/// Extension matching is case-insensitive.
pub fn classify(path: &Path) -> Option<MediaType> {
    // to_ascii_lowercase is used instead of to_lowercase to avoid Unicode overhead, since all target extensions are pure ASCII.
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

    #[test]
    fn temp_and_partial_files_are_filtered() {
        // Every hardcoded scanner-level temp suffix is recognized, including on
        // otherwise media-looking names (B-3). Repeated calls are stateless and
        // return the same verdict, so repeated watcher events never slip a
        // record or error through.
        for name in [
            "download.mp4.crdownload",
            "movie.mkv.partial",
            ".notes.txt.swp",
            "photo.jpg~",
            "PHOTO.JPG.CRDOWNLOAD", // case-insensitive
        ] {
            let p = PathBuf::from(name);
            assert!(is_temp_file(&p), "{name} must be filtered as a temp file");
            assert!(is_temp_file(&p), "{name} filter is stable across events");
        }
    }

    #[test]
    fn real_media_is_not_treated_as_temp() {
        for name in ["photo.jpg", "clip.mp4", "still.PNG", "noext"] {
            assert!(
                !is_temp_file(&PathBuf::from(name)),
                "{name} is real media, not a temp file"
            );
        }
    }
}
