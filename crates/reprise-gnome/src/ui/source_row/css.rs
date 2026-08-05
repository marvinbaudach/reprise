//! Shared source-row style.

use super::skeleton::{MEDIA_HEIGHT, MEDIA_WIDTH, ROW_MIN_HEIGHT, SIZE_SLOT_WIDTH};

pub(in crate::ui) fn css() -> String {
    r#"
.reprise-source-row { min-height: __ROW_MIN_HEIGHT__px; }
.reprise-source-row-media { min-width: __MEDIA_WIDTH__px; min-height: __MEDIA_HEIGHT__px; }
.reprise-source-row-size { min-width: __SIZE_SLOT_WIDTH__px; }
.reprise-source-row-chip {
  border-radius: 999px;
  padding: 1px 7px;
  min-height: 18px;
  font-size: 0.8em;
}
"#
    .replace("__ROW_MIN_HEIGHT__", &ROW_MIN_HEIGHT.to_string())
    .replace("__MEDIA_WIDTH__", &MEDIA_WIDTH.to_string())
    .replace("__MEDIA_HEIGHT__", &MEDIA_HEIGHT.to_string())
    .replace("__SIZE_SLOT_WIDTH__", &SIZE_SLOT_WIDTH.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The row height lives in CSS because a `min-height` on a plain box is
    /// the only thing that survives a child taller than the box.
    #[test]
    fn src_16_the_row_height_is_carried_by_the_shared_style() {
        let css = css();
        assert!(css.contains(".reprise-source-row"));
        assert!(css.contains("min-height: 56px"));
    }

    /// `SRC-16`: the style is what the widgets actually get — a `min-width` in
    /// CSS outranks the `set_size_request` the skeleton makes, so a literal
    /// here would silently win over the constant and the constant would look
    /// like it still governed the layout. Both come from one place, and this
    /// test is what keeps it that way.
    #[test]
    fn src_16_the_style_takes_its_measurements_from_the_shared_constants() {
        let css = css();
        assert!(css.contains(&format!("min-width: {MEDIA_WIDTH}px")));
        assert!(css.contains(&format!("min-height: {MEDIA_HEIGHT}px")));
        assert!(css.contains(&format!("min-width: {SIZE_SLOT_WIDTH}px")));
        assert!(
            !css.contains("64px") || MEDIA_WIDTH == 64,
            "a literal outlived the constant it was meant to mirror"
        );
    }
}
