//! Per-mode scene builders. Each mode module owns exactly one function,
//! `scene(ctx: &ModeCtx) -> Vec<Shape>`, translating the shared reactive
//! state in [`ModeCtx`] into resolution-independent shapes. `build_scene`
//! dispatches to the right one; the engine wraps the result with the accent
//! wash and flash overlay (see [`super::engine::VisualEngine::scene`]).

use super::engine::{ModeCtx, VisualMode};
use super::scene::Shape;

mod bars;
mod flow;
mod grid;
mod neon;
mod particles;
mod pulse;
mod rings;
mod tunnel;

pub(crate) fn build_scene(mode: VisualMode, ctx: &ModeCtx) -> Vec<Shape> {
    match mode {
        VisualMode::Grid => grid::scene(ctx),
        VisualMode::Bars => bars::scene(ctx),
        VisualMode::Rings => rings::scene(ctx),
        VisualMode::Flow => flow::scene(ctx),
        VisualMode::Pulse => pulse::scene(ctx),
        VisualMode::Particles => particles::scene(ctx),
        VisualMode::Neon => neon::scene(ctx),
        VisualMode::Tunnel => tunnel::scene(ctx),
    }
}
