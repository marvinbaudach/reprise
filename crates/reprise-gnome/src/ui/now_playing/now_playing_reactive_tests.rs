use super::*;

#[test]
fn ac_24_static_ellipse_dims_so_the_bloom_can_carry_the_movement() {
    // Both layers animating at once stacks two brightnesses and is instantly
    // too much: the ellipse stays put at a lower alpha, the bloom moves.
    assert_eq!(crate::ui::style::tokens::NOW_PLAYING_GLOW_ALPHA, "0.26");
    let css = crate::ui::now_playing::surface_css::css();
    assert!(css.contains("0.26"));
    // The idle rule (no track at all) is untouched: with no cover there is no
    // bloom either, so the panel must still go dark.
    assert!(css.contains(".reprise-now-playing-idle .reprise-now-playing-glow"));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn ac_24_bloom_sits_behind_the_cover_inside_the_head_overlay() {
    if gtk4::init().is_err() {
        return;
    }
    let (_window, panel) =
        super::tests::test_panel("org.reprise.Reprise.NowPlayingReactiveBloomTest");
    let bloom = panel.bloom_widget();
    // Behind the head (cover + title) and above the panel background.
    assert!(bloom.is_ancestor(panel.stage_for_test()));
    assert!(!bloom.can_target());
}
