//! The full device page's playlist/profile/sync/eject action bindings —
//! split out of `device_sync_page.rs` to keep that file under the 800-line
//! limit as the page grew a Content/Next-synchronization panel (design 7a).

use std::rc::Rc;

use reprise_core::device_sync::TransferProfile;

use super::device_sync_runtime::DeviceSyncRuntime;

#[derive(Clone)]
pub(super) struct PageActions {
    pub(super) set_profile: Rc<dyn Fn(TransferProfile)>,
    pub(super) set_playlist: Rc<dyn Fn(reprise_core::device_sync::SelectionSource, bool)>,
    pub(super) start: Rc<dyn Fn()>,
    pub(super) cancel: Rc<dyn Fn()>,
    pub(super) eject: Rc<dyn Fn()>,
}

impl PageActions {
    pub(super) fn for_runtime(runtime: &Rc<DeviceSyncRuntime>, device_id: &str) -> Self {
        let set_profile = {
            let runtime = runtime.clone();
            let device_id = device_id.to_string();
            Rc::new(move |profile| {
                if let Err(error) = runtime.set_transfer_profile(&device_id, profile) {
                    tracing::warn!(%error, "could not update Android sync transfer profile");
                }
            }) as Rc<dyn Fn(TransferProfile)>
        };
        let set_playlist = {
            let runtime = runtime.clone();
            let device_id = device_id.to_string();
            Rc::new(move |source, selected| {
                if let Err(error) = runtime.set_playlist_selected(&device_id, source, selected) {
                    tracing::warn!(%error, "could not update Android sync playlist");
                }
            }) as Rc<dyn Fn(reprise_core::device_sync::SelectionSource, bool)>
        };
        let start = {
            let runtime = runtime.clone();
            let device_id = device_id.to_string();
            Rc::new(move || match runtime.sync_now(&device_id) {
                Ok(()) => tracing::info!(device_id, "device sync started from page"),
                Err(error) => {
                    tracing::warn!(%error, "could not start Android synchronization");
                }
            }) as Rc<dyn Fn()>
        };
        let cancel = {
            let runtime = runtime.clone();
            let device_id = device_id.to_string();
            Rc::new(move || runtime.cancel_current(&device_id)) as Rc<dyn Fn()>
        };
        let eject = {
            let runtime = runtime.clone();
            let device_id = device_id.to_string();
            Rc::new(move || runtime.eject(&device_id)) as Rc<dyn Fn()>
        };
        Self {
            set_profile,
            set_playlist,
            start,
            cancel,
            eject,
        }
    }
}
