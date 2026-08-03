//! The controller's callback slot types, split out of `player_controller.rs`
//! purely to keep that file under the 800-line cap the architecture gate
//! enforces — same rationale as its `mpris_mirror`/`now_playing_wiring`
//! siblings.

use std::rc::Rc;

use reprise_core::playback::{PlaybackState, SpectrumFrame};

use super::player_controller::NowPlaying;

pub(in crate::ui) type OnNowPlayingPanelTrackChanged = Rc<dyn Fn(Option<NowPlaying>)>;
pub(in crate::ui) type OnNowPlayingPanelStateChanged = Rc<dyn Fn(PlaybackState)>;
pub(in crate::ui) type OnSongVisualSpectrumChanged = Rc<dyn Fn(SpectrumFrame)>;
/// `(kick, pressure)` — the bass pair, for surfaces outside the player bar.
/// The track list's marker uses it to set its loop's tempo; nothing else in
/// that view reads it (AC-24).
pub(in crate::ui) type OnBassChanged = Rc<dyn Fn(f32, f32)>;
