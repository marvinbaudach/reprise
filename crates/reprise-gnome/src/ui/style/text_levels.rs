//! Shared foreground levels for app-authored text hierarchy.

use super::tokens::SECONDARY_TEXT_ALPHA;

pub(super) fn css() -> String {
    format!(
        ".reprise-text-primary {{ color: @reprise_primary_fg_color; }}\n\
         .reprise-text-secondary {{ color: @reprise_secondary_fg_color; }}\n\
         .reprise-text-hint {{ color: @reprise_hint_fg_color; }}\n\
         .reprise-rhythmbox-import-option .subtitle {{\n\
             color: @reprise_secondary_fg_color;\n\
         }}\n\
         /* Adwaita dims `.dim-label` to 0.55, calibrated for its own surface \
            ladder. On ours it lands below 4.5:1 — 4.27 on a dark popover, \
            3.84 on a light one, and 3.57 on a hovered menu row, which is the \
            worst case because hover lightens the surface under the text. \
            Raising the opacity rather than setting a colour keeps the \
            inheritance: a `.dim-label` inside an accent-coloured button stays \
            accent-coloured, only less muted. There are 86 call sites; fixing \
            them one by one would leave the next one to reintroduce it. */\n\
         .dim-label {{ opacity: {SECONDARY_TEXT_ALPHA}; }}\n\
         /* Menu accelerators are dimmed by Adwaita itself, on their own node \
            rather than via `.dim-label`, so the rule above does not reach \
            them. Measured in a real popover menu they sat at 4.27:1, and \
            3.57:1 on a hovered row — the worst text in the whole menu. \
            This one sets the colour, not the opacity: Adwaita dims the node \
            via `color: alpha(currentColor, …)`, so an added opacity would \
            multiply with it. It did, and measured 2.90:1 — worse than the \
            problem. Same target level, opposite mechanism. */\n\
         modelbutton > accelerator,\n\
         accelerator {{ color: @reprise_secondary_fg_color; opacity: 1; }}"
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn rhythmbox_option_subtitles_use_the_verified_secondary_level() {
        let css = super::css();

        assert!(css.contains(".reprise-rhythmbox-import-option .subtitle"));
        assert!(css.contains("color: @reprise_secondary_fg_color"));
    }

    #[test]
    fn contrast_3_the_global_dim_label_level_clears_aa_on_every_surface() {
        use super::super::color_math::{composite, contrast_ratio, parse_hex_rgb};
        use super::super::theme::Theme;

        // `.dim-label` is an opacity over whatever colour it inherits, so the
        // rendered pixel is the foreground composited onto the surface at that
        // alpha. Measure that, rather than asserting the declaration exists.
        let level: f64 = super::SECONDARY_TEXT_ALPHA;
        assert!(
            super::css().contains(&format!(".dim-label {{ opacity: {level}; }}")),
            "the global dim-label override must use the shared secondary level"
        );

        for theme in Theme::all() {
            for (appearance, palette) in
                [("dark", theme.palette()), ("light", theme.light_palette())]
            {
                let foreground = parse_hex_rgb(palette.fg).expect("palette fg is valid hex");
                for surface in palette.surfaces() {
                    let background = parse_hex_rgb(surface).expect("palette surface is valid hex");
                    let rendered = composite(foreground, background, level);
                    let ratio = contrast_ratio(rendered, background);
                    assert!(
                        ratio >= 4.5,
                        "{theme:?} {appearance}: dim-label reaches only {ratio:.2}:1 on {surface}"
                    );
                }
            }
        }
    }

    #[test]
    fn contrast_3_menu_accelerators_are_recoloured_not_re_dimmed() {
        // Adwaita dims the accelerator node through its colour. Adding an
        // opacity on top multiplies with that instead of replacing it — tried,
        // measured 2.90:1, worse than the 4.27:1 it was meant to fix. Setting
        // the colour and pinning opacity to 1 is what actually lands at 5.88:1.
        let css = super::css();
        let accelerator = css
            .split("accelerator {")
            .nth(1)
            .and_then(|rules| rules.split('}').next())
            .expect("the accelerator override exists");

        assert!(
            accelerator.contains("color: @reprise_secondary_fg_color"),
            "accelerators must take the shared secondary colour"
        );
        assert!(
            accelerator.contains("opacity: 1"),
            "accelerators must not stack an opacity on Adwaita's own dimming"
        );
    }
}
