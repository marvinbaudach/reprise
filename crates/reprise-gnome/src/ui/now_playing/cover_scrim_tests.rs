use super::*;
use crate::ui::now_playing::cover_cloud::field;
use crate::ui::style::tokens;

#[test]
fn npc_10_the_scrim_hits_the_three_stops_the_mockup_names() {
    // 0 % at the top, 15 % at 40 % of the field, fully opaque from 55 % down.
    assert!((scrim_alpha(0.0) - 0.0).abs() < 1e-9);
    assert!((scrim_alpha(0.40) - 0.15).abs() < 1e-9);
    assert!((scrim_alpha(0.55) - 1.0).abs() < 1e-9);
    assert!((scrim_alpha(1.0) - 1.0).abs() < 1e-9);
    // Clamped rather than extrapolated on either side.
    assert!((scrim_alpha(-0.5) - 0.0).abs() < 1e-9);
    assert!((scrim_alpha(4.0) - 1.0).abs() < 1e-9);
}

#[test]
fn npc_11_the_scrim_only_ever_darkens_on_the_way_down() {
    // No hard edge means no step and no dip: the fade rises the whole way.
    let mut previous = scrim_alpha(0.0);
    for step in 0..=200 {
        let y = f64::from(step) / 200.0;
        let alpha = scrim_alpha(y);
        assert!(
            alpha >= previous - 1e-12,
            "the scrim lightens again at y={y}"
        );
        previous = alpha;
    }
}

#[test]
fn npc_12_the_text_never_sits_on_a_moving_ground() {
    // The title block begins where the artwork band ends. Whatever the field's
    // height, the scrim has to be fully opaque long before that.
    let cover = f64::from(tokens::NOW_PLAYING_COVER_SIZE);
    let (_, top, _, height) = field(300.0, cover);
    let opaque_at = top + SCRIM_FULL_Y * height;

    // The band's own end is the weak claim — anything under 280 would pass it,
    // including a scrim that closed at 279 and put a moving edge right beneath
    // the title. The claim worth making is that the light is already gone by
    // the time the cover ends: below that edge there is nothing left to move.
    let cover_bottom = 22.0 + cover;
    assert!(
        opaque_at < cover_bottom,
        "the scrim closes at y={opaque_at:.1}, below the cover's own edge at {cover_bottom:.1}"
    );
    let band = f64::from(tokens::NOW_PLAYING_ARTWORK_BAND);
    assert!(
        opaque_at < band,
        "and it must close inside the {band:.1}px band"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn npc_29_the_scrim_cache_reuses_only_the_same_theme_appearance_and_geometry() {
    use crate::ui::style::theme::Theme;

    gtk4::init().expect("gtk");
    let previous_theme = crate::ui::style::current_theme();
    crate::ui::style::set_theme(Theme::PerpetualRain);
    let cache = RefCell::new(None);

    let first = cached_scrim(&cache, true, -42.0, 308.0);
    let same = cached_scrim(&cache, true, -42.0, 308.0);
    assert_eq!(pattern_identity(&first), pattern_identity(&same));

    let light = cached_scrim(&cache, false, -42.0, 308.0);
    assert_ne!(pattern_identity(&same), pattern_identity(&light));

    crate::ui::style::set_theme(Theme::NightTerrain);
    let themed = cached_scrim(&cache, false, -42.0, 308.0);
    assert_ne!(pattern_identity(&light), pattern_identity(&themed));

    let moved = cached_scrim(&cache, false, -41.0, 308.0);
    assert_ne!(pattern_identity(&themed), pattern_identity(&moved));

    let resized = cached_scrim(&cache, false, -41.0, 309.0);
    assert_ne!(pattern_identity(&moved), pattern_identity(&resized));
    crate::ui::style::set_theme(previous_theme);
}

fn pattern_identity(gradient: &cairo::LinearGradient) -> usize {
    let pattern: &cairo::Pattern = gradient.as_ref();
    pattern.to_raw_none() as usize
}
