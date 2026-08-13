//! Admission policy for the bounded source-artwork worker queue.

/// Submits one request without blocking the GTK thread.
///
/// Returning `false` means the worker side is unavailable. A full but live
/// queue is not an unavailable worker pool: callers still own a visible image
/// request that must eventually be admitted.
pub(super) fn submit<T: 'static>(
    queue: async_channel::Sender<T>,
    task: T,
    request: String,
) -> bool {
    match queue.try_send(task) {
        Ok(()) => true,
        Err(async_channel::TrySendError::Full(task)) => {
            tracing::debug!(%request, "source artwork request is waiting for worker capacity");
            gtk4::glib::MainContext::ref_thread_default().spawn_local(async move {
                if queue.send(task).await.is_ok() {
                    tracing::debug!(%request, "source artwork request entered worker queue after waiting");
                } else {
                    tracing::warn!("source artwork worker queue closed while waiting for capacity");
                }
            });
            true
        }
        Err(async_channel::TrySendError::Closed(_)) => false,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn full_queue_retries_and_satisfies_the_next_visible_artwork_request() {
        let context = gtk4::glib::MainContext::new();
        context
            .with_thread_default(|| {
                let (queue, worker) = async_channel::bounded(1);
                let (first_response, _first_result) = async_channel::bounded(1);
                queue
                    .try_send(super::super::ArtworkTask {
                        url: "https://images.test/first.png".into(),
                        cache_scope: reprise_core::remote_image::CacheScope::Persistent,
                        width: 40,
                        height: 40,
                        response: first_response,
                    })
                    .unwrap();
                let (second_response, second_result) = async_channel::bounded(1);

                assert!(
                    super::submit(
                        queue,
                        super::super::ArtworkTask {
                            url: "https://images.test/second.png".into(),
                            cache_scope: reprise_core::remote_image::CacheScope::Persistent,
                            width: 40,
                            height: 40,
                            response: second_response,
                        },
                        "https://images.test/second.png".into(),
                    ),
                    "a full live queue must retain the request instead of refusing it"
                );
                assert_eq!(
                    worker.try_recv().unwrap().url,
                    "https://images.test/first.png"
                );
                let second = context.block_on(futures_lite::future::race(
                    async { worker.recv().await.ok() },
                    async {
                        gtk4::glib::timeout_future(std::time::Duration::from_secs(1)).await;
                        None
                    },
                ));
                let second =
                    second.expect("the overflowed request must enter after capacity frees");
                assert_eq!(second.url, "https://images.test/second.png");
                second.response.send_blocking(None).unwrap();
                assert!(
                    matches!(second_result.try_recv(), Ok(None)),
                    "the original requester must receive the worker's answer"
                );
            })
            .unwrap();
    }
}
