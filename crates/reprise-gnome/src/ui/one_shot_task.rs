//! Named background work that produces exactly one result.

/// Runs `task` on a named OS thread and returns the single-result receiver.
/// Dropping the receiver is cancellation-safe: the worker simply discards
/// its result. Long-lived workers and progress streams deliberately use
/// their own channels instead of this helper.
pub(crate) fn spawn<T, F>(name: &str, task: F) -> std::io::Result<async_channel::Receiver<T>>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    if name.as_bytes().contains(&0) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "thread name contains an interior NUL byte",
        ));
    }
    let (sender, receiver) = async_channel::bounded(1);
    std::thread::Builder::new()
        .name(name.to_string())
        .spawn(move || {
            let _ = sender.send_blocking(task());
        })?;
    Ok(receiver)
}

#[cfg(test)]
mod tests {
    #[test]
    fn one_shot_task_delivers_the_worker_result() {
        let receiver = super::spawn("reprise-one-shot-test", || 42).unwrap();

        assert_eq!(receiver.recv_blocking().unwrap(), 42);
    }

    #[test]
    fn invalid_thread_name_is_reported_to_the_caller() {
        assert!(super::spawn("invalid\0thread", || ()).is_err());
    }
}
