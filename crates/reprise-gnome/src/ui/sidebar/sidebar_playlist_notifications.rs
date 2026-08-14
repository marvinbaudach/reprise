//! Playlist-change propagation from the sidebar to connected device pages.

use std::rc::Rc;

use super::{rebuild, Shared, Sidebar};

impl Sidebar {
    /// Refreshes the sidebar and the connected-device playlist projection
    /// after one successful playlist mutation.
    pub fn refresh_after_playlist_change(&self, reason: &str) {
        rebuild(&self.shared, None, reason);
        notify_playlists_changed(&self.shared);
    }

    /// Invalidates device playlist projections when another surface already
    /// performed the sidebar navigation refresh itself.
    pub(in crate::ui) fn notify_playlists_changed(&self) {
        notify_playlists_changed(&self.shared);
    }

    /// Shows connected devices below the navigation rows and routes card
    /// activation through the existing source-selection callback.
    pub fn bind_device_sync(
        &self,
        runtime: &Rc<crate::ui::device_sync_runtime::DeviceSyncRuntime>,
        on_open: Rc<dyn Fn(String, String)>,
    ) {
        let runtime_weak = Rc::downgrade(runtime);
        *self.shared.on_playlists_changed.borrow_mut() = Some(Rc::new(move || {
            if let Some(runtime) = runtime_weak.upgrade() {
                runtime.library_playlists_changed();
            }
        }));
        let section = super::sidebar_device_section::bind(runtime, on_open);
        self.activity_slot.set_device_section(&section);
    }
}

pub(in crate::ui) fn notify_playlists_changed(shared: &Rc<Shared>) {
    let callback = shared.on_playlists_changed.borrow().clone();
    if let Some(callback) = callback {
        callback();
    }
}

#[cfg(test)]
#[path = "sidebar_playlist_notification_tests.rs"]
mod tests;
