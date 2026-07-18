pub(in crate::ui) mod artist_portrait_worker;
pub(in crate::ui) mod now_playing_column;
pub(in crate::ui) mod now_playing_empty_state;
pub(in crate::ui) mod now_playing_feedback;
pub(in crate::ui) mod now_playing_portrait;
pub(in crate::ui) mod now_playing_state;
#[path = "now_playing.rs"]
mod surface;

#[allow(unused_imports)]
use super::*;
pub(in crate::ui) use surface::NowPlayingPanel;
