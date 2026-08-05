//! Constructs the concrete playback backend and bundles it into the
//! `PlayerControllerBackends` the window hands to `PlayerController`.
//!
//! It lives in its own file rather than in `window.rs` because `window.rs` is
//! the composition root and the architecture lint holds it below 600 lines;
//! this was the only item in that file doing work rather than composing.
//!
//! `ui/playback` and `ui/scan` are barred from naming `reprise_platform_linux`
//! at all — `scripts/check-architecture.sh` enforces that — and the window
//! layer is where the concrete backends are allowed to be named instead. This
//! module is not the *only* such place: `window.rs` still names the waveform,
//! MPRIS and device-monitor backends inline, and `window_runtime_wiring.rs`
//! names the fingerprint backend. It is only the place where the concrete
//! `Player` and its event channel are built.

use std::sync::Arc;

use reprise_core::playback::{PlaybackError, PlayerEvent};
use reprise_core::waveform::RenderDataBackend;
use reprise_platform_linux::player::Player;

use super::player_controller::PlayerControllerBackends;

pub(super) fn build(
    waveform: Arc<dyn RenderDataBackend>,
    media: reprise_core::media_integration::MediaIntegrationHandles,
) -> Result<PlayerControllerBackends, PlaybackError> {
    let (sender, playback_events) = async_channel::unbounded::<PlayerEvent>();
    let player = Player::new(Box::new(move |event| {
        if let Err(error) = sender.try_send(event) {
            tracing::warn!(%error, "player event dropped: UI receiver is gone");
        }
    }))?;

    Ok(PlayerControllerBackends {
        playback: Box::new(player),
        playback_events,
        media,
        waveform,
    })
}
