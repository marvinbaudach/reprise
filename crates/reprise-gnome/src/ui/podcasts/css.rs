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
  border-left: 3px solid transparent;
}
.reprise-podcast-episode-row:focus-visible {
  outline: 2px solid alpha(@accent_color, 0.65);
  outline-offset: -2px;
}
.reprise-podcast-episode-thumbnail {
  border-radius: 6px;
  background: alpha(currentColor, 0.08);
}
.reprise-podcast-episode-play-glyph,
.reprise-podcast-episode-marker {
  border-radius: 999px;
  background: alpha(black, 0.58);
}
.reprise-podcast-episode-play-glyph {
  min-width: 24px;
  min-height: 24px;
  color: white;
}
/* The marker sits on top of artwork it does not control — a channel
   thumbnail, or the fallback glyph tile. Without this backdrop its bars
   read as part of whatever is underneath: on the fallback video symbol they
   look like a fill pattern inside it. The chip hugs the bars instead of
   taking the glyph's fixed 24px box, which would pad it out on one side and
   leave the marker sitting off-centre on the thumbnail. Padding stays
   symmetric: the bars are 13px inside eq_bars' own 16px box and therefore
   already sit ~3px left of its centre, so uneven padding here compounds the
   offset instead of correcting it — and that box is shared with the track
   list and the sidebar, which is not worth reshaping over a few pixels. */
.reprise-podcast-episode-marker { padding: 4px; }
/* Dim the artwork the marker sits on, the same way hovering dims it behind
   the play glyph. The marker is a sibling overlay, so it keeps full
   strength and stops competing with the fallback video symbol. */
.reprise-podcast-playing .reprise-podcast-episode-thumbnail { opacity: 0.4; }
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
.reprise-podcast-episode-row.reprise-podcast-playing {
  border-left-color: @reprise_player_accent;
  background: alpha(@accent_bg_color, 0.16);
}
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playing_episode_tint_uses_existing_accent_roles_and_a_visible_edge() {
        let css = css();
        assert!(css.contains(".reprise-podcast-episode-row.reprise-podcast-playing"));
        assert!(css.contains("border-left-color: @reprise_player_accent"));
        assert!(css.contains("background: alpha(@accent_bg_color, 0.16)"));
    }
}
