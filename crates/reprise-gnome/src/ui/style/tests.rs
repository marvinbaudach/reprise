#[test]
fn style_2_side_surfaces_follow_the_theme() {
    let css = super::app_css();

    assert!(css.contains(
        ".reprise-library-split .reprise-library-sidebar { background-color: @sidebar_bg_color;"
    ));
    assert!(css.contains(".reprise-now-playing-stage { background-color: @sidebar_bg_color;"));
    assert!(css.contains("border-right: 1px solid @reprise_hairline"));
    assert!(css.contains("border-left: 1px solid @reprise_hairline"));

    for theme in super::theme::Theme::all() {
        for (is_dark, palette) in [(true, theme.palette()), (false, theme.light_palette())] {
            assert!(
                super::theme::theme_css(theme, is_dark, super::accent::AccentSource::App).contains(
                    &format!("@define-color sidebar_bg_color {};", palette.sidebar_bg)
                ),
                "{theme:?} did not project its sidebar surface into both consumers"
            );
        }
    }
}

#[test]
fn app_css_contains_every_feature_section() {
    let css = super::app_css();

    for marker in [
        ".reprise-equalizer scale > trough > highlight",
        "popover.menu > contents",
        ".toast button.text-button",
        ".player-bar-play",
        ".reprise-surface",
        ".reprise-hover:hover",
        ".reprise-btn-icon:hover",
        ".reprise-btn-toggle:checked",
        ".reprise-text-primary",
        ".reprise-text-secondary",
        ".reprise-text-hint",
        ".reprise-filter-chip",
        ".reprise-column-drop-before",
        ".reprise-column-row:hover",
        ".reprise-track-cell",
        ".reprise-now-playing-stage",
        ".lyrics-line-active",
        ".reprise-preview-zone",
        ".reprise-rating-star",
        ".reprise-list-status-bar",
        ".reprise-track-list > header label",
        ".reprise-reorder-target",
        ".stats-chart",
        ".reprise-concerts-view",
        ".reprise-releases-view",
        ".new-release-badge",
        ".reprise-podcasts-source",
        ".reprise-radio-source",
        ".reprise-tag-editor",
        "floating-sheet > sheet",
        ".issue-card",
        ".mini-player-card",
        ".scan-card",
    ] {
        assert!(css.contains(marker), "missing section marker: {marker}");
    }
}

#[test]
fn app_css_has_no_palette_colors() {
    // Palette @define-colors belong to the separate theme provider, not
    // the structural CSS, so a theme switch never has to rebuild this.
    assert!(!super::app_css().contains("@define-color"));
}

/// Every `color:` declaration painting text with an accent role that was
/// never contrast-checked. Matches on the parsed property rather than on
/// one spelling of the whole declaration: `color:@accent_color` and
/// `color: alpha(@accent_color, 0.85)` are the same violation, and the
/// latter is precisely what CONTRAST-5 replaced — a whole-string compare
/// waved both through.
fn unverified_accent_foregrounds(css: &str) -> Vec<String> {
    css.split(['{', '}', ';'])
        .filter_map(|declaration| {
            let (property, value) = declaration.split_once(':')?;
            let property = property.trim().rsplit([' ', '\n', '\t']).next()?;
            let paints_text = property == "color";
            let unverified_role =
                value.contains("@accent_color") || value.contains("@accent_bg_color");
            (paints_text && unverified_role).then(|| declaration.trim().to_owned())
        })
        .collect()
}

/// Every accent-tinted *background* alpha the app CSS paints, parsed out of
/// the composed stylesheet rather than read off a hand-kept token list —
/// several tint alphas are still literals inside their feature's `css()`,
/// and a list would not see them.
fn accent_background_tints(css: &str) -> Vec<(String, f64)> {
    css.split(['{', '}', ';'])
        .filter_map(|declaration| {
            let (property, value) = declaration.split_once(':')?;
            let property = property.trim().rsplit([' ', '\n', '\t']).next()?;
            if property != "background-color"
                && property != "background"
                && property != "background-image"
            {
                return None;
            }
            let rest = value.split_once("alpha(")?.1;
            let (role, alpha) = rest.split_once(',')?;
            let role = role.trim();
            if role != "@accent_bg_color" && role != "@accent_color" {
                return None;
            }
            let alpha = alpha.split(')').next()?.trim().parse().ok()?;
            Some((declaration.trim().to_owned(), alpha))
        })
        .collect()
}

/// Accent-tinted surfaces that carry no foreground at all, so the ceiling
/// `@reprise_accent_text_color` is derived against does not bind them.
///
/// This list is the *only* legitimate way past the guard, and it is per
/// rule on purpose. Exempting a property or a colour role instead — say,
/// "gradients over `@accent_color` are never checked" — would hand a free
/// pass to every future text-bearing surface that happens to use the same
/// shape, which is precisely the blind spot CONTRAST-5a exists to close.
/// Adding an entry means proving the surface has no foreground; name the
/// widget and say why it cannot carry one.
const DECORATIVE_ACCENT_SURFACES: &[&str] = &[
    // The rail left of the online-children card in preferences: a bare
    // `gtk4::Box`, `min-width: 2px`, constructed with no children and never
    // given any (`preference_plugins.rs`). It is a fade-to-transparent
    // subordination cue, so its 0.55 top stop is decoration; lowering it to
    // the ceiling would all but erase a 2px sliver to pay a legibility tax
    // for text that does not exist.
    "linear-gradient(to bottom, alpha(@accent_color, 0.55)",
];

