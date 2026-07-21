//! Resolution-independent visual scene model with color tools.

pub mod color;
pub mod scene;

// Re-export public types
pub use color::{hsla_to_rgb, hue_shift, rgb_hue, secondary_accent};
pub use scene::{Fill, Geom, Rgba, Scene, Shape};
