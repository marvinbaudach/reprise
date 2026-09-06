//! Small synchronous phase timer shared by playback instrumentation.

use std::rc::Rc;

pub(super) struct QueueListenerTimes {
    pub(super) queue_model_ms: u128,
    pub(super) sidebar_queue_reload_ms: u128,
    pub(super) now_playing_ms: u128,
    pub(super) total_ms: u128,
}

pub(super) fn timed<T>(operation: impl FnOnce() -> T) -> (T, u128) {
    let started = std::time::Instant::now();
    let result = operation();
    (result, started.elapsed().as_millis())
}

pub(super) fn time_queue_listeners(callbacks: Vec<Rc<dyn Fn()>>) -> QueueListenerTimes {
    let mut elapsed = Vec::with_capacity(callbacks.len());
    for callback in callbacks {
        let ((), elapsed_ms) = timed(|| callback());
        elapsed.push(elapsed_ms);
    }
    QueueListenerTimes {
        queue_model_ms: elapsed.first().copied().unwrap_or(0),
        sidebar_queue_reload_ms: elapsed.get(1).copied().unwrap_or(0),
        now_playing_ms: elapsed.get(2).copied().unwrap_or(0),
        total_ms: elapsed.iter().sum(),
    }
}

pub(super) fn remaining_ms(started: std::time::Instant, measured: &[u128]) -> u128 {
    measured
        .iter()
        .fold(started.elapsed().as_millis(), |total, phase| {
            total.saturating_sub(*phase)
        })
}
