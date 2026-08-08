//! The row grammar every source list shares.

mod css;
mod detail_line;
mod media_column;
mod reveal;
mod skeleton;

pub(in crate::ui) use css::css;
pub(in crate::ui) use detail_line::{chip, detail_line, resume_percent, ChipSpec};
pub(in crate::ui) use media_column::{media, media_size, MediaShape};
pub(in crate::ui) use reveal::Reveal;
#[allow(unused_imports)]
pub(in crate::ui) use skeleton::{
    skeleton, Skeleton, EPISODE_INDENT, MEDIA_HEIGHT, MEDIA_WIDTH, ROW_CSS_CLASS, ROW_MIN_HEIGHT,
    SIZE_SLOT_WIDTH,
};
