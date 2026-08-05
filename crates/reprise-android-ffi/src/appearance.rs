//! Typed appearance settings at the Android boundary.
//!
//! The Core settings table deliberately stays generic. This boundary does not:
//! reads retain an unsupported theme id so a surface can fall back without
//! destroying another surface's choice, while writes admit only palettes the
//! Android surface can actually render.

use reprise_core::library::settings;

use crate::{LibraryError, MusicLibrary};

const THEME_SETTING_KEY: &str = "ui.theme";
const VISUALIZER_SETTING_KEY: &str = "ui.now_playing.visualizer";
const LIBRARY_RATING_SETTING_KEY: &str = "ui.mobile.library_rating";

/// What the shared `ui.theme` key currently contains.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum AndroidStoredTheme {
    Unset,
    Nocturne,
    Dynamic,
    Unsupported { id: String },
}

impl AndroidStoredTheme {
    fn from_setting(value: Option<&str>) -> Self {
        match value {
            None => Self::Unset,
            Some("nocturne") => Self::Nocturne,
            Some("dynamic") => Self::Dynamic,
            Some(id) => Self::Unsupported { id: id.to_owned() },
        }
    }
}

/// Theme ids Android is allowed to persist.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum AndroidThemeChoice {
    Nocturne,
    Dynamic,
}

impl AndroidThemeChoice {
    fn setting_id(self) -> &'static str {
        match self {
            Self::Nocturne => "nocturne",
            Self::Dynamic => "dynamic",
        }
    }
}

/// What the shared Now Playing visualizer key currently contains.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum AndroidStoredVisualizer {
    Unset,
    Cover,
    Spectrum,
    PreviewBand,
    Ambient,
    Unsupported { id: String },
}

impl AndroidStoredVisualizer {
    fn from_setting(value: Option<&str>) -> Self {
        match value {
            None => Self::Unset,
            Some("cover") => Self::Cover,
            Some("spectrum") => Self::Spectrum,
            Some("preview-band") => Self::PreviewBand,
            Some("ambient") => Self::Ambient,
            Some(id) => Self::Unsupported { id: id.to_owned() },
        }
    }
}

/// What the Android library-row rating key currently contains.
///
/// This key is surface-scoped because desktop rating visibility is a column in
/// `ui.column_layout`, not a boolean. Sharing either setting would couple two
/// different presentation contracts.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum AndroidStoredLibraryRating {
    Unset,
    Off,
    On,
    Unsupported { id: String },
}

impl AndroidStoredLibraryRating {
    fn from_setting(value: Option<&str>) -> Self {
        match value {
            Some("0") => Self::Off,
            Some("1") => Self::On,
            None => Self::Unset,
            Some(id) => Self::Unsupported { id: id.to_owned() },
        }
    }
}

/// Visualizer ids Android is allowed to persist after an explicit choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum AndroidVisualizerChoice {
    Cover,
    Spectrum,
    PreviewBand,
    Ambient,
}

impl AndroidVisualizerChoice {
    fn setting_id(self) -> &'static str {
        match self {
            Self::Cover => "cover",
            Self::Spectrum => "spectrum",
            Self::PreviewBand => "preview-band",
            Self::Ambient => "ambient",
        }
    }
}

/// The shared light/dark preference used only by a switchable palette.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum AndroidColorScheme {
    Light,
    Dark,
    System,
}

