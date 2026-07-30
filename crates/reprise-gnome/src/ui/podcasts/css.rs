//! Podcasts source structural styles.

pub(super) fn css() -> String {
    r#"
.reprise-podcasts-source { min-width: 0; }
.reprise-podcasts-table { border-top: 1px solid alpha(currentColor, 0.10); }
.reprise-podcast-group {
  border-radius: 12px;
  background: alpha(currentColor, 0.045);
  border: 1px solid alpha(currentColor, 0.08);
}
.reprise-podcast-group-artwork {
  min-width: 40px;
  min-height: 40px;
  border-radius: 8px;
  background: alpha(currentColor, 0.08);
}
.reprise-podcast-episodes {
  border-top: 1px solid alpha(currentColor, 0.08);
}
.reprise-podcast-episode-row {
  border-bottom: 1px solid alpha(currentColor, 0.06);
}
.reprise-podcast-episode-row:focus-visible {
  outline: 2px solid alpha(@accent_color, 0.65);
  outline-offset: -2px;
}
.reprise-podcast-episode-thumbnail {
  border-radius: 6px;
  background: alpha(currentColor, 0.08);
}
.reprise-podcast-episode-play-glyph {
  min-width: 24px;
  min-height: 24px;
  border-radius: 999px;
  color: white;
  background: alpha(black, 0.58);
}
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
.reprise-source-image {
  border-radius: 8px;
  background: alpha(currentColor, 0.08);
}
.reprise-podcast-result-section {
  border-radius: 10px;
  padding: 10px;
  background: alpha(currentColor, 0.04);
}
"#
    .to_owned()
}
