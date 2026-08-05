//! Persisted controls for the optional Sound Similarity module.

use crate::db::Db;
use crate::library::settings;
use crate::sound_distance::DistanceWeights;

const EXCLUDE_ALBUM_KEY: &str = "sound_similarity.exclude_same_album";
const EXCLUDE_ARTIST_KEY: &str = "sound_similarity.exclude_same_artist";
const INCLUDE_TEMPO_KEY: &str = "sound_similarity.include_tempo";
const WEIGHTING_KEY: &str = "sound_similarity.weighting";
const MATCH_COUNT_KEY: &str = "sound_similarity.match_count";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SoundWeighting {
    #[default]
    Default,
    Timbre,
    Dynamics,
}

impl SoundWeighting {
    pub fn weights(self) -> DistanceWeights {
        match self {
            Self::Default => DistanceWeights::DEFAULT,
            Self::Timbre => DistanceWeights::TIMBRE,
            Self::Dynamics => DistanceWeights::DYNAMICS,
        }
    }

    pub fn setting(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Timbre => "timbre",
            Self::Dynamics => "dynamics",
        }
    }

    fn from_setting(value: Option<&str>) -> Self {
        match value {
            Some("timbre") => Self::Timbre,
            Some("dynamics") => Self::Dynamics,
            _ => Self::Default,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SoundSimilarityPreferences {
    pub exclude_same_album: bool,
    pub exclude_same_artist: bool,
    pub include_tempo: bool,
    pub weighting: SoundWeighting,
    pub match_count: usize,
}

impl Default for SoundSimilarityPreferences {
    fn default() -> Self {
        Self {
            exclude_same_album: true,
            exclude_same_artist: false,
            include_tempo: false,
            weighting: SoundWeighting::Default,
            match_count: 7,
        }
    }
}

impl SoundSimilarityPreferences {
    pub fn load(db: &Db) -> Result<Self, rusqlite::Error> {
        let defaults = Self::default();
        let weighting = settings::get_setting(db, WEIGHTING_KEY)?;
        let match_count = settings::get_setting(db, MATCH_COUNT_KEY)?
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|count| (1..=50).contains(count))
            .unwrap_or(defaults.match_count);
        Ok(Self {
            exclude_same_album: settings::get_bool(
                db,
                EXCLUDE_ALBUM_KEY,
                defaults.exclude_same_album,
            )?,
            exclude_same_artist: settings::get_bool(
                db,
                EXCLUDE_ARTIST_KEY,
                defaults.exclude_same_artist,
            )?,
            include_tempo: settings::get_bool(db, INCLUDE_TEMPO_KEY, defaults.include_tempo)?,
            weighting: SoundWeighting::from_setting(weighting.as_deref()),
            match_count,
        })
    }

    pub fn save(self, db: &Db) -> Result<(), rusqlite::Error> {
        settings::set_bool(db, EXCLUDE_ALBUM_KEY, self.exclude_same_album)?;
        settings::set_bool(db, EXCLUDE_ARTIST_KEY, self.exclude_same_artist)?;
        settings::set_bool(db, INCLUDE_TEMPO_KEY, self.include_tempo)?;
        settings::set_setting(db, WEIGHTING_KEY, self.weighting.setting())?;
        settings::set_setting(
            db,
            MATCH_COUNT_KEY,
            &self.match_count.clamp(1, 50).to_string(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sound_preferences_default_and_round_trip() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(
            SoundSimilarityPreferences::load(&db).unwrap(),
            Default::default()
        );
        let expected = SoundSimilarityPreferences {
            exclude_same_album: false,
            exclude_same_artist: true,
            include_tempo: true,
            weighting: SoundWeighting::Dynamics,
            match_count: 12,
        };
        expected.save(&db).unwrap();
        assert_eq!(SoundSimilarityPreferences::load(&db).unwrap(), expected);
    }
}
