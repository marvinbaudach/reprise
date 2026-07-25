//! Resolution-independent visual scene model with color tools.

pub mod color;
pub mod engine;
pub mod impact;
pub mod membrane;
pub mod modes;
mod rng;
pub mod scene;

// Re-export public types
pub use color::{hsla_to_rgb, hue_shift, rgb_hue, secondary_accent};
pub use engine::{GridCtx, VisualEngine};
pub use impact::{ImpactState, ParticleDraw, ShockwaveDraw};
pub use membrane::{Membrane, MEMBRANE_COLS, MEMBRANE_ROWS};
pub use scene::{Fill, Geom, Rgba, Scene, Shape};
