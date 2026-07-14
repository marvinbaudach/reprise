//! Shared helpers for plugin activation state in the preferences UI.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::ui::strings;

pub(super) fn service_subtitle(description: &str, enabled: bool, status: &str) -> String {
    if enabled {
        format!("{description} · {status}")
    } else {
        description.to_string()
    }
}

/// A "Test connection" action row with an inline, transient result subtitle.
///
/// The returned `TestConnectionRow` holds the row and a trigger function. The
/// caller supplies a `spawn_validate` closure that runs on a dedicated thread
/// and returns either `Ok(user_name)` or `Err(human_readable_error)`.
pub(super) struct TestConnectionRow {
    pub row: adw::ActionRow,
    pub button: gtk4::Button,
    generation: Rc<Cell<u64>>,
}

/// A clonable handle for triggering test-connection from a button callback.
#[derive(Clone)]
pub(super) struct TestConnectionTrigger {
    row: glib::WeakRef<adw::ActionRow>,
    button: glib::WeakRef<gtk4::Button>,
    generation: Rc<Cell<u64>>,
}

impl TestConnectionRow {
    /// Returns a clonable trigger handle for use in button callbacks.
    pub fn clone_trigger(&self) -> TestConnectionTrigger {
        TestConnectionTrigger {
            row: {
                let w = glib::WeakRef::new();
                w.set(Some(&self.row));
                w
            },
            button: {
                let w = glib::WeakRef::new();
                w.set(Some(&self.button));
                w
            },
            generation: self.generation.clone(),
        }
    }

    pub fn new() -> Self {
        let button = gtk4::Button::builder()
            .label(strings::text(strings::TEST_CONNECTION))
            .valign(gtk4::Align::Center)
            .build();
        let row = adw::ActionRow::builder()
            .title(strings::text(strings::TEST_CONNECTION))
            .activatable_widget(&button)
            .build();
        row.add_suffix(&button);

        Self {
            row,
            button,
            generation: Rc::new(Cell::new(0)),
        }
    }
}

impl TestConnectionTrigger {
    /// Show an error message without spawning a background thread.
    pub fn show_error(&self, message: &str) {
        if let Some(row) = self.row.upgrade() {
            row.set_subtitle(message);
        }
    }

    /// Spawn a validation on a background thread. The `validate` closure runs
    /// off the main thread and should call `validate_token` or equivalent.
    /// Results are shown transiently in the row's subtitle; a generation guard
    /// discards stale results if the user clicks again before the first
    /// completes.
    pub fn trigger<F>(&self, validate: F)
    where
        F: FnOnce() -> Result<String, String> + Send + 'static,
    {
        let Some(row) = self.row.upgrade() else {
            return;
        };
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);

        if let Some(button) = self.button.upgrade() {
            button.set_sensitive(false);
        }
        row.set_subtitle(&strings::text(strings::LISTENBRAINZ_CONNECTING));

        let (sender, receiver) = async_channel::bounded(1);
        let spawned = std::thread::Builder::new()
            .name("reprise-test-connection".to_string())
            .spawn(move || {
                let _ = sender.send_blocking(validate());
            });
        if spawned.is_err() {
            if let Some(button) = self.button.upgrade() {
                button.set_sensitive(true);
            }
            row.set_subtitle(&strings::text(strings::TEST_CONNECTION_FAILED));
            return;
        }

        let row = self.row.clone();
        let button = self.button.clone();
        let guard = self.generation.clone();
        glib::spawn_future_local(async move {
            let Ok(result) = receiver.recv().await else {
                return;
            };
            if guard.get() != generation {
                return; // stale result
            }
            let Some(row) = row.upgrade() else {
                return;
            };
            match result {
                Ok(user_name) => {
                    row.set_subtitle(&strings::test_connection_ok(&user_name));
                }
                Err(message) => {
                    row.set_subtitle(&message);
                }
            }
            if let Some(button) = button.upgrade() {
                button.set_sensitive(true);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_status_is_only_shown_while_enabled() {
        assert_eq!(
            service_subtitle("Scrobble listens", false, "Connected as Ada"),
            "Scrobble listens"
        );
        assert_eq!(
            service_subtitle("Scrobble listens", true, "Connected as Ada"),
            "Scrobble listens · Connected as Ada"
        );
    }
}
