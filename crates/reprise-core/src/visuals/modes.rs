//! Bars scene construction.

use super::engine::ModeCtx;
use super::scene::Shape;

mod bars;

pub(crate) use bars::{BarsEnvelope, BAR_COUNT};

pub(crate) fn build_scene(ctx: &ModeCtx) -> Vec<Shape> {
    bars::scene(ctx)
}
