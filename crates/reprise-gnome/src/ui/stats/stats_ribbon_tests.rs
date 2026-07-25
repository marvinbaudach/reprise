use gtk4::cairo::{Format, ImageSurface};
use gtk4::prelude::*;

use super::*;

fn pixel_has_ink(surface: &mut ImageSurface, x: i32, y: i32) -> bool {
    surface.flush();
    let stride = surface.stride() as usize;
    let data = surface.data().unwrap();
    let offset = y as usize * stride + x as usize * 4;
    u32::from_ne_bytes(data[offset..offset + 4].try_into().unwrap()) >> 24 != 0
}

fn pixel_red(surface: &mut ImageSurface, x: i32, y: i32) -> u8 {
    surface.flush();
    let stride = surface.stride() as usize;
    let data = surface.data().unwrap();
    data[y as usize * stride + x as usize * 4 + 2]
}

fn rgba(red: f32, green: f32, blue: f32, alpha: f32) -> gtk4::gdk::RGBA {
    gtk4::gdk::RGBA::new(red, green, blue, alpha)
}

#[test]
fn open_marker_starts_a_fresh_cairo_path() {
    let mut surface = ImageSurface::create(Format::ARgb32, 100, 100).unwrap();
    {
        let context = gtk4::cairo::Context::new(&surface).unwrap();
        context.move_to(0.0, 0.0);
        let layout = RibbonLayout {
            points: vec![Point { x: 80.0, y: 80.0 }],
            open_index: Some(0),
        };

        draw_open_marker(&context, &layout, 1.0, 1.0, 1.0, 1.0);
    }

    assert!(
        !(38..=42).any(|x| (38..=42).any(|y| pixel_has_ink(&mut surface, x, y))),
        "the marker must not stroke a line from Cairo's previous current point"
    );
}

#[test]
fn one_bucket_draws_a_full_width_fill_and_line() {
    let layout = RibbonLayout {
        points: vec![Point { x: 50.0, y: 25.0 }],
        open_index: None,
    };
    let mut fill_surface = ImageSurface::create(Format::ARgb32, 100, 100).unwrap();
    {
        let context = gtk4::cairo::Context::new(&fill_surface).unwrap();
        draw_fill(&context, &layout, 100.0, 90.0, 1.0, 1.0, 1.0, 1.0);
    }
    let mut line_surface = ImageSurface::create(Format::ARgb32, 100, 100).unwrap();
    {
        let context = gtk4::cairo::Context::new(&line_surface).unwrap();
        draw_line(&context, &layout, 100.0, 1.0, 1.0, 1.0, 1.0);
    }

    assert!(
        pixel_has_ink(&mut fill_surface, 10, 50),
        "the fill must extend left of the sole bucket point"
    );
    assert!(
        pixel_has_ink(&mut line_surface, 10, 25),
        "the sole bucket must render as a line, not an invisible move-to"
    );
}

#[test]
fn sparse_week_bars_render_zero_ticks_on_a_continuous_baseline() {
    let layout = bar_layout(&[10, 0, 20], 90.0, 90.0, None);
    let mut surface = ImageSurface::create(Format::ARgb32, 90, 100).unwrap();
    {
        let context = gtk4::cairo::Context::new(&surface).unwrap();
        draw_bars(
            &context,
            &layout,
            90.0,
            90.0,
            None,
            BarColors {
                standard: rgba(1.0, 1.0, 1.0, 1.0),
                best: rgba(1.0, 1.0, 1.0, 1.0),
                baseline: rgba(1.0, 1.0, 1.0, 0.18),
            },
            1.0,
        );
    }

    assert!(pixel_has_ink(&mut surface, 15, 80));
    assert!(
        pixel_has_ink(&mut surface, 45, 88),
        "a zero-play week must retain a 2 px baseline tick"
    );
    assert!(
        pixel_has_ink(&mut surface, 30, 89),
        "a 1 px baseline must connect every weekly slot"
    );
    assert!(pixel_has_ink(&mut surface, 75, 40));
}

#[test]
fn best_week_bar_uses_the_lighter_accent_step() {
    let layout = bar_layout(&[20, 20, 20], 90.0, 90.0, None);
    let mut surface = ImageSurface::create(Format::ARgb32, 90, 100).unwrap();
    {
        let context = gtk4::cairo::Context::new(&surface).unwrap();
        draw_bars(
            &context,
            &layout,
            90.0,
            90.0,
            Some(1),
            BarColors {
                standard: rgba(0.2, 0.0, 0.0, 1.0),
                best: rgba(0.8, 0.0, 0.0, 1.0),
                baseline: rgba(0.0, 0.0, 0.0, 0.0),
            },
            1.0,
        );
    }

    assert!(
        pixel_red(&mut surface, 45, 50) > pixel_red(&mut surface, 15, 50),
        "the best-week bar must be one lighter accent step"
    );
}

