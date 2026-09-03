use super::*;

pub type BackendFuture<T> = Pin<Box<dyn Future<Output = Result<T, String>>>>;
pub(super) type StateCallback = Rc<dyn Fn(DeviceSyncState)>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum RefreshPurpose {
    Normal,
    VerifySync(Vec<SelectionSource>),
}

/// The injectable MTP transport seam (`MTP-23`). Every method takes the
/// resolved playlists `target_path` explicitly rather than picking a
/// hard-coded root.
///
/// [`GioDeviceBackend`](crate::ui::device_sync::device_sync_backend::GioDeviceBackend) is the
/// real GVfs/MTP implementation; tests drive a recording double instead
/// (see `FakeBackend` in `device_sync_runtime_tests.rs`), so no test in this
/// module needs a real or simulated phone.
pub trait DeviceBackend {
    fn devices(&self) -> Vec<DeviceDescriptor>;
    fn subscribe_devices(&self, callback: Rc<dyn Fn(Vec<DeviceDescriptor>)>);
    /// The persisted playlists target resolved by the folder browser.
    fn inspect(
        &self,
        root_uri: String,
        target: SyncTarget,
    ) -> BackendFuture<DeviceStorageInspection>;
    fn read_managed_file(
        &self,
        _root_uri: String,
        _target_path: String,
        _storage_id: Option<StorageId>,
        _relative_path: String,
    ) -> BackendFuture<Option<Vec<u8>>> {
        Box::pin(async { Err("device reads are unavailable".into()) })
    }
    /// A real backend must return every present, non-empty regular file among
    /// the requested paths and report a whole-probe failure as `Err`. The
    /// empty default is compatibility-only: the caller can keep
    /// `residency_proven` disarmed on failure only when this contract is kept.
    fn probe_managed_files(
        &self,
        _root_uri: String,
        _target_path: String,
        _storage_id: Option<StorageId>,
        _relative_paths: Vec<String>,
    ) -> BackendFuture<Vec<ManagedDeviceFile>> {
        Box::pin(async { Ok(Vec::new()) })
    }
    /// A real backend must prove that the managed target is a directory before
    /// path probes can arm residency and return `false` or `Err` otherwise.
    /// The optimistic default is compatibility-only: the caller's
    /// `residency_proven` handling depends on a real backend keeping this
    /// contract.
    fn managed_target_exists(
        &self,
        _root_uri: String,
        _target_path: String,
        _storage_id: Option<StorageId>,
    ) -> BackendFuture<bool> {
        Box::pin(async { Ok(true) })
    }
    /// Copies (or overwrites) `source_path` to `relative_target` under
    /// `target_path` on `storage_id` (`None` falls back to the same
    /// "prefer internal" default used before a target was ever repointed),
    /// always replacing any existing file even when its byte count happens
    /// to be unchanged.
    #[allow(clippy::too_many_arguments)]
    fn replace_track(
        &self,
        device_id: String,
        root_uri: String,
        target_path: String,
        storage_id: Option<StorageId>,
        source_path: PathBuf,
        relative_target: String,
        expected_size: u64,
        cancellable: gio::Cancellable,
        progress: Rc<dyn Fn(u64, u64)>,
    ) -> BackendFuture<CopyOutcome>;
    fn cleanup_partials(
        &self,
        _root_uri: String,
        _target_path: String,
        _storage_id: Option<StorageId>,
        _partial_paths: Vec<String>,
    ) -> BackendFuture<u32> {
        Box::pin(async { Ok(0) })
    }
    fn delete_track(
        &self,
        _root_uri: String,
        _target_path: String,
        _storage_id: Option<StorageId>,
        _relative_target: String,
    ) -> BackendFuture<bool> {
        Box::pin(async { Err("device deletion is unavailable".into()) })
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
        target_path: String,
        storage_id: Option<StorageId>,
        name: String,
        contents: Vec<u8>,
    ) -> BackendFuture<()>;
    fn eject(&self, _device_id: String) -> BackendFuture<bool> {
        Box::pin(async { Ok(false) })
    }
    /// `MTP-31` (design 7d): every browsable storage volume at the device
    /// root ("Internal shared storage", "SD card"). Re-listed fresh on
    /// every browser open — see `reprise_core::device_sync::browser`'s
    /// module docs on why nothing MTP-derived is cached across calls.
    fn list_storages(&self, _root_uri: String) -> BackendFuture<Vec<StorageOption>> {
        Box::pin(async { Err("storage browsing is unavailable".into()) })
    }
    /// `MTP-31`: the immediate child folders of `path` on `storage`.
    fn list_folders(
        &self,
        _root_uri: String,
        _storage: StorageId,
        _path: String,
    ) -> BackendFuture<Vec<String>> {
        Box::pin(async { Err("folder browsing is unavailable".into()) })
    }
    /// `MTP-31`'s "New folder". Devices that refuse creation directly at a
    /// storage's own top level surface a distinct error the dialog can
    /// explain rather than a generic failure.
    fn create_folder(
        &self,
        _root_uri: String,
        _storage: StorageId,
        _path: String,
        _name: String,
    ) -> BackendFuture<()> {
        Box::pin(async { Err("folder creation is unavailable".into()) })
    }
    /// `MTP-32`: relocates an already-synced target folder in one MTP move
    /// instead of the sync layer re-copying every file under it. Only
    /// called when `target_relocation_action` resolves to `MoveFolder`.
    fn move_folder(
        &self,
        _root_uri: String,
        _storage: StorageId,
        _from_path: String,
        _to_path: String,
    ) -> BackendFuture<()> {
        Box::pin(async { Err("folder relocation is unavailable".into()) })
    }
}

