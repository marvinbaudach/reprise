use super::*;
use libadwaita::prelude::AnimationExt;
use std::time::Duration;

fn run_main_loop_for(milliseconds: u32) {
    let main_loop = gtk4::glib::MainLoop::new(None, false);
    let quit = main_loop.clone();
    gtk4::glib::timeout_add_local_once(Duration::from_millis(milliseconds.into()), move || {
        quit.quit();
    });
    main_loop.run();
}

fn run_until_idle() {
    let main_loop = gtk4::glib::MainLoop::new(None, false);
    let quit = main_loop.clone();
    gtk4::glib::idle_add_local_once(move || quit.quit());
    main_loop.run();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn player_metadata_uses_native_keyboard_activation() {
    gtk4::init().unwrap();
    let bar = PlayerBar::new();
    bar.set_track("Track title", "Artist name");
    for button in [&bar.cover_button, &bar.title_button, &bar.artist_button] {
        assert!(button.is_focusable());
    }
    let activations = Rc::new(Cell::new(0));
    let title_activations = activations.clone();
    bar.set_on_title_click(move || title_activations.set(title_activations.get() + 1));
    let cover_activations = activations.clone();
    bar.connect_cover_clicked(move || cover_activations.set(cover_activations.get() + 1));
    let artist_activations = activations.clone();
    bar.connect_artist_clicked(move || artist_activations.set(artist_activations.get() + 1));
    bar.title_button.emit_clicked();
    bar.cover_button.emit_clicked();
    bar.artist_button.emit_clicked();
    assert_eq!(activations.get(), 3);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn play_9_idle_play_is_reachable_without_enabling_queue_navigation() {
    gtk4::init().unwrap();
    let bar = PlayerBar::new();

    bar.set_transport_enabled(false, true);

    assert!(bar.widget().is_sensitive());
    assert!(bar.play_pause_button.is_sensitive());
    assert!(!bar.prev_button.is_sensitive());
    assert!(!bar.next_button.is_sensitive());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mot_5_play_pause_pulses_on_state_change() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let settings = gtk4::Settings::default().unwrap();
    let previous = settings.is_gtk_enable_animations();
    settings.set_gtk_enable_animations(true);

    let bar = PlayerBar::new();
    let window = gtk4::Window::new();
    window.set_child(Some(bar.widget()));
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}

    bar.set_state(PlaybackState::Paused);
    while gtk4::glib::MainContext::default().iteration(false) {}
    assert!(!bar.play_pause_button.has_css_class("pulsing"));

    let first_generation = bar.play_pulse_generation.get();
    bar.set_state(PlaybackState::Playing);
    assert_eq!(
        bar.play_pulse_generation.get(),
        first_generation.wrapping_add(1)
    );
    // A pulse which is not replacing an active pulse starts immediately.
    assert!(bar.play_pause_button.has_css_class("pulsing"));

    // Interrupt the pulse immediately instead of waiting out half its duration.
    // Pumping the main loop is not wall-clock faithful: a 75 ms request took
    // ~400 ms under X11/Xvfb, so the pulse had already expired and the next
    // state change became a *fresh* pulse — the retrigger path was never
    // exercised and the assertion below failed for the wrong reason. The pulse
    // is running as of the assertion above, so interrupting it here hits the
    // retrigger path deterministically on every backend.
    let retrigger_generation = bar.play_pulse_generation.get();
    bar.set_state(PlaybackState::Paused);
    assert_eq!(
        bar.play_pulse_generation.get(),
        retrigger_generation.wrapping_add(1)
    );
    assert!(!bar.play_pause_button.has_css_class("pulsing"));

    // An idle callback still belongs to the current frame. The retrigger delay
    // must keep the class absent beyond that frame, otherwise GTK can collapse
    // remove+add into one style recomputation and leave @keyframes running from
    // their old timeline instead of restarting them.
    run_until_idle();
    assert!(!bar.play_pause_button.has_css_class("pulsing"));

    let pending_generation = bar.play_pulse_generation.get();
    bar.set_state(PlaybackState::Playing);
    assert_eq!(
        bar.play_pulse_generation.get(),
        pending_generation.wrapping_add(1)
    );
    assert!(!bar.play_pause_button.has_css_class("pulsing"));
    run_until_idle();
    assert!(!bar.play_pause_button.has_css_class("pulsing"));

    // The backend-independent delay has elapsed, so only the latest generation
    // may re-add the class and start a fresh keyframe timeline.
    run_main_loop_for(30);
    assert!(bar.play_pause_button.has_css_class("pulsing"));

    // Avoid an elapsed-time midpoint assertion here. A requested main-loop
    // duration is only a lower bound under X11/Xvfb, so a busy runner may
    // legitimately process the 150 ms removal before a nominal 95 ms wait
    // returns. The immediate assertion above and eventual removal below are
    // the stable behavior boundaries.
    run_main_loop_for(motion::MICRO_MS + 20);
    assert!(!bar.play_pause_button.has_css_class("pulsing"));

    settings.set_gtk_enable_animations(previous);
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mot_5_player_bar_state_propagates_pause_to_waveform() {
    gtk4::init().unwrap();
    let bar = PlayerBar::new();

    bar.set_state(PlaybackState::Playing);
    assert_eq!(bar.waveform.desaturation_target_for_test(), 0.0);
    bar.set_state(PlaybackState::Paused);
    assert_eq!(bar.waveform.desaturation_target_for_test(), 1.0);
    bar.set_state(PlaybackState::Playing);
    assert_eq!(bar.waveform.desaturation_target_for_test(), 0.0);
    bar.set_state(PlaybackState::Stopped);
    assert_eq!(bar.waveform.desaturation_target_for_test(), 1.0);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mot_6_second_track_and_state_changes_finish_the_previous_visual_state() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let settings = gtk4::Settings::default().unwrap();
    let previous = settings.is_gtk_enable_animations();
    settings.set_gtk_enable_animations(true);

    let bar = PlayerBar::new();
    let window = gtk4::Window::new();
    window.set_child(Some(bar.widget()));
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}

    bar.title_label.set_text("Before");
    bar.artist_label.set_text("Before artist");
    bar.set_track("First", "First artist");
    bar.set_track("Second", "Second artist");
    assert_eq!(bar.title_label.text(), "First");
    assert_eq!(bar.artist_label.text(), "First artist");
    assert_eq!(bar.title_label.opacity(), 1.0);
    {
        let track_animation = bar.current_track_animation.borrow();
        let track_animation = track_animation.as_ref().unwrap();
        assert_eq!(track_animation.duration(), motion::half(motion::STANDARD));
        assert_eq!(track_animation.easing(), motion::STANDARD_EASING);
        assert!(track_animation.follows_enable_animations_setting());
    }

    bar.set_transport_enabled(true, false);
    bar.set_state(PlaybackState::Playing);
    bar.set_state(PlaybackState::Paused);
    assert_eq!(bar.playback_state.get(), PlaybackState::Paused);
    assert_eq!(
        bar.play_pause_button.icon_name().as_deref(),
        Some(ICON_PAUSE)
    );
    assert_eq!(bar.play_pause_button.opacity(), 1.0);
    {
        // Scope the borrow: window.close() below unmaps the bar, whose teardown
        // calls replace_animation() (a slot.borrow_mut()). Holding this .borrow()
        // across the close would re-enter the slot and abort (BorrowMutError
        // inside a C signal callback) — same discipline as the track_animation
        // borrow above.
        let icon_animation = bar.current_icon_animation.borrow();
        let icon_animation = icon_animation.as_ref().unwrap();
        assert_eq!(icon_animation.duration(), motion::half(motion::MICRO));
        assert_eq!(icon_animation.easing(), motion::MICRO_EASING);
        assert!(icon_animation.follows_enable_animations_setting());
    }

    settings.set_gtk_enable_animations(previous);
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mot_7_player_bar_hard_switches_when_system_animations_are_disabled() {
    gtk4::init().unwrap();
    let settings = gtk4::Settings::default().unwrap();
    let previous = settings.is_gtk_enable_animations();
    settings.set_gtk_enable_animations(false);

    let bar = PlayerBar::new();
    bar.set_track("Immediate", "Artist");
    bar.set_transport_enabled(true, false);
    bar.set_state(PlaybackState::Playing);

    assert_eq!(bar.title_label.text(), "Immediate");
    assert_eq!(bar.artist_label.text(), "Artist");
    assert_eq!(bar.title_label.opacity(), 1.0);
    assert_eq!(
        bar.play_pause_button.icon_name().as_deref(),
        Some(ICON_PAUSE)
    );
    assert_eq!(bar.play_pause_button.opacity(), 1.0);

    settings.set_gtk_enable_animations(previous);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn browse_4_player_bar_metadata_has_distinct_track_album_and_artist_targets() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let bar = PlayerBar::new();
    bar.set_track("Track title", "Artist name");

    for (surface, tooltip) in [
        (
            bar.cover_button.clone().upcast::<gtk4::Widget>(),
            "Reveal playing album",
        ),
        (
            bar.title_button.clone().upcast::<gtk4::Widget>(),
            "Jump to now playing",
        ),
        (
            bar.artist_button.clone().upcast::<gtk4::Widget>(),
            "Go to playing artist",
        ),
    ] {
        assert!(surface.is_focusable());
        assert!(gtk4::test_accessible_has_role(
            &surface,
            gtk4::AccessibleRole::Button
        ));
        assert!(gtk4::test_accessible_has_property(
            &surface,
            gtk4::AccessibleProperty::Label
        ));
        assert_eq!(surface.tooltip_text().as_deref(), Some(tooltip));
    }

    assert_eq!(
        crate::ui::strings::text(crate::ui::strings::REVEAL_PLAYING_ALBUM),
        "Reveal playing album"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn repeat_mode_tooltips_explain_current_behavior() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let bar = PlayerBar::new();

    for (repeat, expected) in [
        (Repeat::Off, "Repeat off — playback stops after the queue"),
        (Repeat::All, "Repeat all — the entire queue repeats"),
        (Repeat::One, "Repeat one — the current track repeats"),
    ] {
        bar.set_repeat_indicator(repeat);
        assert_eq!(bar.repeat_button.tooltip_text().as_deref(), Some(expected));
    }
}

#[test]
fn repeat_mode_tooltip_keys_follow_the_current_behavior() {
    assert_eq!(repeat_indicator(Repeat::Off).1, strings::TOOLTIP_REPEAT_OFF);
    assert_eq!(repeat_indicator(Repeat::All).1, strings::TOOLTIP_REPEAT_ALL);
    assert_eq!(repeat_indicator(Repeat::One).1, strings::TOOLTIP_REPEAT_ONE);
}

/// BTN-2: Shuffle and Repeat are toggles, so "on" must be a state the widget
/// keeps — not a flash at click time. The state has to survive the pointer
/// arriving and leaving, and it must not rest on colour alone.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn btn_2_toggle_state_persists_and_non_color_cue() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();

    let bar = PlayerBar::new();
    let window = gtk4::Window::new();
    window.set_child(Some(bar.widget()));
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}

    // Both transport toggles speak the one `:checked` vocabulary.
    for toggle in [
        bar.shuffle_button.clone(),
        bar.repeat_button.clone().upcast::<gtk4::ToggleButton>(),
    ] {
        assert!(toggle.has_css_class(crate::ui::style::buttons::TOGGLE_CLASS));
        assert!(!toggle.is_active(), "toggles start off");
    }

    bar.set_shuffle_indicator(true);
    bar.set_repeat_indicator(Repeat::All);
    assert!(bar.shuffle_button.is_active());
    assert!(bar.repeat_button.is_active());

    // Hover arrives and leaves again: the on-state outlives both.
    for toggle in [
        bar.shuffle_button.clone().upcast::<gtk4::Widget>(),
        bar.repeat_button.clone().upcast::<gtk4::Widget>(),
    ] {
        toggle.set_state_flags(gtk4::StateFlags::PRELIGHT, false);
        while gtk4::glib::MainContext::default().iteration(false) {}
        assert!(
            toggle.state_flags().contains(gtk4::StateFlags::CHECKED),
            "hover dropped the checked state"
        );
        toggle.unset_state_flags(gtk4::StateFlags::PRELIGHT);
        while gtk4::glib::MainContext::default().iteration(false) {}
        assert!(
            toggle.state_flags().contains(gtk4::StateFlags::CHECKED),
            "unhover dropped the checked state"
        );
    }

    // Repeat-one keeps the toggle on and swaps to the icon carrying the "1".
    bar.set_repeat_indicator(Repeat::One);
    assert!(bar.repeat_button.is_active());
    assert_eq!(
        bar.repeat_button.icon_name().as_deref(),
        Some(ICON_REPEAT_ONE)
    );
    // Off is the only mode that clears the state display.
    bar.set_repeat_indicator(Repeat::Off);
    assert!(!bar.repeat_button.is_active());
    assert_eq!(
        bar.repeat_button.icon_name().as_deref(),
        Some(ICON_REPEAT_ALL)
    );

    // The second cue is not a colour: a dot is painted under the icon, so the
    // on-state stays readable with colour vision deficiency.
    let css = crate::ui::style::buttons::css();
    assert!(css.contains("radial-gradient(circle"));
    assert!(css.contains("background-repeat: no-repeat"));

    window.close();
}
