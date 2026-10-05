//! Session-only sleep-timer wiring.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use reprise_view::sleep_timer::SleepTimer;

use super::RuntimeWiring;
use crate::ui::playback::sleep_timer_hooks::{monotonic_now, SleepTimerBinding};
use crate::ui::player_bar::sleep_timer_button::SleepTimerChoice;
use crate::ui::player_controller::PlayerController;

const TICK_INTERVAL: Duration = Duration::from_secs(1);

pub(super) fn wire_sleep_timer(w: &RuntimeWiring<'_>) {
    let Some(player) = w.player else {
        return;
    };
    let timer = Rc::new(std::cell::RefCell::new(SleepTimer::off()));
    let binding = SleepTimerBinding::new(timer);
    let generation = Rc::new(Cell::new(0u64));
    player.install_sleep_timer(&binding);
    player.sync_sleep_timer_button(&binding, monotonic_now());

    let player_weak = Rc::downgrade(player);
    player.bar.connect_sleep_timer_choice({
        let binding = binding.clone();
        let generation = generation.clone();
        move |choice| {
            let Some(player) = player_weak.upgrade() else {
                return;
            };
            let now = monotonic_now();
            let armed = match choice {
                SleepTimerChoice::Minutes(minutes) => {
                    player.arm_sleep_timer_minutes(&binding, now, minutes);
                    true
                }
                SleepTimerChoice::EndOfTrack => player.arm_sleep_timer_end_of_track(&binding),
                SleepTimerChoice::Cancel => {
                    player.cancel_sleep_timer(&binding);
                    false
                }
            };
            player.sync_sleep_timer_button(&binding, now);
            let next_generation = generation.get().wrapping_add(1);
            generation.set(next_generation);
            if armed {
                start_ticks(&player, &binding, &generation, next_generation);
            }
        }
    });
}

fn start_ticks(
    player: &Rc<PlayerController>,
    binding: &Rc<SleepTimerBinding>,
    generation: &Rc<Cell<u64>>,
    expected_generation: u64,
) {
    let player = Rc::downgrade(player);
    let binding = Rc::downgrade(binding);
    let generation = generation.clone();
    gtk4::glib::timeout_add_local(TICK_INTERVAL, move || {
        if generation.get() != expected_generation {
            return gtk4::glib::ControlFlow::Break;
        }
        let (Some(player), Some(binding)) = (player.upgrade(), binding.upgrade()) else {
            return gtk4::glib::ControlFlow::Break;
        };
        player.sleep_timer_tick(&binding, monotonic_now(), 0, 0);
        if binding.timer.borrow().is_armed() {
            gtk4::glib::ControlFlow::Continue
        } else {
            gtk4::glib::ControlFlow::Break
        }
    });
}
