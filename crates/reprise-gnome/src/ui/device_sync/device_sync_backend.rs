//! Production GIO backend for the application-long device sync runtime.

use std::path::PathBuf;
use std::rc::Rc;

use gtk4::gio;
use reprise_core::library::m3u::M3uEntry;
use reprise_platform_linux::device_sync::{
    CopyOutcome, DeviceContents, DeviceDescriptor, DeviceMonitor, DeviceStorage,
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

    fn inspect(&self, root_uri: String) -> BackendFuture<(DeviceContents, Option<u64>)> {
        Box::pin(async move {
            let storage = DeviceStorage::from_uri(&root_uri);
            let contents = storage.inspect().await.map_err(|error| error.to_string())?;
            let available = storage
                .available_bytes()
                .await
                .map_err(|error| error.to_string())?;
            Ok((contents, available))
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
}
