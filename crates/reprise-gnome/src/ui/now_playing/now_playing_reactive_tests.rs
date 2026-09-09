use super::*;

#[test]
fn ac_24_static_ellipse_dims_so_the_bloom_can_carry_the_movement() {
    // Both layers animating at once stacks two brightnesses and is instantly
    // too much: the ellipse stays put at a lower alpha, the bloom moves.
    assert_eq!(crate::ui::style::tokens::NOW_PLAYING_GLOW_ALPHA, "0.15");
    let css = crate::ui::now_playing::surface_css::css();
    assert!(css.contains("@reprise_now_playing_glow"));
    let dark_theme = crate::ui::style::theme::theme_css(
        crate::ui::style::theme::Theme::PerpetualRain,
        true,
        crate::ui::style::accent::AccentSource::App,
    );
    let light_theme = crate::ui::style::theme::theme_css(
        crate::ui::style::theme::Theme::PerpetualRain,
        false,
        crate::ui::style::accent::AccentSource::App,
    );
    assert!(dark_theme
        .contains("@define-color reprise_now_playing_glow alpha(@reprise_player_accent, 0.15);"));
    assert!(light_theme
        .contains("@define-color reprise_now_playing_glow alpha(@reprise_player_accent, 0.05);"));
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
        super::tests::test_panel("io.github.marvinbaudach.Reprise.NowPlayingReactiveBloomTest");
    let bloom = panel.bloom_widget();
    // Behind the cover inside the artwork band and above the panel background.
    // The metadata now starts below that band (NPP-18).
    assert!(bloom.is_ancestor(panel.stage_for_test()));
    assert!(!bloom.can_target());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn ac_24_the_panel_head_looks_the_same_whichever_tab_is_open() {
    if gtk4::init().is_err() {
        return;
    }
    let (_window, panel) =
        super::tests::test_panel("io.github.marvinbaudach.Reprise.NowPlayingShimmerPinTest");
    panel.set_transient_visibility(true);
    panel.set_song_visuals_enabled(true);
    panel.widgets.shimmer.set_light(0.8, 0.7);
    panel.widgets.shimmer.set_frame_time(15_000_000);
    assert!(shimmer_unpinned(&panel));

    // The Visual tab used to pin the backdrop and hide the disc, on the theory
    // that two light languages in one panel fight each other. In use the plain
    // treatment was better there too, so switching tabs must change nothing
    // about the head.
    panel.widgets.tab_stack.set_visible_child_name(VISUAL_PAGE);
    assert!(shimmer_unpinned(&panel));

    // Closing the panel still rests both: a pinned backdrop runs no tick, and
    // without that the paused breath would redraw a widget nobody can see.
    panel.set_transient_visibility(false);
    assert!(!shimmer_unpinned(&panel));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn npp_18_the_turning_disc_survives_a_theme_switch() {
    if gtk4::init().is_err() {
        return;
    }
    let (_window, panel) = super::tests::test_panel(
        "io.github.marvinbaudach.Reprise.NowPlayingShimmerThemeSwitchTest",
    );
    let settings = gtk4::Settings::default().unwrap();
    let animations_were_enabled = settings.is_gtk_enable_animations();
    settings.set_gtk_enable_animations(true);
    crate::ui::style::set_color_scheme("dark");

    panel.set_transient_visibility(true);
    panel.set_song_visuals_enabled(true);
    assert!(shimmer_unpinned(&panel));

    let bytes = gtk4::glib::Bytes::from_owned(vec![0x80_u8; 4 * 4 * 4]);
    let cover =
        gtk4::gdk::MemoryTexture::new(4, 4, gtk4::gdk::MemoryFormat::R8g8b8a8, &bytes, 4 * 4);
    panel.widgets.shimmer.set_cover(Some(cover.upcast_ref()), 1);
    assert!(
        panel.widgets.shimmer.drawn_angle_for_test().is_some(),
        "the shimmer must have a cover or the disc draws nothing"
    );

    panel.widgets.shimmer.set_frame_time(1_000_000);
    panel.widgets.shimmer.set_frame_time(6_000_000);
    panel.widgets.shimmer.set_frame_time(11_000_000);
    let before = panel.widgets.shimmer.drawn_angle_for_test().unwrap();
    assert!(before > 0.0, "the disc must be turning before the switch");

    crate::ui::style::set_color_scheme("light");
    panel.widgets.shimmer.set_frame_time(11_000_001);
    let after = panel.widgets.shimmer.drawn_angle_for_test().unwrap();

    crate::ui::style::set_color_scheme("default");
    settings.set_gtk_enable_animations(animations_were_enabled);
    assert!(
        (after - before).abs() < 1e-6,
        "the disc snapped from {before} to {after} radians across the theme switch"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn ac_26_song_visuals_follow_music_instead_of_the_external_source() {
    if gtk4::init().is_err() {
        return;
    }
    let (_window, panel) =
        super::tests::test_panel("io.github.marvinbaudach.Reprise.NowPlayingPodcastVisuals");
    panel.set_transient_visibility(true);
    panel.set_song_visuals_enabled(true);
    panel.widgets.tab_stack.set_visible_child_name(VISUAL_PAGE);
    assert!(panel.widgets.visual_page.is_visible());
    // The shimmer's own `visible` flag, not `is_visible()`: the latter also
    // asks whether every ancestor is mapped, which an unpresented test window
    // is not — it would answer "hidden" whatever the pin says.
    assert!(shimmer_unpinned(&panel));

    // The panel receives the module switch and the typed session separately;
    // the snapshot's one music predicate decides the effective treatment.
    panel.set_external_snapshot(Some(super::external_tests::external_episode_snapshot()));

    assert!(
        !panel.widgets.visual_page.is_visible(),
        "a podcast leaves no Visual tab to open"
    );
    assert_eq!(
        panel.widgets.tab_stack.visible_child_name().as_deref(),
        Some(UP_NEXT_PAGE),
        "the user standing on the Visual tab lands on Up Next"
    );
    assert!(
        !shimmer_unpinned(&panel),
        "the reactive light rests for speech"
    );

    for snapshot in [
        super::external_tests::external_youtube_snapshot(),
        super::external_tests::external_radio_snapshot(),
    ] {
        panel.set_external_snapshot(Some(snapshot));
        assert!(
            panel.widgets.visual_page.is_visible(),
            "YouTube and radio keep the visuals a podcast took away"
        );
        assert!(
            shimmer_unpinned(&panel),
            "music gets the reactive light back"
        );
    }
}

fn shimmer_unpinned(panel: &NowPlayingPanel) -> bool {
    panel.widgets.shimmer.widget().property::<bool>("visible")
}
