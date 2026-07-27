use super::*;

pub type BackendFuture<T> = Pin<Box<dyn Future<Output = Result<T, String>>>>;
pub(super) type StateCallback = Rc<dyn Fn(DeviceSyncState)>;

pub trait DeviceBackend {
    fn devices(&self) -> Vec<DeviceDescriptor>;
    fn subscribe_devices(&self, callback: Rc<dyn Fn(Vec<DeviceDescriptor>)>);
    fn inspect(
        &self,
        root_uri: String,
    ) -> BackendFuture<(DeviceContents, Option<u64>, Option<u64>)>;
    #[allow(clippy::too_many_arguments)]
    fn copy_track(
        &self,
        device_id: String,
        root_uri: String,
        source_path: PathBuf,
        relative_target: String,
        expected_size: u64,
        cancellable: gio::Cancellable,
        progress: Rc<dyn Fn(u64, u64)>,
    ) -> BackendFuture<CopyOutcome>;
    #[allow(clippy::too_many_arguments)]
    fn replace_track(
        &self,
        device_id: String,
        root_uri: String,
        source_path: PathBuf,
        relative_target: String,
        expected_size: u64,
        cancellable: gio::Cancellable,
        progress: Rc<dyn Fn(u64, u64)>,
    ) -> BackendFuture<CopyOutcome> {
        self.copy_track(
            device_id,
            root_uri,
            source_path,
            relative_target,
            expected_size,
            cancellable,
            progress,
        )
    }
    fn cleanup_partials(&self, _root_uri: String) -> BackendFuture<u32> {
        Box::pin(async { Ok(0) })
    }
    fn cleanup_managed_partials(
        &self,
        root_uri: String,
        root: reprise_core::device_sync::ManagedRoot,
    ) -> BackendFuture<u32> {
        match root {
            reprise_core::device_sync::ManagedRoot::Music => self.cleanup_partials(root_uri),
            reprise_core::device_sync::ManagedRoot::Podcasts => {
                Box::pin(async { Err("podcast cleanup is unavailable".into()) })
            }
        }
    }
    fn delete_track(&self, _root_uri: String, _relative_target: String) -> BackendFuture<bool> {
        Box::pin(async { Err("device deletion is unavailable".into()) })
    }
    fn delete_managed(
        &self,
        root_uri: String,
        root: reprise_core::device_sync::ManagedRoot,
        relative_target: String,
    ) -> BackendFuture<bool> {
        match root {
            reprise_core::device_sync::ManagedRoot::Music => {
                self.delete_track(root_uri, relative_target)
            }
            reprise_core::device_sync::ManagedRoot::Podcasts => {
                Box::pin(async { Err("podcast deletion is unavailable".into()) })
            }
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn replace_managed(
        &self,
        device_id: String,
        root_uri: String,
        root: reprise_core::device_sync::ManagedRoot,
        source_path: PathBuf,
        relative_target: String,
        expected_size: u64,
        cancellable: gio::Cancellable,
        progress: Rc<dyn Fn(u64, u64)>,
    ) -> BackendFuture<CopyOutcome> {
        match root {
            reprise_core::device_sync::ManagedRoot::Music => self.replace_track(
                device_id,
                root_uri,
                source_path,
                relative_target,
                expected_size,
                cancellable,
                progress,
            ),
            reprise_core::device_sync::ManagedRoot::Podcasts => {
                Box::pin(async { Err("podcast transfer is unavailable".into()) })
            }
        }
    }
    fn start_transcodes(
        &self,
        requests: Vec<EncodeRequest>,
        cancelled: Arc<AtomicBool>,
    ) -> Result<async_channel::Receiver<EncodeOutcome>, String> {
        let _ = (requests, cancelled);
        Err("Opus transcoding is unavailable".into())
    }
    fn read_playlist(&self, root_uri: String, name: String) -> BackendFuture<Vec<M3uEntry>>;
    fn replace_playlist(
        &self,
        device_id: String,
        root_uri: String,
        name: String,
        contents: Vec<u8>,
    ) -> BackendFuture<()>;
    fn eject(&self, _device_id: String) -> BackendFuture<bool> {
        Box::pin(async { Ok(false) })
    }
}

#[derive(Clone, Debug)]
pub struct DeviceView {
    pub id: String,
    pub name: String,
    pub icon: gio::Icon,
    pub connected: bool,
    pub available_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
    pub contents: DeviceContents,
    pub scanning: bool,
    pub scan_error: Option<String>,
    pub draft_playlists: Vec<String>,
    pub last_enqueue: Option<EnqueueReceipt>,
    pub snapshot: SyncSnapshot,
    pub settings: DeviceSettings,
    pub delta: Option<SyncDelta>,
    pub sync_phase: PlannedSyncPhase,
    pub sync_error: Option<SyncFailure>,
    pub last_sync: Option<chrono::DateTime<chrono::Utc>>,
    pub tracks: Vec<DeviceTrackView>,
    pub selected_track_count: usize,
    pub podcast_sync: PodcastSyncSummary,
    pub bytes_per_second: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PodcastSyncSummary {
    pub selected: usize,
    pub to_copy: usize,
    pub to_remove: usize,
    pub bytes: u64,
}

impl DeviceView {
    pub fn has_sync_selection(&self) -> bool {
        matches!(self.settings.selection, DeviceSelection::EntireLibrary)
            || matches!(
                &self.settings.selection,
                DeviceSelection::Sources(sources) if !sources.is_empty()
            )
            || self.podcast_sync.selected > 0
    }

    pub fn has_pending_sync(&self) -> bool {
        self.delta
            .as_ref()
            .is_some_and(|delta| !delta.to_copy.is_empty() || !delta.to_remove.is_empty())
            || self.podcast_sync.to_copy > 0
            || self.podcast_sync.to_remove > 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceTrackStatus {
    Queued,
    Remove,
    Synced,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceTrackView {
    pub track_id: i64,
    pub title: String,
    pub artist: String,
    pub device_path: String,
    pub size: u64,
    pub duration_ms: i64,
    pub status: DeviceTrackStatus,
    pub pinned: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncStep {
    Removing,
    Transcoding,
    Copying,
    WritingPlaylists,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlannedSyncPhase {
    Idle,
    ComputingDelta,
    Syncing {
        step: SyncStep,
        done: u32,
        total: u32,
        current_track: String,
        bytes_done: u64,
        bytes_total: u64,
    },
    Finishing,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncFailure {
    pub message: String,
    pub failed_tracks: Vec<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceSelectionOption {
    pub source: reprise_core::device_sync::SelectionSource,
    pub name: String,
    pub track_count: usize,
    pub smart: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnqueueReceipt {
    pub playlist: String,
    pub track_count: usize,
    pub queue_position: usize,
}

#[derive(Clone, Debug, Default)]
pub struct DeviceSyncState {
    pub devices: Vec<DeviceView>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EnqueueError {
    UnknownDevice,
    Busy,
    NoUsableTracks,
    InsufficientSpace {
        required_bytes: u64,
        available_bytes: u64,
    },
    Database(String),
}

impl fmt::Display for EnqueueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownDevice => formatter.write_str("device is not connected"),
            Self::Busy => formatter.write_str("device synchronization is already active"),
            Self::NoUsableTracks => formatter.write_str("no available tracks were selected"),
            Self::InsufficientSpace {
                required_bytes,
                available_bytes,
            } => write!(
                formatter,
                "copy needs {required_bytes} bytes but only {available_bytes} bytes are available"
            ),
            Self::Database(error) => {
                write!(formatter, "could not resolve selected tracks: {error}")
            }
        }
    }
}

impl std::error::Error for EnqueueError {}

pub struct Subscription {
    pub(super) cancel: RefCell<Option<Box<dyn FnOnce()>>>,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(cancel) = self.cancel.borrow_mut().take() {
            cancel();
        }
    }
}

impl Subscription {
    /// Keeps this subscription alive for the whole life of `widget`, dropping
    /// it (and thereby unsubscribing) only when the widget is destroyed.
    ///
    /// Deliberately `connect_destroy`, NOT `connect_unrealize`: GTK4 widgets
    /// unrealize routinely while staying alive — the sidebar's split view
    /// flips its collapsed state during window construction, folding
    /// reparents children, and each such step unrealizes them. An earlier
    /// version dropped the subscription on the first unrealize, which froze
    /// the sidebar device card on its very first render (stale name, stuck
    /// "Checking…") while the rest of the UI kept updating.
    pub(in crate::ui) fn retain_for_widget(self, widget: &impl IsA<gtk4::Widget>) {
        use gtk4::prelude::WidgetExt;
        let subscription = RefCell::new(Some(self));
        widget.connect_destroy(move |_| {
            subscription.borrow_mut().take();
        });
    }
}
