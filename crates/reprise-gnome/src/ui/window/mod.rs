pub(in crate::ui) mod content_stack;
pub(in crate::ui) mod focus_evidence;
pub(in crate::ui) mod library_chrome;
pub(in crate::ui) mod library_shell;
pub(in crate::ui) mod metadata_navigation;
pub(in crate::ui) mod podcast_refresh_scheduler;
pub(in crate::ui) mod responsive_side_panels;
pub(in crate::ui) mod source_views;
#[path = "window.rs"]
mod surface;
pub(in crate::ui) mod window_action_wiring;
pub(in crate::ui) mod window_decoration_strings;
pub(in crate::ui) mod window_decorations;
pub(in crate::ui) mod window_navigation;
pub(in crate::ui) mod window_now_playing_wiring;
pub(in crate::ui) mod window_queue_model;
pub(in crate::ui) mod window_runtime_wiring;
pub(in crate::ui) mod window_smoke;

#[allow(unused_imports)]
use super::*;
pub(crate) use surface::build;
