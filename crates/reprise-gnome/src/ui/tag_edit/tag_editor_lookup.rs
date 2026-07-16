//! Optional MusicBrainz lookup wiring for the single-track tag editor.

use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;

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

pub(in crate::ui) fn wire(is_multi: bool, widgets: LookupWidgets<'_>, update: &Rc<dyn Fn()>) {
    if is_multi {
        return;
    }

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
    let update = update.clone();

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
        let update = update.clone();
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
                        update();
                        hint.set_text(&strings::text(strings::TAG_FETCH_FIELDS_FILLED));
                    } else {
                        hint.set_text(&strings::text(strings::TAG_FETCH_NOTHING_TO_FILL));
                    }
                }
            }
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
}