#[test]
fn best_week_label_is_centered_above_its_bar_with_edge_clearance() {
    let surface = ImageSurface::create(Format::ARgb32, 400, 100).unwrap();
    let context = gtk4::cairo::Context::new(&surface).unwrap();
    let copy = "best week · 6 h 58";
    let marker_x = 300.0;

    let x = best_week_label_x(&context, marker_x, 400.0, copy);
    let extents = context.text_extents(copy).unwrap();

    let label_center = x + extents.x_bearing() + extents.width() / 2.0;
    assert!((label_center - marker_x).abs() < 0.01);
    assert_eq!(best_week_label_y(11.0), 10.0);
    assert_eq!(best_week_label_y(30.0), 22.0);
}

#[test]
fn best_week_highlight_does_not_cut_through_the_chart() {
    let best_start = NaiveDate::from_ymd_opt(2026, 7, 6).unwrap();
    let layout = RibbonLayout {
        points: vec![Point { x: 50.0, y: 20.0 }],
        open_index: None,
    };
    let data = RibbonData {
        bucket_starts: vec![Some(best_start)],
        granularity: Granularity::Week,
        best_week: Some(BestWeek {
            start: best_start,
            total_ms: 25_080_000,
        }),
        ..RibbonData::default()
    };
    let mut surface = ImageSurface::create(Format::ARgb32, 100, 100).unwrap();
    {
        let context = gtk4::cairo::Context::new(&surface).unwrap();
        draw_best_week_highlight(
            &context,
            &layout,
            &data,
            100.0,
            90.0,
            1.0,
            1.0,
            1.0,
            1.0,
            rgba(1.0, 1.0, 1.0, 1.0),
        );
    }

    assert!(
        !pixel_has_ink(&mut surface, 50, 50),
        "the retired marker line must not cut through the best-week bar"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_12_short_week_history_is_compact_and_omits_the_redundant_since_label() {
    gtk4::init().unwrap();
    crate::ui::style::install();
    let ribbon = StatsRibbon::new();
    let timestamp = |month, day| {
        chrono::Local
            .with_ymd_and_hms(2026, month, day, 0, 0, 0)
            .single()
            .unwrap()
            .timestamp()
    };
    let period = PeriodRange {
        start_unix: timestamp(1, 1),
        end_unix: timestamp(7, 13),
        granularity: Granularity::Week,
        sparse_weeks: true,
        buckets: vec![
            reprise_core::library::stats_period::Bucket {
                label: "Week of Jun 29".into(),
                start_unix: timestamp(7, 1),
                end_unix: timestamp(7, 6),
                open: false,
            },
            reprise_core::library::stats_period::Bucket {
                label: "Week of Jul 6".into(),
                start_unix: timestamp(7, 6),
                end_unix: timestamp(7, 13),
                open: true,
            },
        ],
    };

    ribbon.set_data(&period, &[3_600_000, 7_200_000], None);
    // The card's 160px target is its 128px plot plus the stylesheet's padding
    // and border — without the sheet this measures bare theme defaults.
    crate::ui::style::install_css_string_for_test(&super::super::stats_css::css());
    let card = super::super::stats_view_widgets::card(ribbon.widget());
    let window = gtk4::Window::new();
    window.set_default_size(440, -1);
    window.set_child(Some(&card));
    window.present();
    run_main_loop_for_layout();

    let data = ribbon.data.borrow();
    assert!(data.sparse_weeks);
    assert_eq!(data.since_label, None);
    assert_eq!(ribbon.area.height(), SPARSE_RIBBON_HEIGHT);
    // The card's own height requirement is the design property; the test
    // window's allocation is not. Measuring the requirement keeps this honest
    // whatever the surrounding window does.
    let requested = card.measure(gtk4::Orientation::Vertical, -1).0;
    assert!(
        (158..=164).contains(&requested),
        "thin-history card should request about 160 px, got {requested}"
    );
    drop(data);
    window.close();
}

/// CONTRAST: the accent belongs to the data. The axis descriptions take the
/// same secondary tone as every other caption, not the teal of the ribbon.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn ribbon_axis_labels_do_not_borrow_the_data_accent() {
    gtk4::init().unwrap();
    crate::ui::style::install();
    let ribbon = StatsRibbon::new();
    let window = gtk4::Window::new();
    window.set_child(Some(ribbon.widget()));
    window.present();
    run_main_loop_for_layout();

    let data_color = ribbon.area.color();
    let axis_color = ribbon.axis_probe.color();
    let best_week_color = ribbon.best_week_probe.color();
    let baseline_color = ribbon.baseline_probe.color();

    assert_ne!(
        (data_color.red(), data_color.green(), data_color.blue()),
        (axis_color.red(), axis_color.green(), axis_color.blue()),
        "axis labels resolved to the data accent {data_color}"
    );
    assert_ne!(
        (data_color.red(), data_color.green(), data_color.blue()),
        (
            best_week_color.red(),
            best_week_color.green(),
            best_week_color.blue()
        ),
        "the best week must resolve to a lighter accent step"
    );
    assert!(baseline_color.alpha() < data_color.alpha());
    window.close();
}

fn run_main_loop_for_layout() {
    let main_loop = gtk4::glib::MainLoop::new(None, false);
    let quit = main_loop.clone();
    gtk4::glib::timeout_add_local_once(std::time::Duration::from_millis(50), move || {
        quit.quit();
    });
    main_loop.run();
}
