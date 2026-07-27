//! New Releases popover styling (D1): nocturne-toned surfaces built on the
//! live theme accent (`@accent_bg_color`/`@accent_color`), never a hardcoded
//! blurple — a theme switch must recolor this popover along with everything
//! else. Styles exactly the `new-release-*` classes B1–C actually attach to
//! widgets (see `release_row.rs`, `concerts_section.rs`, `popover.rs`,
//! `badge.rs`, `release_cover.rs`).
//!
//! GTK CSS has no `text-transform`, so `.new-release-header` leans on
//! letter-spacing and a dimmed opacity instead of forcing an uppercase
//! rendering the parser would silently drop.
//!
//! Hover tints here are new, bespoke surfaces (not the shared `ICON_CLASS`/
//! `TOGGLE_CLASS` vocabulary in `style::buttons`), so adding them locally does
//! not violate BTN-4 — there is no shared definition being duplicated. No
//! local `:active`/`:focus` states are declared; those stay owned by
//! `style::buttons` once a widget opts into that vocabulary.

pub(in crate::ui) fn css() -> String {
    "\
    /* Give the overlay a stable button-sized anchor. The badge itself does \
       not contribute to natural size, so this keeps it attached to the \
       sparkle instead of allocating as a separate-looking pill. */\
    .updates-trigger {\
        min-width: 32px;\
        min-height: 32px;\
    }\
    /* Header-bar badge (NR-9): a small accent pill with a ring that keeps it \
       readable when it overlaps the ✦ glyph. */\
    .new-release-badge {\
        background-color: @accent_bg_color;\
        color: @accent_fg_color;\
        border-radius: 999px;\
        min-width: 16px;\
        min-height: 16px;\
        font-size: 10px;\
        padding: 0 3px;\
        box-shadow: 0 0 0 2px @headerbar_bg_color;\
    }\
    .new-release-badge:dir(ltr) {\
        transform: translate(5px, -5px);\
    }\
    .new-release-badge:dir(rtl) {\
        transform: translate(-5px, -5px);\
    }\
    /* Popover chrome: a light hairline edge instead of a heavy shadow. */\
    .new-release-popover > contents {\
        border-radius: 14px;\
        border: 1px solid alpha(@window_fg_color, 0.08);\
    }\
    /* Section headers ('New Releases', history group labels): GTK's CSS \
       engine has no case-transform property, so the uppercase look (if the \
       string itself is not already uppercase) is approximated with tracking \
       and dimming instead. */\
    .new-release-header {\
        font-size: 11px;\
        letter-spacing: 0.08em;\
        opacity: 0.55;\
    }\
    /* 'NEW' / history-count pill: accent-tinted, not a solid fill. */\
    .new-release-tag {\
        background-color: alpha(@accent_bg_color, 0.18);\
        color: @accent_color;\
        border-radius: 8px;\
        padding: 1px 6px;\
        font-size: 10.5px;\
    }\
    .new-release-row {\
        border-radius: 8px;\
        padding: 9px 4px;\
    }\
    .new-release-row:hover {\
        background-color: alpha(currentColor, 0.04);\
    }\
    /* Release title: the row's one point of emphasis, medium weight rather \
       than bold so it stays quiet next to the chip/actions stack. */\
    .new-release-title {\
        font-size: 14px;\
        font-weight: 500;\
    }\
    /* Upcoming release chip: a quiet accent tint, one step calmer than a \
       button — thin dark-accent border, light-accent text, near-invisible \
       fill (#4a). */\
    .new-release-chip {\
        border: 1px solid alpha(@accent_bg_color, 0.45);\
        color: @accent_color;\
        background-color: alpha(@accent_bg_color, 0.08);\
        border-radius: 999px;\
        padding: 2px 8px;\
        font-size: 11px;\
    }\
    /* Released / already-in-library chip: neutral dimmed outline, no fill. */\
    .new-release-chip-neutral {\
        border: 1px solid alpha(@window_fg_color, 0.20);\
        color: alpha(@window_fg_color, 0.55);\
        background-color: transparent;\
        border-radius: 999px;\
        padding: 2px 8px;\
        font-size: 11px;\
    }\
    /* Partial-ownership chip: you hold the lead single, not the album. \
       Sits between the neutral \"released\" chip and the accent \"upcoming\" \
       one — a dimmed accent outline says \"related to you\" without \
       claiming the album is yours. */\
    .new-release-chip-partial {\
        border: 1px solid alpha(@accent_bg_color, 0.30);\
        color: alpha(@accent_color, 0.85);\
        background-color: transparent;\
        border-radius: 999px;\
        padding: 2px 8px;\
        font-size: 11px;\
    }\
    .new-release-meta {\
        font-size: 12px;\
        opacity: 0.55;\
    }\
    /* Thin divider between the list and the history entry point, fading at \
       both ends instead of running edge to edge. */\
    .new-release-separator {\
        min-height: 1px;\
        background-image: linear-gradient(to right, transparent, \
                           @borders 48px, @borders calc(100% - 48px), transparent);\
    }\
    /* Navigation, not a primary action: normal weight, dimmed text, an even \
       quieter count fragment (#7). */\
    .new-release-history-row {\
        border-radius: 8px;\
        font-weight: normal;\
    }\
    .new-release-history-row:hover {\
        background-color: alpha(currentColor, 0.04);\
    }\
    .new-release-history-label {\
        opacity: 0.70;\
    }\
    .new-release-history-count {\
        opacity: 0.50;\
    }\
    /* Row action icon buttons ('Show in library', 'Hide', history restore): \
       flat at rest, a soft tint on hover, with enough hit area for a pointer. */\
    .new-release-action {\
        background-color: transparent;\
        border-radius: 8px;\
        min-width: 28px;\
        min-height: 28px;\
    }\
    .new-release-action:hover {\
        background-color: alpha(currentColor, 0.08);\
    }\
    /* Footer 'Fetch now' ghost button: accent text, no border, tinted hover. */\
    .new-release-ghost {\
        color: @accent_color;\
        background-color: transparent;\
        border: none;\
        border-radius: 8px;\
    }\
    .new-release-ghost:hover {\
        background-color: alpha(@accent_bg_color, 0.12);\
    }\
    .new-release-cover {\
        border-radius: 4px;\
    }\
    /* Shared dimming hook retained for release-history surfaces. */\
    .new-release-hidden {\
        opacity: 0.55;\
    }\
    "
    .to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn css_covers_every_new_release_class() {
        let css = super::css();
        for class in [
            ".new-release-badge",
            ".new-release-popover > contents",
            ".new-release-header",
            ".new-release-tag",
            ".new-release-row",
            ".new-release-title",
            ".new-release-chip",
            ".new-release-chip-neutral",
            ".new-release-chip-partial",
            ".new-release-meta",
            ".new-release-separator",
            ".new-release-history-row",
            ".new-release-history-label",
            ".new-release-history-count",
            ".new-release-action",
            ".new-release-ghost",
            ".new-release-cover",
            ".new-release-hidden",
        ] {
            assert!(css.contains(class), "missing selector: {class}");
        }
    }

    #[test]
    fn css_uses_the_theme_accent_not_a_hardcoded_colour() {
        let css = super::css();
        assert!(css.contains("@accent_bg_color"));
        assert!(css.contains("@accent_color"));
        assert!(css.contains("@accent_fg_color"));
        // Beschluss 7: no hardcoded blurple.
        assert!(!css.contains("#5e5cff"));
        assert!(!css.contains("rgb(94, 92, 255)"));
    }

    #[test]
    fn nr_9a_badge_overlaps_the_compact_updates_trigger() {
        let css = super::css();
        assert!(css.contains(".updates-trigger"));
        assert!(css.contains("min-width: 32px"));
        assert!(css.contains("min-height: 32px"));
        assert!(css.contains(
            ".new-release-badge:dir(ltr) {\
                transform: translate(5px, -5px);"
        ));
        assert!(css.contains(
            ".new-release-badge:dir(rtl) {\
                transform: translate(-5px, -5px);"
        ));
    }

    #[test]
    fn css_does_not_rely_on_text_transform() {
        // GTK CSS does not support text-transform; relying on it would parse
        // but silently do nothing.
        assert!(!super::css().contains("text-transform"));
    }

    #[test]
    fn css_declares_no_local_active_or_focus_states() {
        // BTN-4: interaction states for widgets that opt into the shared
        // button vocabulary stay owned by `style::buttons`. Hover-only tints
        // here are fine (STYLE-1); focus/active are not.
        let css = super::css();
        assert!(!css.contains(":active"));
        assert!(!css.contains(":focus"));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn new_releases_css_parses_without_errors() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let errors = crate::ui::style::css_parse_errors(&super::css());
        assert!(errors.is_empty(), "CSS parse errors: {errors:?}");
    }
}
