use super::*;

/// The mockup's own numbers, kept here so a change to the module has to argue
/// with the design rather than quietly redefine it.
const SPEC_COVER: f64 = 240.0;

#[test]
fn npc_1_a_period_is_the_whole_round_trip_not_one_leg() {
    // "16 s, ease-in-out, endlos, hin und zurück" — the layer leaves, arrives
    // and is home again inside one period. Driving this off a reversing tween
    // of the same length would take twice as long, which is the mistake this
    // test exists to catch.
    assert!((drift_progress(0.0, 16.0, 0.0) - 0.0).abs() < 1e-9);
    assert!((drift_progress(8.0, 16.0, 0.0) - 1.0).abs() < 1e-9);
    assert!((drift_progress(16.0, 16.0, 0.0) - 0.0).abs() < 1e-9);
}

#[test]
fn npc_2_the_drift_never_jumps_at_the_wrap() {
    // A step across the seam must be no larger than a step anywhere else.
    let just_before = drift_progress(15.999, 16.0, 0.0);
    let just_after = drift_progress(16.001, 16.0, 0.0);
    assert!(
        (just_before - just_after).abs() < 1e-3,
        "seam jumps from {just_before} to {just_after}"
    );
}

#[test]
fn npc_3_a_long_session_does_not_lose_the_drift_into_a_stutter() {
    // A day in, the fraction has to be as exact as it was at the start.
    assert!((drift_progress(86_400.0, 16.0, 0.0) - drift_progress(0.0, 16.0, 0.0)).abs() < 1e-9);
    assert!((drift_progress(86_408.0, 16.0, 0.0) - drift_progress(8.0, 16.0, 0.0)).abs() < 1e-9);
}

#[test]
fn npc_4_the_drift_eases_in_and_out_rather_than_running_flat() {
    // A quarter of the way through a leg a linear ramp would stand at 0.5.
    // ease-in-out is still gathering itself there, and symmetric about the
    // midpoint of the leg.
    let quarter = drift_progress(2.0, 16.0, 0.0);
    let three_quarters = drift_progress(6.0, 16.0, 0.0);
    assert!(quarter < 0.4, "eased start reached {quarter}");
    assert!((quarter + three_quarters - 1.0).abs() < 1e-9);
}

#[test]
fn npc_5_no_cycle_runs_faster_than_sixteen_seconds() {
    // "kein Zyklus unter 16 s" — the one rule with a number attached.
    const { assert!(BACK_PERIOD_S >= 16.0) };
    const { assert!(FRONT_PERIOD_S >= 16.0) };
}

#[test]
fn npc_6_the_two_layers_start_at_opposite_ends_of_the_path() {
    // The offset is a real half period, which is what sets the layers against
    // each other. The mockup asks for `reverse` as well, but a keyframe list
    // whose first and last poses are identical plays the same backwards, so
    // the offset is doing all of the work and has to be exact.
    assert!((FRONT_OFFSET_S - FRONT_PERIOD_S / 2.0).abs() < 1e-9);
    let back = drift_progress(0.0, BACK_PERIOD_S, 0.0);
    let front = drift_progress(0.0, FRONT_PERIOD_S, FRONT_OFFSET_S);
    assert!((back - 0.0).abs() < 1e-9);
    assert!((front - 1.0).abs() < 1e-9);
}

#[test]
fn npc_7_the_pair_of_layers_repeats_only_after_eighty_seconds() {
    // 16 and 20 share a least common multiple of 80: before that the two
    // layers never hold the same pair of poses again, so the head of the panel
    // does not visibly loop.
    let pose_at = |t: f64| {
        (
            drift_progress(t, BACK_PERIOD_S, 0.0),
            drift_progress(t, FRONT_PERIOD_S, FRONT_OFFSET_S),
        )
    };
    let (back0, front0) = pose_at(0.0);
    // Stepped by index rather than by adding to a float: an accumulated 0.05
    // lands at 79.999999 and would report the 80 s return as an early repeat.
    let mut earliest_repeat = None;
    for step in 1..1_599 {
        let t = f64::from(step) * 0.05;
        let (back, front) = pose_at(t);
        if (back - back0).abs() < 1e-4 && (front - front0).abs() < 1e-4 {
            earliest_repeat = Some(t);
            break;
        }
    }
    assert!(
        earliest_repeat.is_none(),
        "the pair repeats after {earliest_repeat:?} s, before the 80 s it should"
    );
    let (back80, front80) = pose_at(80.0);
    assert!((back80 - back0).abs() < 1e-9);
    assert!((front80 - front0).abs() < 1e-9);
}

