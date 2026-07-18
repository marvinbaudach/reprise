pub(in crate::ui) mod artist_portrait_worker;
pub(in crate::ui) mod now_playing_column;
#[path = "now_playing.rs"]
mod surface;

#[allow(unused_imports)]
use super::*;
pub(in crate::ui) use surface::NowPlayingPanel;
