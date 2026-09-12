use super::*;
use crate::ui::style::tokens;

#[test]
fn ac_24_the_scrim_hits_the_three_stops_the_mockup_names() {
    let title_top = 268.0;
    assert!((scrim_alpha(0.0, title_top) - 0.0).abs() < 1e-9);
    assert!((scrim_alpha(2.0 * title_top / 3.0, title_top) - 0.15).abs() < 1e-9);
    assert!((scrim_alpha(title_top, title_top) - 1.0).abs() < 1e-9);
    // Clamped rather than extrapolated on either side.
    assert!((scrim_alpha(-0.5, title_top) - 0.0).abs() < 1e-9);
    assert!((scrim_alpha(400.0, title_top) - 1.0).abs() < 1e-9);
}

#[test]
fn ac_24_the_scrim_only_ever_darkens_on_the_way_down() {
    // No hard edge means no step and no dip: the fade rises the whole way.
    let title_top = 268.0;
    let mut previous = scrim_alpha(0.0, title_top);
    for step in 0..=200 {
        let y = title_top * f64::from(step) / 200.0;
        let alpha = scrim_alpha(y, title_top);
        assert!(
            alpha >= previous - 1e-12,
            "the scrim lightens again at y={y}"
        );
        previous = alpha;
    }
}

#[test]
fn ac_24_the_text_never_sits_on_a_moving_ground() {
    let title_top = f64::from(
        tokens::NOW_PLAYING_HEAD_TOP
            + tokens::NOW_PLAYING_COVER_SIZE
            + tokens::NOW_PLAYING_COVER_TO_TITLE,
    );
    let cover_bottom = f64::from(tokens::NOW_PLAYING_HEAD_TOP + tokens::NOW_PLAYING_COVER_SIZE);

    assert_eq!(tokens::NOW_PLAYING_ARTWORK_BAND, title_top as i32);
    assert_eq!(scrim_alpha(title_top, title_top), 1.0);
    assert!(scrim_alpha(cover_bottom, title_top) < 1.0);
}

#[test]
fn ac_24_the_left_fade_reaches_the_list_edge_over_eight_percent_of_the_panel() {
    assert_eq!(left_fade_alpha(0.0, 300.0), 1.0);
    assert_eq!(left_fade_alpha(12.0, 300.0), 0.5);
    assert_eq!(left_fade_alpha(24.0, 300.0), 0.0);
    assert_eq!(left_fade_alpha(100.0, 300.0), 0.0);
    assert_eq!(left_fade_alpha(16.0, 400.0), 0.5);
    assert_eq!(left_fade_alpha(32.0, 400.0), 0.0);
}

#[test]
fn ac_24_degenerate_fades_are_transparent() {
    assert_eq!(scrim_alpha(10.0, 0.0), 0.0);
    assert_eq!(left_fade_alpha(10.0, 0.0), 0.0);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn ac_24_the_scrim_cache_reuses_only_the_same_theme_appearance_and_width() {
    use crate::ui::style::theme::Theme;

    gtk4::init().expect("gtk");
    let previous_theme = crate::ui::style::current_theme();
    crate::ui::style::set_theme(Theme::PerpetualRain);
    let cache = RefCell::new(None);

    let first = cached_scrim(&cache, true, 300.0);
    let same = cached_scrim(&cache, true, 300.0);
    assert_eq!(pattern_identity(&first.0), pattern_identity(&same.0));
    assert_eq!(pattern_identity(&first.1), pattern_identity(&same.1));

    let light = cached_scrim(&cache, false, 300.0);
    assert_ne!(pattern_identity(&same.0), pattern_identity(&light.0));

    crate::ui::style::set_theme(Theme::NightTerrain);
    let themed = cached_scrim(&cache, false, 300.0);
    assert_ne!(pattern_identity(&light.0), pattern_identity(&themed.0));

    let resized = cached_scrim(&cache, false, 301.0);
    assert_ne!(pattern_identity(&themed.1), pattern_identity(&resized.1));
    crate::ui::style::set_theme(previous_theme);
}

fn pattern_identity(gradient: &cairo::LinearGradient) -> usize {
    let pattern: &cairo::Pattern = gradient.as_ref();
    pattern.to_raw_none() as usize
}