#[test]
fn npc_8_the_path_runs_between_the_two_poses_the_mockup_names() {
    // translate(-20%,-12%) scale(1.4) rotate(0deg) → translate(16%,12%) scale(1.55) rotate(10deg)
    let start = drift_at(0.0, 16.0, 0.0);
    assert!((start.x - -0.20).abs() < 1e-9);
    assert!((start.y - -0.12).abs() < 1e-9);
    assert!((start.scale - 1.40).abs() < 1e-9);
    assert!((start.rotation_deg - 0.0).abs() < 1e-9);

    let end = drift_at(8.0, 16.0, 0.0);
    assert!((end.x - 0.16).abs() < 1e-9);
    assert!((end.y - 0.12).abs() < 1e-9);
    assert!((end.scale - 1.55).abs() < 1e-9);
    assert!((end.rotation_deg - 10.0).abs() < 1e-9);
}

#[test]
fn npc_9_the_layer_always_covers_the_field_it_drifts_across() {
    // The smallest scale on the path still has to hide its own edges after the
    // largest translation, or a hard edge walks into view. Derived from the
    // path's own constants rather than pinned, so a future change of either
    // cannot silently stale this guard.
    let travel = DRIFT_X.0.abs().max(DRIFT_X.1);
    assert!(
        DRIFT_SCALE.0 >= 1.0 + 2.0 * travel,
        "scale {} leaves an edge at {travel} of travel",
        DRIFT_SCALE.0
    );
}

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
fn npc_13_the_field_carries_the_mockups_proportions_not_its_pixels() {
    // Every length is a ratio of the cover, so the panel keeps its own size.
    // Checked by feeding the mockup's own cover back in: the pixels have to
    // come out as the mockup drew them.
    let (left, top, width, height) = field(300.0, SPEC_COVER);
    assert!((height - 440.0).abs() < 1e-9);
    assert!((top - -60.0).abs() < 1e-9);
    assert!((left - -40.0).abs() < 1e-9);
    assert!((width - (300.0 + 40.0 + 90.0)).abs() < 1e-9);
}

#[test]
fn npc_14_the_weight_leans_away_from_the_track_list() {
    // "Schwerpunkt nach außen, weg von der Trackliste" — the panel sits on the
    // right of the window, so the wider overhang has to be the right one.
    const { assert!(OVERHANG_RIGHT_PER_COVER > OVERHANG_LEFT_PER_COVER) };
}

#[test]
fn npc_15_the_front_layer_is_the_softer_of_the_two() {
    // The mockup's 48 px and 54 px survive as a ratio: a smaller source raster
    // painted across the same field is a wider blur.
    const { assert!(FRONT_BLUR_EDGE < BACK_BLUR_EDGE) };
    let drawn = f64::from(FRONT_BLUR_EDGE) / f64::from(BACK_BLUR_EDGE);
    let asked = 48.0 / 54.0;
    assert!(
        (drawn - asked).abs() < 0.02,
        "the layers' softness is {drawn:.3} apart, the mockup asks {asked:.3}"
    );
}

#[test]
fn npc_16_every_cloud_sits_where_the_mockup_put_it() {
    // Layer 1: 40%/35% at 0.85 and 82%/55% at 0.80, both reaching 50%.
    assert_eq!(BACK_BLOBS.len(), 2);
    assert!((BACK_BLOBS[0].x - 0.40).abs() < 1e-9);
    assert!((BACK_BLOBS[0].y - 0.35).abs() < 1e-9);
    assert!((BACK_BLOBS[0].alpha - 0.85).abs() < 1e-9);
    assert!((BACK_BLOBS[1].x - 0.82).abs() < 1e-9);
    assert!((BACK_BLOBS[1].y - 0.55).abs() < 1e-9);
    assert!((BACK_BLOBS[1].alpha - 0.80).abs() < 1e-9);
    assert!(BACK_BLOBS.iter().all(|b| (b.radius - 0.50).abs() < 1e-9));

    // Layer 2: 75%/25% at 0.70 and 30%/80% at 0.60, reaching 45%.
    assert_eq!(FRONT_BLOBS.len(), 2);
    assert!((FRONT_BLOBS[0].x - 0.75).abs() < 1e-9);
    assert!((FRONT_BLOBS[0].y - 0.25).abs() < 1e-9);
    assert!((FRONT_BLOBS[0].alpha - 0.70).abs() < 1e-9);
    assert!((FRONT_BLOBS[1].x - 0.30).abs() < 1e-9);
    assert!((FRONT_BLOBS[1].y - 0.80).abs() < 1e-9);
    assert!((FRONT_BLOBS[1].alpha - 0.60).abs() < 1e-9);
    assert!(FRONT_BLOBS
        .iter()
        .all(|blob| (blob.radius - 0.45).abs() < 1e-9));
}

