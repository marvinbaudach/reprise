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

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn ac_24_the_panel_head_looks_the_same_whichever_tab_is_open() {
    if gtk4::init().is_err() {
        return;
    }
    let (_window, panel) = super::tests::test_panel("org.reprise.Reprise.NowPlayingShimmerPinTest");
    panel.set_transient_visibility(true);
    panel.set_song_visuals_enabled(true);
    panel.widgets.shimmer.set_light(0.8, 0.7);
    panel.widgets.shimmer.set_frame_time(15_000_000);
    assert!(panel.widgets.shimmer.widget().is_visible());

    // The Visual tab used to pin the backdrop and hide the disc, on the theory
    // that two light languages in one panel fight each other. In use the plain
    // treatment was better there too, so switching tabs must change nothing
    // about the head.
    panel.widgets.tab_stack.set_visible_child_name(VISUAL_PAGE);
    assert!(panel.widgets.shimmer.widget().is_visible());

    // Closing the panel still rests both: a pinned backdrop runs no tick, and
    // without that the paused breath would redraw a widget nobody can see.
    panel.set_transient_visibility(false);
    assert!(!panel.widgets.shimmer.widget().is_visible());
}
