use std::path::Path;

use unicode_normalization::UnicodeNormalization;

pub(crate) fn normalize_search_text(value: &str) -> String {
    let nfc: String = value.nfc().collect();
    glib::casefold(&nfc).chars().nfc().collect()
}

pub(crate) fn normalized_basename(filename: &str) -> String {
    let basename = Path::new(filename)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(filename);
    normalize_search_text(basename)
}

#[cfg(test)]
mod tests {
    use super::normalize_search_text;

    #[test]
    fn normalization_composes_and_casefolds_unicode() {
        assert_eq!(
            normalize_search_text("  CAFÉ  ").trim(),
            normalize_search_text("cafe\u{301}")
        );
        assert_eq!(
            normalize_search_text("STRASSE"),
            normalize_search_text("Straße")
        );
    }
}
