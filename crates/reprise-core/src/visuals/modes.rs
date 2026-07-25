//! Grid scene builder. It translates the shared reactive state in
//! [`ModeCtx`] into resolution-independent shapes. The engine wraps the
//! result with the accent wash and flash overlay (see
//! [`super::engine::VisualEngine::scene`]).

use super::engine::ModeCtx;
use super::scene::Shape;

mod grid;

pub(crate) fn build_scene(ctx: &ModeCtx) -> Vec<Shape> {
    grid::scene(ctx)
}
