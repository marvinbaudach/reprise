//! Design 7d's device folder browser: the runtime-side actions it drives
//! (`MTP-31`/`MTP-32`). The browser dialog itself never talks to the
//! backend directly — it gathers facts through the async wrappers below
//! and hands the chosen folder to [`DeviceSyncRuntime::set_target_folder`],
//! which is the only place a browser change is actually persisted.

use reprise_core::device_sync::browser::{
    target_relocation_action, StorageOption, TargetRelocation,
};
use reprise_core::device_sync::targets::{load_target, save_target};

use super::*;

impl DeviceSyncRuntime {
    fn root_uri(&self, device_id: &str) -> Option<String> {
        self.device_states
            .borrow()
            .iter()
            .find(|device| device.descriptor.id == device_id)
            .map(|device| device.descriptor.root_uri.clone())
    }

    /// Design 7d: the current, persisted target for `kind` — the folder
    /// browser's starting point and the value its "Reset to default" and
    /// playlist-conflict warning compare against.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn current_target(&self, device_id: &str, kind: SyncTargetKind) -> Option<SyncTarget> {
        self.device_states
            .borrow()
            .iter()
            .find(|device| device.descriptor.id == device_id)
            .and_then(|device| {
                device
                    .targets
                    .iter()
                    .find(|target| target.kind == kind)
                    .cloned()
            })
    }

    /// Design 7d's storage selection: every browsable storage volume on
    /// this device, listed fresh (`MTP-31`).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn browse_storages(self: &Rc<Self>, device_id: &str) -> BackendFuture<Vec<StorageOption>> {
        let backend = self.backend.clone();
        let root_uri = self.root_uri(device_id);
        Box::pin(async move {
            let root_uri = root_uri.ok_or_else(|| "device is not connected".to_string())?;
            backend.list_storages(root_uri).await
        })
    }

    /// Design 7d's folder tree: the immediate child folders of `path` on
    /// `storage`.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn browse_folders(
        self: &Rc<Self>,
        device_id: &str,
        storage: StorageId,
        path: String,
    ) -> BackendFuture<Vec<String>> {
        let backend = self.backend.clone();
        let root_uri = self.root_uri(device_id);
        Box::pin(async move {
            let root_uri = root_uri.ok_or_else(|| "device is not connected".to_string())?;
            backend.list_folders(root_uri, storage, path).await
        })
    }

    /// Design 7d's "New folder".
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn create_target_folder(
        self: &Rc<Self>,
        device_id: &str,
        storage: StorageId,
        path: String,
        name: String,
    ) -> BackendFuture<()> {
        let backend = self.backend.clone();
        let root_uri = self.root_uri(device_id);
        Box::pin(async move {
            let root_uri = root_uri.ok_or_else(|| "device is not connected".to_string())?;
            backend.create_folder(root_uri, storage, path, name).await
        })
    }

    /// Design 7d's "Save": persists the chosen storage/path for `kind`
    /// immediately, the same pattern as `set_target_enabled`. When the
    /// change is a same-storage rename (`MTP-32`'s `TargetRelocation::
    /// MoveFolder`), also relocates whatever is already on the device —
    /// best-effort: a relocation failure never blocks the save or is shown
    /// to the user, because the next sync simply copies fresh into the new
    /// folder instead of finding it already there.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn set_target_folder(
        self: &Rc<Self>,
        device_id: &str,
        kind: SyncTargetKind,
        storage_id: Option<StorageId>,
        path: String,
    ) -> Result<(), String> {
        {
            let devices = self.device_states.borrow();
            let device = devices
                .iter()
                .find(|device| device.descriptor.id == device_id)
                .ok_or_else(|| "device is not connected".to_string())?;
            if device.is_busy() {
                return Err("device synchronization is active".into());
            }
        }
        let previous = {
            let conn = self.conn.borrow();
            load_target(&conn, device_id, kind)
                .map_err(|error| error.to_string())?
                .unwrap_or_else(|| SyncTarget::default_for(kind))
        };
        let next = SyncTarget {
            storage_id,
            path,
            ..previous.clone()
        };
        save_target(&self.conn.borrow(), device_id, &next).map_err(|error| error.to_string())?;
        if let TargetRelocation::MoveFolder { from_path } =
            target_relocation_action(&previous, &next)
        {
            if let Some(storage_id) = next.storage_id {
                self.relocate_target_folder(device_id, storage_id, from_path, next.path.clone());
            }
        }
        self.recompute_delta(device_id)
    }

    fn relocate_target_folder(
        self: &Rc<Self>,
        device_id: &str,
        storage_id: StorageId,
        from_path: String,
        to_path: String,
    ) {
        let Some(root_uri) = self.root_uri(device_id) else {
            return;
        };
        let backend = self.backend.clone();
        let id = device_id.to_string();
        gtk4::glib::MainContext::ref_thread_default().spawn_local(async move {
            if let Err(error) = backend
                .move_folder(root_uri, storage_id, from_path.clone(), to_path.clone())
                .await
            {
                tracing::warn!(
                    device_id = id,
                    %error,
                    from = from_path,
                    to = to_path,
                    "could not relocate Android sync target folder; the next sync will copy fresh instead"
                );
            }
        });
    }
}
