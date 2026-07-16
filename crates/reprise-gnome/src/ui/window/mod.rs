pub(in crate::ui) mod library_chrome;
pub(in crate::ui) mod library_shell;
#[path = "window.rs"]
mod surface;
pub(in crate::ui) mod window_action_wiring;
pub(in crate::ui) mod window_decoration_strings;
pub(in crate::ui) mod window_decorations;
pub(in crate::ui) mod window_navigation;
pub(in crate::ui) mod window_runtime_wiring;
pub(in crate::ui) mod window_smoke;

#[allow(unused_imports)]
use super::*;
pub(crate) use surface::build;
