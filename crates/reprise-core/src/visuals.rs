//! Resolution-independent visual scene model with color tools.

pub mod color;
pub mod engine;
mod impact;
pub mod membrane;
pub mod modes;
pub mod scene;

// Re-export public types
pub use color::{hsla_to_rgb, hue_shift, rgb_hue, secondary_accent};
pub use engine::{ModeCtx, VisualEngine, VisualMode};
pub use membrane::{Membrane, MEMBRANE_COLS, MEMBRANE_ROWS};
pub use scene::{Fill, Geom, Rgba, Scene, Shape};

#[cfg(test)]
mod visual_mode_source_tests {
    use super::engine::VisualMode;

    #[test]
    fn ac_20_exports_exactly_grid_and_bars() {
        assert_eq!(VisualMode::ALL, [VisualMode::Grid, VisualMode::Bars]);
        assert_eq!(VisualMode::default(), VisualMode::Grid);
        assert_eq!(VisualMode::Grid.id(), "grid");
        assert_eq!(VisualMode::Bars.id(), "bars");
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

        let modes = include_str!("visuals/modes.rs");
        for removed in [
            "mod flow",
            "mod pulse",
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
