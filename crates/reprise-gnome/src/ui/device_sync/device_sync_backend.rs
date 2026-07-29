//! Production GIO backend for the application-long device sync runtime.

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use gtk4::gio;
use reprise_core::device_sync::browser::StorageOption;
use reprise_core::device_sync::{DeviceStorageInspection, StorageId, SyncTarget};
use reprise_platform_linux::device_sync::{
    CopyOutcome, DeviceDescriptor, DeviceMonitor, DeviceStorage,
};
use reprise_platform_linux::device_transfer::{
    probe_transcode_capability, transcode_audio, TranscodeProfile, TranscodeRequest, TranscodedFile,
};

use super::device_sync_runtime::{BackendFuture, DeviceBackend};

pub(in crate::ui) struct GioDeviceBackend {
    monitor: DeviceMonitor,
}

impl GioDeviceBackend {
    pub(in crate::ui) fn new(monitor: DeviceMonitor) -> Self {
        Self { monitor }
    }
}

impl DeviceBackend for GioDeviceBackend {
    fn devices(&self) -> Vec<DeviceDescriptor> {
        self.monitor.devices()
    }

    fn subscribe_devices(&self, callback: Rc<dyn Fn(Vec<DeviceDescriptor>)>) {
        self.monitor.subscribe(callback);
    }

    fn inspect(
        &self,
        root_uri: String,
        targets: [SyncTarget; 3],
    ) -> BackendFuture<DeviceStorageInspection> {
        Box::pin(async move {
            let storage = DeviceStorage::from_uri(&root_uri);
            storage
                .inspect(&targets)
                .await
                .map_err(|error| error.to_string())
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn replace_track(
        &self,
        _device_id: String,
        root_uri: String,
        target_path: String,
        storage_id: Option<StorageId>,
        source_path: PathBuf,
        relative_target: String,
        expected_size: u64,
        cancellable: gio::Cancellable,
        progress: Rc<dyn Fn(u64, u64)>,
    ) -> BackendFuture<CopyOutcome> {
        Box::pin(async move {
            DeviceStorage::from_uri(&root_uri)
                .replace_managed(
                    storage_id,
                    &target_path,
                    &gio::File::for_path(source_path),
                    &relative_target,
                    expected_size,
                    &cancellable,
                    move |copied, total| progress(copied, total),
                )
                .await
                .map_err(|error| error.to_string())
        })
    }

    fn cleanup_partials(
        &self,
        root_uri: String,
        target_path: String,
        storage_id: Option<StorageId>,
    ) -> BackendFuture<u32> {
        Box::pin(async move {
            DeviceStorage::from_uri(&root_uri)
                .cleanup_partials_in(storage_id, &target_path)
                .await
                .map_err(|error| error.to_string())
        })
    }

    fn delete_track(
        &self,
        root_uri: String,
        target_path: String,
        storage_id: Option<StorageId>,
        relative_target: String,
    ) -> BackendFuture<bool> {
        Box::pin(async move {
            DeviceStorage::from_uri(&root_uri)
                .delete_managed(storage_id, &target_path, &relative_target)
                .await
                .map_err(|error| error.to_string())
        })
    }

    fn probe_transcode(&self, profile: TranscodeProfile) -> Result<(), String> {
        probe_transcode_capability(profile).map_err(|error| error.to_string())
    }

    fn transcode_track(
        &self,
        request: TranscodeRequest,
        cancelled: Arc<AtomicBool>,
    ) -> BackendFuture<TranscodedFile> {
        Box::pin(async move {
            let (sender, receiver) = async_channel::bounded(1);
            std::thread::Builder::new()
                .name("reprise-device-audio-encoder".into())
                .spawn(move || {
                    let output = request.output.clone();
                    let result =
                        transcode_audio(&request, &cancelled).map_err(|error| error.to_string());
                    if sender.send_blocking(result).is_err() {
                        let _ = std::fs::remove_file(output);
                    }
                })
                .map_err(|error| error.to_string())?;
            receiver
                .recv()
                .await
                .map_err(|_| "audio encoder stopped without a result".to_string())?
        })
    }

    fn replace_playlist(
        &self,
        _device_id: String,
        root_uri: String,
        target_path: String,
        storage_id: Option<StorageId>,
        name: String,
        contents: Vec<u8>,
    ) -> BackendFuture<()> {
        Box::pin(async move {
            DeviceStorage::from_uri(&root_uri)
                .replace_playlist(storage_id, &target_path, &name, contents)
                .await
                .map_err(|error| error.to_string())
        })
    }

    fn eject(&self, device_id: String) -> BackendFuture<bool> {
        let monitor = self.monitor.clone();
        Box::pin(async move {
            monitor
                .eject(&device_id)
                .await
                .map_err(|error| error.to_string())
        })
    }

    fn list_storages(&self, root_uri: String) -> BackendFuture<Vec<StorageOption>> {
        Box::pin(async move {
            DeviceStorage::from_uri(&root_uri)
                .list_storage_volumes()
                .await
                .map_err(|error| error.to_string())
        })
    }

    fn list_folders(
        &self,
        root_uri: String,
        storage: StorageId,
        path: String,
    ) -> BackendFuture<Vec<String>> {
        Box::pin(async move {
            DeviceStorage::from_uri(&root_uri)
                .list_child_folders(storage, &path)
                .await
                .map_err(|error| error.to_string())
        })
    }

    fn create_folder(
        &self,
        root_uri: String,
        storage: StorageId,
        path: String,
        name: String,
    ) -> BackendFuture<()> {
        Box::pin(async move {
            DeviceStorage::from_uri(&root_uri)
                .create_child_folder(storage, &path, &name)
                .await
                .map_err(|error| error.to_string())
        })
    }

    fn move_folder(
        &self,
        root_uri: String,
        storage: StorageId,
        from_path: String,
        to_path: String,
    ) -> BackendFuture<()> {
        Box::pin(async move {
            DeviceStorage::from_uri(&root_uri)
                .move_child_folder(storage, &from_path, &to_path)
                .await
                .map_err(|error| error.to_string())
        })
    }
}
