//! Bars scene construction.

use super::engine::ModeCtx;
use super::scene::Shape;

mod bars;

pub(crate) fn build_scene(ctx: &ModeCtx) -> Vec<Shape> {
    bars::scene(ctx)
}