#[derive(Clone, Debug)]
pub struct DeviceView {
    pub id: String,
    pub name: String,
    pub icon: gio::Icon,
    pub connected: bool,
    /// `MTP-49`: whether changes can be attached to a stable device key.
    pub rememberable: bool,
    /// Honest user-facing explanation when the platform exposed no stable key.
    pub memory_status: Option<String>,
    /// `MTP-48`: whether this detected device owns the sole MTP session or
    /// is only listed while another connected device owns it.
    pub session_state: reprise_core::device_sync::DeviceSessionState,
    pub storage: DeviceStorageSnapshot,
    /// Whether `storage` came from a successful inspection in this session.
    pub storage_measured: bool,
    pub scan_error: Option<String>,
    pub settings: DeviceSettings,
    pub sync_phase: PlannedSyncPhase,
    pub sync_error: Option<SyncFailure>,
    pub last_sync: Option<chrono::DateTime<chrono::Utc>>,
    pub verified_managed_track_count: Option<usize>,
    /// Last verified total across Reprise-owned target folders. For a
    /// remembered device this is history, not a live storage reading.
    pub size_on_device_bytes: Option<u64>,
    pub managed_track_count: usize,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub bytes_per_second: u64,
    pub units_done: u32,
    pub units_total: u32,
    pub estimated_remaining: Option<std::time::Duration>,
    pub page: SyncPageState,
    /// `MTP-26` (design 7a): whether this device's on-device contents have
    /// ever been successfully inspected this session.
    pub contents_state: reprise_core::device_sync::device_view::DeviceContentsState,
    pub content_row: reprise_core::device_sync::device_view::CategoryContentRow,
    pub target_reading: reprise_core::device_sync::MusicReading,
    /// Whether selected smart playlists follow their live definition.
    pub keep_smart_playlists_updated: bool,
}

/// Test-only "nothing known yet" baseline for the playlists target.
#[cfg(test)]
pub(in crate::ui) fn empty_target_reading() -> reprise_core::device_sync::MusicReading {
    reprise_core::device_sync::device_view::project_device_music_reading(
        reprise_core::device_sync::MusicDiff::default(),
    )
}

#[cfg(test)]
pub(in crate::ui) fn empty_content_row(
) -> reprise_core::device_sync::device_view::CategoryContentRow {
    reprise_core::device_sync::device_view::project_category_content_row(
        &reprise_core::device_sync::SyncTarget::default(),
        0,
        0,
    )
}

// The run's phase is produced by the core state machine, not by this
// frontend. Re-exported here so the widgets keep their existing import path.
pub use reprise_core::device_sync::{PlannedSyncPhase, SyncStep};

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
