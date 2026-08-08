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
  color: @reprise_accent_text_color;
}
.reprise-podcast-status-played { opacity: 0.55; }
.reprise-podcast-episode-row.reprise-podcast-playing {
  border-left-color: @reprise_player_accent;
  background: alpha(@accent_bg_color, 0.16);
}
/* `SRC-12b`: selection is a neutral row tint. The loaded row keeps its
   playback-accent edge and title, so the two states remain distinct. */
.reprise-podcast-episode-row.reprise-podcast-episode-selected {
  background: alpha(currentColor, 0.12);
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

    /// `SRC-14`: a selected row reads as selected, and does so distinctly from
    /// the loaded row — a shared tint would leave a user unable to tell which
    /// rows an action is about to hit.
    #[test]
    fn src_12b_selection_tint_is_neutral_beside_the_playback_accent() {
        let css = css();
        let selected = css
            .find(".reprise-podcast-episode-row.reprise-podcast-episode-selected")
            .expect("a selected row has its own style");
        let playing = css
            .find(".reprise-podcast-episode-row.reprise-podcast-playing")
            .expect("the loaded row keeps its own style");

        assert!(
            selected > playing,
            "the selected rule must come last, or a selected loaded row would not look selected"
        );
        assert!(css.contains("background: alpha(currentColor, 0.12)"));
    }
}
