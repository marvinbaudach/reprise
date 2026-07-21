//! Resolution-independent visual scene model with color tools.

pub mod color;
pub mod dust;
pub mod engine;
pub mod impact;
pub mod modes;
pub mod scene;
pub mod water;

// Re-export public types
pub use color::{hsla_to_rgb, hue_shift, rgb_hue, secondary_accent};
pub use dust::{advance_dust, make_dust, Dust, DUST_COUNT};
pub use engine::{ModeCtx, VisualEngine, VisualMode};
pub use impact::{ImpactState, ParticleDraw, ShockwaveDraw};
pub use scene::{Fill, Geom, Rgba, Scene, Shape};
pub use water::{WaterGrid, WATER_COLS, WATER_ROWS};
