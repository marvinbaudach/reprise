pub(in crate::ui) mod library_player_bar;
mod player_bar_cover;
mod player_bar_external;
pub(in crate::ui) mod player_bar_layout;
pub(in crate::ui) mod player_bar_seek;
pub(in crate::ui) mod player_bar_state;
mod player_bar_volume;
pub(in crate::ui) mod seek_colouring;
pub(in crate::ui) mod seek_legend;
mod seek_menu;
pub(in crate::ui) mod sleep_timer_button;
#[path = "player_bar.rs"]
mod surface;
mod transport_glyph;
mod waveform_playhead;
mod waveform_primitives;
pub(in crate::ui) mod waveform_seek;
pub(in crate::ui) use reprise_view::waveform as waveform_shape;

#[allow(
    unused_imports,
    reason = "child modules share the parent UI vocabulary through this import"
)]
use super::*;
pub(in crate::ui) use surface::{
    PlayerBar, ICON_NEXT, ICON_PREVIOUS, ICON_REPEAT_ALL, ICON_SHUFFLE,
};
