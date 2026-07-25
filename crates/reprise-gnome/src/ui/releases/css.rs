//! Structural styling owned by the Releases full view.

pub(in crate::ui) fn css() -> String {
    r#"
.reprise-releases-view { min-height: 1px; }
.reprise-releases-table { border-spacing: 0; }
.reprise-release-pill {
  border-radius: 999px;
  padding: 2px 8px;
  font-size: 0.85em;
}
.reprise-release-pill-owned {
  color: @accent_fg_color;
  background: @accent_bg_color;
}
.reprise-release-pill-upcoming {
  color: @window_fg_color;
  background: alpha(@window_fg_color, 0.10);
}
.reprise-release-pill-released {
  color: alpha(@window_fg_color, 0.72);
  background: alpha(@window_fg_color, 0.06);
}
"#
    .into()
}
