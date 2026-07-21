//! Mix Builder labels and status copy.

use super::formatted;

pub const MIX_BUILDER_TITLE: &str = N_!("Create Similar Mix");
pub const MIX_BUILDER_SEEDS: &str = N_!("Based on");
pub const MIX_BUILDER_STATS_TARGET: &str = N_!("Your My Stats audio character");
pub const MIX_BUILDER_STATS_TARGET_DESCRIPTION: &str =
    N_!("Uses the profile direction shown for the selected period");
pub const MIX_BUILDER_OPTIONS: &str = N_!("Mix options");
pub const MIX_BUILDER_CRITERION: &str = N_!("Criterion");
pub const MIX_BUILDER_CRITERION_AUDIO: &str = N_!("Audio character");
pub const MIX_BUILDER_CRITERION_GENRE: &str = N_!("Genre");
pub const MIX_BUILDER_CRITERION_RELATED: &str = N_!("Related artists");
pub const MIX_BUILDER_CRITERION_BALANCED: &str = N_!("Balanced");
pub const MIX_BUILDER_DURATION: &str = N_!("Duration");
pub const MIX_BUILDER_DURATION_30: &str = N_!("30 min");
pub const MIX_BUILDER_DURATION_60: &str = N_!("60 min");
pub const MIX_BUILDER_DURATION_90: &str = N_!("90 min");
pub const MIX_BUILDER_FAMILIARITY: &str = N_!("Familiarity");
pub const MIX_BUILDER_FAMILIAR: &str = N_!("Familiar");
pub const MIX_BUILDER_DISCOVER: &str = N_!("Discover");
pub const MIX_BUILDER_VARIETY: &str = N_!("Variety");
pub const MIX_BUILDER_COHESIVE: &str = N_!("Cohesive");
pub const MIX_BUILDER_WIDE: &str = N_!("Wide");
pub const MIX_BUILDER_ENERGY: &str = N_!("Energy curve");
pub const MIX_BUILDER_FLAT: &str = N_!("Flat");
pub const MIX_BUILDER_RISE: &str = N_!("Rise");
pub const MIX_BUILDER_FALL: &str = N_!("Fall");
pub const MIX_BUILDER_ARC: &str = N_!("Arc");
pub const MIX_BUILDER_PREVIEW: &str = N_!("Preview Mix");
pub const MIX_BUILDER_PREVIEW_HEADING: &str = N_!("Preview");
pub const MIX_BUILDER_PREVIEW_EMPTY: &str =
    N_!("Adjust the options, then preview the exact mix before applying it.");
pub const MIX_BUILDER_PLAY: &str = N_!("Play");
pub const MIX_BUILDER_QUEUE: &str = N_!("Add to Queue");
pub const MIX_BUILDER_SAVE: &str = N_!("Save as Playlist…");
pub const MIX_BUILDER_SAVE_ACTION: &str = N_!("Save");
pub const MIX_BUILDER_SAVE_TITLE: &str = N_!("Save Mix as Playlist");
pub const MIX_BUILDER_SAVE_PLACEHOLDER: &str = N_!("Playlist name");
pub const MIX_BUILDER_SAVED: &str = N_!("Mix saved as playlist");
pub const MIX_BUILDER_FAILED: &str = N_!("Could not create a mix preview");
pub const MIX_BUILDER_SAVE_FAILED: &str = N_!("Could not save the mix playlist");
pub const MIX_BUILDER_SUMMARY: &str =
    N_!("{count} tracks · {minutes} min · {analyzed}/{total} analyzed{diagnostics}");
pub const MIX_BUILDER_DIAGNOSTICS: &str = N_!(" · {count} diagnostics: {details}");
pub const MIX_REASON_INTENSITY: &str = N_!("intensity match");
pub const MIX_REASON_BRIGHTNESS: &str = N_!("brightness match");
pub const MIX_REASON_DYNAMICITY: &str = N_!("dynamicity match");
pub const MIX_REASON_RHYTHMICITY: &str = N_!("rhythmicity match");
pub const MIX_REASON_GENRE: &str = N_!("genre match");
pub const MIX_REASON_RELATED_ARTIST: &str = N_!("related artist");
pub const MIX_REASON_FAMILIARITY: &str = N_!("familiarity fit");
pub const MIX_REASON_DIVERSITY: &str = N_!("artist diversity");
pub const MIX_REASON_DURATION: &str = N_!("duration fit");
pub const MIX_DIAGNOSTIC_ARTIST_GAP: &str = N_!("artist spacing relaxed");
pub const MIX_DIAGNOSTIC_DURATION: &str = N_!("target duration not reached");
pub const MIX_DIAGNOSTIC_AUDIO: &str = N_!("some audio evidence is missing");
pub const MIX_DIAGNOSTIC_GENRE: &str = N_!("some genre evidence is missing");
pub const MIX_DIAGNOSTIC_RELATED: &str = N_!("related-artist evidence is missing");
pub const MIX_DISCOVERY_TITLE: &str = N_!("Artists outside your library");
pub const MIX_DISCOVERY_DESCRIPTION: &str =
    N_!("Optional ListenBrainz suggestions stay separate from the playable mix.");
pub const MIX_DISCOVERY_FIND: &str = N_!("Find Related Artists");
pub const MIX_DISCOVERY_LOADING: &str = N_!("Finding related artists…");
pub const MIX_DISCOVERY_DISABLED: &str =
    N_!("Enable Related Artist Discovery in Plugins to contact ListenBrainz.");
pub const MIX_DISCOVERY_NO_SEED_ID: &str =
    N_!("The selected artists need MusicBrainz IDs for discovery.");
pub const MIX_DISCOVERY_EMPTY: &str = N_!("No new artists were found for this selection.");
pub const MIX_DISCOVERY_FAILED: &str = N_!("Related artist discovery failed");
pub const MIX_DISCOVERY_HIDDEN: &str = N_!("Hidden artist suggestions");
pub const MIX_DISCOVERY_OPEN: &str = N_!("Open");
pub const MIX_DISCOVERY_HIDE: &str = N_!("Hide");
pub const MIX_DISCOVERY_RESTORE: &str = N_!("Restore");
const MIX_DISCOVERY_SUBTITLE: &str = N_!("{reason} · Source: {source}");

pub fn mix_discovery_subtitle(reason: &str, source: &str) -> String {
    formatted(
        MIX_DISCOVERY_SUBTITLE,
        &[("reason", reason), ("source", source)],
    )
}
