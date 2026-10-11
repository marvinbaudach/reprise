//! The mailbox a podcast worker job answers the UI through.
//!
//! Progress is coalesced: an unread progress update is replaced by a newer
//! one, so a fast download cannot flood the main loop. A terminal response is
//! never replaced. A fill-up job reports several episodes over one mailbox, and
//! episode 1's `Downloaded` must still arrive after episode 2 has started —
//! a single overwritable slot lost it and left the row "Downloading" forever.
//! Publishing never blocks, so a worker cannot hang on a UI that went away.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, PoisonError};

use super::PodcastsResponse;

#[derive(Debug, Default)]
struct Mailbox {
    responses: VecDeque<PodcastsResponse>,
    /// Whether the newest unread response is progress that a newer progress
    /// update may replace.
    tail_is_progress: bool,
}

#[derive(Debug)]
pub(in crate::ui) struct PodcastsResponseChannel {
    mailbox: Arc<Mutex<Mailbox>>,
    wake: async_channel::Sender<()>,
}

#[derive(Debug)]
pub(in crate::ui) struct PodcastsResponseReceiver {
    mailbox: Arc<Mutex<Mailbox>>,
    wake: async_channel::Receiver<()>,
}

pub(in crate::ui) fn podcasts_response_channel(
) -> (PodcastsResponseChannel, PodcastsResponseReceiver) {
    let mailbox = Arc::new(Mutex::new(Mailbox::default()));
    let (sender, receiver) = async_channel::bounded(1);
    (
        PodcastsResponseChannel {
            mailbox: Arc::clone(&mailbox),
            wake: sender,
        },
        PodcastsResponseReceiver {
            mailbox,
            wake: receiver,
        },
    )
}

impl PodcastsResponseChannel {
    pub(super) fn publish_latest(&self, response: PodcastsResponse) {
        self.publish(response, true);
    }

    pub(super) fn publish_terminal(&self, response: PodcastsResponse) {
        self.publish(response, false);
    }

    fn publish(&self, response: PodcastsResponse, progress: bool) {
        if self.wake.is_closed() {
            tracing::debug!("podcast worker response receiver is unavailable");
            return;
        }
        {
            let mut mailbox = self.mailbox.lock().unwrap_or_else(PoisonError::into_inner);
            if progress && mailbox.tail_is_progress {
                mailbox.responses.pop_back();
            }
            mailbox.responses.push_back(response);
            mailbox.tail_is_progress = progress;
        }
        // One pending wake-up is enough: the receiver drains the whole mailbox.
        let _ = self.wake.force_send(());
    }
}

impl PodcastsResponseReceiver {
    pub(in crate::ui) async fn recv(&self) -> Result<PodcastsResponse, async_channel::RecvError> {
        loop {
            if let Some(response) = self.pop() {
                return Ok(response);
            }
            self.wake.recv().await?;
        }
    }

    #[cfg(test)]
    pub(in crate::ui) fn try_recv(&self) -> Result<PodcastsResponse, async_channel::TryRecvError> {
        if let Some(response) = self.pop() {
            return Ok(response);
        }
        if self.wake.is_closed() {
            return Err(async_channel::TryRecvError::Closed);
        }
        Err(async_channel::TryRecvError::Empty)
    }

    fn pop(&self) -> Option<PodcastsResponse> {
        let mut mailbox = self.mailbox.lock().unwrap_or_else(PoisonError::into_inner);
        let response = mailbox.responses.pop_front();
        if mailbox.responses.is_empty() {
            // The progress tail was just read; the next progress update must
            // not replace whatever is published after it.
            mailbox.tail_is_progress = false;
        }
        response
    }
}
