//! User-facing copy for the local Sound Similarity module.

pub const SOUND: &str = N_!("Sound");
pub const SOUND_PROFILE: &str = N_!("Sound profile");
pub const SOUND_TIMBRE_AXIS: &str = N_!("Timbre · dark ↔ bright");
pub const SOUND_DYNAMICS_AXIS: &str = N_!("Dynamics · dense ↔ open");
pub const SOUND_TEMPO_AXIS: &str = N_!("Tempo · slow ↔ fast");
pub const SOUND_SOUNDS_LIKE: &str = N_!("Sounds like · of {count}");
pub const SOUND_ADD_TO_QUEUE: &str = N_!("Add to queue");
pub const SOUND_MORE_ACTIONS: &str = N_!("More actions");
pub const SOUND_INFO_TOOLTIP: &str = N_!("Show sound details");
#[allow(dead_code)] // installed in the track menu in package P6
pub const SOUND_FIND_SIMILAR: &str = N_!("Find similar tracks");
pub const SOUND_ANALYSING: &str = N_!("Analysing your library — {ready} of {total}");
pub const SOUND_ANALYSIS_FAILED: &str = N_!("Sound analysis is unavailable");
pub const SOUND_FILE_UP_TO: &str = N_!("up to {frequency}");
#[allow(dead_code)] // displayed by the module preference in package P6
pub const SOUND_TEMPO_WARNING: &str =
    N_!("Estimated from onsets; halftime and time changes can put it out by a factor of two.");

pub fn sound_sounds_like(count: usize) -> String {
    super::formatted(SOUND_SOUNDS_LIKE, &[("count", &count.to_string())])
}

pub fn sound_analysing(ready: usize, total: usize) -> String {
    super::formatted(
        SOUND_ANALYSING,
        &[("ready", &ready.to_string()), ("total", &total.to_string())],
    )
}
