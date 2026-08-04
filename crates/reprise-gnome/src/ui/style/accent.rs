#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the persistence contract is wired in accent-color source step 2"
    )
)]

/// Reprise's brand accent: the thick barline of the repeat-sign logo.
pub(in crate::ui) const APP_ACCENT: &str = "#4FDBD4";

/// Settings key persisting the selected [`AccentSource`].
pub(in crate::ui) const ACCENT_SOURCE_SETTING_KEY: &str = "ui.accent-source";

/// The source used for libadwaita's accent roles and Rust-side accent readers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum AccentSource {
    App,
    System,
}

impl AccentSource {
    /// A fresh install starts with Reprise's own brand accent.
    pub(in crate::ui) const DEFAULT: Self = Self::App;

    /// Stable persistence key.
    pub(in crate::ui) const fn id(self) -> &'static str {
        match self {
            Self::App => "app",
            Self::System => "system",
        }
    }

    /// Restores a persisted source, falling back safely for unknown values.
    pub(in crate::ui) fn from_id(id: &str) -> Self {
        match id {
            "system" => Self::System,
            "app" => Self::App,
            _ => Self::DEFAULT,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accent_source_ids_round_trip_and_unknown_ids_use_the_default() {
        for source in [AccentSource::App, AccentSource::System] {
            assert_eq!(AccentSource::from_id(source.id()), source);
        }
        assert_eq!(
            AccentSource::from_id("does-not-exist"),
            AccentSource::DEFAULT
        );
        assert_eq!(AccentSource::DEFAULT, AccentSource::App);
        assert_eq!(APP_ACCENT, "#4FDBD4");
        assert_eq!(ACCENT_SOURCE_SETTING_KEY, "ui.accent-source");
    }
}
