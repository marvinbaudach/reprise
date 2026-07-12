//! Single construction point for the "prompt the user for a name" dialog
//! pattern. Two call sites built the *same* `AdwAlertDialog` shape
//! independently — the sidebar's "New playlist" row and the track list's
//! context-menu "New playlist…" leaf — each with its own copy of the entry,
//! the Suggested-appearance confirm response, the UI-side empty-name
//! validation, and the response-id consts (the sidebar's own comment
//! admitted "same shape … but not shared code"). `prompt_name` funnels that
//! shape into one function; the two call sites keep their *different*
//! follow-up behavior via the `on_confirm` callback (sidebar:
//! create-and-switch; context menu: create-and-add the selected ids). Only
//! the repeated *shape* is shared — this is not a wrapper over
//! `AdwAlertDialog` (see `ui`'s containment policy).

use gtk4::gio;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::ui::strings;

/// `AdwAlertDialog` response ids for the name-prompt dialog — internal
/// identifiers, not user-facing text (the button labels come from
/// `add_response`'s separate `label` argument). Previously duplicated,
/// private, in both `ui::sidebar` and `ui::track_list_context_menu`.
const RESPONSE_CANCEL: &str = "cancel";
const RESPONSE_CREATE: &str = "create";

/// Prompts for a single line of text (a name) via an `AdwAlertDialog`: an
/// entry (`confirm` disabled until the trimmed text is non-blank), a Cancel
/// response, and a Suggested-appearance confirm response labelled
/// `confirm_label`. `on_confirm` runs with the trimmed name when the user
/// confirms (never for Cancel/dismiss); callers pass their own follow-up
/// behavior there. The backend accepts empty/whitespace names
/// (`playlists::create`'s doc comment: "backend is dumb; UI validates") —
/// the enable-on-non-blank wiring below is that UI-side validation.
pub(super) fn prompt_name(
    parent: &adw::ApplicationWindow,
    heading: &str,
    placeholder: &str,
    confirm_label: &str,
    on_confirm: impl Fn(String) + 'static,
) {
    let entry = gtk4::Entry::builder()
        .placeholder_text(placeholder)
        .activates_default(true)
        .build();

    let dialog = adw::AlertDialog::builder()
        .heading(heading)
        .default_response(RESPONSE_CREATE)
        .close_response(RESPONSE_CANCEL)
        .extra_child(&entry)
        .build();
    dialog.add_response(RESPONSE_CANCEL, strings::CANCEL);
    dialog.add_response(RESPONSE_CREATE, confirm_label);
    dialog.set_response_appearance(RESPONSE_CREATE, adw::ResponseAppearance::Suggested);
    dialog.set_response_enabled(RESPONSE_CREATE, false);

    entry.connect_changed({
        let dialog = dialog.clone();
        move |entry| {
            let has_name = !entry.text().trim().is_empty();
            dialog.set_response_enabled(RESPONSE_CREATE, has_name);
        }
    });

    dialog.choose(Some(parent), gio::Cancellable::NONE, move |response| {
        if response.as_str() != RESPONSE_CREATE {
            return;
        }
        let name = entry.text().trim().to_string();
        on_confirm(name);
    });
}
