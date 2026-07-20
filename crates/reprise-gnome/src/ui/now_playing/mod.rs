pub(in crate::ui) mod artist_portrait_worker;
pub(in crate::ui) mod audio_character_view;
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
