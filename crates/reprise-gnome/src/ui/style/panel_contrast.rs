use super::tokens::{PRIMARY_TEXT_ALPHA, SECONDARY_TEXT_ALPHA};

type CssFn = fn() -> String;

struct PanelRole {
    css: CssFn,
    selector: &'static str,
    role: &'static str,
    minimum: f64,
}

const PANEL_ROLES: [PanelRole; 13] = [
    PanelRole {
        css: crate::ui::now_playing::css,
        selector: ".reprise-now-playing-stage",
        role: "@sidebar_fg_color",
        minimum: 4.5,
    },
    PanelRole {
        css: crate::ui::now_playing::css,
        selector: ".reprise-now-playing-title",
        role: "@reprise_primary_fg_color",
        minimum: 4.5,
    },
    PanelRole {
        css: crate::ui::now_playing::css,
        selector: ".reprise-now-playing-subtitle",
        role: "@reprise_secondary_fg_color",
        minimum: 4.5,
    },
    PanelRole {
        css: crate::ui::now_playing::css,
        selector: ".reprise-now-playing-footer",
        role: "@reprise_secondary_fg_color",
        minimum: 4.5,
    },
    PanelRole {
        css: crate::ui::now_playing::css,
        selector: ".reprise-song-visual-analysis-name",
        role: "@reprise_secondary_fg_color",
        minimum: 4.5,
    },
    PanelRole {
        css: crate::ui::now_playing::up_next_panel::css,
        selector: ".reprise-up-next-section",
        role: "@reprise_secondary_fg_color",
        minimum: 4.5,
    },
    PanelRole {
        css: crate::ui::now_playing::up_next_panel::css,
        selector: ".reprise-up-next-remove",
        role: "@reprise_secondary_fg_color",
        minimum: 4.5,
    },
    PanelRole {
        css: crate::ui::now_playing::up_next_panel::css,
        selector: ".reprise-up-next-remove:hover",
        role: "@reprise_primary_fg_color",
        minimum: 4.5,
    },
    PanelRole {
        css: crate::ui::now_playing::up_next_panel::css,
        selector: ".reprise-up-next-title",
        role: "@reprise_primary_fg_color",
        minimum: 4.5,
    },
    PanelRole {
        css: crate::ui::now_playing::up_next_panel::css,
        selector: ".reprise-up-next-artist",
        role: "@reprise_secondary_fg_color",
        minimum: 4.5,
    },
    PanelRole {
        css: crate::ui::now_playing::up_next_panel::css,
        selector: ".reprise-up-next-empty",
        role: "@reprise_secondary_fg_color",
        minimum: 4.5,
    },
    PanelRole {
        css: crate::ui::lyrics_view::css,
        selector: ".lyrics-line {",
        role: "@sidebar_fg_color",
        minimum: 4.5,
    },
    PanelRole {
        css: crate::ui::lyrics_view::css,
        selector: ".lyrics-unsynced {",
        role: "@reprise_secondary_fg_color",
        minimum: 4.5,
    },
];

fn rule_body<'a>(css: &'a str, selector: &str) -> &'a str {
    css.split(selector)
        .nth(1)
        .and_then(|rest| rest.split('}').next())
        .unwrap_or_else(|| panic!("no rules for {selector}"))
}

fn color_declaration<'a>(css: &'a str, selector: &str) -> &'a str {
    rule_body(css, selector)
        .split(';')
        .find_map(|declaration| {
            let (property, value) = declaration.split_once(':')?;
            let property = property.trim().rsplit([' ', '\n', '\t']).next()?;
            (property == "color").then(|| value.trim())
        })
        .unwrap_or_else(|| panic!("no color declaration for {selector}"))
}

fn rendered_foreground(value: &str, foreground: [u8; 3], surface: [u8; 3]) -> [u8; 3] {
    use super::color_math::{composite, parse_hex_rgb};

    match value {
        "@sidebar_fg_color" => foreground,
        "@reprise_primary_fg_color" => composite(foreground, surface, PRIMARY_TEXT_ALPHA),
        "@reprise_secondary_fg_color" => composite(foreground, surface, SECONDARY_TEXT_ALPHA),
        literal => {
            if let Some(rgb) = parse_hex_rgb(literal) {
                return rgb;
            }
            let arguments = literal
                .strip_prefix("alpha(")
                .and_then(|value| value.strip_suffix(')'))
                .unwrap_or_else(|| panic!("unsupported color declaration: {literal}"));
            let (color, alpha) = arguments
                .split_once(',')
                .unwrap_or_else(|| panic!("invalid alpha color declaration: {literal}"));
            let color = parse_hex_rgb(color.trim())
                .unwrap_or_else(|| panic!("unsupported alpha color in declaration: {literal}"));
            let alpha = alpha
                .trim()
                .parse()
                .unwrap_or_else(|_| panic!("invalid alpha in color declaration: {literal}"));
            composite(color, surface, alpha)
        }
    }
}

