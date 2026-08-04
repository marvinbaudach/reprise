//! The row grammar every source list shares.

mod css;
mod detail_line;
#[allow(dead_code)]
mod skeleton;

pub(in crate::ui) use css::css;
#[allow(unused_imports)]
pub(in crate::ui) use detail_line::{chip, detail_line, resume_percent, ChipSpec};
#[allow(unused_imports)]
pub(in crate::ui) use skeleton::{
    skeleton, Skeleton, MEDIA_HEIGHT, MEDIA_WIDTH, ROW_CSS_CLASS, ROW_MIN_HEIGHT, SIZE_SLOT_WIDTH,
};
