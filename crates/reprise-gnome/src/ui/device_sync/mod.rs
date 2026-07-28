pub(in crate::ui) mod device_sync_backend;
mod device_sync_category_bar;
mod device_sync_content_panel;
pub(in crate::ui) mod device_sync_feedback;
pub(in crate::ui) mod device_sync_launcher;
pub(in crate::ui) mod device_sync_page;
mod device_sync_page_actions;
mod device_sync_page_layout;
pub(in crate::ui) mod device_sync_runtime;
pub(in crate::ui) mod device_sync_smoke;
pub(in crate::ui) mod device_sync_storage_bar;
pub(in crate::ui) mod device_sync_storage_copy;
pub(in crate::ui) mod device_sync_strings;

#[cfg(test)]
mod device_sync_rate_tests;
#[cfg(test)]
mod device_sync_runtime_tests;
#[cfg(test)]
mod device_sync_surface_tests;
#[allow(unused_imports)]
use super::*;
