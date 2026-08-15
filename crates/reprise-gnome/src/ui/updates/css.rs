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
    .updates-tag.updates-tag-neutral-filled {\
        border: 1px solid alpha(@window_fg_color, 0.20);\
        color: @reprise_secondary_fg_color;\
        background-color: alpha(@window_fg_color, 0.08);\
    }\
    .updates-tag.updates-tag-quiet {\
        border: 1px solid alpha(@window_fg_color, 0.12);\
        color: @reprise_hint_fg_color;\
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
    use std::collections::BTreeSet;

    use gtk4::prelude::*;

    fn rules_for<'a>(css: &'a str, selector: &str) -> &'a str {
        css.split(&format!("{selector} {{"))
            .nth(1)
            .and_then(|rules| rules.split('}').next())
            .unwrap_or_else(|| panic!("missing rules for {selector}"))
    }

    fn declarations_for(css: &str, selector: &str) -> BTreeSet<String> {
        rules_for(css, selector)
            .split(';')
            .map(str::trim)
            .filter(|declaration| !declaration.is_empty())
            .map(|declaration| declaration.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect()
    }

    #[test]
    fn the_popover_ticket_tones_declare_what_the_concerts_table_declares() {
        let popover_css = super::css();
        let table_css = crate::ui::concerts::css::css();
        for (popover_selector, table_selector) in [
            (".updates-tag", ".reprise-concert-ticket-tag"),
            (
                ".updates-tag.updates-tag-accent",
                ".reprise-concert-ticket-tag.on-sale",
            ),
            (
                ".updates-tag.updates-tag-neutral-filled",
                ".reprise-concert-ticket-tag.off-sale",
            ),
            (
                ".updates-tag.updates-tag-quiet",
                ".reprise-concert-ticket-tag.unknown",
            ),
        ] {
            let popover = declarations_for(&popover_css, popover_selector);
            let table = declarations_for(&table_css, table_selector);
            let popover_only = popover.difference(&table).cloned().collect::<Vec<_>>();
            let table_only = table.difference(&popover).cloned().collect::<Vec<_>>();

            assert_eq!(
                popover, table,
                "declarations differ for {popover_selector} and {table_selector}; \
                 popover only: {popover_only:?}; table only: {table_only:?}"
            );
        }
    }

    struct RenderedPill {
        width: i32,
        height: i32,
        stride: usize,
        pixels: Vec<u8>,
    }

    impl RenderedPill {
        fn rgba_at(&self, (x, y): (i32, i32)) -> [u8; 4] {
            assert!((0..self.width).contains(&x), "sample x {x} outside pill");
            assert!((0..self.height).contains(&y), "sample y {y} outside pill");
            let offset = y as usize * self.stride + x as usize * 4;
            self.pixels[offset..offset + 4]
                .try_into()
                .expect("one RGBA pixel")
        }
    }

    fn pill_label(text: &str, classes: &[&str]) -> gtk4::Label {
        let label = gtk4::Label::new(Some(text));
        for class in classes {
            label.add_css_class(class);
        }
        label
    }

    fn render_pill(window: &gtk4::Window, label: &gtk4::Label) -> RenderedPill {
        let width = label.width();
        let height = label.height();
        let paintable = gtk4::WidgetPaintable::new(Some(label));
        let snapshot = gtk4::Snapshot::new();
        paintable.snapshot(&snapshot, f64::from(width), f64::from(height));
        let node = snapshot
            .to_node()
            .expect("the allocated pill paints a node");
        let renderer = window
            .native()
            .and_then(|native| native.renderer())
            .expect("the presented window has a renderer");
        let texture = renderer.render_texture(&node, None);
        assert!(
            !texture.save_to_png_bytes().is_empty(),
            "the rendered pill must encode as PNG"
        );
        let stride = texture.width() as usize * 4;
        let mut pixels = vec![0; stride * texture.height() as usize];
        texture.download(&mut pixels, stride);
        RenderedPill {
            width,
            height,
            stride,
            pixels,
        }
    }

    fn assert_samples_match(
        pair: &str,
        popover: &RenderedPill,
        table: &RenderedPill,
        sample_name: &str,
        point: (i32, i32),
    ) {
        let popover_rgba = popover.rgba_at(point);
        let table_rgba = table.rgba_at(point);
        for channel in 0..4 {
            assert!(
                popover_rgba[channel].abs_diff(table_rgba[channel]) <= 1,
                "{pair} {sample_name} differs at {point:?}, channel {channel}: \
                 popover {popover_rgba:?}, table {table_rgba:?}"
            );
        }
    }

    fn assert_rendered_pair_matches(pair: &str, popover: &RenderedPill, table: &RenderedPill) {
        assert_eq!(
            (popover.width, popover.height),
            (table.width, table.height),
            "{pair} pill geometry differs between popover and table"
        );
        let width = popover.width;
        let height = popover.height;
        let samples = [
            ("left border", (0, height / 2)),
            ("middle fill", (width / 2, 2.min(height - 1))),
            ("text body", (width * 2 / 5, height / 2)),
        ];
        for (sample_name, point) in samples {
            assert_samples_match(pair, popover, table, sample_name, point);
        }
        let border = popover.rgba_at(samples[0].1);
        let fill = popover.rgba_at(samples[1].1);
        let text = popover.rgba_at(samples[2].1);
        assert_ne!(border, fill, "{pair} border sample landed in the fill");
        assert_ne!(text, fill, "{pair} text sample landed in the fill");
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn the_popover_ticket_pills_render_exactly_as_the_table_pills() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        crate::ui::style::install_css_string_for_test(&crate::ui::style::app_css_for_test());

        let off_sale_text = crate::ui::strings::text(crate::ui::strings::CONCERTS_OFF_SALE);
        let unknown_text = crate::ui::strings::text(crate::ui::strings::CONCERTS_UNKNOWN);
        let popover_off_sale = pill_label(
            &off_sale_text,
            &["updates-tag", "updates-tag-neutral-filled"],
        );
        let table_off_sale =
            pill_label(&off_sale_text, &["reprise-concert-ticket-tag", "off-sale"]);
        let popover_unknown = pill_label(&unknown_text, &["updates-tag", "updates-tag-quiet"]);
        let table_unknown = pill_label(&unknown_text, &["reprise-concert-ticket-tag", "unknown"]);

        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        row.append(&popover_off_sale);
        row.append(&table_off_sale);
        row.append(&popover_unknown);
        row.append(&table_unknown);
        let window = gtk4::Window::new();
        window.set_default_size(480, 80);
        window.set_child(Some(&row));
        window.present();
        crate::ui::test_settle::settle_for(std::time::Duration::from_millis(100));

        let popover_off_sale = render_pill(&window, &popover_off_sale);
        let table_off_sale = render_pill(&window, &table_off_sale);
        let popover_unknown = render_pill(&window, &popover_unknown);
        let table_unknown = render_pill(&window, &table_unknown);
        assert_rendered_pair_matches("Off sale", &popover_off_sale, &table_off_sale);
        assert_rendered_pair_matches("Unknown", &popover_unknown, &table_unknown);

        window.close();
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
            ".updates-tag.updates-tag-neutral-filled",
            ".updates-tag.updates-tag-quiet",
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
            (".updates-tag.updates-tag-quiet", "@reprise_hint_fg_color"),
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
        // The neutral-filled tone is excluded because its table-matching
        // background contains the foreground substring this guard rejects.
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
