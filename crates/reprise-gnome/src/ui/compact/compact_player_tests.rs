//! Tests for `CompactPlayer`. Split out of `compact_player.rs` so that file
//! stays under the 800-line gate.

use super::*;

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mini_player_has_root_handle_and_cover_image() {
    if gtk4::init().is_err() {
        return;
    }
    let player = CompactPlayer::new();
    assert!(player.handle().parent().is_none()); // not yet in a window
                                                 // cover_image() returns the image widget
    assert!(player.cover_image().is::<gtk4::Image>());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mini_2_restore_action_reopens_full_window() {
    if gtk4::init().is_err() {
        return;
    }
    let player = CompactPlayer::new();
    let fired = Rc::new(Cell::new(false));
    let fired2 = fired.clone();
    player.set_on_restore(Rc::new(move || fired2.set(true)));
    player.activate_restore_for_test();
    assert!(fired.get());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mot_6_compact_track_change_replaces_the_running_animation_slot() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let settings = gtk4::Settings::default().unwrap();
    let previous = settings.is_gtk_enable_animations();
    settings.set_gtk_enable_animations(true);

    let player = CompactPlayer::new();
    let window = gtk4::Window::new();
    window.set_child(Some(player.handle()));
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}

    player.0.widgets.title_label.set_text("Before");
    player.set_track("First", "First artist");
    player.set_track("Second", "Second artist");

    assert_eq!(player.0.widgets.title_label.text(), "First");
    assert_eq!(player.0.widgets.artist_label.text(), "First artist");
    assert_eq!(player.0.widgets.title_label.opacity(), 1.0);
    {
        let animation = player.0.current_track_animation.borrow();
        let animation = animation.as_ref().unwrap();
        assert_eq!(animation.duration(), motion::half(motion::STANDARD));
        assert_eq!(animation.easing(), motion::STANDARD_EASING);
        assert!(animation.follows_enable_animations_setting());
    }

    settings.set_gtk_enable_animations(previous);
    window.close();
}

/// The mini-player's half of the player bar's guard: the same title and
/// artist arriving twice names no new track, so its labels stay put.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mot_5_re_feeding_the_same_track_does_not_crossfade_the_mini_player() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let settings = gtk4::Settings::default().unwrap();
    let previous = settings.is_gtk_enable_animations();
    settings.set_gtk_enable_animations(true);

    let player = CompactPlayer::new();
    let window = gtk4::Window::new();
    window.set_child(Some(player.handle()));
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}

    player.set_track("Title", "Artist");
    let settled = player.0.track_animation_generation.get();
    player.set_track("Title", "Artist");
    assert_eq!(
        player.0.track_animation_generation.get(),
        settled,
        "the unchanged title and artist were cross-faded again"
    );

    player.set_track("Second", "Artist");
    assert_eq!(
        player.0.track_animation_generation.get(),
        settled.wrapping_add(1)
    );

    settings.set_gtk_enable_animations(previous);
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mot_5_compact_player_state_propagates_pause_to_waveform() {
    gtk4::init().unwrap();
    let player = CompactPlayer::new();

    player.set_state(PlaybackState::Playing);
    assert_eq!(
        player.0.widgets.waveform.desaturation_target_for_test(),
        0.0
    );
    player.set_state(PlaybackState::Paused);
    assert_eq!(
        player.0.widgets.waveform.desaturation_target_for_test(),
        1.0
    );
    player.set_state(PlaybackState::Playing);
    assert_eq!(
        player.0.widgets.waveform.desaturation_target_for_test(),
        0.0
    );
    player.set_state(PlaybackState::Stopped);
    assert_eq!(
        player.0.widgets.waveform.desaturation_target_for_test(),
        1.0
    );
}

