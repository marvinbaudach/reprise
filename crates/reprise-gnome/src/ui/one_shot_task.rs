//! Named background work that produces exactly one result — optionally
//! reporting progress along the way.
//!
//! Both helpers here exist so UI modules never hand-roll a thread: a raw
//! `std::thread::spawn` loses the thread name that makes a hung worker
//! identifiable in a backtrace, and every ad-hoc progress channel re-derives
//! the same "latest wins" eviction rule. `check-architecture.sh` enforces
//! that for the listed consumer files.

/// Runs `task` on a named OS thread and returns the single-result receiver.
/// Dropping the receiver is cancellation-safe: the worker simply discards
/// its result. Long-lived workers that stream more than progress (a result
/// per item, say) still want their own channels; a single result plus
/// progress is what [`spawn_with_progress`] covers.
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

/// Like [`spawn`], but hands `task` a `publish` callback and returns the
/// progress stream alongside the result receiver.
///
/// Progress is **latest wins**: the channel holds a single slot, and a new
/// value evicts an unread stale one instead of blocking the worker. A UI that
/// renders every intermediate count would only be drawing frames nobody sees;
/// what matters is that it never falls behind, and that a slow or detached
/// reader can never stall the write loop. The final value is not special —
/// callers that must react to completion use the result receiver, which is
/// ordered after the last progress value by the drop below.
///
/// The progress channel closes when `task` returns, so a
/// `while let Ok(p) = progress.recv().await` loop terminates on its own.
pub(crate) fn spawn_with_progress<T, P, F>(
    name: &str,
    task: F,
) -> std::io::Result<(async_channel::Receiver<P>, async_channel::Receiver<T>)>
where
    T: Send + 'static,
    P: Send + 'static,
    F: FnOnce(&mut dyn FnMut(P)) -> T + Send + 'static,
{
    if name.as_bytes().contains(&0) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "thread name contains an interior NUL byte",
        ));
    }
    let (progress_sender, progress_receiver) = async_channel::bounded(1);
    let (result_sender, result_receiver) = async_channel::bounded(1);
    let stale_receiver = progress_receiver.clone();
    std::thread::Builder::new()
        .name(name.to_string())
        .spawn(move || {
            let result = {
                let mut publish =
                    |value: P| publish_latest(&progress_sender, &stale_receiver, value);
                task(&mut publish)
            };
            // Ends the consumer's progress loop before the result lands, so a
            // caller awaiting both sees no progress value after completion.
            drop(progress_sender);
            drop(stale_receiver);
            let _ = result_sender.send_blocking(result);
        })?;
    Ok((progress_receiver, result_receiver))
}

/// Sends `value`, evicting an unread stale value rather than blocking.
///
/// `receiver` is the worker's own clone of the progress receiver, used purely
/// to make room; consuming from it cannot steal a value from the UI, because
/// the slot it drains is by definition one the UI has not read and is about
/// to be superseded anyway.
fn publish_latest<T>(
    sender: &async_channel::Sender<T>,
    receiver: &async_channel::Receiver<T>,
    value: T,
) {
    match sender.try_send(value) {
        Ok(()) => {}
        Err(async_channel::TrySendError::Full(value)) => {
            let _ = receiver.try_recv();
            if let Err(error) = sender.try_send(value) {
                tracing::warn!(%error, "progress dropped: the receiver is gone");
            }
        }
        Err(async_channel::TrySendError::Closed(_)) => {
            tracing::warn!("progress dropped: the receiver is gone");
        }
    }
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

    #[test]
    fn progress_task_reports_progress_and_then_the_result() {
        let (progress, result) = super::spawn_with_progress("reprise-progress-test", |publish| {
            publish(1_usize);
            publish(2);
            "done"
        })
        .unwrap();

        // Drain everything the reader actually observed. Latest-wins does NOT
        // promise the reader outruns the writer and only ever sees the final
        // value — reading between the two publishes legitimately yields 1
        // first. What it DOES promise is that a stale value never appears
        // after a newer one, and that the last value before the stream closes
        // is the final published one. Asserting a single recv == 2 was racy
        // and flaked ~half the time; assert the real contract instead.
        let mut seen = Vec::new();
        while let Ok(value) = progress.recv_blocking() {
            seen.push(value);
        }
        assert_eq!(seen.last().copied(), Some(2), "final value must be 2");
        assert!(
            seen.windows(2).all(|pair| pair[0] <= pair[1]),
            "values must never go backwards: {seen:?}"
        );
        // The stream closed on the worker's return rather than hanging.
        assert!(progress.recv_blocking().is_err());
        assert_eq!(result.recv_blocking().unwrap(), "done");
    }

    #[test]
    fn progress_task_never_blocks_on_an_unread_slot() {
        // 100 publishes into a single-slot channel nobody is draining: if
        // eviction were missing, the worker would deadlock here.
        let (_progress, result) = super::spawn_with_progress("reprise-progress-flood", |publish| {
            for value in 0..100_usize {
                publish(value);
            }
            "survived"
        })
        .unwrap();

        assert_eq!(result.recv_blocking().unwrap(), "survived");
    }

    #[test]
    fn invalid_thread_name_is_reported_by_the_progress_variant_too() {
        let spawned =
            super::spawn_with_progress("invalid\0thread", |publish: &mut dyn FnMut(())| {
                publish(());
            });
        assert!(spawned.is_err());
    }
}
