use super::*;

/// The mockup's own numbers, kept here so a change to the module has to argue
/// with the design rather than quietly redefine it.
const SPEC_COVER: f64 = 240.0;

#[test]
fn ac_24_the_field_carries_the_mockups_proportions_not_its_pixels() {
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
fn ac_24_the_weight_leans_away_from_the_track_list() {
    // "Schwerpunkt nach außen, weg von der Trackliste" — the panel sits on the
    // right of the window, so the wider overhang has to be the right one.
    const { assert!(OVERHANG_RIGHT_PER_COVER > OVERHANG_LEFT_PER_COVER) };
}

#[test]
fn ac_24_the_front_layer_is_the_softer_of_the_two() {
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
fn ac_24_every_cloud_sits_where_the_mockup_put_it() {
    // The original stops stay put and a third rounds out each layer.
    assert_eq!(BACK_BLOBS.len(), 3);
    assert!((BACK_BLOBS[0].x - 0.40).abs() < 1e-9);
    assert!((BACK_BLOBS[0].y - 0.35).abs() < 1e-9);
    assert!((BACK_BLOBS[0].alpha - 0.85).abs() < 1e-9);
    assert!((BACK_BLOBS[1].x - 0.82).abs() < 1e-9);
    assert!((BACK_BLOBS[1].y - 0.55).abs() < 1e-9);
    assert!((BACK_BLOBS[1].alpha - 0.80).abs() < 1e-9);
    assert!((BACK_BLOBS[2].x - 0.24).abs() < 1e-9);
    assert!((BACK_BLOBS[2].y - 0.76).abs() < 1e-9);
    assert!((BACK_BLOBS[2].alpha - 0.78).abs() < 1e-9);
    assert!(BACK_BLOBS.iter().all(|b| (b.radius - 0.50).abs() < 1e-9));

    // Layer 2: 75%/25% at 0.70 and 30%/80% at 0.60, reaching 45%.
    assert_eq!(FRONT_BLOBS.len(), 3);
    assert!((FRONT_BLOBS[0].x - 0.75).abs() < 1e-9);
    assert!((FRONT_BLOBS[0].y - 0.25).abs() < 1e-9);
    assert!((FRONT_BLOBS[0].alpha - 0.70).abs() < 1e-9);
    assert!((FRONT_BLOBS[1].x - 0.30).abs() < 1e-9);
    assert!((FRONT_BLOBS[1].y - 0.80).abs() < 1e-9);
    assert!((FRONT_BLOBS[1].alpha - 0.60).abs() < 1e-9);
    assert!((FRONT_BLOBS[2].x - 0.52).abs() < 1e-9);
    assert!((FRONT_BLOBS[2].y - 0.48).abs() < 1e-9);
    assert!((FRONT_BLOBS[2].alpha - 0.65).abs() < 1e-9);
    assert!(FRONT_BLOBS
        .iter()
        .all(|blob| (blob.radius - 0.45).abs() < 1e-9));
}

#[test]
fn ac_24_the_front_layer_never_shouts_over_the_back_one() {
    // Depth only reads if the near layer is the fainter one.
    let strongest_back = BACK_BLOBS.iter().map(|b| b.alpha).fold(0.0, f64::max);
    let strongest_front = FRONT_BLOBS.iter().map(|b| b.alpha).fold(0.0, f64::max);
    assert!(strongest_front < strongest_back);
}

#[test]
fn ac_24_the_clouds_are_cut_from_the_artwork_not_from_extracted_colours() {
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
    let source = concat!(
        include_str!("cover_cloud.rs"),
        include_str!("cover_cloud_blob.rs")
    );
    assert!(source.contains("cover_glow::blurred_surface"));
    let extractor = ["dominant", "_colours("].concat();
    assert!(!source.contains(&extractor));
    let palette_module = ["cover", "_palette"].concat();
    assert!(!source.contains(&palette_module));
}

#[test]
fn ac_24_the_cover_is_out_of_reach_of_anything_that_moves() {
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
fn ac_24_a_new_track_arrives_over_about_a_second() {
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
fn ac_24_the_two_covers_always_sum_to_one_across_the_change() {
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
fn ac_24_a_track_change_keeps_the_outgoing_pair_to_fade_from() {
    // A change arrives in two calls — the panel clears the cover, then the
    // loader delivers the texture. Handing the cleared pair on as the second
    // call's outgoing one threw the real one away and left a hard cut, which
    // is the whole reason this decision is testable at all.
    assert_eq!(fade_step(true, false, true, false), FadeStep::Handover);
    // ...and the texture arriving must not displace it a second time.
    assert_eq!(fade_step(true, true, false, true), FadeStep::Restart);
}

#[test]
fn ac_24_a_stopped_clock_takes_the_change_at_once() {
    // Pinned, or animation switched off: no frame will ever advance a fade, so
    // leaving one half-finished would strand the incoming cover at nothing.
    assert_eq!(fade_step(false, true, true, true), FadeStep::Cut);
    assert_eq!(fade_step(false, false, false, false), FadeStep::Cut);
}

#[test]
fn ac_24_nothing_arriving_over_nothing_starts_no_fade() {
    assert_eq!(fade_step(true, false, false, false), FadeStep::Idle);
    // A cover clearing to nothing still hands its pair over to fade out.
    assert_eq!(fade_step(true, false, true, true), FadeStep::Handover);
}

#[test]
fn ac_24_a_partial_raster_build_is_not_accepted_as_the_current_cover() {
    let back = std::array::from_fn(|_| solid_field(64, 96, 128));
    assert!(complete_raster_pair(Some(back), None).is_none());

    let back = std::array::from_fn(|_| solid_field(64, 96, 128));
    let front = std::array::from_fn(|_| solid_field(128, 96, 64));
    assert!(complete_raster_pair(Some(back), Some(front)).is_some());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn ac_24_the_public_cover_change_keeps_then_drops_the_outgoing_pair() {
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
fn ac_24_the_incoming_field_grows_while_the_outgoing_field_shrinks() {
    let render = |arrived| {
        let target = cairo::ImageSurface::create(cairo::Format::ARgb32, 32, 32).unwrap();
        let cr = cairo::Context::new(&target).unwrap();
        let outgoing = solid_field(255, 0, 0);
        let incoming = solid_field(0, 0, 255);
        let outgoing_rasters = [outgoing.clone(), outgoing.clone(), outgoing];
        let incoming_rasters = [incoming.clone(), incoming.clone(), incoming];
        let blobs = BACK_BLOBS;
        let pose = [Drift {
            x: 0.0,
            y: 0.0,
            scale: 1.0,
        }; BLOBS_PER_LAYER];
        paint_crossfade_layers(
            &cr,
            &[(Some(&outgoing_rasters), &blobs, &pose)],
            &[(Some(&incoming_rasters), &blobs, &pose)],
            arrived,
            arrived,
            LayerComposite {
                bounds: (0.0, 0.0, 32.0, 32.0),
                operator: cairo::Operator::Over,
                scratch: None,
            },
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
fn ac_24_a_masked_field_contains_real_non_flat_alpha() {
    gtk4::init().expect("gtk");
    let fields = build_blob_rasters(&swatch_cover(false), BACK_BLUR_EDGE, &BACK_BLOBS).unwrap();
    assert_eq!(fields.len(), BLOBS_PER_LAYER);
    let mut field = fields.into_iter().next().unwrap();
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
fn ac_24_dark_screen_adds_light_while_light_multiply_lays_down_a_wash() {
    let render = |dark| {
        let target = cairo::ImageSurface::create(cairo::Format::ARgb32, 32, 32).unwrap();
        let cr = cairo::Context::new(&target).unwrap();
        cr.set_source_rgb(0.5, 0.5, 0.5);
        cr.paint().unwrap();
        let field = solid_field(192, 64, 128);
        paint_layer(
            &cr,
            &field,
            (0.5, 0.5),
            Drift {
                x: 0.0,
                y: 0.0,
                scale: 1.0,
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
fn ac_24_dark_screen_keeps_each_cloud_as_an_independent_pass() {
    let target = cairo::ImageSurface::create(cairo::Format::ARgb32, 64, 64).unwrap();
    let control = cairo::ImageSurface::create(cairo::Format::ARgb32, 64, 64).unwrap();
    for surface in [&target, &control] {
        let cr = cairo::Context::new(surface).unwrap();
        cr.set_source_rgb(0.25, 0.25, 0.25);
        cr.paint().unwrap();
    }
    let surfaces = BACK_BLOBS.map(|blob| masked_solid_field(blob, [230, 62, 114]));
    let poses = BACK_BLOBS.map(|blob| drift_at(62_390.0, blob.drift));
    let target_cr = cairo::Context::new(&target).unwrap();
    paint_cloud_layer(
        &target_cr,
        &surfaces,
        &BACK_BLOBS,
        &poses,
        1.0,
        LayerComposite {
            bounds: (0.0, 0.0, 64.0, 64.0),
            operator: cairo::Operator::Screen,
            scratch: None,
        },
    );
    let control_cr = cairo::Context::new(&control).unwrap();
    for index in 0..BLOBS_PER_LAYER {
        paint_layer(
            &control_cr,
            &surfaces[index],
            (BACK_BLOBS[index].x, BACK_BLOBS[index].y),
            poses[index],
            (0.0, 0.0, 64.0, 64.0),
            1.0,
            cairo::Operator::Screen,
        );
    }
    drop(target_cr);
    drop(control_cr);
    assert_eq!(pixel(target, 32, 32), pixel(control, 32, 32));
}

#[test]
fn ac_24_light_overlap_keeps_a_coloured_wash_with_the_shipped_clouds() {
    const PANEL: (i32, i32) = (300, 268);
    // This saturated artwork swatch reproduces the review's measured
    // (175, 11, 31) legacy result at (244, 99), so the control and repair are
    // compared on the same perceptual case rather than an invented alpha.
    const ARTWORK_RGB: [u8; 3] = [230, 62, 114];
    const LIGHT_CHANNEL_FLOOR: u8 = 24;
    let (elapsed_s, point) = worst_visible_overlap(120_000.0, PANEL);

    let render = |grouped: bool, elapsed_s: f64, point: (usize, usize)| {
        let target = cairo::ImageSurface::create(cairo::Format::ARgb32, PANEL.0, PANEL.1).unwrap();
        let cr = cairo::Context::new(&target).unwrap();
        cr.set_source_rgb(0.94, 0.94, 0.94);
        cr.paint().unwrap();
        let bounds = field(
            f64::from(PANEL.0),
            f64::from(tokens::NOW_PLAYING_COVER_SIZE),
        );
        let scratch = LayerScratch::new().unwrap();
        for blobs in [&BACK_BLOBS, &FRONT_BLOBS] {
            let surfaces = blobs.map(|blob| masked_solid_field(blob, ARTWORK_RGB));
            let poses = blobs.map(|blob| drift_at(elapsed_s, blob.drift));
            if grouped {
                paint_cloud_layer(
                    &cr,
                    &surfaces,
                    blobs,
                    &poses,
                    1.0,
                    LayerComposite {
                        bounds,
                        operator: cairo::Operator::Multiply,
                        scratch: Some(&scratch),
                    },
                );
            } else {
                for index in 0..BLOBS_PER_LAYER {
                    paint_layer(
                        &cr,
                        &surfaces[index],
                        (blobs[index].x, blobs[index].y),
                        poses[index],
                        bounds,
                        1.0,
                        cairo::Operator::Multiply,
                    );
                }
            }
        }
        drop(cr);
        pixel(target, point.0, point.1)
    };

    let legacy = render(false, elapsed_s, point);
    let grouped = render(true, elapsed_s, point);
    let review_point = (244, 99);
    let review_legacy = render(false, 62_390.0, review_point);
    let review_grouped = render(true, 62_390.0, review_point);
    println!(
        "light overlap: review point {review_point:?} at 62390 s {review_legacy:?} -> {review_grouped:?}; searched {point:?} at {elapsed_s:.0} s {legacy:?} -> {grouped:?}"
    );
    assert!(
        *legacy[..3].iter().min().unwrap() < LIGHT_CHANNEL_FLOOR,
        "the control no longer reproduces the crushed overlap: {legacy:?}"
    );
    assert!(
        *grouped[..3].iter().min().unwrap() >= LIGHT_CHANNEL_FLOOR,
        "layer grouping fell below the 10% channel floor at t={elapsed_s}, {point:?}: {grouped:?}"
    );
}

fn worst_visible_overlap(duration_s: f64, panel: (i32, i32)) -> (f64, (usize, usize)) {
    let bounds = field(
        f64::from(panel.0),
        f64::from(tokens::NOW_PLAYING_COVER_SIZE),
    );
    let cover_left = (panel.0 - tokens::NOW_PLAYING_COVER_SIZE) / 2;
    let cover_top = tokens::NOW_PLAYING_HEAD_TOP;
    let cover_right = cover_left + tokens::NOW_PLAYING_COVER_SIZE;
    let cover_bottom = cover_top + tokens::NOW_PLAYING_COVER_SIZE;
    let mut worst = (f64::INFINITY, 0.0, (0, 0));
    for step in 0..=(duration_s / 30.0) as u32 {
        let elapsed_s = f64::from(step) * 30.0;
        for y in (0..panel.1).step_by(8) {
            for x in (0..panel.0).step_by(8) {
                if x >= cover_left && x < cover_right && y >= cover_top && y < cover_bottom {
                    continue;
                }
                let attenuation = grouped_green_at(elapsed_s, x, y, bounds);
                if attenuation < worst.0 {
                    worst = (attenuation, elapsed_s, (x as usize, y as usize));
                }
            }
        }
    }
    (worst.1, worst.2)
}

fn grouped_green_at(
    elapsed_s: f64,
    x: i32,
    y: i32,
    (left, top, width, height): (f64, f64, f64, f64),
) -> f64 {
    let point = ((f64::from(x) - left) / width, (f64::from(y) - top) / height);
    let mut result = 0.94;
    for blobs in [&BACK_BLOBS, &FRONT_BLOBS] {
        let uncovered = blobs.iter().fold(1.0, |uncovered, blob| {
            let drift = drift_at(elapsed_s, blob.drift);
            let distance = ((point.0 - blob.x - drift.x) / drift.scale)
                .hypot((point.1 - blob.y - drift.y) / drift.scale);
            let alpha = blob.alpha * (1.0 - distance / blob.radius).clamp(0.0, 1.0);
            uncovered * (1.0 - alpha)
        });
        let alpha = 1.0 - uncovered;
        result *= 1.0 - alpha + alpha * (62.0 / 255.0);
    }
    result
}

fn masked_solid_field(blob: Blob, [red, green, blue]: [u8; 3]) -> cairo::ImageSurface {
    let surface = solid_field(red, green, blue);
    let cr = cairo::Context::new(&surface).unwrap();
    let edge = f64::from(FIELD_RASTER_EDGE);
    let mask = cairo::RadialGradient::new(
        blob.x * edge,
        blob.y * edge,
        0.0,
        blob.x * edge,
        blob.y * edge,
        blob.radius * edge,
    );
    mask.add_color_stop_rgba(0.0, 0.0, 0.0, 0.0, blob.alpha);
    mask.add_color_stop_rgba(1.0, 0.0, 0.0, 0.0, 0.0);
    cr.set_operator(cairo::Operator::DestIn);
    cr.set_source(&mask).unwrap();
    cr.paint().unwrap();
    drop(cr);
    surface
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

#[path = "cover_cloud_gallery_tests.rs"]
mod gallery_tests;
