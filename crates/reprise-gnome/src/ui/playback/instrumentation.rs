//! Small synchronous phase timer shared by playback instrumentation.

pub(super) fn timed<T>(operation: impl FnOnce() -> T) -> (T, u128) {
    let started = std::time::Instant::now();
    let result = operation();
    (result, started.elapsed().as_millis())
}
