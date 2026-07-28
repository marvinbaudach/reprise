//! Linux implementations of reprise-core's platform seams: `player`
//! (GStreamer playbin3 `PlaybackBackend`) and `mpris` (D-Bus media
//! integration returning `MediaIntegrationHandles`), plus waveform extraction
//! (`waveform`). Any Linux frontend — GNOME today,
//! KDE/Qt later — composes these with reprise-core; macOS and
//! Windows get sibling crates implementing the same contracts (see the
//! plan's "Repository & frontend strategy").

mod crossfade;
pub mod device_sync;
pub mod device_transfer;
pub mod fingerprint;
mod gapless;
pub mod location;
pub mod mpris;
pub mod player;
mod player_effects;
pub mod runtime_client;
pub mod runtime_service;
pub mod trash;
pub mod waveform;

#[cfg(test)]
mod fingerprint_tests;
