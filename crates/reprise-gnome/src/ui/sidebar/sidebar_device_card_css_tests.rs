#[test]
fn css_covers_the_sync_card_vocabulary() {
    let css = super::css();
    for marker in [
        ".device-card {",
        ".device-card:hover",
        ".device-card:focus-visible",
        ".device-card-icon",
        ".device-card-glyph",
        ".device-card-detail",
        ".device-card-percent",
        ".device-card-progress trough",
        ".device-card-progress progress",
    ] {
        assert!(css.contains(marker), "missing rule: {marker}");
    }
    assert!(
        !css.contains("#1CA98F"),
        "the accent must come from the theme, not a literal, or non-default palettes break"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn css_parses_in_gtk_without_dropping_declarations() {
    if gtk4::init().is_err() {
        return;
    }
    let combined = format!(
        "{}\n{}",
        crate::ui::style::theme::theme_css(crate::ui::style::theme::Theme::DEFAULT, true),
        super::css()
    );
    let errors = crate::ui::style::css_parse_errors(&combined);
    assert!(
        errors.is_empty(),
        "GTK reported CSS parsing errors: {errors:?}"
    );
}
