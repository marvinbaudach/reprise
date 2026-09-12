use std::time::Duration;

use gtk4::prelude::*;

use crate::ui::style::tokens;

fn realized_panel(
    application_id: &str,
) -> (
    libadwaita::ApplicationWindow,
    std::rc::Rc<super::super::surface::NowPlayingPanel>,
) {
    crate::ui::style::install_css_string_for_test(&crate::ui::style::app_css_for_test());
    let (window, panel) = super::super::surface::tests::test_panel(application_id);
    panel.widgets.column.set_visible(true);
    window.set_default_size(900, 800);
    window.present();
    crate::ui::test_settle::settle_for(Duration::from_millis(100));
    (window, panel)
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn npp_2_the_cover_sits_fifty_px_down_and_the_title_thirty_four_below_it() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let (window, panel) =
        realized_panel("io.github.marvinbaudach.Reprise.NowPlayingSettledHeadGeometryTest");
    let widgets = &panel.widgets;
    let cover = widgets.cover.compute_bounds(&widgets.stage).unwrap();
    let metadata = widgets.metadata.compute_bounds(&widgets.stage).unwrap();

    assert_eq!(cover.y().round() as i32, tokens::NOW_PLAYING_HEAD_TOP);
    assert_eq!(cover.width().round() as i32, tokens::NOW_PLAYING_COVER_SIZE);
    assert_eq!(
        cover.height().round() as i32,
        tokens::NOW_PLAYING_COVER_SIZE
    );
    assert_eq!(
        metadata.y().round() as i32,
        tokens::NOW_PLAYING_ARTWORK_BAND
    );
    assert_eq!(
        widgets.artwork_band.height(),
        tokens::NOW_PLAYING_ARTWORK_BAND
    );
    assert!(!widgets.title.wraps());
    assert!(!widgets.artist.wraps());
    assert!(!widgets.album.wraps());
    assert!(!widgets.album.uses_markup());
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn npp_2_the_segment_control_is_thirty_px_tall_with_two_px_gaps() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let (window, panel) =
        realized_panel("io.github.marvinbaudach.Reprise.NowPlayingSettledSegmentGeometryTest");
    let widgets = &panel.widgets;
    let switcher = widgets.tab_switcher.compute_bounds(&widgets.stage).unwrap();
    assert_eq!(
        switcher.height().round() as i32,
        tokens::NOW_PLAYING_SEGMENT_HEIGHT
    );
    assert_eq!(switcher.width().round() as i32, widgets.stage.width() - 36);

    let group = widgets
        .tab_switcher
        .first_child()
        .expect("inline switcher owns a toggle group");
    assert_eq!(group.css_name(), "toggle-group");
    assert_eq!(group.height(), 26);
    let toggles = std::iter::successors(group.first_child(), WidgetExt::next_sibling)
        .filter(|widget| widget.css_name() == "toggle")
        .collect::<Vec<_>>();
    assert_eq!(toggles.len(), 3);
    for toggle in &toggles {
        assert_eq!(toggle.height(), 26);
    }
    for pair in toggles.windows(2) {
        let first = pair[0].compute_bounds(&group).unwrap();
        let second = pair[1].compute_bounds(&group).unwrap();
        assert_eq!((second.x() - (first.x() + first.width())).round() as i32, 2);
    }
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn npp_2_a_hairline_with_run_out_separates_the_switcher_from_the_list() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let (window, panel) =
        realized_panel("io.github.marvinbaudach.Reprise.NowPlayingSettledListRuleGeometryTest");
    let widgets = &panel.widgets;
    let switcher = widgets.tab_switcher.compute_bounds(&widgets.stage).unwrap();
    let rule = widgets.list_rule.compute_bounds(&widgets.stage).unwrap();
    let stack = widgets.tab_stack.compute_bounds(&widgets.stage).unwrap();

    assert_eq!(rule.height().round() as i32, 1);
    assert_eq!(
        (rule.y() - (switcher.y() + switcher.height())).round() as i32,
        tokens::NOW_PLAYING_LIST_RULE_ABOVE
    );
    assert_eq!(
        (stack.y() - (rule.y() + rule.height())).round() as i32,
        tokens::NOW_PLAYING_LIST_RULE_BELOW
    );
    assert_eq!(rule.width().round() as i32, widgets.stage.width());
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn npp_18_the_fades_hand_the_title_calm_ground_and_keep_the_list_edge_clean() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let width = super::super::now_playing_column::PANEL_WIDTH;
    let band = tokens::NOW_PLAYING_ARTWORK_BAND;
    let cloud = super::super::cover_cloud::CoverCloud::new();
    let texture = saturated_cover();
    cloud.set_cover(Some(&texture), 1);
    cloud.set_pinned(true);

    let render = |with_fades| {
        let surface =
            gtk4::cairo::ImageSurface::create(gtk4::cairo::Format::ARgb32, width, band).unwrap();
        let cr = gtk4::cairo::Context::new(&surface).unwrap();
        let [r, g, b] = crate::ui::style::accent::sidebar_background_rgb();
        cr.set_source_rgb(
            f64::from(r) / 255.0,
            f64::from(g) / 255.0,
            f64::from(b) / 255.0,
        );
        cr.paint().unwrap();
        if with_fades {
            cloud.draw_for_test(&cr, width, band);
        } else {
            cloud.paint_layers_only_for_test(&cr, width, band);
        }
        drop(cr);
        surface
    };
    let panel = crate::ui::style::accent::sidebar_background_rgb();
    let [top_left, title_edge, middle] = pixels(render(true), [(0, 0), (150, 267), (150, 120)]);
    assert_close(top_left, panel, 0);
    assert_close(title_edge, panel, 4);
    assert!(
        middle
            .iter()
            .zip(panel)
            .any(|(channel, panel)| channel.abs_diff(panel) >= 20),
        "the clouds disappeared from the open middle of the band"
    );

    let [control_left, control_title] = pixels(render(false), [(0, 0), (150, 267)]);
    assert_ne!(control_left, panel);
    assert_ne!(control_title, panel);
    assert_eq!(
        super::super::cover_scrim::left_fade_alpha(0.0, f64::from(width)),
        1.0
    );
    assert!(super::super::cover_scrim::scrim_alpha(267.5, f64::from(band)) > 0.99);
}

fn saturated_cover() -> gtk4::gdk::Texture {
    let edge = 64usize;
    let mut data = vec![0u8; edge * edge * 4];
    for y in 0..edge {
        for x in 0..edge {
            let offset = (y * edge + x) * 4;
            let (red, green, blue) = if (x / 8 + y / 8) % 2 == 0 {
                (255, 24, 180)
            } else {
                (20, 210, 255)
            };
            data[offset] = blue;
            data[offset + 1] = green;
            data[offset + 2] = red;
            data[offset + 3] = 255;
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

fn pixels<const N: usize>(
    mut surface: gtk4::cairo::ImageSurface,
    coordinates: [(usize, usize); N],
) -> [[u8; 3]; N] {
    surface.flush();
    let stride = usize::try_from(surface.stride()).unwrap();
    let data = surface.data().unwrap();
    coordinates.map(|(x, y)| {
        let offset = y * stride + x * 4;
        [data[offset + 2], data[offset + 1], data[offset]]
    })
}

fn assert_close(actual: [u8; 3], expected: [u8; 3], tolerance: u8) {
    assert!(
        actual
            .into_iter()
            .zip(expected)
            .all(|(actual, expected)| actual.abs_diff(expected) <= tolerance),
        "actual {actual:?} differs from panel {expected:?}"
    );
}
