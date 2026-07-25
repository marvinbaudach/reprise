//! Resolution-independent visual scene model with color tools.

pub mod color;
pub mod engine;
mod impact;
pub mod membrane;
pub mod modes;
pub mod scene;

// Re-export public types
pub use color::{hsla_to_rgb, hue_shift, rgb_hue, secondary_accent};
pub use engine::{GridCtx, VisualEngine};
pub use membrane::{Membrane, MEMBRANE_COLS, MEMBRANE_ROWS};
pub use scene::{Fill, Geom, Rgba, Scene, Shape};

#[cfg(test)]
mod grid_only_source_tests {
    #[test]
    fn grid_only_core_has_no_removed_mode_state_or_geometry() {
        let engine = include_str!("visuals/engine.rs");
        for removed in [
            "bands_peaks",
            "PEAK_DECAY",
            "MID_HIGH_RELEASE",
            "MID_RANGE",
            "HIGH_RANGE",
            "PROFILE_GROUP",
            "static_profile",
            "set_static_profile",
            "pub bands:",
            "pub peaks:",
            "pub bass:",
            "pub mid:",
            "pub high:",
            "pub kick:",
            "pub clock:",
            "pub impact:",
            "hsla_fill",
            "fn band(",
        ] {
            assert!(
                !engine.contains(removed),
                "removed visual modes left engine marker {removed}"
            );
        }

        let impact = include_str!("visuals/impact.rs");
        for removed in [
            "Shockwave",
            "Particle",
            "spawn_beat",
            "accent_boost",
            "kick",
        ] {
            assert!(
                !impact.contains(removed),
                "removed visual modes left impact marker {removed}"
            );
        }

        let scene = include_str!("visuals/scene.rs");
        for removed in ["Arc {", "Disc {", "Rect {"] {
            assert!(
                !scene.contains(removed),
                "removed visual modes left scene geometry {removed}"
            );
        }
    }
}