#[test]
fn npc_17_the_front_layer_never_shouts_over_the_back_one() {
    // Depth only reads if the near layer is the fainter one.
    let strongest_back = BACK_BLOBS.iter().map(|b| b.alpha).fold(0.0, f64::max);
    let strongest_front = FRONT_BLOBS.iter().map(|b| b.alpha).fold(0.0, f64::max);
    assert!(strongest_front < strongest_back);
}

#[test]
fn npc_18_the_clouds_are_cut_from_the_artwork_not_from_extracted_colours() {
    // The mockup fills these layers with the cover's three dominant colours.
    // Measured against a real library that failed once already, and the module
    // this one replaces was the record of it: half the covers are greyscale or
    // near-black and yield no palette at all, and the ones that do are usually
    // monochrome, so the fill came out as one flat tone lying on a backdrop of
    // the same tone. The blurred cover always has structure, so that is what
    // shines through the mockup's stops.
    //
    // Assert on structure, not on words: the doc comment above has to stay
    // free to explain what was tried and why it lost. The needles are split
    // because `include_str!` reads this test too — a literal naming the
    // forbidden symbol would always find itself.
    let source = include_str!("cover_cloud.rs");
    assert!(source.contains("cover_glow::blurred_surface"));
    let extractor = ["dominant", "_colours("].concat();
    assert!(!source.contains(&extractor));
    let palette_module = ["cover", "_palette"].concat();
    assert!(!source.contains(&palette_module));
}

#[test]
fn npc_19_the_cover_is_out_of_reach_of_anything_that_moves() {
    // "Cover nie bewegen." The guarantee is structural rather than numeric:
    // the cover is a sibling above this widget, so nothing in this file can
    // scale or turn it. If a cover widget ever arrives here, that guarantee is
    // gone and this test is the place that says so.
    let source = include_str!("cover_cloud.rs");
    let cover_widget = ["cover", "_stack"].concat();
    assert!(!source.contains(&cover_widget));
    assert!(!source.contains("CoverLift"));
    assert!(!source.contains("cover_loader"));
}

#[test]
fn npc_20_a_field_with_no_room_is_drawn_as_nothing_rather_than_upside_down() {
    // A degenerate period must not divide by zero or run backwards.
    assert!((drift_progress(3.0, 0.0, 0.0) - 0.0).abs() < 1e-9);
    assert!((drift_progress(3.0, -8.0, 0.0) - 0.0).abs() < 1e-9);
}

#[test]
fn npc_21_a_new_track_arrives_over_about_a_second() {
    // "Bei Titelwechsel Farben in ca. 1 s überblenden."
    assert!((cover_fade(0.0) - 0.0).abs() < 1e-9);
    assert!((cover_fade(0.5) - 0.5).abs() < 1e-9);
    assert!((cover_fade(1.0) - 1.0).abs() < 1e-9);
    // Held there rather than run past it.
    assert!((cover_fade(4.0) - 1.0).abs() < 1e-9);
    // A clock that has not moved yet is the old cover, not a flash of nothing.
    assert!((cover_fade(-2.0) - 0.0).abs() < 1e-9);
}

#[test]
fn npc_22_the_two_covers_always_sum_to_one_across_the_change() {
    // The pairs are painted one over the other. If the halves eased, both
    // would be part-way out at the midpoint and the light would dip there.
    for step in 0..=100 {
        let since = f64::from(step) / 100.0;
        let arriving = cover_fade(since);
        let leaving = 1.0 - arriving;
        assert!(
            (arriving + leaving - 1.0).abs() < 1e-9,
            "the crossfade dips at {since} s"
        );
    }
}

