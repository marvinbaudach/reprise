pub(in crate::ui) mod library_player_bar;
mod player_bar_cover;
mod player_bar_external;
pub(in crate::ui) mod player_bar_layout;
pub(in crate::ui) mod player_bar_seek;
pub(in crate::ui) mod player_bar_state;
pub(in crate::ui) mod seek_legend;
mod seek_menu;
#[path = "player_bar.rs"]
mod surface;
mod transport_glyph;
mod waveform_playhead;
mod waveform_primitives;
pub(in crate::ui) mod waveform_seek;
pub(in crate::ui) use reprise_view::waveform as waveform_shape;

#[allow(unused_imports)]
use super::*;
pub(in crate::ui) use surface::{
    PlayerBar, ICON_NEXT, ICON_PREVIOUS, ICON_REPEAT_ALL, ICON_SHUFFLE,
};
