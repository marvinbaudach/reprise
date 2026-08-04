//! Shared source-row style.

pub(in crate::ui) fn css() -> String {
    r#"
.reprise-source-row { min-height: 56px; }
.reprise-source-row-media { min-width: 64px; min-height: 40px; }
.reprise-source-row-size { min-width: 110px; }
.reprise-source-row-chip {
  border-radius: 999px;
  padding: 1px 7px;
  min-height: 18px;
  font-size: 0.8em;
}
"#
    .to_owned()
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
}