/// CONTRAST-5a's other half. `@reprise_accent_text_color` is derived against
/// a surface tinted to `tokens::ACCENT_TINT_CEILING`, so that derivation is
/// only a guarantee while no rule tints further. A louder fill added
/// anywhere in the app silently invalidates the role for every widget that
/// reads it — which is exactly how the checked player-bar toggle shipped at
/// 2.97:1 with the full contrast suite green.
#[test]
fn contrast_5a_no_app_surface_tints_past_the_ceiling() {
    let ceiling: f64 = super::tokens::ACCENT_TINT_CEILING
        .parse()
        .expect("the accent tint ceiling is a decimal fraction");
    let css = super::app_css();
    let tints = accent_background_tints(&css);

    assert!(
        !tints.is_empty(),
        "the tint parser found nothing — app CSS always paints accent tints, \
         so this is the parser breaking, not the app improving"
    );

    let over: Vec<_> = tints
        .into_iter()
        .filter(|(declaration, alpha)| {
            *alpha > ceiling
                && !DECORATIVE_ACCENT_SURFACES
                    .iter()
                    .any(|decorative| declaration.contains(decorative))
        })
        .collect();
    assert!(
        over.is_empty(),
        "app CSS tints a surface past the {ceiling} ceiling that \
         @reprise_accent_text_color is derived against: {over:#?}\n\
         either lower the fill or raise ACCENT_TINT_CEILING — and if you \
         raise it, contrast_5a_accent_text_survives_every_tint_up_to_the_ceiling \
         decides whether the accent can still follow"
    );
}

#[test]
fn contrast_5a_the_tint_ceiling_guard_catches_a_louder_fill() {
    let over = accent_background_tints(".x { background-color: alpha(@accent_bg_color, 0.9); }");
    assert_eq!(over.len(), 1, "guard missed a louder accent fill");
    assert!((over[0].1 - 0.9).abs() < f64::EPSILON);

    let gradient = accent_background_tints(
        ".x { background-image: linear-gradient(alpha(@accent_bg_color, 0.9), transparent); }",
    );
    assert_eq!(
        gradient.len(),
        1,
        "guard missed a louder accent gradient fill"
    );
    assert!((gradient[0].1 - 0.9).abs() < f64::EPSILON);

    // The role a gradient tints with does not exempt it. An earlier version
    // of this guard skipped `@accent_color` gradients wholesale to silence
    // the one decorative rail; that would have waved through any
    // text-bearing panel painted the same way.
    let accent_role = accent_background_tints(
        ".x { background-image: linear-gradient(alpha(@accent_color, 0.9), transparent); }",
    );
    assert_eq!(
        accent_role.len(),
        1,
        "guard missed a louder @accent_color gradient fill"
    );
    assert!((accent_role[0].1 - 0.9).abs() < f64::EPSILON);

    for ignored in [
        ".x { color: alpha(@accent_color, 0.9); }",
        ".x { background-color: alpha(currentColor, 0.9); }",
        ".x { border-color: alpha(@accent_color, 0.9); }",
    ] {
        assert!(
            accent_background_tints(ignored).is_empty(),
            "guard false-flagged {ignored}"
        );
    }
}

/// An exemption that no longer matches any rule is a silent licence: it
/// stops proving anything about the app and starts hiding the next rule
/// that happens to be written the same way. Either the surface is still
/// there or the entry goes.
#[test]
fn contrast_5a_every_decorative_exemption_still_names_a_live_rule() {
    let css = super::app_css();

    for decorative in DECORATIVE_ACCENT_SURFACES {
        assert!(
            css.contains(decorative),
            "the decorative-surface exemption {decorative:?} matches no rule \
             in the app stylesheet any more — drop it instead of leaving a \
             blind spot behind"
        );
    }
}

#[test]
fn contrast_5a_app_css_never_uses_unverified_accent_as_foreground() {
    let unverified = unverified_accent_foregrounds(&super::app_css());

    assert!(
        unverified.is_empty(),
        "app CSS paints text with an unverified accent: {unverified:#?}\n\
         use @reprise_accent_text_color instead (CONTRAST-5a)"
    );
}

#[test]
fn contrast_5a_the_foreground_guard_catches_every_spelling() {
    // This guard is the regression net for the whole sweep, so prove it
    // actually trips — on each way the violation can be written.
    for offender in [
        ".x { color: @accent_color; }",
        ".x { color:@accent_color; }",
        ".x { color:   @accent_color; }",
        ".x { color: alpha(@accent_color, 0.85); }",
        ".x { color: shade(@accent_color, 1.2); }",
        ".x { color: @accent_bg_color; }",
        ".x {\n           color: @accent_color;\n       }",
    ] {
        assert_eq!(
            unverified_accent_foregrounds(offender).len(),
            1,
            "guard missed {offender}"
        );
    }

    for allowed in [
        ".x { background-color: @accent_color; }",
        ".x { border-color: @accent_color; }",
        ".x { outline-color: @accent_color; }",
        ".x { box-shadow: inset 2px 0 0 @accent_color; }",
        ".x { color: @reprise_accent_text_color; }",
        ".x { background-image: radial-gradient(circle, @accent_color 0%, transparent); }",
        ".x { border: 1px solid alpha(@accent_bg_color, 0.45); }",
    ] {
        assert!(
            unverified_accent_foregrounds(allowed).is_empty(),
            "guard false-flagged {allowed}"
        );
    }
}

#[test]
fn install_without_a_display_does_not_arm_the_once_guard() {
    // Plain unit-test processes have no GDK display; install() must stay
    // re-runnable so the first real (xvfb) caller still gets the CSS.
    if gtk4::gdk::Display::default().is_some() {
        return;
    }
    super::install();
    super::install();
    super::INSTALLED.with(|installed| assert!(!installed.get()));
}
