//! Production GIO backend for the application-long device sync runtime.

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use gtk4::gio;
use reprise_core::library::m3u::M3uEntry;
use reprise_platform_linux::device_sync::{
    CopyOutcome, DeviceContents, DeviceDescriptor, DeviceMonitor, DeviceStorage,
};
use reprise_platform_linux::device_transfer::{
    probe_mp3_transcode_capability, transcode_to_mp3, Mp3TranscodeRequest, TranscodedFile,
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
    ) -> BackendFuture<(DeviceContents, Option<u64>, Option<u64>)> {
        Box::pin(async move {
            let storage = DeviceStorage::from_uri(&root_uri);
            let contents = storage.inspect().await.map_err(|error| error.to_string())?;
            let (available, total) = storage
                .capacity_bytes()
                .await
                .map_err(|error| error.to_string())?;
            Ok((contents, available, total))
        })
    }

    fn copy_track(
        &self,
        _device_id: String,
        root_uri: String,
        source_path: PathBuf,
        relative_target: String,
        expected_size: u64,
        cancellable: gio::Cancellable,
        progress: Rc<dyn Fn(u64, u64)>,
    ) -> BackendFuture<CopyOutcome> {
        Box::pin(async move {
            DeviceStorage::from_uri(&root_uri)
                .copy_track(
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

    fn read_playlist(&self, root_uri: String, name: String) -> BackendFuture<Vec<M3uEntry>> {
        Box::pin(async move {
            DeviceStorage::from_uri(&root_uri)
                .read_playlist(&name)
                .await
                .map_err(|error| error.to_string())
        })
    }

    fn replace_track(
        &self,
        _device_id: String,
        root_uri: String,
        source_path: PathBuf,
        relative_target: String,
        expected_size: u64,
        cancellable: gio::Cancellable,
        progress: Rc<dyn Fn(u64, u64)>,
    ) -> BackendFuture<CopyOutcome> {
        Box::pin(async move {
            DeviceStorage::from_uri(&root_uri)
                .replace_track(
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

    fn cleanup_partials(&self, root_uri: String) -> BackendFuture<u32> {
        Box::pin(async move {
            DeviceStorage::from_uri(&root_uri)
                .cleanup_partials()
                .await
                .map_err(|error| error.to_string())
        })
    }

    fn delete_track(&self, root_uri: String, relative_target: String) -> BackendFuture<bool> {
        Box::pin(async move {
            DeviceStorage::from_uri(&root_uri)
                .delete_track(&relative_target)
                .await
                .map_err(|error| error.to_string())
        })
    }

    fn probe_mp3_transcode(&self) -> Result<(), String> {
        probe_mp3_transcode_capability().map_err(|error| error.to_string())
    }

    fn transcode_track(
        &self,
        request: Mp3TranscodeRequest,
        cancelled: Arc<AtomicBool>,
    ) -> BackendFuture<TranscodedFile> {
        Box::pin(async move {
            let (sender, receiver) = async_channel::bounded(1);
            std::thread::Builder::new()
                .name("reprise-device-mp3-encoder".into())
                .spawn(move || {
                    let output = request.output.clone();
                    let result =
                        transcode_to_mp3(&request, &cancelled).map_err(|error| error.to_string());
                    if sender.send_blocking(result).is_err() {
                        let _ = std::fs::remove_file(output);
                    }
                })
                .map_err(|error| error.to_string())?;
            receiver
                .recv()
                .await
                .map_err(|_| "MP3 encoder stopped without a result".to_string())?
        })
    }

    fn replace_playlist(
        &self,
        _device_id: String,
        root_uri: String,
        name: String,
        contents: Vec<u8>,
    ) -> BackendFuture<()> {
        Box::pin(async move {
            DeviceStorage::from_uri(&root_uri)
                .replace_playlist(&name, contents)
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
}
