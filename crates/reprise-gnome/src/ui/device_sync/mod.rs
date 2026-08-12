pub(in crate::ui) mod device_sync_backend;
mod device_sync_content_copy;
mod device_sync_dock;
pub(in crate::ui) mod device_sync_feedback;
pub(in crate::ui) mod device_sync_launcher;
mod device_sync_on_device;
pub(in crate::ui) mod device_sync_page;
mod device_sync_page_actions;
mod device_sync_page_copy;
mod device_sync_page_layout;
mod device_sync_picker;
mod device_sync_playlist_card;
mod device_sync_remembered;
pub(in crate::ui) mod device_sync_rename;
pub(in crate::ui) mod device_sync_runtime;
pub(in crate::ui) mod device_sync_smoke;
pub(in crate::ui) mod device_sync_storage_bar;
pub(in crate::ui) mod device_sync_storage_copy;
pub(in crate::ui) mod device_sync_strings;
mod device_sync_target_browser;
mod device_sync_time_copy;
mod device_sync_verification_copy;

#[cfg(test)]
mod device_sync_rate_tests;
#[cfg(test)]
mod device_sync_runtime_tests;
#[cfg(test)]
mod device_sync_surface_tests;
#[allow(unused_imports)]
use super::*;
