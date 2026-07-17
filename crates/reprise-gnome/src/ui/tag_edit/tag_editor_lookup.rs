//! MusicBrainz lookup wiring for the tag editor.
//!
//! Single-track mode's fetch is unchanged since before this rework: one
//! lookup for the one open track, filling empty fields directly.
//!
//! G2 (Beschluss #3) adds Multi mode's fetch: **one** release lookup for the
//! whole selection, gated on its *effective* artist+album being uniform
//! (`TagEditSession::mb_uniform_artist_album`) — never a per-track lookup.
//! A successful fetch fills Year/Album artist/Genre only where the whole
//! selection is aggregately empty on that field, and always through the
//! widgets' own already-wired `changed` signal (`tag_editor_dirty.rs`) by
//! calling `.set_text()` — exactly like the pre-existing single-track path
//! below — so a filled value becomes a completely normal pending change,
//! reviewed and reverted like anything typed by hand (never a direct write).

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;

use reprise_core::library::tag_edit_session::{SessionMode, TagEditSession, TagField};
use reprise_core::release_lookup;

use crate::ui::autocomplete_entry::AutocompleteEntry;
use crate::ui::{one_shot_task, strings};

#[derive(Clone, Copy)]
pub(in crate::ui) struct LookupWidgets<'a> {
    pub(in crate::ui) button: &'a gtk4::Button,
    pub(in crate::ui) hint: &'a gtk4::Label,
    pub(in crate::ui) year: &'a libadwaita::EntryRow,
    pub(in crate::ui) artist: &'a Rc<AutocompleteEntry>,
    pub(in crate::ui) album: &'a Rc<AutocompleteEntry>,
    pub(in crate::ui) album_artist: &'a Rc<AutocompleteEntry>,
    pub(in crate::ui) genre: &'a Rc<AutocompleteEntry>,
}

pub(in crate::ui) fn wire(
    session: &Rc<RefCell<TagEditSession>>,
    widgets: LookupWidgets<'_>,
    update: &Rc<dyn Fn()>,
) {
    let is_multi = matches!(session.borrow().mode(), SessionMode::Multi);
    if is_multi {
        wire_multi(session, widgets, update);
    } else {
        wire_single(widgets);
    }
}

fn wire_single(widgets: LookupWidgets<'_>) {
    let LookupWidgets {
        button,
        hint,
        year,
        artist,
        album,
        album_artist,
        genre,
    } = widgets;
    button.set_sensitive(true);
    let button = button.clone();
    let hint = hint.clone();
    let year = year.clone();
    let album_artist = album_artist.clone();
    let genre = genre.clone();
    let artist = artist.clone();
    let album = album.clone();

    button.clone().connect_clicked(move |_| {
        let artist_name = artist.text();
        let album_name = album.text();
        if artist_name.trim().is_empty() || album_name.trim().is_empty() {
            hint.set_text(&strings::text(strings::TAG_FETCH_NO_RESULTS));
            return;
        }

        button.set_sensitive(false);
        hint.set_text(&strings::text(strings::TAG_FETCH_LOADING));
        let rx = match one_shot_task::spawn("reprise-mb-lookup", move || {
            release_lookup::lookup_release(&artist_name, &album_name)
                .map_err(|error| error.to_string())
        }) {
            Ok(receiver) => receiver,
            Err(error) => {
                tracing::warn!(%error, "could not start MusicBrainz lookup thread");
                button.set_sensitive(true);
                hint.set_text(&strings::text(strings::TAG_FETCH_NETWORK_ERROR));
                return;
            }
        };

        let button = button.clone();
        let hint = hint.clone();
        let year = year.clone();
        let album_artist = album_artist.clone();
        let genre = genre.clone();
        glib::spawn_future_local(async move {
            let Ok(result) = rx.recv().await else {
                button.set_sensitive(true);
                return;
            };
            button.set_sensitive(true);
            match result {
                Err(error) => {
                    tracing::warn!(%error, "MusicBrainz lookup failed");
                    hint.set_text(&strings::text(error_message(&error)));
                }
                Ok(lookup) => {
                    let mut filled_any = false;
                    if let Some(value) = lookup.year {
                        if year.text().is_empty() {
                            year.set_text(&value.to_string());
                            filled_any = true;
                        }
                    }
                    if let Some(value) = &lookup.album_artist {
                        if album_artist.text().is_empty() {
                            album_artist.set_text(value);
                            filled_any = true;
                        }
                    }
                    if let Some(value) = &lookup.genre {
                        if genre.text().is_empty() {
                            genre.set_text(value);
                            filled_any = true;
                        }
                    }
                    if filled_any {
                        hint.set_text(&strings::text(strings::TAG_FETCH_FIELDS_FILLED));
                    } else {
                        hint.set_text(&strings::text(strings::TAG_FETCH_NOTHING_TO_FILL));
                    }
                }
            }
        });
    });
}

