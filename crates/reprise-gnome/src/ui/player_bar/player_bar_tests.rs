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
fn mot_5_play_pause_pulses_on_state_change() {
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

    run_main_loop_for(motion::half(motion::MICRO) + 20);
    assert!(bar.play_pause_button.has_css_class("pulsing"));
    run_main_loop_for(motion::half(motion::MICRO) + 20);
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

    bar.set_transport_enabled(true);
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
    bar.set_transport_enabled(true);
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
