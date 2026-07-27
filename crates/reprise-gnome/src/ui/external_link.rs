//! The single gateway from Reprise to the desktop's URI handler.
//!
//! Every URL the app opens externally — concert tickets, event pages, release
//! announcements — is copied out of third-party provider JSON. Routing all of
//! them through one function keeps the scheme allowlist
//! (`reprise_core::external_link::is_launchable_url`) impossible to forget:
//! a refused URL behaves exactly like a missing one, so nothing opens and the
//! user sees no failure they did not cause.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gio;

pub(in crate::ui) type LaunchErrorCallback = Rc<dyn Fn(String)>;
/// Late-bound sink for launch failures the surrounding view wants to surface.
pub(in crate::ui) type LaunchErrorSlot = Rc<RefCell<Option<LaunchErrorCallback>>>;

/// Opens `url` externally when it is a web link, otherwise does nothing.
///
/// `context` only labels the log line, so a refused or failed launch can be
/// traced back to the surface that asked for it.
pub(in crate::ui) fn launch(url: &str, context: &'static str, on_error: Option<&LaunchErrorSlot>) {
    if !reprise_core::external_link::is_launchable_url(url) {
        tracing::warn!(url, context, "refused to open a URL that is not a web link");
        return;
    }
    let on_error = on_error.cloned();
    let url_for_log = url.to_owned();
    gtk4::UriLauncher::new(url).launch(
        None::<&gtk4::Window>,
        gio::Cancellable::NONE,
        move |result| {
            let Err(error) = result else {
                return;
            };
            tracing::warn!(%error, url = url_for_log, context, "could not open URL");
            let callback = on_error.as_ref().and_then(|slot| slot.borrow().clone());
            if let Some(callback) = callback {
                callback(error.to_string());
            }
        },
    );
}