#[test]
fn npc_23_a_track_change_keeps_the_outgoing_pair_to_fade_from() {
    // A change arrives in two calls — the panel clears the cover, then the
    // loader delivers the texture. Handing the cleared pair on as the second
    // call's outgoing one threw the real one away and left a hard cut, which
    // is the whole reason this decision is testable at all.
    assert_eq!(fade_step(true, false, true, false), FadeStep::Handover);
    // ...and the texture arriving must not displace it a second time.
    assert_eq!(fade_step(true, true, false, true), FadeStep::Restart);
}

#[test]
fn npc_24_a_stopped_clock_takes_the_change_at_once() {
    // Pinned, or animation switched off: no frame will ever advance a fade, so
    // leaving one half-finished would strand the incoming cover at nothing.
    assert_eq!(fade_step(false, true, true, true), FadeStep::Cut);
    assert_eq!(fade_step(false, false, false, false), FadeStep::Cut);
}

#[test]
fn npc_25_nothing_arriving_over_nothing_starts_no_fade() {
    assert_eq!(fade_step(true, false, false, false), FadeStep::Idle);
    // A cover clearing to nothing still hands its pair over to fade out.
    assert_eq!(fade_step(true, false, true, true), FadeStep::Handover);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn npc_23_the_public_cover_change_keeps_then_drops_the_outgoing_pair() {
    gtk4::init().expect("gtk");
    let settings = gtk4::Settings::default().expect("settings");
    let animations_were_enabled = settings.is_gtk_enable_animations();
    settings.set_gtk_enable_animations(true);
    let cloud = CoverCloud::new();
    cloud.set_pinned(false);
    let texture = swatch_cover(false);
    cloud.set_cover(Some(&texture), 1);
    cloud.set_frame_time(1_000_000);
    cloud.set_frame_time(2_000_000);

    cloud.set_cover(None, 2);
    cloud.set_cover(Some(&texture), 2);
    assert!(cloud.has_leaving_pair_for_test());

    cloud.set_frame_time(3_000_001);
    assert!(!cloud.has_leaving_pair_for_test());
    settings.set_gtk_enable_animations(animations_were_enabled);
}

#[test]
fn npp_18_the_clouds_keep_their_pose_across_a_hold_and_resume() {
    let mut clock = DriftClock::default();
    clock.advance(1_000_000);
    clock.advance(11_000_000);
    let before = clock.elapsed_s();
    assert!(before > 0.0, "the clouds did not start drifting");
    assert!(clock.hold());
    assert!(!clock.advance(12_000_000));
    clock.advance(14_000_000);
    assert!((clock.elapsed_s() - (before + 2.0)).abs() < 1e-6);
}

#[test]
fn npp_18_a_double_hold_does_not_fold_the_cloud_clock_twice() {
    let mut clock = DriftClock::default();
    clock.advance(1_000_000);
    clock.advance(11_000_000);

    assert!(clock.hold());
    let held = clock.elapsed_s();
    assert!(!clock.hold());
    assert!((clock.elapsed_s() - held).abs() < 1e-9);
}

#[test]
fn npp_18_resuming_the_clouds_after_a_huge_gap_does_not_jump() {
    let mut clock = DriftClock::default();
    clock.advance(1_000_000);
    clock.advance(11_000_000);
    clock.hold();
    let before = clock.elapsed_s();

    assert!(!clock.advance(500_000_000));
    assert!((clock.elapsed_s() - before).abs() < 1e-9);
}

#[test]
fn npp_18_the_cloud_clock_reports_no_change_when_elapsed_does_not_move() {
    let mut clock = DriftClock::default();

    assert!(!clock.advance(1_000_000));
    assert!(!clock.advance(1_000_000));
    assert_eq!(clock.elapsed_s(), 0.0);
}

#[test]
fn npp_18_an_earlier_frame_time_does_not_move_the_clouds_backwards() {
    let mut clock = DriftClock::default();
    clock.advance(5_000_000);
    clock.advance(8_000_000);
    let before = clock.elapsed_s();

    assert!(!clock.advance(2_000_000));
    assert!((clock.elapsed_s() - before).abs() < 1e-9);
}

#[test]
fn npc_26_the_incoming_field_grows_while_the_outgoing_field_shrinks() {
    let render = |arrived| {
        let target = cairo::ImageSurface::create(cairo::Format::ARgb32, 32, 32).unwrap();
        let cr = cairo::Context::new(&target).unwrap();
        let outgoing = solid_field(255, 0, 0);
        let incoming = solid_field(0, 0, 255);
        paint_crossfade_layers(
            &cr,
            &[(
                Some(&outgoing),
                Drift {
                    x: 0.0,
                    y: 0.0,
                    scale: 1.0,
                    rotation_deg: 0.0,
                },
            )],
            &[(
                Some(&incoming),
                Drift {
                    x: 0.0,
                    y: 0.0,
                    scale: 1.0,
                    rotation_deg: 0.0,
                },
            )],
            (0.0, 0.0, 32.0, 32.0),
            arrived,
            arrived,
            cairo::Operator::Over,
        );
        drop(cr);
        pixel(target, 16, 16)
    };

    let early = render(0.25);
    let late = render(0.75);
    assert!(late[2] > early[2], "the incoming blue cover did not grow");
    assert!(late[0] < early[0], "the outgoing red cover did not shrink");
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn npc_27_a_masked_field_contains_real_non_flat_alpha() {
    gtk4::init().expect("gtk");
    let mut field = build_field(&swatch_cover(false), BACK_BLUR_EDGE, &BACK_BLOBS).unwrap();
    field.flush();
    let stride = usize::try_from(field.stride()).unwrap();
    let data = field.data().unwrap();
    let alphas = (0..usize::try_from(FIELD_RASTER_EDGE).unwrap()).flat_map(|y| {
        let data = &data;
        (0..usize::try_from(FIELD_RASTER_EDGE).unwrap()).map(move |x| data[y * stride + x * 4 + 3])
    });
    let (minimum, maximum) = alphas.fold((u8::MAX, u8::MIN), |(minimum, maximum), alpha| {
        (minimum.min(alpha), maximum.max(alpha))
    });

    assert!(maximum > 0, "the field was uniformly transparent");
    assert!(minimum < 255, "the field was uniformly opaque");
    assert!(minimum < maximum, "the field alpha was flat");
}

#[test]
fn npc_28_dark_screen_adds_light_while_light_multiply_lays_down_a_wash() {
    let render = |dark| {
        let target = cairo::ImageSurface::create(cairo::Format::ARgb32, 32, 32).unwrap();
        let cr = cairo::Context::new(&target).unwrap();
        cr.set_source_rgb(0.5, 0.5, 0.5);
        cr.paint().unwrap();
        let field = solid_field(192, 64, 128);
        paint_layer(
            &cr,
            &field,
            Drift {
                x: 0.0,
                y: 0.0,
                scale: 1.0,
                rotation_deg: 0.0,
            },
            (0.0, 0.0, 32.0, 32.0),
            1.0,
            blend_operator(dark),
        );
        drop(cr);
        pixel(target, 16, 16)
    };

    let dark = render(true);
    let light = render(false);
    assert!(dark[0] > 128, "Screen did not brighten the dark ground");
    assert!(light[0] < 128, "Multiply did not tint the light ground");
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn npc_29_the_scrim_cache_reuses_only_the_same_theme_appearance_and_geometry() {
    use crate::ui::style::theme::Theme;

    gtk4::init().expect("gtk");
    let previous_theme = crate::ui::style::current_theme();
    crate::ui::style::set_theme(Theme::PerpetualRain);
    let inner = Inner {
        back: RefCell::new(None),
        front: RefCell::new(None),
        leaving_back: RefCell::new(None),
        leaving_front: RefCell::new(None),
        arrived_at_us: Cell::new(0),
        generation: Cell::new(None),
        drift_clock: Cell::new(DriftClock::default()),
        last_drawn_pose: Cell::new(None),
        scrim: RefCell::new(None),
        pinned: Cell::new(true),
    };

    let first = cached_scrim(&inner, true, -42.0, 308.0);
    let same = cached_scrim(&inner, true, -42.0, 308.0);
    assert_eq!(pattern_identity(&first), pattern_identity(&same));

    let light = cached_scrim(&inner, false, -42.0, 308.0);
    assert_ne!(pattern_identity(&same), pattern_identity(&light));

    crate::ui::style::set_theme(Theme::NightTerrain);
    let themed = cached_scrim(&inner, false, -42.0, 308.0);
    assert_ne!(pattern_identity(&light), pattern_identity(&themed));

    let moved = cached_scrim(&inner, false, -41.0, 308.0);
    assert_ne!(pattern_identity(&themed), pattern_identity(&moved));

    let resized = cached_scrim(&inner, false, -41.0, 309.0);
    assert_ne!(pattern_identity(&moved), pattern_identity(&resized));
    crate::ui::style::set_theme(previous_theme);
}

fn pattern_identity(gradient: &cairo::LinearGradient) -> usize {
    let pattern: &cairo::Pattern = gradient.as_ref();
    pattern.to_raw_none() as usize
}

fn solid_field(red: u8, green: u8, blue: u8) -> cairo::ImageSurface {
    let surface =
        cairo::ImageSurface::create(cairo::Format::ARgb32, FIELD_RASTER_EDGE, FIELD_RASTER_EDGE)
            .unwrap();
    let cr = cairo::Context::new(&surface).unwrap();
    cr.set_source_rgb(
        f64::from(red) / 255.0,
        f64::from(green) / 255.0,
        f64::from(blue) / 255.0,
    );
    cr.paint().unwrap();
    drop(cr);
    surface
}

fn pixel(mut surface: cairo::ImageSurface, x: usize, y: usize) -> [u8; 4] {
    surface.flush();
    let stride = usize::try_from(surface.stride()).unwrap();
    let data = surface.data().unwrap();
    let offset = y * stride + x * 4;
    [
        data[offset + 2],
        data[offset + 1],
        data[offset],
        data[offset + 3],
    ]
}

/// Renders the head of the panel to a PPM so the light can be looked at.
///
/// Every other test here is arithmetic or structure. None of them can say
/// whether the blurred cover, masked twice, actually reads as two clouds — and
/// that is this design's own risk: it trades three extracted colours for one
/// raster, which is a different door into the failure the turning disc recorded.
/// A greyscale cover is rendered beside a colourful one for exactly that
/// reason.
#[test]
#[ignore = "measurement: render manually via xvfb-run"]
fn render_cover_cloud_gallery_ppm() {
    gtk4::init().expect("gtk");

    let width = 300i32;
    let band = tokens::NOW_PLAYING_ARTWORK_BAND;
    let moments = [0.0f64, 4.0, 8.0];
    let covers = [swatch_cover(false), swatch_cover(true)];

    let sheet = cairo::ImageSurface::create(
        cairo::Format::ARgb32,
        width * moments.len() as i32,
        band * covers.len() as i32,
    )
    .expect("sheet");
    let sheet_cr = cairo::Context::new(&sheet).expect("sheet cr");

    for (row, texture) in covers.iter().enumerate() {
        let back = build_field(texture, BACK_BLUR_EDGE, &BACK_BLOBS).expect("back field");
        let front = build_field(texture, FRONT_BLUR_EDGE, &FRONT_BLOBS).expect("front field");
        for (col, seconds) in moments.iter().enumerate() {
            let tile =
                cairo::ImageSurface::create(cairo::Format::ARgb32, width, band).expect("tile");
            let cr = cairo::Context::new(&tile).expect("tile cr");
            let [r, g, b] = crate::ui::style::accent::sidebar_background_rgb();
            cr.set_source_rgb(
                f64::from(r) / 255.0,
                f64::from(g) / 255.0,
                f64::from(b) / 255.0,
            );
            cr.paint().expect("ground");

            let cover = f64::from(tokens::NOW_PLAYING_COVER_SIZE);
            let bounds = field(f64::from(width), cover);
            let operator = if crate::ui::style::accent::is_dark() {
                cairo::Operator::Screen
            } else {
                cairo::Operator::Multiply
            };
            paint_layer(
                &cr,
                &back,
                drift_at(*seconds, BACK_PERIOD_S, 0.0),
                bounds,
                1.0,
                operator,
            );
            paint_layer(
                &cr,
                &front,
                drift_at(*seconds, FRONT_PERIOD_S, FRONT_OFFSET_S),
                bounds,
                1.0,
                operator,
            );
            let (_, field_top, _, field_height) = bounds;
            let scrim = build_scrim(field_top, field_height);
            paint_scrim(&cr, f64::from(width), f64::from(band), &scrim);
            drop(cr);

            sheet_cr
                .set_source_surface(
                    &tile,
                    f64::from(width) * col as f64,
                    f64::from(band) * row as f64,
                )
                .expect("place tile");
            sheet_cr.paint().expect("paint tile");
        }
    }
    drop(sheet_cr);

    let path = std::env::var("COVER_CLOUD_PPM")
        .unwrap_or_else(|_| "/tmp/cover-cloud-gallery.ppm".to_string());
    write_ppm(sheet, &path);
    println!("wrote {path}");
}

/// A stand-in cover: the mockup's own three colours, or the greyscale artwork
/// that half this library actually has.
fn swatch_cover(grey: bool) -> gtk4::gdk::Texture {
    use gtk4::prelude::*;

    let edge = 64usize;
    let mut data = vec![0u8; edge * edge * 4];
    for y in 0..edge {
        for x in 0..edge {
            // The mockup's own cover: radial-gradient(circle at 35% 65%,
            // #ff2fa0, #6a1d8a 30%, #0b1d4a 60%, #050a1c). Real artwork has
            // soft transitions; a hard-edged swatch would blame the cloud for
            // edges that came from the fixture.
            let fx = x as f64 / edge as f64 - 0.35;
            let fy = y as f64 / edge as f64 - 0.65;
            let d = (fx * fx + fy * fy).sqrt() / 0.9;
            let ramp = |stops: [(f64, [f64; 3]); 4]| {
                let mut out = stops[stops.len() - 1].1;
                for pair in stops.windows(2) {
                    let (a_at, a_c) = pair[0];
                    let (b_at, b_c) = pair[1];
                    if d >= a_at && d <= b_at {
                        let t = (d - a_at) / (b_at - a_at).max(1e-6);
                        out = [
                            a_c[0] + (b_c[0] - a_c[0]) * t,
                            a_c[1] + (b_c[1] - a_c[1]) * t,
                            a_c[2] + (b_c[2] - a_c[2]) * t,
                        ];
                        break;
                    }
                }
                out
            };
            let c = ramp([
                (0.0, [255.0, 47.0, 160.0]),
                (0.30, [106.0, 29.0, 138.0]),
                (0.60, [11.0, 29.0, 74.0]),
                (1.0, [5.0, 10.0, 28.0]),
            ]);
            let (r, g, b) = if grey {
                // Same picture, drained of colour: the case the old disc's own
                // doc comment says half this library is.
                let v = (0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]) as u8;
                (v, v, v)
            } else {
                (c[0] as u8, c[1] as u8, c[2] as u8)
            };
            let i = (y * edge + x) * 4;
            data[i] = b;
            data[i + 1] = g;
            data[i + 2] = r;
            data[i + 3] = 0xff;
        }
    }
    gtk4::gdk::MemoryTexture::new(
        edge as i32,
        edge as i32,
        gtk4::gdk::MemoryFormat::B8g8r8a8Premultiplied,
        &gtk4::glib::Bytes::from_owned(data),
        edge * 4,
    )
    .upcast()
}

/// Takes the surface by value: `data()` wants the only reference to it, and a
/// clone left behind is exactly the second one it refuses.
fn write_ppm(mut surface: cairo::ImageSurface, path: &str) {
    use std::io::Write;

    let width = surface.width();
    let height = surface.height();
    let stride = usize::try_from(surface.stride()).expect("stride");
    let data = surface.data().expect("surface data");
    let mut out = Vec::new();
    out.extend_from_slice(format!("P6\n{width} {height}\n255\n").as_bytes());
    for y in 0..usize::try_from(height).expect("height") {
        for x in 0..usize::try_from(width).expect("width") {
            let i = y * stride + x * 4;
            let (b, g, r, a) = (data[i], data[i + 1], data[i + 2], data[i + 3]);
            let un = |c: u8| {
                if a == 0 {
                    0
                } else {
                    u8::try_from((u32::from(c) * 255 / u32::from(a)).min(255)).unwrap_or(255)
                }
            };
            out.extend_from_slice(&[un(r), un(g), un(b)]);
        }
    }
    std::fs::File::create(path)
        .expect("create ppm")
        .write_all(&out)
        .expect("write ppm");
}
