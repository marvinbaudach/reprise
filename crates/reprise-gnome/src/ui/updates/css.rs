//! New Releases popover styling (D1): nocturne-toned surfaces built on the
//! live theme accent (`@accent_bg_color`/`@reprise_accent_text_color`), never a hardcoded
//! blurple — a theme switch must recolor this popover along with everything
//! else. Styles exactly the `new-release-*` classes B1–C actually attach to
//! widgets (see `release_row.rs`, `concerts_section.rs`, `popover.rs`,
//! `badge.rs`, `release_cover.rs`).
//!
//! GTK CSS has no `text-transform`, so `.new-release-header` leans on
//! letter-spacing and the shared secondary text level instead of forcing an uppercase
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
       and the shared secondary text level instead. */\
    .new-release-header {\
        font-size: 11px;\
        letter-spacing: 0.08em;\
        color: @reprise_secondary_fg_color;\
    }\
    /* Full-batch count pill: the filled accent outranks outlined row-status \
       chips without inventing a second colour. */\
    .new-release-tag {\
        background-color: @accent_bg_color;\
        color: @accent_fg_color;\
        border-radius: 8px;\
        padding: 1px 6px;\
        font-size: 10.5px;\
    }\
    .new-release-row {\
        border-radius: 8px;\
        border-left: 2px solid transparent;\
    }\
    .new-release-row:hover,\
    .new-release-row:focus-within {\
        border-left-color: @accent_bg_color;\
        background-color: alpha(currentColor, 0.06);\
    }\
    .new-release-activation {\
        padding: 9px 4px;\
        border-radius: 8px;\
        font-weight: normal;\
    }\
    /* Release title: the row's one point of emphasis, medium weight rather \
       than bold so it stays quiet next to the chip/actions stack. */\
    .new-release-title {\
        font-size: 15px;\
        font-weight: 500;\
    }\
    .new-release-title-suffix {\
        color: @reprise_secondary_fg_color;\
    }\
    .updates-tag {\
        border-radius: 999px;\
        padding: 2px 8px;\
        font-size: 11px;\
    }\
    .updates-tag.updates-tag-accent {\
        border: 1px solid alpha(@accent_bg_color, 0.45);\
        color: @reprise_accent_text_color;\
        background-color: alpha(@accent_bg_color, 0.08);\
    }\
    .updates-tag.updates-tag-neutral {\
        border: 1px solid alpha(@window_fg_color, 0.20);\
        color: @reprise_secondary_fg_color;\
        background-color: transparent;\
    }\
    .new-release-meta {\
        font-size: 13px;\
        color: @reprise_secondary_fg_color;\
        opacity: 0.78;\
    }\
    /* Thin divider between the list and the history entry point, fading at \
       both ends instead of running edge to edge. */\
    .new-release-separator {\
        min-height: 1px;\
        background-image: linear-gradient(to right, transparent, \
                           @borders 48px, @borders calc(100% - 48px), transparent);\
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
    .new-release-row-actions {\
        opacity: 0.55;\
    }\
    .new-release-row:hover .new-release-row-actions,\
    .new-release-row:focus-within .new-release-row-actions {\
        opacity: 1;\
    }\
    .updates-section-header {\
        padding: 2px 4px;\
        font-weight: normal;\
    }\
    .new-release-cover {\
        border-radius: 4px;\
        min-width: 44px;\
        min-height: 44px;\
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
    fn rules_for<'a>(css: &'a str, selector: &str) -> &'a str {
        css.split(&format!("{selector} {{"))
            .nth(1)
            .and_then(|rules| rules.split('}').next())
            .unwrap_or_else(|| panic!("missing rules for {selector}"))
    }

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
            ".new-release-title-suffix",
            ".updates-tag",
            ".updates-tag.updates-tag-accent",
            ".updates-tag.updates-tag-neutral",
            ".new-release-meta",
            ".new-release-separator",
            ".new-release-activation",
            ".new-release-action",
            ".new-release-row-actions",
            ".updates-section-header",
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
        assert!(css.contains("@reprise_accent_text_color"));
        assert!(css.contains("@accent_fg_color"));
        // Beschluss 7: no hardcoded blurple.
        assert!(!css.contains("#5e5cff"));
        assert!(!css.contains("rgb(94, 92, 255)"));
    }

    #[test]
    fn count_chip_uses_the_theme_accent_fill() {
        let css = super::css();
        let tag = css
            .split(".new-release-tag {")
            .nth(1)
            .and_then(|rules| rules.split('}').next())
            .expect("count-chip rules");

        assert!(tag.contains("background-color: @accent_bg_color"));
        assert!(tag.contains("color: @accent_fg_color"));
    }

    #[test]
    fn contrast_1_text_classes_consume_roles_without_local_dimming() {
        let css = super::css();
        for (selector, role) in [
            (".new-release-header", "@reprise_secondary_fg_color"),
            (".new-release-title-suffix", "@reprise_secondary_fg_color"),
            (
                ".updates-tag.updates-tag-neutral",
                "@reprise_secondary_fg_color",
            ),
        ] {
            let rules = rules_for(&css, selector);
            assert!(
                rules.contains(&format!("color: {role}")),
                "{selector} did not consume {role}"
            );
            assert!(!rules.contains("opacity:"), "{selector} locally dims text");
            assert!(
                !rules.contains("color: alpha(@window_fg_color"),
                "{selector} locally mixes its foreground"
            );
        }
    }

    #[test]
    fn nr_29_badge_overlaps_the_compact_updates_trigger() {
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
    fn rows_reserve_the_accent_border_and_share_hover_and_focus_treatment() {
        let css = super::css();
        assert!(!css.contains(":active"));
        let row = rules_for(&css, ".new-release-row");
        assert!(row.contains("border-left: 2px solid transparent"));
        assert!(css.contains(".new-release-row:focus-within"));
        assert!(css.contains("border-left-color: @accent_bg_color"));
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