impl AndroidColorScheme {
    fn from_core(value: &str) -> Self {
        match value {
            "light" => Self::Light,
            "dark" => Self::Dark,
            _ => Self::System,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AndroidAppearanceSettings {
    pub theme: AndroidStoredTheme,
    pub color_scheme: AndroidColorScheme,
}

#[uniffi::export]
impl MusicLibrary {
    pub fn appearance_settings(&self) -> Result<AndroidAppearanceSettings, LibraryError> {
        let state = self.lock()?;
        let theme = settings::get_setting(&state.db, THEME_SETTING_KEY).map_err(|error| {
            LibraryError::Database {
                detail: error.to_string(),
            }
        })?;
        Ok(AndroidAppearanceSettings {
            theme: AndroidStoredTheme::from_setting(theme.as_deref()),
            color_scheme: AndroidColorScheme::from_core(settings::get_color_scheme(&state.db)),
        })
    }

    pub fn set_theme(&self, theme: AndroidThemeChoice) -> Result<(), LibraryError> {
        let state = self.lock()?;
        settings::set_setting(&state.db, THEME_SETTING_KEY, theme.setting_id()).map_err(|error| {
            LibraryError::Database {
                detail: error.to_string(),
            }
        })
    }

    pub fn visualizer_setting(&self) -> Result<AndroidStoredVisualizer, LibraryError> {
        let state = self.lock()?;
        let value = settings::get_setting(&state.db, VISUALIZER_SETTING_KEY).map_err(|error| {
            LibraryError::Database {
                detail: error.to_string(),
            }
        })?;
        Ok(AndroidStoredVisualizer::from_setting(value.as_deref()))
    }

    pub fn set_visualizer(&self, visualizer: AndroidVisualizerChoice) -> Result<(), LibraryError> {
        let state = self.lock()?;
        settings::set_setting(&state.db, VISUALIZER_SETTING_KEY, visualizer.setting_id()).map_err(
            |error| LibraryError::Database {
                detail: error.to_string(),
            },
        )
    }

    pub fn library_rating_setting(&self) -> Result<AndroidStoredLibraryRating, LibraryError> {
        let state = self.lock()?;
        let value =
            settings::get_setting(&state.db, LIBRARY_RATING_SETTING_KEY).map_err(|error| {
                LibraryError::Database {
                    detail: error.to_string(),
                }
            })?;
        Ok(AndroidStoredLibraryRating::from_setting(value.as_deref()))
    }

    pub fn set_library_rating(&self, enabled: bool) -> Result<(), LibraryError> {
        let state = self.lock()?;
        settings::set_bool(&state.db, LIBRARY_RATING_SETTING_KEY, enabled).map_err(|error| {
            LibraryError::Database {
                detail: error.to_string(),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use reprise_core::library::settings;

    use super::{
        AndroidColorScheme, AndroidStoredLibraryRating, AndroidStoredTheme,
        AndroidStoredVisualizer, AndroidThemeChoice, AndroidVisualizerChoice,
    };
    use crate::MusicLibrary;

    #[test]
    fn appearance_settings_cross_typed_boundary_without_discarding_unknown_theme() {
        let directory = tempfile::tempdir().unwrap();
        let library = MusicLibrary::open(
            directory.path().to_str().unwrap(),
            directory.path().join("cache").to_str().unwrap(),
        )
        .unwrap();
        {
            let state = library.lock().unwrap();
            settings::set_setting(&state.db, super::THEME_SETTING_KEY, "night-terrain").unwrap();
            settings::set_color_scheme(&state.db, "dark").unwrap();
        }

        assert_eq!(
            library.appearance_settings().unwrap(),
            super::AndroidAppearanceSettings {
                theme: AndroidStoredTheme::Unsupported {
                    id: "night-terrain".to_owned(),
                },
                color_scheme: AndroidColorScheme::Dark,
            },
        );

        // Asserted as a whole record: writing the theme must leave the colour
        // scheme alone. One key per setter guarantees that today, so the
        // assertion is here to catch the day a setter writes both through one
        // helper.
        library.set_theme(AndroidThemeChoice::Dynamic).unwrap();
        assert_eq!(
            library.appearance_settings().unwrap(),
            super::AndroidAppearanceSettings {
                theme: AndroidStoredTheme::Dynamic,
                color_scheme: AndroidColorScheme::Dark,
            },
        );
    }

    #[test]
    fn visualizer_setting_crosses_the_typed_boundary_without_stealing_unknown_values() {
        let directory = tempfile::tempdir().unwrap();
        let library = MusicLibrary::open(
            directory.path().to_str().unwrap(),
            directory.path().join("cache").to_str().unwrap(),
        )
        .unwrap();
        {
            let state = library.lock().unwrap();
            settings::set_setting(
                &state.db,
                super::VISUALIZER_SETTING_KEY,
                "future-data-driven-mode",
            )
            .unwrap();
        }

        assert_eq!(
            library.visualizer_setting().unwrap(),
            AndroidStoredVisualizer::Unsupported {
                id: "future-data-driven-mode".to_owned(),
            },
        );
        library
            .set_visualizer(AndroidVisualizerChoice::Ambient)
            .unwrap();
        assert_eq!(
            library.visualizer_setting().unwrap(),
            AndroidStoredVisualizer::Ambient,
        );
    }

    #[test]
    fn stored_library_rating_true_reads_as_on() {
        let directory = tempfile::tempdir().unwrap();
        let library = MusicLibrary::open(
            directory.path().to_str().unwrap(),
            directory.path().join("cache").to_str().unwrap(),
        )
        .unwrap();
        {
            let state = library.lock().unwrap();
            settings::set_setting(&state.db, super::LIBRARY_RATING_SETTING_KEY, "1").unwrap();
        }

        assert_eq!(
            library.library_rating_setting().unwrap(),
            AndroidStoredLibraryRating::On,
        );
    }

    #[test]
    fn unsupported_library_rating_value_is_reported_without_being_destroyed() {
        let directory = tempfile::tempdir().unwrap();
        let library = MusicLibrary::open(
            directory.path().to_str().unwrap(),
            directory.path().join("cache").to_str().unwrap(),
        )
        .unwrap();
        {
            let state = library.lock().unwrap();
            settings::set_setting(
                &state.db,
                super::LIBRARY_RATING_SETTING_KEY,
                "future-choice",
            )
            .unwrap();
        }

        assert_eq!(
            library.library_rating_setting().unwrap(),
            AndroidStoredLibraryRating::Unsupported {
                id: "future-choice".to_owned(),
            },
        );
        let state = library.lock().unwrap();
        assert_eq!(
            settings::get_setting(&state.db, super::LIBRARY_RATING_SETTING_KEY)
                .unwrap()
                .as_deref(),
            Some("future-choice"),
        );
    }

    #[test]
    fn unset_library_rating_is_distinct_from_stored_off() {
        let directory = tempfile::tempdir().unwrap();
        let library = MusicLibrary::open(
            directory.path().to_str().unwrap(),
            directory.path().join("cache").to_str().unwrap(),
        )
        .unwrap();

        assert_eq!(
            library.library_rating_setting().unwrap(),
            AndroidStoredLibraryRating::Unset,
        );
        {
            let state = library.lock().unwrap();
            settings::set_setting(&state.db, super::LIBRARY_RATING_SETTING_KEY, "0").unwrap();
        }
        assert_eq!(
            library.library_rating_setting().unwrap(),
            AndroidStoredLibraryRating::Off,
        );
    }

    #[test]
    fn library_rating_write_is_visible_through_a_fresh_handle() {
        let directory = tempfile::tempdir().unwrap();
        let cache = directory.path().join("cache");
        let library =
            MusicLibrary::open(directory.path().to_str().unwrap(), cache.to_str().unwrap())
                .unwrap();

        library.set_library_rating(true).unwrap();
        drop(library);

        let fresh = MusicLibrary::open(directory.path().to_str().unwrap(), cache.to_str().unwrap())
            .unwrap();
        assert_eq!(
            fresh.library_rating_setting().unwrap(),
            AndroidStoredLibraryRating::On,
        );
    }
}
