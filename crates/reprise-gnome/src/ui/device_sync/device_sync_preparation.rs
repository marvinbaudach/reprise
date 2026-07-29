//! Design 7f's preparation phase, made visible on the device page (E9,
//! `MTP-43`).
//!
//! `MTP-42`'s `reprise_core::device_sync::preparation` already decided
//! everything about *whether* a preparation phase exists and what it is —
//! this module never re-derives that. It only:
//!
//! 1. Runs the actual downloads through `MTP-44`'s priority lane
//!    ([`PreparationDownloader`], adapted from `PodcastsRuntime` in
//!    production, faked in tests) when the primary button's action is
//!    `DownloadAndSync`.
//! 2. Tracks the GTK-only progress of that download run
//!    ([`PreparationRunState`]) — deliberately not a variant of
//!    `PlannedSyncPhase`, because the core transfer machine has no concept
//!    of podcast downloads and must not grow one just to display a step
//!    counter.
//! 3. Hands off to the existing transfer machine once every download has
//!    been attempted (successfully or not — a failed download simply stays
//!    `wanted_on_device` and is skipped by the transfer, exactly like
//!    `MTP-41`'s `waiting` set already handles a missing file).
//!
//! Cancelling only stops issuing *further* downloads — nothing here ever
//! deletes or rolls back a file that already finished, because nothing in
//! this module ever deletes a podcast download at all.

use std::future::Future;
use std::pin::Pin;

use reprise_core::device_sync::{preparation::MissingFile, PrimaryAction};

use super::*;

/// The GTK-visible progress of a preparation download run. Distinct from
/// `PlannedSyncPhase` on purpose — see the module doc. `pub`, not
/// `pub(super)`: `DeviceView` carries it, and `device_sync_page.rs`/
/// `device_sync_page_layout.rs` (siblings of `device_sync_runtime`, not
/// descendants) need to read it, the same way they already read
/// `PlannedSyncPhase`/`SyncStep` re-exported from `device_sync_types.rs`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum PreparationRunState {
    #[default]
    Idle,
    Downloading {
        /// Downloads fully attempted so far (success or failure both count
        /// once their terminal state arrives) — "done" in the sense of "no
        /// longer waiting on this one", not "succeeded".
        done: usize,
        total: usize,
        title: String,
        received_bytes: u64,
        total_bytes: Option<u64>,
    },
}

/// The seam `MTP-44`'s priority lane is reached through. Production code
/// adapts `PodcastsRuntime` ([`PodcastsPreparationDownloader`]); tests
/// substitute a recording double so the switch's *behavior* — does a
/// download actually get requested — is provable without a real worker
/// thread, real network I/O, or a test that can hang on a channel.
pub(in crate::ui::device_sync) trait PreparationDownloader {
    /// Requests `episode_id` be downloaded ahead of ordinary queued work and
    /// resolves once it reaches a terminal state (downloaded or failed).
    /// `on_progress` reports intermediate `(received_bytes, total_bytes)`
    /// samples; a downloader that only tracks a single terminal outcome may
    /// simply never call it.
    fn download(
        &self,
        episode_id: i64,
        on_progress: Rc<dyn Fn(u64, Option<u64>)>,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>>>>;
}

/// Adapts [`crate::ui::podcasts::PodcastsRuntime`] to [`PreparationDownloader`]
/// by building the same [`crate::ui::podcasts::PodcastsRequest`] the
/// podcasts page itself sends for a manual "Download" click — only with
/// `PodcastsPriority::High` (`MTP-44`) instead of `Normal`. There is no
/// second download path: this is the one `PodcastsOperation::Download`,
/// routed through the one worker thread, merely queued ahead of ordinary
/// work.
pub(super) struct PodcastsPreparationDownloader {
    runtime: Rc<crate::ui::podcasts::PodcastsRuntime>,
}

impl PodcastsPreparationDownloader {
    pub(super) fn new(runtime: Rc<crate::ui::podcasts::PodcastsRuntime>) -> Self {
        Self { runtime }
    }
}

impl PreparationDownloader for PodcastsPreparationDownloader {
    fn download(
        &self,
        episode_id: i64,
        on_progress: Rc<dyn Fn(u64, Option<u64>)>,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>>>> {
        use crate::ui::podcasts::{
            podcasts_response_channel, PodcastsOperation, PodcastsPriority, PodcastsRequest,
            PodcastsWorkerResult,
        };
        use reprise_core::podcasts::download_state::DownloadState;

        let runtime = self.runtime.clone();
        Box::pin(async move {
            let (response, receiver) = podcasts_response_channel();
            let queued = runtime.request(PodcastsRequest {
                generation: 0,
                operation: PodcastsOperation::Download { episode_id },
                priority: PodcastsPriority::High,
                response,
            });
            if !queued {
                return Err("could not queue preparation download".to_string());
            }
            loop {
                let response = receiver
                    .recv()
                    .await
                    .map_err(|_| "preparation download worker stopped".to_string())?;
                match response.result {
                    Ok(PodcastsWorkerResult::DownloadState { state, .. }) => match state {
                        DownloadState::Downloaded { .. } => return Ok(()),
                        DownloadState::Failed { message } => return Err(message),
                        DownloadState::Downloading {
                            received_bytes,
                            total_bytes,
                        } => on_progress(received_bytes, total_bytes),
                        DownloadState::Queued
                        | DownloadState::NotDownloaded
                        | DownloadState::Missing => {}
                    },
                    // A refresh/load-more response racing on the same
                    // channel shape never actually arrives here — each
                    // request gets its own response channel — but a defensive
                    // read never hangs waiting for a state that will not come.
                    Ok(_) => {}
                    Err(error) => return Err(error),
                }
            }
        })
    }
}

impl DeviceSyncRuntime {
    /// Wires the real download manager into preparation. Called once from
    /// `window.rs`, mirroring `bind_agent_device_sync`'s "construct, then
    /// bind" shape — every existing constructor and test fixture keeps
    /// working unbound, where preparation simply falls back to a plain sync
    /// (see [`begin_prepared_sync`]) instead of a mandatory constructor
    /// argument nothing but this one caller needs.
    pub(in crate::ui) fn bind_preparation_downloader(
        self: &Rc<Self>,
        podcasts: &Rc<crate::ui::podcasts::PodcastsRuntime>,
    ) {
        self.preparation_downloader
            .replace(Some(Rc::new(PodcastsPreparationDownloader::new(
                podcasts.clone(),
            ))));
    }