#[test]
fn mini_4_ctrl_arrows_map_to_prev_next() {
    use gtk4::gdk::{Key, ModifierType};
    assert_eq!(
        compact_key_action(Key::Right, ModifierType::CONTROL_MASK),
        Some("next")
    );
    assert_eq!(
        compact_key_action(Key::Left, ModifierType::CONTROL_MASK),
        Some("previous")
    );
    // Plain arrows fall through so the waveform can seek.
    assert_eq!(compact_key_action(Key::Right, ModifierType::empty()), None);
    assert_eq!(compact_key_action(Key::Left, ModifierType::empty()), None);
    // Other Ctrl combos are not ours to claim.
    assert_eq!(
        compact_key_action(Key::Up, ModifierType::CONTROL_MASK),
        None
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mini_4_next_previous_actions_fire_callbacks() {
    if gtk4::init().is_err() {
        return;
    }
    let player = CompactPlayer::new();
    let seen = Rc::new(RefCell::new(Vec::<&'static str>::new()));
    let s1 = seen.clone();
    player.connect_next(move || s1.borrow_mut().push("next"));
    let s2 = seen.clone();
    player.connect_previous(move || s2.borrow_mut().push("previous"));
    player.0.menu.action_group.activate_action("next", None);
    player.0.menu.action_group.activate_action("previous", None);
    assert_eq!(*seen.borrow(), vec!["next", "previous"]);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mini_2_quit_action_quits_the_app() {
    if gtk4::init().is_err() {
        return;
    }
    // The card carries no ✕: Quit is reachable from the context menu
    // (MINI-3) and Ctrl+Q (MINI-4); both route through the "quit" action.
    let player = CompactPlayer::new();
    let quit = Rc::new(Cell::new(false));
    let q = quit.clone();
    player.set_on_quit(Rc::new(move || q.set(true)));
    player.0.menu.action_group.activate_action("quit", None);
    assert!(quit.get(), "the quit action must quit the app (MINI-2)");
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mini_waveform_progress_uses_playback_accent() {
    if gtk4::init().is_err() {
        return;
    }
    let player = CompactPlayer::new();
    // Real peaks + a mid-track position: the mini waveform draws the true
    // shape with progress, not the pseudo-random skeleton fallback.
    player.set_analysis(vec![80u8, 200, 40, 255, 120, 60, 180, 30], None);
    player.set_position(30_000, 60_000);
    assert!(
        player.0.widgets.waveform.has_raw_peaks_for_test(),
        "compact set_analysis must reach the mini waveform"
    );
    // Played bars take the effective accent (the same @reprise_player_accent as
    // the play button); unplayed bars stay dim white (frame 1e).
    let css = crate::ui::compact::compact_player_layouts::mini_css();
    assert!(css.contains(".waveform-seek { color: @reprise_player_accent; }"));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mini_1_waveform_stays_hittable_when_external_clears_before_a_duration_is_known() {
    if gtk4::init().is_err() {
        return;
    }
    let player = CompactPlayer::new();
    // Startup order: the external-media session reports "nothing external"
    // before any local track has produced a duration. An insensitive
    // waveform is skipped by GTK hit-testing entirely — pointer picking
    // lands on the card below it, so click-to-seek, drag-to-scrub (MINI-1)
    // and keyboard seek all die, and the card's WindowHandle turns the
    // scrub into a window move (MINI-2 exempts the waveform from dragging).
    player.set_external_snapshot(None);
    assert!(
        player.0.widgets.waveform.widget().get_sensitive(),
        "clearing external playback must leave the mini waveform hittable"
    );
    // …and it must still be hittable once a track supplies a duration,
    // because nothing re-evaluates sensitivity after this point.
    player.set_position(0, 180_000);
    assert!(
        player.0.widgets.waveform.widget().get_sensitive(),
        "the mini waveform must stay hittable once a track is loaded"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mini_1_waveform_is_the_pointer_target_inside_the_card() {
    if gtk4::init().is_err() {
        return;
    }
    // The card padding is CSS, and the waveform's position depends on it.
    crate::ui::style::install_css_string_for_test(
        &crate::ui::compact::compact_player_layouts::mini_css(),
    );
    let player = CompactPlayer::new();
    // Same startup order the app produces.
    player.set_external_snapshot(None);
    player.set_position(0, 180_000);

    let win = gtk4::Window::new();
    win.set_child(Some(player.handle()));
    win.set_default_size(
        crate::ui::compact::compact_player_layouts::MINI_WIDTH,
        crate::ui::compact::compact_player_layouts::MINI_HEIGHT,
    );
    win.present();
    while gtk4::glib::MainContext::default().iteration(false) {}

    let root = player.handle().clone().upcast::<gtk4::Widget>();
    let waveform = player.0.widgets.waveform.widget().clone();
    let bounds = waveform.compute_bounds(&root).expect("waveform bounds");
    assert!(
        bounds.width() > 0.0 && bounds.height() > 0.0,
        "the mini waveform must have an allocation to be clickable"
    );
    // Pointer picking is what click-to-seek and drag-to-scrub ride on
    // (MINI-1). GTK skips insensitive widgets while picking, so a
    // regression there silently hands every press to the card's
    // WindowHandle, which moves the window instead (MINI-2).
    for fraction in [0.1_f32, 0.5, 0.9] {
        let x = f64::from(bounds.x() + bounds.width() * fraction);
        let y = f64::from(bounds.y() + bounds.height() / 2.0);
        let hit = root
            .pick(x, y, gtk4::PickFlags::DEFAULT)
            .expect("a widget under the pointer");
        assert_eq!(
            hit,
            waveform.clone().upcast::<gtk4::Widget>(),
            "the waveform must be the pick target at {fraction} of its width"
        );
    }
    win.close();
}
