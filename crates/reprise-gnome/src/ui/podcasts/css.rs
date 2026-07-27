//! Podcasts source structural styles.

pub(super) fn css() -> String {
    r#"
.reprise-podcasts-source { min-width: 0; }
.reprise-podcasts-table { border-top: 1px solid alpha(currentColor, 0.10); }
.reprise-podcast-source,
.reprise-podcast-status-new,
.reprise-podcast-status-resume,
.reprise-podcast-status-played {
  border-radius: 999px;
  padding: 2px 8px;
  min-height: 20px;
}
.reprise-podcast-source,
.reprise-podcast-status-resume { border: 1px solid alpha(currentColor, 0.22); }
.reprise-podcast-status-new {
  background: alpha(@accent_bg_color, 0.16);
  color: @accent_color;
}
.reprise-podcast-status-played { opacity: 0.55; }
.reprise-podcast-playing { background: alpha(@accent_bg_color, 0.07); }
.reprise-podcast-result { padding: 6px; }
.reprise-podcast-glyph-tile {
  min-width: 40px;
  min-height: 40px;
  border-radius: 8px;
  background: alpha(currentColor, 0.08);
}
"#
    .to_owned()
}
