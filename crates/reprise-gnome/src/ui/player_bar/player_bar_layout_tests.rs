//! Tests for `player_bar_layout`, in a sibling file so the layout module
//! itself stays under the 800-line cap the architecture gate enforces.

use std::time::Duration;

use gtk4::prelude::*;

use super::{build, ring_alpha, PLAY_CSS_CLASS, PLAY_RING_CSS_CLASS};

#[test]
fn ac_24_ring_adds_a_kick_on_top_of_a_pressure_bed() {
    assert!((ring_alpha(0.0, 0.0) - 0.10).abs() < 1e-9);
    assert!((ring_alpha(0.0, 0.9) - 0.19).abs() < 1e-9);
    assert!((ring_alpha(1.0, 1.0) - 0.46).abs() < 1e-9);
    assert!((ring_alpha(4.0, 4.0) - 0.46).abs() < 1e-9);
}

#[test]
fn ac_24_ring_is_its_own_layer_and_leaves_the_button_alone() {
    let css = super::css();
    // Its own element with its own property: the button's box-shadow tiers
    // (BTN-3) and the outline focus ring (BTN-1) stay exactly as they are,
    // and the ring sits outside the hit area rather than resizing it.
    assert!(css.contains(&format!(".{PLAY_RING_CSS_CLASS}")));
    assert!(css.contains("border-radius: 50%"));
    assert!(!css.contains(&format!(".{PLAY_RING_CSS_CLASS} {{ outline")));
    // The button never scales with the music — a breathing button moves the
    // hit area under the cursor.
    assert!(!css.contains(&format!(".{PLAY_CSS_CLASS}.reactive")));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn ac_24_ring_leaves_the_play_button_round_and_its_hit_area_untouched() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    crate::ui::style::install_css_string_for_test(&super::css());
    let layout = build();
    let window = gtk4::Window::builder()
        .default_width(1_200)
        .default_height(180)
        .child(&layout.root)
        .build();
    window.present();
    wait_for_layout();

    let button = layout
        .play_pause_button
        .compute_bounds(&layout.root)
        .expect("play button has player-bar bounds");
    let ring = layout
        .play_ring
        .compute_bounds(&layout.root)
        .expect("play ring has player-bar bounds");

    // Square, and exactly the size it was before the ring existed: the ring
    // is a layer around the button, never a change to the button. An
    // overlay child left on `Align::Fill` gets stretched to the ring and
    // the circle silently becomes an ellipse with a wider hit area.
    assert_eq!(
        (button.width(), button.height()),
        (
            super::PLAY_BUTTON_SIZE as f32,
            super::PLAY_BUTTON_SIZE as f32
        ),
        "the ring deformed the play button"
    );
    // The ring encloses the button on every side, and the button stays
    // centred inside it.
    assert!(ring.width() > button.width() && ring.height() > button.height());
    let dx = (ring.x() + ring.width() / 2.0) - (button.x() + button.width() / 2.0);
    let dy = (ring.y() + ring.height() / 2.0) - (button.y() + button.height() / 2.0);
    assert!(
        dx.abs() <= 0.5 && dy.abs() <= 0.5,
        "ring off-centre: {dx}, {dy}"
    );
    // A visible gap remains between the button edge and the ring stroke.
    let gap = (ring.width() - button.width()) / 2.0;
    assert!((2.0..=5.0).contains(&gap), "ring gap out of range: {gap}");

    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn ac_24_the_glyph_offset_never_moves_the_circle_or_the_ring() {
    // The correction belongs to the glyph. The circle and the bass ring are
    // the button's geometry; moving either would make the error visible twice.
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    crate::ui::style::install_css_string_for_test(&super::css());
    let layout = build();
    let window = gtk4::Window::builder()
        .default_width(1_200)
        .default_height(180)
        .child(&layout.root)
        .build();
    window.present();
    wait_for_layout();

    let button = layout
        .play_pause_button
        .compute_bounds(&layout.root)
        .expect("play button has player-bar bounds");
    let ring = layout
        .play_ring
        .compute_bounds(&layout.root)
        .expect("play ring has player-bar bounds");
    let dx = (ring.x() + ring.width() / 2.0) - (button.x() + button.width() / 2.0);
    let dy = (ring.y() + ring.height() / 2.0) - (button.y() + button.height() / 2.0);
    assert!(
        dx.abs() <= 0.5 && dy.abs() <= 0.5,
        "glyph correction moved the circle or ring: {dx}, {dy}"
    );

    window.close();
}

fn wait_for_layout() {
    let main_loop = gtk4::glib::MainLoop::new(None, false);
    let quit = main_loop.clone();
    gtk4::glib::timeout_add_local_once(Duration::from_millis(50), move || quit.quit());
    main_loop.run();
}

fn descendant_buttons(root: &gtk4::Widget) -> Vec<gtk4::Button> {
    let mut buttons = Vec::new();
    let mut pending = vec![root.clone()];
    while let Some(widget) = pending.pop() {
        if let Ok(button) = widget.clone().downcast::<gtk4::Button>() {
            buttons.push(button);
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            child = current.next_sibling();
            pending.push(current);
        }
    }
    buttons
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn que_1_player_bar_has_no_queue_button() {
    gtk4::init().unwrap();
    let layout = build();
    let queue_buttons = descendant_buttons(layout.root.upcast_ref())
        .into_iter()
        .filter(|button| button.icon_name().as_deref() == Some("view-list-symbolic"))
        .count();

    assert_eq!(
        queue_buttons, 0,
        "the player bar still renders a queue button"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn library_bar_has_three_zones_via_centerbox() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    if gtk4::init().is_err() {
        return;
    }
    let layout = build();
    let window = gtk4::Window::builder()
        .default_width(1_200)
        .child(&layout.root)
        .build();
    window.set_size_request(1_200, -1);
    window.present();
    wait_for_layout();

    assert_eq!(layout.root.width(), 1_200);
    // Start zone: info_box is the start widget of center_box.
    assert_eq!(
        layout.center_box.start_widget(),
        Some(layout.info_box.clone().upcast())
    );
    assert!(layout.cover.is_ancestor(&layout.info_box));
    assert!(layout.title_label.is_ancestor(&layout.info_box));
    assert!(layout.artist_label.is_ancestor(&layout.info_box));
    // Transport controls are within the center zone.
    assert!(layout.shuffle_button.is_ancestor(&layout.root));
    assert!(layout.play_pause_button.is_ancestor(&layout.root));
    assert!(layout.prev_button.is_ancestor(&layout.root));
    assert!(layout.next_button.is_ancestor(&layout.root));
    assert!(layout.repeat_button.is_ancestor(&layout.root));
    // Seek row widgets are present.
    assert!(layout.position_label.is_ancestor(&layout.root));
    assert!(layout.duration_label.is_ancestor(&layout.root));
    assert!(layout.waveform.widget().is_ancestor(&layout.root));
    // End zone has volume controls.
    assert!(layout.volume_scale.is_ancestor(&layout.root));
    assert!(layout.volume_icon.is_ancestor(&layout.root));
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn style_5_player_bar_fits_a_narrow_short_window_without_clipping() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let layout = build();
    let content = gtk4::ScrolledWindow::builder()
        .child(&gtk4::Label::new(Some("Scrollable library content")))
        .vexpand(true)
        .build();
    let shell = crate::ui::library_player_bar::LibraryPlayerBarShell::new(
        &content,
        Some(layout.root.upcast_ref()),
        reprise_core::library::settings::PlayerBarPosition::Bottom,
    );
    let window = gtk4::Window::builder()
        .default_width(720)
        .default_height(420)
        .child(shell.widget())
        .build();

    window.present();
    wait_for_layout();

    assert!(
        window.width() <= 720,
        "the full player forced a half-screen window wider: {}",
        window.width()
    );
    assert_eq!(layout.root.width(), shell.widget().width());
    assert!(
        layout.root.height() >= super::BAR_HEIGHT,
        "the player bar lost its full structural height"
    );
    for (name, widget) in [
        ("cover", layout.cover_button.upcast_ref::<gtk4::Widget>()),
        (
            "play/pause",
            layout.play_pause_button.upcast_ref::<gtk4::Widget>(),
        ),
        (
            "waveform",
            layout.waveform.widget().upcast_ref::<gtk4::Widget>(),
        ),
        (
            "position",
            layout.position_label.upcast_ref::<gtk4::Widget>(),
        ),
        (
            "duration",
            layout.duration_label.upcast_ref::<gtk4::Widget>(),
        ),
        ("volume", layout.volume_scale.upcast_ref::<gtk4::Widget>()),
    ] {
        let bounds = widget
            .compute_bounds(&layout.root)
            .unwrap_or_else(|| panic!("{name} has no player-bar bounds"));
        assert!(
            bounds.x() >= 0.0
                && bounds.y() >= 0.0
                && bounds.x() + bounds.width() <= layout.root.width() as f32
                && bounds.y() + bounds.height() <= layout.root.height() as f32,
            "{name} is clipped outside the player bar: {bounds:?}, bar={}x{}",
            layout.root.width(),
            layout.root.height()
        );
    }

    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn style_5_long_title_keeps_transport_controls_centered() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let layout = build();
    layout.title_label.set_text(
        "Siren's Lament — Dark Melodic Metalcore Instrumental Mix | \
         Cinematic Heavy Atmospheric Metal With An Extremely Long Name",
    );
    layout
        .artist_label
        .set_text("Hollow Fallen — Videos With Another Long Channel Name");
    let window = gtk4::Window::builder()
        .default_width(1_200)
        .default_height(180)
        .child(&layout.root)
        .build();

    window.present();
    wait_for_layout();

    let play_bounds = layout
        .play_pause_button
        .compute_bounds(&layout.root)
        .expect("play button has player-bar bounds");
    let play_center = play_bounds.x() + play_bounds.width() / 2.0;
    let bar_center = layout.root.width() as f32 / 2.0;
    assert!(
        (play_center - bar_center).abs() <= 1.0,
        "long metadata shifted transport controls: play={play_center}, bar={bar_center}"
    );

    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn tip_1d_player_bar_buttons_follow_tooltip_discipline() {
    if gtk4::init().is_err() {
        return;
    }
    let layout = build();
    let violations = crate::ui::tooltip_discipline::tooltip_violations(layout.root.upcast_ref());
    assert!(violations.is_empty(), "{violations:?}");
}

#[test]
fn css_styles_the_glow_play_button_and_surface() {
    let css = super::css();
    assert!(css.contains(".player-bar-play"));
    assert!(css.contains("@reprise_player_accent"));
    assert!(css.contains(".player-bar-surface"));
    assert!(css.contains("background-color: @headerbar_bg_color"));
    assert!(css.contains("border-top: 1px solid"));
    assert!(css.contains("@keyframes reprise-play-pulse"));
    assert!(css.contains("transform: scale(0.92)"));
    assert!(css.contains("inset 0 2px 1px alpha(#ffffff, 0.34)"));
    assert!(css.contains("inset 0 -4px 3px alpha(#000000, 0.30)"));
    assert!(css.contains("0 6px 12px alpha(#000000, 0.36)"));
    assert!(css.contains("inset 0 4px 6px alpha(#000000, 0.44)"));
    assert!(css.contains("0 1px 2px alpha(#000000, 0.22)"));
    assert!(css.contains(&format!(
        "animation: reprise-play-pulse {}ms {} 1",
        crate::ui::motion::MICRO_MS,
        crate::ui::motion::MICRO_CSS_EASING
    )));
}

#[test]
fn tip_1d_player_bar_artist_names_its_navigation_action() {
    let source = include_str!("player_bar_layout.rs")
        .split("#[cfg(test)]")
        .next()
        .unwrap();
    let artist_tooltip = ".tooltip_text(strings::text(strings::GO_TO_PLAYING_ARTIST))";

    assert_eq!(source.matches(&artist_tooltip).count(), 1);
}

#[test]
fn css_includes_new_cover_and_label_classes() {
    let css = super::css();
    assert!(css.contains(".player-bar-cover"));
    assert!(css.contains(".player-bar-title"));
    assert!(css.contains(".player-bar-artist"));
    assert!(css.contains("border-radius: 8px"));
}

/// The press sink is no longer the player bar's own business: it comes
/// from the one central set (BTN-4), and the bar only adds the louder
/// accent ring the main action is allowed (BTN-3).
#[test]
fn btn_3_play_button_has_sculpted_depth_and_a_distinct_pressed_well() {
    use crate::ui::style::buttons;

    let css = super::css();
    assert!(css.contains(&format!(".{}:active", super::PLAY_CSS_CLASS)));
    assert!(css.contains("inset 0 2px 1px alpha(#ffffff, 0.34)"));
    assert!(css.contains("inset 0 -4px 3px alpha(#000000, 0.30)"));
    assert!(css.contains("0 6px 12px alpha(#000000, 0.36)"));
    assert!(css.contains("inset 0 4px 6px alpha(#000000, 0.44)"));
    assert!(css.contains("0 0 0 4px alpha(@reprise_player_accent"));
    // The MOT-5 play/pause pulse keyframes stay; only the *press* scale
    // moved out, so no local rule may restate it.
    let press_scale = format!("scale({})", crate::ui::style::tokens::BTN_PRESS_SCALE);
    assert!(
        !css.contains(&press_scale),
        "the press scale belongs to style::buttons, not to a per-button tint"
    );

    let shared = buttons::css();
    assert!(shared.contains(&format!(".{}:active", buttons::PRIMARY_CLASS)));
    assert!(shared.contains(&press_scale));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn player_bar_css_parses_without_errors() {
    gtk4::init().unwrap();
    let errors = crate::ui::style::css_parse_errors(&super::css());
    assert!(errors.is_empty(), "CSS parse errors: {errors:?}");
}
