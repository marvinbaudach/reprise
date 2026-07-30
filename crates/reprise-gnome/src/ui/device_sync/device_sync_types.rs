use super::*;

pub type BackendFuture<T> = Pin<Box<dyn Future<Output = Result<T, String>>>>;
pub(super) type StateCallback = Rc<dyn Fn(DeviceSyncState)>;

/// The injectable MTP transport seam (`MTP-23`). Every method takes the
/// resolved absolute `target_path` of one of the three named sync targets
/// (`MTP-38`, e.g. `/Music/Reprise-YouTube`) explicitly, rather than
/// picking a hard-coded root — that is what makes routing "the actual
/// transfer through three named targets" a caller-side decision instead of
/// something this trait has to special-case per content kind.
///
/// [`GioDeviceBackend`](crate::ui::device_sync::device_sync_backend::GioDeviceBackend) is the
/// real GVfs/MTP implementation; tests drive a recording double instead
/// (see `FakeBackend` in `device_sync_runtime_tests.rs`), so no test in this
/// module needs a real or simulated phone.
pub trait DeviceBackend {
    fn devices(&self) -> Vec<DeviceDescriptor>;
    fn subscribe_devices(&self, callback: Rc<dyn Fn(Vec<DeviceDescriptor>)>);
    /// `targets`: the device's three named sync targets (`MTP-38`), each
    /// carrying the persisted `storage_id`/path the folder browser (`MTP-31`)
    /// resolved for it — inspection walks exactly those, never a
    /// hard-coded folder name, so a repointed target is recognized as its
    /// own category's inventory (`MTP-1` finding: storage/folder honoured
    /// end to end).
    fn inspect(
        &self,
        root_uri: String,
        targets: [SyncTarget; 3],
    ) -> BackendFuture<DeviceStorageInspection>;
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
    /// `MTP-47`: whether this detected device owns the sole MTP session or
    /// is only listed while another connected device owns it.
    pub session_state: reprise_core::device_sync::DeviceSessionState,
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
    /// `MTP-26` (design 7a): whether this device's on-device contents have
    /// ever been successfully inspected this session.
    pub contents_state: reprise_core::device_sync::device_view::DeviceContentsState,
    /// `MTP-37` (design 7a): the Content section's three rows, in
    /// `SyncTargetKind::ALL` order — target folder, per-device activation,
    /// size on device, cap. The cap is editable here (`E-6`); per-item
    /// selection is edited on the podcast/channel pages and playlist list,
    /// never duplicated in this row.
    pub content_rows: [reprise_core::device_sync::device_view::CategoryContentRow; 3],
    /// `MTP-22`/`MTP-37`: each category's diff reading, same order as
    /// [`Self::content_rows`] — the Next synchronization panel's rows and
    /// the sidebar card's aggregate balance are both projected from this.
    pub category_readings: [reprise_core::device_sync::CategoryReading; 3],
    /// Bytes on this session's YouTube-audio and podcast-episode target
    /// folders, summed once here so the storage bar (`MTP-27`) does not
    /// re-walk the raw inventory lists.
    pub youtube_bytes: u64,
    pub podcast_bytes: u64,
    /// `MTP-37`: the Content section's live selection summary for YouTube
    /// audio and podcast episodes — "N of M channels/shows selected",
    /// read from `POD-12`'s existing per-device subscription selection
    /// rather than computed here.
    pub youtube_selection: reprise_core::device_sync::YoutubeSelectionSummary,
    pub podcast_selection: reprise_core::device_sync::PodcastSelectionSummary,
    /// `MTP-46`: which content sources the user currently has switched on.
    /// A switched-off source contributes no candidates in core, and its
    /// Content row is hidden here — a row reporting "0 of 3 channels" for a
    /// feature the user has turned off is noise that invites the reader to
    /// re-enable something they deliberately disabled. Carried on the
    /// snapshot rather than read here, because the panel does not touch the
    /// database (`ARCH-2`'s thin frontend).
    pub enabled_sources: reprise_core::device_sync::podcasts::EnabledSyncSources,
    /// The recorded runs shown under "Recent transfers" (MTP-20).
    pub history: Vec<crate::ui::device_sync::device_sync_history::RunWithDeviations>,
    /// `MTP-42`'s preparation-phase projection (design 7f, `MTP-43`) — the
    /// device page's preparation overview, switch behavior, and primary
    /// button label are all driven from this, never re-derived.
    pub preparation: reprise_core::device_sync::PreparationPhase,
    /// The missing-file list `preparation` was computed from — carries the
    /// episode titles the overview lists, which `PreparationPhase`'s
    /// variants (counts and bytes only) do not.
    pub preparation_missing: Vec<reprise_core::device_sync::preparation::MissingFile>,
    /// The GTK-only progress of an in-flight preparation download run.
    pub preparation_run: PreparationRunState,
    /// Whether the current/most recent run's transfer phase was preceded by
    /// a preparation download — drives the "Step 2 of 2" progress reading.
    pub prepared_this_run: bool,
}

/// Test-only "nothing known yet" baseline for the three category fields —
/// shared so `DeviceView` test fixtures across the page, sidebar card and
/// runtime tests do not each hand-roll the same `SyncTargetKind::ALL.map`.
#[cfg(test)]
pub(in crate::ui) fn empty_category_readings() -> [reprise_core::device_sync::CategoryReading; 3] {
    reprise_core::device_sync::SyncTargetKind::ALL.map(|kind| {
        let target = reprise_core::device_sync::SyncTarget::default_for(kind);
        reprise_core::device_sync::device_view::project_device_category_reading(
            &target,
            reprise_core::device_sync::CategoryDiff::default(),
        )
    })
}

#[cfg(test)]
pub(in crate::ui) fn empty_content_rows(
) -> [reprise_core::device_sync::device_view::CategoryContentRow; 3] {
    reprise_core::device_sync::SyncTargetKind::ALL.map(|kind| {
        let target = reprise_core::device_sync::SyncTarget::default_for(kind);
        reprise_core::device_sync::device_view::project_category_content_row(&target, 0)
    })
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
