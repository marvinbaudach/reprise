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
mod pulse;

pub(crate) fn build_scene(mode: VisualMode, ctx: &ModeCtx) -> Vec<Shape> {
    match mode {
        VisualMode::Grid => grid::scene(ctx),
        VisualMode::Bars => bars::scene(ctx),
        VisualMode::Flow => flow::scene(ctx),
        VisualMode::Pulse => pulse::scene(ctx),
    }
}