fn fixed_foregrounds(css: &str) -> Vec<String> {
    css.split(['{', '}', ';'])
        .filter_map(|declaration| {
            let (property, value) = declaration.split_once(':')?;
            let property = property.trim().rsplit([' ', '\n', '\t']).next()?;
            let value = value.to_ascii_lowercase();
            let fixed = value.contains("#ffffff")
                || value.contains("#fff")
                || value.contains("white")
                || value.contains("255, 255, 255");
            (property == "color" && fixed).then(|| declaration.trim().to_owned())
        })
        .collect()
}

#[test]
fn npp_17_the_panel_takes_its_foreground_from_the_appearance() {
    for offender in [
        ".x { color: #ffffff; }",
        ".x { color: #fff; }",
        ".x { color:alpha(#ffffff, 0.5); }",
        ".x { color: white; }",
    ] {
        assert_eq!(
            fixed_foregrounds(offender).len(),
            1,
            "guard missed {offender}"
        );
    }

    for allowed in [
        ".x { background-color: alpha(#ffffff, 0.06); }",
        ".x { border-left: 1px solid rgba(255, 255, 255, 0.06); }",
    ] {
        assert!(
            fixed_foregrounds(allowed).is_empty(),
            "guard false-flagged {allowed}"
        );
    }

    for css in [crate::ui::now_playing::css(), crate::ui::lyrics_view::css()] {
        let fixed = fixed_foregrounds(&css);
        assert!(
            fixed.is_empty(),
            "Now Playing CSS paints text with a fixed foreground: {fixed:#?}"
        );
    }

    for row in &PANEL_ROLES {
        let css = (row.css)();
        let rules = rule_body(&css, row.selector);
        assert!(
            rules.contains(row.role),
            "{} does not consume {}: {rules}",
            row.selector,
            row.role
        );
        assert!(
            !rules.contains("opacity:"),
            "{} locally dims its verified role: {rules}",
            row.selector
        );
    }
}

#[test]
fn contrast_3_now_playing_roles_clear_aa_on_the_panel_surface() {
    use super::color_math::{contrast_ratio, parse_hex_rgb};
    use super::theme::Theme;

    for theme in Theme::all() {
        for (appearance, palette) in [("dark", theme.palette()), ("light", theme.light_palette())] {
            let foreground = parse_hex_rgb(palette.fg).expect("palette fg is valid hex");
            let surface = parse_hex_rgb(palette.sidebar_bg).expect("palette sidebar is valid hex");

            for row in &PANEL_ROLES {
                let minimum = row.minimum;
                let css = (row.css)();
                let color = color_declaration(&css, row.selector);
                let rendered = rendered_foreground(color, foreground, surface);
                let ratio = contrast_ratio(rendered, surface);
                assert!(
                    ratio >= minimum,
                    "{theme:?} {appearance}: {} reaches only {ratio:.2}:1 (minimum \
                     {minimum:.1}:1)",
                    row.selector
                );
            }
        }
    }
}

#[test]
fn contrast_3_now_playing_head_band_roles_clear_aa_over_every_glow_extreme() {
    use super::color_math::{composite, contrast_ratio, parse_hex_rgb};
    use super::theme::Theme;

    let glow_alpha = super::tokens::NOW_PLAYING_GLOW_ALPHA
        .parse::<f64>()
        .expect("the shipped glow alpha is numeric");
    let glow_css = crate::ui::now_playing::css();
    let glow_rule = rule_body(&glow_css, ".reprise-now-playing-glow");
    assert!(
        glow_rule.contains(&format!(
            "alpha(@reprise_player_accent, {})",
            super::tokens::NOW_PLAYING_GLOW_ALPHA
        )),
        "the contrast model must read the alpha used by the shipped glow: {glow_rule}"
    );
    let roles = PANEL_ROLES
        .iter()
        .filter(|row| {
            matches!(
                row.selector,
                ".reprise-now-playing-title" | ".reprise-now-playing-subtitle"
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(roles.len(), 2, "the head band has title and subtitle roles");
    for theme in Theme::all() {
        for (appearance, palette) in [("dark", theme.palette()), ("light", theme.light_palette())] {
            let foreground = parse_hex_rgb(palette.fg).expect("palette fg is valid hex");
            let surface = parse_hex_rgb(palette.sidebar_bg).expect("palette sidebar is valid hex");
            for (accent_name, accent) in [("black", [0, 0, 0]), ("white", [255, 255, 255])] {
                let head_surface = composite(accent, surface, glow_alpha);
                for row in &roles {
                    let css = (row.css)();
                    let color = color_declaration(&css, row.selector);
                    let rendered = rendered_foreground(color, foreground, head_surface);
                    let ratio = contrast_ratio(rendered, head_surface);
                    assert!(
                        ratio >= row.minimum,
                        "{theme:?} {appearance}, {accent_name} accent: {} reaches only \
                         {ratio:.2}:1 over the head-band glow (minimum {:.1}:1)",
                        row.selector,
                        row.minimum
                    );
                }
            }
        }
    }
}
