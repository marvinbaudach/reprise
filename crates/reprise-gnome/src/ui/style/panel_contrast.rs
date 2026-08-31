use super::tokens::{PRIMARY_TEXT_ALPHA, SECONDARY_TEXT_ALPHA};

/// WCAG 1.4.3's minimum contrast ratio for normal text, matching
/// `accent::ACCENT_TEXT_MINIMUM_RATIO`.
const NORMAL_TEXT_MINIMUM_RATIO: f64 = 4.5;

/// White-glyph contrast measured against the default Reprise brand accent.
const DEFAULT_BRAND_ACCENT_GLYPH_RATIO: f64 = 1.69;

/// Rounding tolerance for the recorded default-brand-accent measurement.
const DEFAULT_BRAND_ACCENT_GLYPH_RATIO_TOLERANCE: f64 = 0.01;

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

    // Sweeping two sections is how the play/pause glyph kept a fixed white on
    // the accent surface — 1.69:1 on the app's most prominent control — while
    // this guard stayed green. The sweep runs over the whole stylesheet.
    //
    // One surface owns *both* of its colours and is therefore legitimately
    // appearance-independent: the dev-build badge, white on a fixed #b5432f at
    // 5.51:1. It is exempted by cutting its rule out before the sweep rather
    // than by filtering the resulting declarations — a filter keyed on the
    // colour would have waved through every other white foreground too. Cutting
    // by selector also fails loudly if the rule is renamed, instead of quietly
    // widening the exemption.
    let css = super::app_css();
    let badge = rule_body(&css, ".reprise-build-badge");
    assert!(
        badge.contains("#b5432f"),
        "the build badge no longer owns its own background, so its exemption \
         from the fixed-foreground sweep no longer holds: {badge}"
    );
    let mut swept = css.replace(badge, "");
    // PLAY-16 deliberately exempts the two play faces from CONTRAST-5a's fixed
    // foreground sweep. Remove only their live rule bodies; widening this to a
    // colour-based exemption would hide unrelated fixed-white regressions.
    for selector in [".player-bar-play", ".mini-player-play"] {
        let play = rule_body(&swept, selector).to_owned();
        swept = swept.replace(&play, "");
    }
    let fixed = fixed_foregrounds(&swept);
    assert!(
        fixed.is_empty(),
        "app CSS paints a foreground with a fixed colour instead of a themed \
         role: {fixed:#?}"
    );

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
fn play_16_the_play_buttons_keep_the_playback_accent_and_white_glyph() {
    use super::color_math::{contrast_ratio, parse_hex_rgb};

    for (css, selector) in [
        (crate::ui::player_bar_layout::css(), ".player-bar-play"),
        (
            crate::ui::compact_player_layouts::mini_css(),
            ".mini-player-play",
        ),
    ] {
        let rule = rule_body(&css, selector);
        assert!(
            rule.contains("background-color: @reprise_player_accent"),
            "PLAY-16: {selector} lost the playback accent: {rule}"
        );
        assert!(
            rule.contains("color: #ffffff"),
            "PLAY-16: {selector} lost its deliberately white glyph: {rule}"
        );
    }

    let white = [u8::MAX; 3];
    let accent = parse_hex_rgb(super::accent::APP_ACCENT).expect("brand accent is valid hex");
    let ratio = contrast_ratio(white, accent);
    assert!(
        (ratio - DEFAULT_BRAND_ACCENT_GLYPH_RATIO).abs()
            < DEFAULT_BRAND_ACCENT_GLYPH_RATIO_TOLERANCE,
        "PLAY-16 records the default brand accent's measured 1.69:1 cost; a build with a \
         different REPRISE_APP_ACCENT will legitimately differ, measured {ratio:.2}:1"
    );
}

#[test]
fn named_button_and_grounded_checkbox_foreground_pairs_clear_normal_text_minimum() {
    use super::color_math::{composite, contrast_ratio, parse_hex_rgb};
    use super::theme::Theme;
    use super::tokens::{
        BTN_CHECKED_FILL_ALPHA, PRIMARY_DISABLED_FILL_ALPHA, SECONDARY_TEXT_ALPHA,
    };

    fn hex(rgb: [u8; 3]) -> String {
        format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
    }

    let palette = Theme::DEFAULT.palette();
    let accent = parse_hex_rgb(super::accent::APP_ACCENT).expect("brand accent is valid hex");
    let accent_fg = parse_hex_rgb(
        super::accent::accent_fg(super::accent::AccentSource::App)
            .expect("the app accent owns a surface foreground"),
    )
    .expect("app accent foreground is valid hex");
    let theme_fg = parse_hex_rgb(palette.fg).expect("theme foreground is valid hex");
    let headerbar = parse_hex_rgb(palette.headerbar_bg).expect("headerbar is valid hex");
    let card = parse_hex_rgb(palette.card_bg).expect("card is valid hex");
    let window = parse_hex_rgb(palette.window_bg).expect("window is valid hex");

    let accent_text = parse_hex_rgb(&super::accent::accent_text_color(
        accent,
        palette.critical_accent_surface(true, accent),
        true,
    ))
    .expect("derived accent text is valid hex");
    let checked_fill = composite(
        accent,
        headerbar,
        BTN_CHECKED_FILL_ALPHA
            .parse()
            .expect("checked fill is a fraction"),
    );
    let disabled_fill = composite(
        theme_fg,
        card,
        SECONDARY_TEXT_ALPHA
            * PRIMARY_DISABLED_FILL_ALPHA
                .parse::<f64>()
                .expect("disabled fill is a fraction"),
    );
    let disabled_text = composite(theme_fg, disabled_fill, SECONDARY_TEXT_ALPHA);

    let css = crate::ui::style::buttons::css();
    assert!(css.contains("color: @reprise_accent_text_color"));
    assert!(css.contains("color: @reprise_secondary_fg_color"));
    let doctor_css = crate::ui::library_doctor::css();
    assert!(doctor_css.contains(
        ".doctor-album-check:checked, .doctor-review-select-all:checked { background: \
         var(--accent-bg-color); color: var(--window-bg-color); }"
    ));

    // The device-sync playlist checkbox carries no Reprise CSS rule, so no
    // foreground/background pair can be tied to it here. Whether it needs one
    // remains an open user decision from the plan's Task 5.
    let measurements = [
        ("resting filled primary", accent_fg, accent),
        ("checked shuffle toggle", accent_text, checked_fill),
        ("disabled primary", disabled_text, disabled_fill),
        ("Library Doctor album checkbox", window, accent),
    ];

    for (target, foreground, background) in measurements {
        let ratio = contrast_ratio(foreground, background);
        eprintln!(
            "MEASURE {target}: foreground={} background={} ratio={ratio:.3}:1",
            hex(foreground),
            hex(background)
        );
        assert!(
            ratio >= NORMAL_TEXT_MINIMUM_RATIO,
            "{target} reaches only {ratio:.3}:1 (minimum \
             {NORMAL_TEXT_MINIMUM_RATIO:.1}:1)"
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
