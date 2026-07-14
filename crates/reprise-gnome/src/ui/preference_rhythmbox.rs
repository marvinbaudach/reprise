use std::path::PathBuf;
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library::rhythmbox_import::{
    self, RhythmboxImportChoices, RhythmboxImportSummary, RhythmboxPlaylist,
    RhythmboxPlaylistSummary, RhythmboxTrackStats,
};

use super::preferences::PreferencesContext;
use super::strings;

const RESPONSE_CANCEL: &str = "cancel";
const RESPONSE_IMPORT: &str = "import";
const RHYTHMDB_PATH_ENV: &str = "REPRISE_RHYTHMDB_PATH";
const PLAYLISTS_PATH_ENV: &str = "REPRISE_RHYTHMBOX_PLAYLISTS_PATH";
const SMOKE_IMPORT_ENV: &str = "REPRISE_SMOKE_RHYTHMDB_IMPORT";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RhythmboxOption {
    ColumnLayout,
    Ratings,
    PlayCounts,
    DateAdded,
    Playlists,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ImportOptionSpec {
    id: RhythmboxOption,
    selected: bool,
}

fn import_option_specs() -> [ImportOptionSpec; 5] {
    [
        ImportOptionSpec {
            id: RhythmboxOption::ColumnLayout,
            selected: false,
        },
        ImportOptionSpec {
            id: RhythmboxOption::Ratings,
            selected: true,
        },
        ImportOptionSpec {
            id: RhythmboxOption::PlayCounts,
            selected: true,
        },
        ImportOptionSpec {
            id: RhythmboxOption::DateAdded,
            selected: true,
        },
        ImportOptionSpec {
            id: RhythmboxOption::Playlists,
            selected: true,
        },
    ]
}

struct ImportDialogSurface {
    dialog: adw::AlertDialog,
    rows: Vec<adw::SwitchRow>,
}

fn option_title(option: RhythmboxOption) -> String {
    strings::text(match option {
        RhythmboxOption::ColumnLayout => strings::ONBOARDING_RHYTHMBOX_COLUMN_LAYOUT,
        RhythmboxOption::Ratings => strings::RHYTHMBOX_IMPORT_RATINGS,
        RhythmboxOption::PlayCounts => strings::RHYTHMBOX_IMPORT_PLAY_COUNTS,
        RhythmboxOption::DateAdded => strings::RHYTHMBOX_IMPORT_DATE_ADDED,
        RhythmboxOption::Playlists => strings::RHYTHMBOX_IMPORT_PLAYLISTS,
    })
}

fn build_import_dialog() -> ImportDialogSurface {
    let list = gtk4::ListBox::new();
    list.add_css_class("boxed-list");
    let rows = import_option_specs()
        .into_iter()
        .map(|spec| {
            let row = adw::SwitchRow::builder()
                .title(option_title(spec.id))
                .active(spec.selected)
                .build();
            list.append(&row);
            row
        })
        .collect();
    let dialog = adw::AlertDialog::builder()
        .heading(strings::text(strings::ONBOARDING_IMPORT_FROM_RHYTHMBOX))
        .body(strings::text(strings::RHYTHMBOX_IMPORT_DIALOG_BODY))
        .extra_child(&list)
        .default_response(RESPONSE_IMPORT)
        .close_response(RESPONSE_CANCEL)
        .build();
    dialog.add_response(RESPONSE_CANCEL, &strings::text(strings::CANCEL));
    dialog.add_response(
        RESPONSE_IMPORT,
        &strings::text(strings::RHYTHMBOX_IMPORT_ACTION),
    );
    dialog.set_response_appearance(RESPONSE_IMPORT, adw::ResponseAppearance::Suggested);
    ImportDialogSurface { dialog, rows }
}

fn default_rhythmdb_path() -> PathBuf {
    std::env::var_os(RHYTHMDB_PATH_ENV).map_or_else(
        || glib::user_data_dir().join("rhythmbox/rhythmdb.xml"),
        PathBuf::from,
    )
}

fn default_playlists_path(rhythmdb_path: &std::path::Path) -> PathBuf {
    std::env::var_os(PLAYLISTS_PATH_ENV).map_or_else(
        || rhythmdb_path.with_file_name("playlists.xml"),
        PathBuf::from,
    )
}

struct ParsedImport {
    tracks: Option<Vec<RhythmboxTrackStats>>,
    playlists: Option<Result<Vec<RhythmboxPlaylist>, String>>,
}

struct ImportResult {
    stats: Option<RhythmboxImportSummary>,
    playlists: Option<RhythmboxPlaylistSummary>,
    playlist_error: Option<String>,
}

pub(super) fn add_rhythmbox_import_row(
    context: &Rc<PreferencesContext>,
    group: &adw::PreferencesGroup,
) {
    let row = adw::ActionRow::builder()
        .title(strings::text(strings::ONBOARDING_IMPORT_FROM_RHYTHMBOX))
        .subtitle(strings::text(strings::RHYTHMBOX_IMPORT_DESCRIPTION))
        .build();
    let button = gtk4::Button::with_label(&strings::text(strings::RHYTHMBOX_IMPORT_ACTION));
    button.set_valign(gtk4::Align::Center);
    let weak = Rc::downgrade(context);
    button.connect_clicked(move |button| {
        if let Some(context) = weak.upgrade() {
            context.present_rhythmbox_import(button);
        }
    });
    row.add_suffix(&button);
    group.add(&row);
    if std::env::var(SMOKE_IMPORT_ENV).is_ok() {
        let weak = Rc::downgrade(context);
        let button = button.clone();
        glib::idle_add_local_once(move || {
            if let Some(context) = weak.upgrade() {
                context.start_rhythmbox_import(
                    &button,
                    false,
                    RhythmboxImportChoices {
                        ratings: true,
                        play_counts: true,
                        added_at: true,
                    },
                    true,
                );
            }
        });
    }
}

impl PreferencesContext {
    fn present_rhythmbox_import(self: &Rc<Self>, button: &gtk4::Button) {
        let surface = build_import_dialog();
        let weak = Rc::downgrade(self);
        let button = button.clone();
        let rows = surface.rows;
        surface.dialog.choose(
            Some(&self.window),
            gio::Cancellable::NONE,
            move |response| {
                if response != RESPONSE_IMPORT {
                    return;
                }
                let Some(context) = weak.upgrade() else {
                    return;
                };
                context.start_rhythmbox_import(
                    &button,
                    rows[0].is_active(),
                    RhythmboxImportChoices {
                        ratings: rows[1].is_active(),
                        play_counts: rows[2].is_active(),
                        added_at: rows[3].is_active(),
                    },
                    rows[4].is_active(),
                );
            },
        );
    }

    fn start_rhythmbox_import(
        self: &Rc<Self>,
        button: &gtk4::Button,
        column_layout: bool,
        choices: RhythmboxImportChoices,
        import_playlists: bool,
    ) {
        button.set_sensitive(false);
        let rhythmdb_path = default_rhythmdb_path();
        let playlists_path = default_playlists_path(&rhythmdb_path);
        let weak = Rc::downgrade(self);
        let button = button.clone();
        glib::spawn_future_local(async move {
            let parsed = gio::spawn_blocking(move || -> Result<ParsedImport, String> {
                let tracks = if choices.ratings || choices.play_counts || choices.added_at {
                    Some(
                        rhythmbox_import::parse_rhythmdb(&rhythmdb_path)
                            .map_err(|error| error.to_string())?,
                    )
                } else {
                    None
                };
                let playlists = import_playlists.then(|| {
                    rhythmbox_import::parse_playlists(&playlists_path)
                        .map_err(|error| error.to_string())
                });
                Ok(ParsedImport { tracks, playlists })
            })
            .await;
            let parsed = match parsed {
                Ok(result) => result,
                Err(_) => Err("Rhythmbox import worker panicked".to_string()),
            };
            let Some(context) = weak.upgrade() else {
                return;
            };
            let result = match parsed {
                Ok(parsed) => {
                    let mut conn = context.conn.borrow_mut();
                    (|| -> Result<ImportResult, String> {
                        let stats = parsed
                            .tracks
                            .map(|tracks| {
                                rhythmbox_import::merge_stats(&mut conn, &tracks, choices)
                            })
                            .transpose()
                            .map_err(|error| error.to_string())?;
                        let (playlists, playlist_error) = match parsed.playlists {
                            Some(Ok(playlists)) => (
                                Some(
                                    rhythmbox_import::merge_playlists(&mut conn, &playlists)
                                        .map_err(|error| error.to_string())?,
                                ),
                                None,
                            ),
                            Some(Err(error)) => (None, Some(error)),
                            None => (None, None),
                        };
                        Ok(ImportResult {
                            stats,
                            playlists,
                            playlist_error,
                        })
                    })()
                }
                Err(error) => Err(error),
            };
            if result.is_ok() && column_layout {
                context.import_rhythmbox_column_layout();
            }
            if result.is_ok() {
                context.track_list.reload();
            }
            if result
                .as_ref()
                .is_ok_and(|result| result.playlists.is_some())
            {
                context.sidebar.refresh("Rhythmbox playlist import");
            }
            match &result {
                Ok(imported) => tracing::info!(
                    matched = imported.stats.map_or(0, |summary| summary.matched),
                    ratings = imported.stats.map_or(0, |summary| summary.ratings_imported),
                    play_counts = imported
                        .stats
                        .map_or(0, |summary| summary.play_counts_raised),
                    dates = imported.stats.map_or(0, |summary| summary.dates_imported),
                    playlists = imported.playlists.map_or(0, |summary| summary.imported),
                    playlist_tracks = imported.playlists.map_or(0, |summary| summary.tracks_added),
                    playlist_warning = imported.playlist_error.is_some(),
                    "Rhythmbox import finished"
                ),
                Err(error) => tracing::warn!(%error, "Rhythmbox import failed"),
            }
            button.set_sensitive(true);
            if std::env::var(SMOKE_IMPORT_ENV).is_err() {
                context.show_rhythmbox_result(result);
            }
        });
    }

    fn import_rhythmbox_column_layout(&self) {
        match super::column_layout::read_rhythmbox_visible_columns() {
            Ok(tokens) => {
                let layout = super::column_layout::import_rhythmbox_tokens(&tokens);
                if let Err(error) = self.track_list.apply_column_layout(&layout) {
                    tracing::warn!(%error, "could not persist imported Rhythmbox column layout");
                }
            }
            Err(error) => tracing::warn!(%error, "could not read Rhythmbox visible columns"),
        }
    }

    fn show_rhythmbox_result(&self, result: Result<ImportResult, String>) {
        let (heading, body) = match result {
            Ok(result) => {
                let mut lines = Vec::new();
                if let Some(summary) = result.stats {
                    lines.push(strings::rhythmbox_import_summary(
                        summary.matched,
                        summary.ratings_imported,
                        summary.play_counts_raised,
                        summary.dates_imported,
                        summary.skipped,
                    ));
                }
                if let Some(summary) = result.playlists {
                    lines.push(strings::rhythmbox_playlist_import_summary(
                        summary.imported,
                        summary.tracks_added,
                        summary.skipped_tracks,
                    ));
                }
                if let Some(error) = result.playlist_error {
                    lines.push(strings::rhythmbox_playlist_import_error(&error));
                    (
                        strings::text(strings::RHYTHMBOX_IMPORT_PARTIAL),
                        lines.join("\n"),
                    )
                } else {
                    (
                        strings::text(strings::RHYTHMBOX_IMPORT_COMPLETE),
                        lines.join("\n"),
                    )
                }
            }
            Err(error) => (
                strings::text(strings::RHYTHMBOX_IMPORT_FAILED),
                strings::rhythmbox_import_error(&error),
            ),
        };
        let dialog = adw::AlertDialog::builder()
            .heading(heading)
            .body(body)
            .close_response("close")
            .build();
        dialog.add_response("close", &strings::text(strings::CLOSE));
        dialog.present(Some(&self.window));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statistics_are_selected_but_column_layout_requires_opt_in() {
        let options = import_option_specs();
        assert_eq!(options.len(), 5);
        assert_eq!(options[0].id, RhythmboxOption::ColumnLayout);
        assert!(!options[0].selected);
        assert_eq!(options[1].id, RhythmboxOption::Ratings);
        assert!(options[1].selected);
        assert_eq!(options[2].id, RhythmboxOption::PlayCounts);
        assert!(options[2].selected);
        assert_eq!(options[3].id, RhythmboxOption::DateAdded);
        assert!(options[3].selected);
        assert_eq!(options[4].id, RhythmboxOption::Playlists);
        assert!(options[4].selected);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn import_dialog_exposes_all_five_explicit_choices() {
        gtk4::init().unwrap();
        let surface = build_import_dialog();
        assert_eq!(surface.rows.len(), 5);
        assert_eq!(surface.rows[0].title(), "Column layout");
        assert_eq!(surface.rows[1].title(), "Ratings");
        assert_eq!(surface.rows[2].title(), "Play counts");
        assert_eq!(surface.rows[3].title(), "Date added");
        assert_eq!(surface.rows[4].title(), "Playlists");
        assert!(!surface.rows[0].is_active());
        assert!(surface.rows[1].is_active());
        assert!(surface.rows[2].is_active());
        assert!(surface.rows[3].is_active());
        assert!(surface.rows[4].is_active());
    }
}