/// P-2's disabled-button reason for the Multi MB-fetch button: a thin,
/// testable presentation decision over `TagEditSession::
/// mb_uniform_artist_album`'s core result (kept here, not in reprise-core,
/// since it's purely "how does the button look" — the domain question is
/// already fully answered by that method).
pub(in crate::ui) fn mb_button_state(
    uniform: Option<&(String, String)>,
) -> (bool, Option<&'static str>) {
    match uniform {
        Some(_) => (true, None),
        None => (false, Some(strings::TAG_FETCH_REQUIRES_UNIFORM)),
    }
}

fn apply_button_state(button: &gtk4::Button, uniform: Option<&(String, String)>) {
    let (sensitive, tooltip) = mb_button_state(uniform);
    button.set_sensitive(sensitive);
    match tooltip {
        Some(key) => button.set_tooltip_text(Some(&strings::text(key))),
        None => button.set_tooltip_text(None),
    }
}

/// A field counts as "aggregately empty" for the Multi MB-fill gate
/// (Beschluss #3: "füllt leere aggregierte Felder") only when every track's
/// *effective* value for it is not just uniform (`mixed_placeholder` agrees)
/// but genuinely empty. `TagEditSession::effective_display` renders that
/// state as the literal `"empty"` sentinel — the same TAG-2 display
/// vocabulary a mixed placeholder's own "Mixed — Deathcore, empty" entry
/// uses — so matching that text is the only way to tell "uniformly empty"
/// apart from "uniformly some real value" from outside `reprise-core`
/// without re-deriving `effective_value` a second time here.
fn field_is_uniformly_empty(session: &TagEditSession, field: TagField) -> bool {
    if session.mixed_placeholder(field).is_some() {
        return false;
    }
    session
        .effective_display(session.current_track_id(), field)
        .is_some_and(|display| display == "empty")
}

/// Fills `year`/`album_artist`/`genre` from `lookup` wherever
/// [`field_is_uniformly_empty`] allows it, via `.set_text()` — never a
/// direct session write, so the already-wired `changed` signal
/// (`tag_editor_dirty.rs`) registers each fill as a normal
/// `PendingScope::AllTracks` pending change, reviewable and revertible like
/// anything typed by hand. Returns whether anything was actually filled.
fn fill_empty_aggregated_fields(
    session: &TagEditSession,
    lookup: &release_lookup::ReleaseLookupResult,
    year: &libadwaita::EntryRow,
    album_artist: &Rc<AutocompleteEntry>,
    genre: &Rc<AutocompleteEntry>,
) -> bool {
    let mut filled_any = false;
    if let Some(value) = lookup.year {
        if field_is_uniformly_empty(session, TagField::Year) {
            year.set_text(&value.to_string());
            filled_any = true;
        }
    }
    if let Some(value) = &lookup.album_artist {
        if field_is_uniformly_empty(session, TagField::AlbumArtist) {
            album_artist.set_text(value);
            filled_any = true;
        }
    }
    if let Some(value) = &lookup.genre {
        if field_is_uniformly_empty(session, TagField::Genre) {
            genre.set_text(value);
            filled_any = true;
        }
    }
    filled_any
}

