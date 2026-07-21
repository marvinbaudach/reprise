//! Grid: placeholder — Task 11 replaces this with the water-surface mesh.
//! Until then it renders the same scene as Bars, so every mode is already a
//! valid, drawable member of `VisualMode::ALL`.

use super::super::engine::ModeCtx;
use super::super::scene::Shape;

pub(crate) fn scene(ctx: &ModeCtx) -> Vec<Shape> {
    super::bars::scene(ctx)
}
