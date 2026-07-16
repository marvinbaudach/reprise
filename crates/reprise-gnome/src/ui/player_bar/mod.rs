pub(in crate::ui) mod library_player_bar;
pub(in crate::ui) mod player_bar_layout;
pub(in crate::ui) mod player_bar_seek;
pub(in crate::ui) mod player_bar_state;
#[path = "player_bar.rs"]
mod surface;
pub(in crate::ui) mod waveform_seek;
pub(in crate::ui) mod waveform_shape;

#[allow(unused_imports)]
use super::*;
pub(in crate::ui) use surface::{
    PlayerBar, ICON_NEXT, ICON_PLAY, ICON_PREVIOUS, ICON_REPEAT_ALL, ICON_SHUFFLE,
};
