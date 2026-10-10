//! Linux implementations of reprise-core's platform seams: `player`
//! (GStreamer playbin3 `PlaybackBackend`) and `mpris` (D-Bus media
//! integration returning `MediaIntegrationHandles`), plus waveform extraction
//! (`waveform`). Any Linux frontend — GNOME today,
//! KDE/Qt later — composes these with reprise-core; macOS and
//! Windows get sibling crates implementing the same contracts (see the
//! plan's "Repository & frontend strategy").

// Production code must not panic on bad input or broken invariants: every
// fallible step returns an error instead. A genuine, documented invariant is
// suppressed at its site with `#[expect(clippy::<lint>, reason = "…")]`, and
// scripts/check-crate-layers.sh counts those suppressions against a budget.
// Test builds are exempt; test doubles legitimately use unreachable!/expect.
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented,
        clippy::unreachable
    )
)]

mod cava_stage;
mod crossfade;
pub mod device_sync;
pub mod device_transfer;
pub mod diagnostics;
pub mod fingerprint;
mod gapless;
pub mod location;
pub mod mpris;
pub mod player;
mod player_effects;
pub(crate) mod player_pipeline;
pub mod signals;
pub mod spectrogram_backfill;
pub mod termination;
pub mod trash;
pub mod waveform;

#[cfg(test)]
mod fingerprint_tests;
