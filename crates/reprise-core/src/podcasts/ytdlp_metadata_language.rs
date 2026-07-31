//! Preferred-language arguments for localized YouTube metadata.

use std::ffi::OsString;

use super::YtDlp;

impl YtDlp {
    /// Prefers localized YouTube metadata when the provider supplies it.
    pub fn with_metadata_language(mut self, language: Option<&str>) -> Self {
        self.metadata_language = language.and_then(normalize_metadata_language);
        self
    }

    pub(super) fn append_metadata_language(&self, arguments: &mut Vec<OsString>) {
        let Some(language) = &self.metadata_language else {
            return;
        };
        arguments.extend([
            OsString::from("--extractor-args"),
            OsString::from(format!("youtube:lang={language}")),
        ]);
    }
}

fn normalize_metadata_language(language: &str) -> Option<String> {
    let normalized = language.trim().replace('_', "-");
    let mut parts = normalized.split('-');
    let primary = parts.next()?.to_ascii_lowercase();
    if primary.is_empty()
        || !primary
            .chars()
            .all(|character| character.is_ascii_alphabetic())
    {
        return None;
    }
    if primary == "zh" {
        let territory = parts.next()?.to_ascii_uppercase();
        return matches!(territory.as_str(), "CN" | "TW" | "HK").then(|| format!("zh-{territory}"));
    }
    Some(primary)
}
