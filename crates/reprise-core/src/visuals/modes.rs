//! Scene dispatch for the deliberately small Grid/Bars mode set.

use super::engine::{ModeCtx, VisualMode};
use super::scene::Shape;

mod bars;
mod grid;

pub(crate) fn build_scene(mode: VisualMode, ctx: &ModeCtx) -> Vec<Shape> {
    match mode {
        VisualMode::Grid => grid::scene(ctx),
        VisualMode::Bars => bars::scene(ctx),
    }
}
