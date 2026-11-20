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
//! names the fingerprint backend. It is the place where the concrete `Player`
//! and its event channel are built, and where the mount-table lookup behind the
//! fault-probe bound is wired in as the controller's `MountResolver`.

use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use reprise_core::playback::{PlaybackError, StreamEvent};
use reprise_core::waveform::RenderDataBackend;
use reprise_platform_linux::player::Player;

use super::player_controller::PlayerControllerBackends;
use crate::ui::playback::fault_probe_slots::{MountKey, MountResolver};

pub(super) fn build(
    waveform: Arc<dyn RenderDataBackend>,
    media: reprise_core::media_integration::MediaIntegrationHandles,
) -> Result<PlayerControllerBackends, PlaybackError> {
    let (sender, playback_events) = async_channel::unbounded::<StreamEvent>();
    // Tagged with the stream generation each event was produced under, so the
    // drain can tell an event that predates the user's latest start from one
    // that does not (see `stale_events::deliver_burst`).
    let player = Player::new_with_generation(Box::new(move |event| {
        if let Err(error) = sender.try_send(event) {
            tracing::warn!(%error, "player event dropped: UI receiver is gone");
        }
    }))?;

    Ok(PlayerControllerBackends {
        playback: Box::new(player),
        playback_events,
        media,
        waveform,
        mount_resolver: procfs_mount_resolver(),
    })
}

/// The production mount resolver: the platform crate's mount-table lookup, read
/// on each call so a mount made since startup is seen. A path it cannot place
/// shares the one `Unresolved` key.
fn procfs_mount_resolver() -> MountResolver {
    Rc::new(|path: &Path| {
        reprise_platform_linux::mounts::mount_point_of(path)
            .map_or(MountKey::Unresolved, MountKey::Mount)
    })
}
