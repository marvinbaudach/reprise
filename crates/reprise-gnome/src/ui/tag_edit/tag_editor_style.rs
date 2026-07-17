//! Tag-editor redesign chrome: surface treatment, multi-edit badges,
//! cover art, mixed-field states, pending-change bar, rating stars,
//! navigation, and MusicBrainz button. Installed app-wide by
//! [`super::style`]; palette colors come from the theme provider.

pub(in crate::ui) fn css() -> String {
    use crate::ui::style::tokens::{
        DIALOG_CARD_ALPHA, FOCUS_GLOW_ALPHA, FOCUS_GLOW_BLUR, RADIUS_SURFACE, SURFACE_BORDER_ALPHA,
        SURFACE_SHADOW, TRANSITION,
    };
    format!(
        /* --- Dialog shell --- */
        ".reprise-tag-editor {{ \
           border-radius: {RADIUS_SURFACE}; \
           border: 1px solid alpha(@window_fg_color, {SURFACE_BORDER_ALPHA}); \
           box-shadow: {SURFACE_SHADOW}; }}\n\
         \
         /* --- Multi-edit badge (\"same on all\", \"per track\") --- */
         .reprise-tag-badge {{ \
           font-size: 11px; \
           padding: 1px 8px; \
           border-radius: 9999px; \
           background: alpha(@accent_bg_color, 0.15); \
           color: @reprise_dim_fg_color; \
           transition: background {TRANSITION}; }}\n\
         \
         /* --- Applied-to-all hint --- */
         .reprise-tag-hint {{ \
           color: @accent_color; \
           font-size: 12px; }}\n\
         \
         /* --- Autocomplete popover --- */
         .reprise-autocomplete-popover {{ \
           background: @window_bg_color; \
           border: 1px solid alpha(@window_fg_color, {SURFACE_BORDER_ALPHA}); \
           border-radius: 8px; \
           box-shadow: {SURFACE_SHADOW}; \
           padding: 4px 0; }}\n\
         .reprise-autocomplete-list row {{ \
           padding: 6px 12px; }}\n\
         .reprise-autocomplete-list row:selected {{ \
           background: alpha(@accent_bg_color, 0.15); }}\n\
         \
         /* --- Cover art --- */
         .reprise-tag-cover {{ \
           border-radius: 8px; \
           background: alpha(@window_fg_color, 0.06); }}\n\
         .reprise-tag-cover picture {{ \
           border-radius: 8px; }}\n\
         .reprise-tag-cover-stack {{ \
           padding: 8px 8px 0 0; }}\n\
         .reprise-tag-cover-badge {{ \
           font-size: 10px; \
           font-weight: 600; \
           padding: 2px 8px; \
           border-radius: 9999px; \
           background: alpha(@window_bg_color, 0.85); \
           color: @window_fg_color; }}\n\
         \
         /* --- Field annotation labels --- */
         .reprise-tag-field-annotation {{ \
           font-size: 11px; \
           color: @reprise_dim_fg_color; \
           padding: 0 4px; }}\n\
         .reprise-tag-field-annotation.accent {{ \
           color: @accent_color; }}\n\
         \
         /* --- 3a layout: cover-left header row + entry grid (TAG-2/TAG-3) --- */
         .reprise-tag-field {{ \
           margin-bottom: 2px; }}\n\
         .reprise-tag-field-label {{ \
           font-size: 12px; \
           font-weight: 600; \
           color: @reprise_dim_fg_color; \
           padding: 0 2px; }}\n\
         /* Reserved \"was: …\" line (TAG-5, P-4): space is always allocated, \
            even while its text is empty, so the first real edit never \
            reflows the grid around it. */
         .reprise-tag-old-value {{ \
           font-size: 11px; \
           color: alpha(@window_fg_color, 0.40); \
           min-height: 14px; }}\n\
         \
         /* --- Mixed-field state (TAG-2): editable from the start, dashed/ \
            italic until the user's first keystroke or Backspace/Delete \
            arms it --- */
         .reprise-tag-mixed > .header {{ \
           border-style: dashed; }}\n\
         .reprise-tag-mixed > .header text > placeholder {{ \
           font-style: italic; \
           color: alpha(@window_fg_color, 0.45); }}\n\
         .reprise-tag-field-armed > .header {{ \
           border-color: @accent_color; \
           border-width: 1.5px; }}\n\
         \
         /* --- Hide AdwEntryRow's built-in edit pencil (Beschluss #1: the \
            3a design wants plain entry fields, no pencil chrome). The fields \
            stay AdwEntryRow-based because dirty/save are pinned to that type, \
            so the icon is suppressed via CSS. Matches libadwaita's own node \
            path (row.entry > box.header > .editable-area > .edit-icon) with \
            the dialog-class prefix, so it outweighs the theme rule that gives \
            the icon its 24px min-size. --- */
         .reprise-tag-editor row.entry box.header .editable-area .edit-icon {{ \
           opacity: 0; \
           min-width: 0; \
           min-height: 0; \
           margin: 0; \
           padding: 0; }}\n\
         \
         /* --- Per-track field lock (TAG-3): Title/Track-number read-only \
            in Multi mode --- */
         .reprise-tag-per-track {{ \
           opacity: 0.6; }}\n\
         .reprise-tag-per-track > .header text {{ \
           font-style: normal; }}\n\
         \
         /* --- Pending-change bar (legacy; superseded by the review footer \
            below but kept — some of its rows may return once Package E's \
            Wave-4 keyboard work revisits index-based field identity) --- */
         .reprise-tag-pending {{ \
           background: alpha(@accent_bg_color, 0.08); \
           border-radius: 8px; \
           padding: 8px 12px; \
           margin-top: 4px; }}\n\
         .reprise-tag-pending-header {{ \
           font-size: 12px; \
           font-weight: 600; \
           color: @accent_color; \
           margin-bottom: 4px; }}\n\
         .reprise-tag-pending-item {{ \
           padding: 4px 0; }}\n\
         .reprise-tag-pending-item label {{ \
           font-size: 12px; }}\n\
         .reprise-tag-pending-item button {{ \
           font-size: 11px; \
           padding: 1px 8px; \
           min-height: 20px; }}\n\
         \
         /* --- Review footer (TAG-5, F1): summary line + \"Review changes\" \
            expander, mounted in the same slot the pending bar used to \
            occupy --- */
         .reprise-tag-review {{ \
           background: alpha(@accent_bg_color, 0.08); \
           border-radius: 8px; \
           padding: 8px 12px; \
           margin-top: 4px; }}\n\
         .reprise-tag-review-summary {{ \
           font-size: 12px; \
           font-weight: 600; \
           color: @accent_color; }}\n\
         .reprise-tag-review expander {{ \
           font-size: 12px; \
           margin-top: 4px; }}\n\
         \
         /* --- In-field ↺ revert (TAG-2): hidden until a field arms --- */
         .reprise-tag-field-revert {{ \
           min-width: 24px; \
           min-height: 24px; \
           padding: 2px; }}\n\
         \
         /* --- Rating stars --- */
         .reprise-tag-stars button {{ \
           min-width: 28px; \
           min-height: 28px; \
           padding: 2px; \
           border-radius: 50%; \
           transition: background {TRANSITION}; }}\n\
         .reprise-tag-stars button:hover {{ \
           background: alpha(@accent_bg_color, 0.15); }}\n\
         .reprise-tag-stars .star-filled {{ \
           color: @accent_color; }}\n\
         .reprise-tag-stars .star-outline {{ \
           color: alpha(@window_fg_color, 0.35); }}\n\
         \
         /* --- Navigation (prev / next) --- */
         .reprise-tag-nav {{ \
           margin-top: 4px; }}\n\
         .reprise-tag-nav button {{ \
           min-width: 32px; \
           min-height: 32px; \
           border-radius: 50%; \
           transition: background {TRANSITION}; }}\n\
         .reprise-tag-nav button:hover {{ \
           background: alpha(@accent_bg_color, 0.12); }}\n\
         \
         /* --- MusicBrainz button --- */
         .reprise-tag-mb {{ \
           margin-top: 4px; }}\n\
         .reprise-tag-mb button {{ \
           padding: 8px 16px; \
           border-radius: 8px; \
           border: 1px solid alpha(@window_fg_color, 0.12); \
           transition: background {TRANSITION}; }}\n\
         .reprise-tag-mb button:hover {{ \
           background: alpha(@accent_bg_color, 0.10); }}\n\
         .reprise-tag-mb-hint {{ \
           font-size: 11px; \
           color: @reprise_dim_fg_color; \
           margin-top: 2px; }}\n\
         \
         /* --- Error label --- */
         .reprise-tag-error {{ \
           color: @error_color; \
           font-size: 12px; }}\n\
         \
         /* --- Focus glow on entry rows inside the editor --- */
         .reprise-tag-editor row:focus-within {{ \
           box-shadow: 0 0 {FOCUS_GLOW_BLUR} alpha(@accent_color, {FOCUS_GLOW_ALPHA}); \
           border-radius: 8px; }}\n\
         \
         /* --- Field columns get a subtle card tint, replacing the old \
            boxed-list/PreferencesGroup look the 3a layout retired --- */
         .reprise-tag-editor .reprise-tag-field .header {{ \
           background: alpha(@window_fg_color, {DIALOG_CARD_ALPHA}); \
           border-radius: 8px; }}"
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn css_defines_tag_editor_styles() {
        let css = super::css();
        assert!(css.contains(".reprise-tag-editor"));
        assert!(css.contains(".reprise-tag-badge"));
        assert!(css.contains(".reprise-tag-hint"));
        assert!(css.contains(".reprise-tag-cover"));
        assert!(css.contains(".reprise-tag-mixed"));
        assert!(css.contains(".edit-icon"));
        assert!(css.contains("text > placeholder"));
        assert!(css.contains("alpha(@window_fg_color, 0.45)"));
        assert!(css.contains(".reprise-tag-pending"));
        assert!(css.contains(".reprise-tag-review"));
        assert!(css.contains(".reprise-tag-field-revert"));
        assert!(css.contains(".reprise-tag-stars"));
        assert!(css.contains(".reprise-tag-nav"));
        assert!(css.contains(".reprise-tag-mb"));
        assert!(css.contains("@accent_color"));
        assert!(css.contains("@reprise_dim_fg_color"));
    }
}
