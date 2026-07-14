//! Linux implementations of reprise-core's two platform seams: `player`
//! (GStreamer playbin3 `PlaybackBackend`) and `mpris` (D-Bus media
//! integration returning `MediaIntegrationHandles`). Any Linux frontend —
//! GNOME today, KDE/Qt later — composes these with reprise-core; macOS and
//! Windows get sibling crates implementing the same contracts (see the
//! plan's "Repository & frontend strategy").

pub mod device_sync;
pub mod mpris;
pub mod player;
mod player_effects;
pub mod trash;
