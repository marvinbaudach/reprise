pub(in crate::ui) mod artist_portrait_worker;
// Task ordering lands the self-contained layer before the panel consumes it.
// Remove this temporary allowance when the wiring lands in the next task.
#[allow(dead_code)]
mod cover_bloom;
pub(in crate::ui) mod now_playing_column;
mod panel_state;
pub(in crate::ui) mod song_visualizer;
#[path = "now_playing.rs"]
mod surface;
mod surface_css;
pub(in crate::ui) mod up_next_panel;

#[allow(unused_imports)]
use super::*;
pub(in crate::ui) use surface::NowPlayingPanel;

pub(in crate::ui) fn css() -> String {
    [surface::css(), up_next_panel::css(), song_visualizer::css()].join("\n")
}
