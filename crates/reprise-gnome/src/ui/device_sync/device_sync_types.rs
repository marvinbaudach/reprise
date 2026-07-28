use super::*;

pub type BackendFuture<T> = Pin<Box<dyn Future<Output = Result<T, String>>>>;
pub(super) type StateCallback = Rc<dyn Fn(DeviceSyncState)>;

pub trait DeviceBackend {
    fn devices(&self) -> Vec<DeviceDescriptor>;
    fn subscribe_devices(&self, callback: Rc<dyn Fn(Vec<DeviceDescriptor>)>);
    fn inspect(&self, root_uri: String) -> BackendFuture<DeviceStorageInspection>;
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
    fn probe_transcode(&self, _profile: TranscodeProfile) -> Result<(), String> {
        Err("audio transcoding is unavailable".into())
    }
    fn transcode_track(
        &self,
        request: TranscodeRequest,
        cancelled: Arc<AtomicBool>,
    ) -> BackendFuture<TranscodedFile> {
        let _ = (request, cancelled);
        Box::pin(async { Err("audio transcoding is unavailable".into()) })
    }
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
    pub storage: DeviceStorageSnapshot,
    pub scan_error: Option<String>,
    pub settings: DeviceSettings,
    pub sync_phase: PlannedSyncPhase,
    pub sync_error: Option<SyncFailure>,
    pub last_sync: Option<chrono::DateTime<chrono::Utc>>,
    pub verified_managed_track_count: Option<usize>,
    pub managed_track_count: usize,
    pub bytes_per_second: u64,
    pub page: SyncPageState,
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

#[derive(Clone, Debug, Default)]
pub struct DeviceSyncState {
    pub devices: Vec<DeviceView>,
}

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
