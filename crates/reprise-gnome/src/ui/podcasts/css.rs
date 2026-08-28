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
.reprise-podcast-group.reprise-podcast-group-syncing {
  border-color: alpha(@accent_bg_color, 0.22);
  background-image: linear-gradient(to bottom, alpha(@accent_bg_color, 0.06), transparent);
  transition: border-color 250ms ease, background-image 250ms ease;
}
.reprise-podcast-group-syncing > title > arrow { opacity: 0.38; }
.reprise-podcast-sync-row { min-height: 56px; }
.reprise-podcast-sync-cover {
  min-width: 40px;
  min-height: 40px;
  border-radius: 8px;
  background: alpha(currentColor, 0.08);
}
.reprise-podcast-sync-cover-icon { opacity: 0.62; }
.reprise-podcast-sync-shimmer {
  min-width: 7px;
  background: alpha(@accent_bg_color, 0.14);
  animation: reprise-podcast-shimmer 1900ms linear infinite;
}
.reprise-podcast-sync-spin {
  color: @reprise_accent_text_color;
  animation: reprise-podcast-spin 900ms linear infinite;
}
.reprise-podcast-sync-breathe {
  animation: reprise-podcast-breathe 2000ms ease-in-out infinite;
}
.reprise-podcast-sync-dot {
  min-width: 6px;
  min-height: 6px;
  margin: 5px;
  border-radius: 999px;
  background: alpha(currentColor, 0.24);
}
.reprise-podcast-sync-dot-active { background: @accent_bg_color; }
.reprise-podcast-sync-step-done { opacity: 0.62; }
.reprise-podcast-sync-step-active { color: @reprise_accent_text_color; }
.reprise-podcast-sync-step-pending { opacity: 0.48; }
.reprise-podcast-sync-step-failed { color: @error_color; }
@keyframes reprise-podcast-shimmer {
  from { transform: translate(-8px, 0); opacity: 0; }
  18% { opacity: 0.7; }
  82% { opacity: 0.7; }
  to { transform: translate(48px, 0); opacity: 0; }
}
@keyframes reprise-podcast-spin {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}
@keyframes reprise-podcast-breathe {
  from { opacity: 0.42; }
  to { opacity: 0.78; }
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

    #[test]
    fn pod_26_loading_chrome_uses_named_accent_roles_and_the_approved_motion() {
        let css = css();
        assert!(css.contains(".reprise-podcast-group-syncing"));
        assert!(css.contains("border-color: alpha(@accent_bg_color, 0.22)"));
        assert!(
            css.contains("linear-gradient(to bottom, alpha(@accent_bg_color, 0.06), transparent)")
        );
        assert!(css.contains("animation: reprise-podcast-shimmer 1900ms linear infinite"));
        assert!(css.contains("animation: reprise-podcast-spin 900ms linear infinite"));
        assert!(css.contains("animation: reprise-podcast-breathe 2000ms ease-in-out infinite"));
        assert!(css.contains("transition: border-color 250ms"));
        assert!(css.contains(".reprise-podcast-group-syncing > title > arrow { opacity: 0.38; }"));
        assert!(!css.contains("#4ddac4"));
    }
}