fn wire_multi(
    session: &Rc<RefCell<TagEditSession>>,
    widgets: LookupWidgets<'_>,
    update: &Rc<dyn Fn()>,
) {
    let LookupWidgets {
        button,
        hint,
        year,
        artist,
        album,
        album_artist,
        genre,
    } = widgets;

    hint.set_text(&strings::text(strings::TAG_FETCH_HINT_MULTI));
    apply_button_state(button, session.borrow().mb_uniform_artist_album().as_ref());

    // Re-evaluates the uniformity gate live as Artist/Album change during a
    // Multi bulk edit — an independent `changed` listener alongside
    // `tag_editor_dirty.rs`'s own (GTK signals support any number of
    // subscribers, in connection order), so this needs no change there.
    let recompute: Rc<dyn Fn()> = {
        let session = session.clone();
        let button = button.clone();
        Rc::new(move || {
            apply_button_state(&button, session.borrow().mb_uniform_artist_album().as_ref());
        })
    };
    for entry in [artist, album] {
        let recompute = recompute.clone();
        entry.connect_changed(move || recompute());
    }

    let button = button.clone();
    let hint = hint.clone();
    let year = year.clone();
    let album_artist = album_artist.clone();
    let genre = genre.clone();
    let session = session.clone();
    let update = update.clone();
    let recompute = recompute.clone();

    button.clone().connect_clicked(move |_| {
        let uniform = session.borrow().mb_uniform_artist_album();
        let Some((artist_name, album_name)) = uniform else {
            // The button is disabled whenever this is `None` (kept in sync
            // by `recompute` above) — defensive, not reachable through the
            // UI, but never worth a panic if it somehow is.
            return;
        };

        button.set_sensitive(false);
        hint.set_text(&strings::text(strings::TAG_FETCH_LOADING));
        let rx = match one_shot_task::spawn("reprise-mb-lookup", move || {
            release_lookup::lookup_release(&artist_name, &album_name)
                .map_err(|error| error.to_string())
        }) {
            Ok(receiver) => receiver,
            Err(error) => {
                tracing::warn!(%error, "could not start MusicBrainz lookup thread");
                hint.set_text(&strings::text(strings::TAG_FETCH_NETWORK_ERROR));
                recompute();
                return;
            }
        };

        let hint = hint.clone();
        let year = year.clone();
        let album_artist = album_artist.clone();
        let genre = genre.clone();
        let session = session.clone();
        let update = update.clone();
        let recompute = recompute.clone();
        glib::spawn_future_local(async move {
            let Ok(result) = rx.recv().await else {
                recompute();
                return;
            };
            match result {
                Err(error) => {
                    tracing::warn!(%error, "MusicBrainz lookup failed");
                    hint.set_text(&strings::text(error_message(&error)));
                }
                Ok(lookup) => {
                    let filled_any = fill_empty_aggregated_fields(
                        &session.borrow(),
                        &lookup,
                        &year,
                        &album_artist,
                        &genre,
                    );
                    if filled_any {
                        update();
                        hint.set_text(&strings::text(strings::TAG_FETCH_FIELDS_FILLED));
                    } else {
                        hint.set_text(&strings::text(strings::TAG_FETCH_NOTHING_TO_FILL));
                    }
                }
            }
            recompute();
        });
    });
}

fn error_message(error: &str) -> &'static str {
    if error.contains("no matching") {
        strings::TAG_FETCH_NO_RESULTS
    } else {
        strings::TAG_FETCH_NETWORK_ERROR
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::library::tag_edit::EditableTags;
    use reprise_core::library::tag_edit_session::{FieldValue, PendingScope, SessionTrack};
    use std::path::PathBuf;

    #[test]
    fn lookup_errors_distinguish_no_match_from_transport_failure() {
        assert_eq!(
            super::error_message("no matching release"),
            crate::ui::strings::TAG_FETCH_NO_RESULTS
        );
        assert_eq!(
            super::error_message("connection refused"),
            crate::ui::strings::TAG_FETCH_NETWORK_ERROR
        );
    }

    #[test]
    fn mb_multi_requires_effective_uniform_artist_album() {
        let uniform = Some(("Suicide Silence".to_string(), "The Black Crown".to_string()));
        assert_eq!(mb_button_state(uniform.as_ref()), (true, None));
        assert_eq!(
            mb_button_state(None),
            (false, Some(strings::TAG_FETCH_REQUIRES_UNIFORM))
        );
    }

    fn track(id: i64, artist: &str, album: &str, genre: &str, year: Option<u32>) -> SessionTrack {
        SessionTrack {
            id,
            path: PathBuf::from(format!("/music/{id}.flac")),
            tags: EditableTags {
                title: format!("Title {id}"),
                artist: artist.into(),
                album: album.into(),
                album_artist: String::new(),
                year,
                track_no: Some(1),
                genre: genre.into(),
            },
            rating: 0,
        }
    }

    #[test]
    fn field_is_uniformly_empty_distinguishes_mixed_from_uniform_empty() {
        let session = TagEditSession::new(
            vec![
                track(1, "Artist", "Album", "", None),
                track(2, "Artist", "Album", "", None),
            ],
            SessionMode::Multi,
        );
        assert!(field_is_uniformly_empty(&session, TagField::Genre));
        assert!(field_is_uniformly_empty(&session, TagField::Year));

        let mixed = TagEditSession::new(
            vec![
                track(1, "Artist", "Album", "Rock", None),
                track(2, "Artist", "Album", "", None),
            ],
            SessionMode::Multi,
        );
        assert!(
            !field_is_uniformly_empty(&mixed, TagField::Genre),
            "a mixed field is never treated as empty, even though one track's own value is empty"
        );

        let mut uniform_value = TagEditSession::new(
            vec![
                track(1, "Artist", "Album", "Rock", None),
                track(2, "Artist", "Album", "Rock", None),
            ],
            SessionMode::Multi,
        );
        assert!(!field_is_uniformly_empty(&uniform_value, TagField::Genre));

        // Pending edits participate in the *effective* value, same as
        // everywhere else in the session (mirrors `mb_uniformity_uses_
        // effective_values` in reprise-core's own test suite).
        uniform_value.set_pending(
            PendingScope::AllTracks,
            TagField::Genre,
            &FieldValue::Text(String::new()),
        );
        assert!(field_is_uniformly_empty(&uniform_value, TagField::Genre));
    }
}