    #[cfg(test)]
    pub(in crate::ui::device_sync) fn bind_test_preparation_downloader(
        self: &Rc<Self>,
        downloader: Rc<dyn PreparationDownloader>,
    ) {
        self.preparation_downloader.replace(Some(downloader));
    }
}

/// Starts the download-then-sync path (`primary_action` answered
/// `DownloadAndSync`). Runs every missing file sequentially through the
/// bound [`PreparationDownloader`] — sequential, not parallel, because
/// `MTP-44`'s single worker thread would just serialize them anyway — then
/// hands off to the existing transfer machine.
pub(super) fn begin_prepared_sync(
    runtime: &Rc<DeviceSyncRuntime>,
    device_id: &str,
    missing: Vec<MissingFile>,
) {
    let Some(downloader) = runtime.preparation_downloader.borrow().clone() else {
        // Defensive only: production always binds one at startup. Falling
        // back to a plain sync means an unbound downloader degrades to
        // "skip preparation" rather than wedging the primary button.
        tracing::warn!(
            device_id,
            "no preparation downloader bound; starting synchronization without preparation"
        );
        if let Err(error) = runtime.start_transfer_now(device_id) {
            tracing::warn!(%error, "could not start Android synchronization");
        }
        return;
    };
    let cancel_flag = Rc::new(Cell::new(false));
    let total = missing.len();
    {
        let mut devices = runtime.device_states.borrow_mut();
        let Some(device) = devices
            .iter_mut()
            .find(|device| device.descriptor.id == device_id)
        else {
            return;
        };
        device.preparing = true;
        device.preparation_cancel = Some(cancel_flag.clone());
        device.preparation_run = PreparationRunState::Downloading {
            done: 0,
            total,
            title: missing
                .first()
                .map_or_else(String::new, |file| file.title.clone()),
            received_bytes: 0,
            total_bytes: None,
        };
    }
    runtime.notify();
    let weak = Rc::downgrade(runtime);
    let device_id = device_id.to_string();
    gtk4::glib::MainContext::ref_thread_default().spawn_local(async move {
        run_preparation_then_sync(weak, device_id, missing, downloader, cancel_flag).await;
    });
}

async fn run_preparation_then_sync(
    weak: Weak<DeviceSyncRuntime>,
    device_id: String,
    missing: Vec<MissingFile>,
    downloader: Rc<dyn PreparationDownloader>,
    cancel_flag: Rc<Cell<bool>>,
) {
    let total = missing.len();
    for (index, file) in missing.iter().enumerate() {
        if cancel_flag.get() {
            finish_preparation(&weak, &device_id, false);
            return;
        }
        let Some(runtime) = weak.upgrade() else {
            return;
        };
        set_preparation_progress(&runtime, &device_id, index, total, &file.title, 0, None);
        let on_progress: Rc<dyn Fn(u64, Option<u64>)> = {
            let weak = weak.clone();
            let device_id = device_id.clone();
            let title = file.title.clone();
            Rc::new(move |received_bytes, total_bytes| {
                if let Some(runtime) = weak.upgrade() {
                    set_preparation_progress(
                        &runtime,
                        &device_id,
                        index,
                        total,
                        &title,
                        received_bytes,
                        total_bytes,
                    );
                }
            })
        };
        if let Err(error) = downloader.download(file.episode_id, on_progress).await {
            tracing::warn!(
                episode_id = file.episode_id,
                %error,
                "preparation download failed; the episode stays wanted for the next attempt"
            );
        }
    }
    if cancel_flag.get() {
        finish_preparation(&weak, &device_id, false);
        return;
    }
    let Some(runtime) = weak.upgrade() else {
        return;
    };
    finish_preparation(&weak, &device_id, true);
    // The downloads just landed on disk, so the sync plan computed before
    // this loop started is stale — recompute before handing off, the same
    // discipline `resume_planned`/auto-start already follow.
    if let Err(error) = runtime.recompute_delta(&device_id) {
        tracing::warn!(%error, "could not refresh the sync plan after preparation downloads");
        return;
    }
    if let Err(error) = runtime.start_transfer_now(&device_id) {
        tracing::warn!(%error, "could not start synchronization after preparation downloads");
    }
}

fn set_preparation_progress(
    runtime: &Rc<DeviceSyncRuntime>,
    device_id: &str,
    done: usize,
    total: usize,
    title: &str,
    received_bytes: u64,
    total_bytes: Option<u64>,
) {
    {
        let mut devices = runtime.device_states.borrow_mut();
        let Some(device) = devices
            .iter_mut()
            .find(|device| device.descriptor.id == device_id)
        else {
            return;
        };
        if !device.preparing {
            return;
        }
        device.preparation_run = PreparationRunState::Downloading {
            done,
            total,
            title: title.to_string(),
            received_bytes,
            total_bytes,
        };
    }
    runtime.notify();
}

/// Clears the preparation-in-progress state. `success == false` means a
/// cancellation: the loop stops issuing further downloads, but every
/// episode that already finished stays exactly as downloaded — this
/// function does not touch files or `wanted_on_device` at all, it only
/// resets the GTK-side progress bookkeeping so the page returns to idle
/// instead of showing a stuck download bar.
fn finish_preparation(weak: &Weak<DeviceSyncRuntime>, device_id: &str, success: bool) {
    let Some(runtime) = weak.upgrade() else {
        return;
    };
    {
        let mut devices = runtime.device_states.borrow_mut();
        let Some(device) = devices
            .iter_mut()
            .find(|device| device.descriptor.id == device_id)
        else {
            return;
        };
        device.preparing = false;
        device.preparation_cancel = None;
        device.preparation_run = PreparationRunState::Idle;
        if success {
            device.prepared_this_run = true;
        } else {
            device.prepared_this_run = false;
            device.sync_phase = reprise_core::device_sync::PlannedSyncPhase::Idle;
        }
    }
    runtime.notify();
}

/// Whether `sync_now` should route through [`begin_prepared_sync`] rather
/// than starting the transfer directly — a thin, testable wrapper so
/// `sync_now`'s own branch reads as one call instead of re-deriving
/// `PrimaryAction`'s meaning inline.
pub(super) fn should_prepare(action: PrimaryAction, missing: &[MissingFile]) -> bool {
    matches!(action, PrimaryAction::DownloadAndSync) && !missing.is_empty()
}

/// Extends [`cancel_device_run`] to also stop a preparation download in
/// progress. Kept here (not inlined into `cancel_device_run` itself) so the
/// "what does cancel actually stop" list stays in the module that owns
/// preparation's own cancellation flag.
pub(super) fn cancel_preparation(device: &mut DeviceState) {
    if let Some(cancel) = &device.preparation_cancel {
        cancel.set(true);
    }
    cancel_device_run(device);
}
