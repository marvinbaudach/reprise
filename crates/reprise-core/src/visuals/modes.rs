//! Grid scene builder. It translates the shared reactive state in
//! [`GridCtx`] into resolution-independent shapes. The engine wraps the
//! result with the accent wash and flash overlay (see
//! [`super::engine::VisualEngine::scene`]).

use super::engine::GridCtx;
use super::scene::Shape;

mod grid;

pub(crate) fn build_scene(ctx: &GridCtx) -> Vec<Shape> {
    grid::scene(ctx)
}
