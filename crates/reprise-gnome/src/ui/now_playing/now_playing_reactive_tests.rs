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
        super::tests::test_panel("io.github.marvinbaudach.Reprise.NowPlayingCloudPinTest");
    panel.set_transient_visibility(true);
    panel.set_song_visuals_enabled(true);
    panel.widgets.cloud.set_frame_time(15_000_000);
    assert!(cloud_unpinned(&panel));

    // The Visual tab used to pin the backdrop and hide the clouds, on the theory
    // that two light languages in one panel fight each other. In use the plain
    // treatment was better there too, so switching tabs must change nothing
    // about the head.
    panel.widgets.tab_stack.set_visible_child_name(VISUAL_PAGE);
    assert!(cloud_unpinned(&panel));

    // Closing the panel still rests both: a pinned backdrop runs no tick, and
    // without that the paused breath would redraw a widget nobody can see.
    panel.set_transient_visibility(false);
    assert!(!cloud_unpinned(&panel));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn npp_18_the_drifting_clouds_survive_a_theme_switch() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    if gtk4::init().is_err() {
        return;
    }
    let (window, panel) =
        super::tests::test_panel("io.github.marvinbaudach.Reprise.NowPlayingCloudThemeSwitch");
    let settings = gtk4::Settings::default().unwrap();
    let animations_were_enabled = settings.is_gtk_enable_animations();
    settings.set_gtk_enable_animations(true);
    crate::ui::style::set_color_scheme("dark");

    panel.set_transient_visibility(true);
    panel.set_song_visuals_enabled(true);
    panel.set_playback_state(PlaybackState::Playing);
    assert!(cloud_unpinned(&panel));

    let texture: gtk4::gdk::Texture = gtk4::gdk::MemoryTexture::new(
        1,
        1,
        gtk4::gdk::MemoryFormat::B8g8r8a8Premultiplied,
        &gtk4::glib::Bytes::from_static(&[0x40, 0x60, 0x80, 0xff]),
        4,
    )
    .upcast();
    panel.widgets.cloud.set_cover(Some(&texture), 1);
    panel.widgets.cloud.set_frame_time(1_000_000);
    panel.widgets.cloud.set_frame_time(11_000_000);
    assert!(
        panel.widgets.cloud.drawn_pose_for_test().is_none(),
        "setting the clock must not masquerade as a draw"
    );

    panel.retain_for_window(&window);
    window.set_default_size(1_200, 800);
    window.present();
    assert!(crate::ui::test_settle::settle_until(
        crate::ui::test_settle::DISPLAY_TEST_TIMEOUT,
        || panel.widgets.cloud.drawn_pose_for_test().is_some()
    ));
    let dark = panel
        .widgets
        .cloud
        .drawn_pose_for_test()
        .expect("cloud pose in dark appearance");

    // The disc this replaced turned at one rate in the dark and another in the
    // light, so a theme switch could jump its angle — that is what the test
    // standing here guarded. The clouds answer it by construction instead: the
    // theme reaches only the blend operator, never the clock, so the drift
    // cannot move at a switch. Asserted rather than assumed, because a later
    // theme-dependent period would reintroduce exactly the old bug.
    crate::ui::style::set_color_scheme("light");
    panel.widgets.cloud.widget().queue_draw();
    crate::ui::test_settle::settle_for(std::time::Duration::from_millis(20));
    let light = panel
        .widgets
        .cloud
        .drawn_pose_for_test()
        .expect("cloud pose in light appearance");

    let expected_back = super::cover_cloud::drift_at(10.0, super::cover_cloud::BACK_DRIFT);

    crate::ui::style::set_color_scheme("default");
    settings.set_gtk_enable_animations(animations_were_enabled);
    window.close();
    assert_eq!(
        dark.0, expected_back,
        "the test must observe an advanced pose"
    );
    assert_eq!(
        dark, light,
        "the drift must not depend on the theme, or a switch snaps it"
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
    // The clouds' own `visible` flag, not `is_visible()`: the latter also
    // asks whether every ancestor is mapped, which an unpresented test window
    // is not — it would answer "hidden" whatever the pin says.
    assert!(cloud_unpinned(&panel));

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
        !cloud_unpinned(&panel),
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
        assert!(cloud_unpinned(&panel), "music gets the reactive light back");
    }
}

fn cloud_unpinned(panel: &NowPlayingPanel) -> bool {
    panel.widgets.cloud.widget().property::<bool>("visible")
}
