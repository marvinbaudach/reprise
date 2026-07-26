//! Resolution-independent visual scene model with color tools.

pub mod color;
pub mod engine;
pub mod modes;
pub mod scene;

// Re-export public types
pub use color::{hsla_to_rgb, hue_shift, rgb_hue, secondary_accent};
pub use engine::{ModeCtx, VisualEngine};
pub use scene::{Fill, Geom, Rgba, Scene, Shape};

#[cfg(test)]
mod bars_source_tests {
    #[test]
    fn ac_21_builds_bars_without_visual_mode_state() {
        let engine = include_str!("visuals/engine.rs");
        for removed in ["pub enum VisualMode", "set_mode(", "fn mode("] {
            assert!(
                !engine.contains(removed),
                "Bars-only visuals must not retain mode state: {removed}"
            );
        }
        assert!(include_str!("visuals/modes.rs").contains("bars::scene(ctx)"));
    }

    #[test]
    fn removed_visual_modes_leave_no_state_or_geometry() {
        let engine = include_str!("visuals/engine.rs");
        for removed in [
            "MID_HIGH_RELEASE",
            "MID_RANGE",
            "HIGH_RANGE",
            "PROFILE_GROUP",
            "static_profile",
            "set_static_profile",
            "pub bass:",
            "pub mid:",
            "pub high:",
            "pub kick:",
            "pub clock:",
            "pub impact:",
            "Membrane",
            "hsla_fill",
            "fn band(",
        ] {
            assert!(
                !engine.contains(removed),
                "removed visual modes left engine marker {removed}"
            );
        }

        let modes = include_str!("visuals/modes.rs");
        for removed in [
            "mod grid",
            "mod flow",
            "mod pulse",
            "VisualMode",
            "VisualMode::Flow",
            "VisualMode::Pulse",
        ] {
            assert!(
                !modes.contains(removed),
                "removed visual mode dispatcher marker remains: {removed}"
            );
        }

        let scene = include_str!("visuals/scene.rs");
        for removed in ["Arc {", "Disc {"] {
            assert!(
                !scene.contains(removed),
                "removed visual modes left scene geometry {removed}"
            );
        }
    }
}
