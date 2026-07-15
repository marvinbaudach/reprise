use std::path::{Path, PathBuf};
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

const RHYTHMDB_PATH_ENV: &str = "REPRISE_RHYTHMDB_PATH";
const PLAYLISTS_PATH_ENV: &str = "REPRISE_RHYTHMBOX_PLAYLISTS_PATH";
const SMOKE_IMPORT_ENV: &str = "REPRISE_SMOKE_RHYTHMDB_IMPORT";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RhythmboxOption {
    ColumnLayout,
    Ratings,
    PlayCountsAndLastPlayed,
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
            id: RhythmboxOption::PlayCountsAndLastPlayed,
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

struct ImportPageSurface {
    page: adw::NavigationPage,
    rows: Vec<adw::SwitchRow>,
    import_button: gtk4::Button,
}

fn option_title(option: RhythmboxOption) -> String {
    strings::text(match option {
        RhythmboxOption::ColumnLayout => strings::ONBOARDING_RHYTHMBOX_COLUMN_LAYOUT,
        RhythmboxOption::Ratings => strings::RHYTHMBOX_IMPORT_RATINGS,
        RhythmboxOption::PlayCountsAndLastPlayed => strings::RHYTHMBOX_PLAY_COUNTS_AND_LAST_PLAYED,
        RhythmboxOption::DateAdded => strings::RHYTHMBOX_IMPORT_DATE_ADDED,
        RhythmboxOption::Playlists => strings::RHYTHMBOX_IMPORT_PLAYLISTS,
    })
}

fn build_import_page() -> ImportPageSurface {
    let group = adw::PreferencesGroup::builder()
        .description(strings::text(strings::RHYTHMBOX_IMPORT_DIALOG_BODY))
        .build();
    let rows = import_option_specs()
        .into_iter()
        .map(|spec| {
            let row = adw::SwitchRow::builder()
                .title(option_title(spec.id))
                .active(spec.selected)
                .build();
            group.add(&row);
            row
        })
        .collect();
    let content = adw::PreferencesPage::new();
    content.add(&group);
    let import_button = gtk4::Button::with_label(&strings::text(strings::RHYTHMBOX_IMPORT_START));
    import_button.add_css_class("suggested-action");
    let header = adw::HeaderBar::new();
    header.pack_end(&import_button);
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&content));
    let title = strings::text(strings::ONBOARDING_IMPORT_FROM_RHYTHMBOX);
    let page = adw::NavigationPage::with_tag(&toolbar, &title, "rhythmbox-import");
    ImportPageSurface {
        page,
        rows,
        import_button,
    }
}

fn default_rhythmdb_path() -> PathBuf {
    std::env::var_os(RHYTHMDB_PATH_ENV).map_or_else(
        || glib::user_data_dir().join("rhythmbox/rhythmdb.xml"),
        PathBuf::from,
    )
}

fn rhythmbox_data_available(rhythmdb_path: &Path) -> bool {
    rhythmdb_path.is_file()
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

struct ImportRowSurface {
    row: adw::ActionRow,
}

fn build_import_row(rhythmdb_path: &Path) -> Option<ImportRowSurface> {
    if !rhythmbox_data_available(rhythmdb_path) {
        return None;
    }
    let row = adw::ActionRow::builder()
        .title(strings::text(strings::ONBOARDING_IMPORT_FROM_RHYTHMBOX))
        .subtitle(strings::text(strings::RHYTHMBOX_IMPORT_DESCRIPTION))
        .activatable(true)
        .build();
    row.add_suffix(&gtk4::Image::from_icon_name("go-next-symbolic"));
    Some(ImportRowSurface { row })
}

pub(super) fn add_rhythmbox_import_row(
    context: &Rc<PreferencesContext>,
    group: &adw::PreferencesGroup,
) {
    let Some(surface) = build_import_row(&default_rhythmdb_path()) else {
        return;
    };
    let ImportRowSurface { row } = surface;
    let weak = Rc::downgrade(context);
    row.connect_activated(move |_| {
        if let Some(context) = weak.upgrade() {
            context.open_rhythmbox_import();
        }
    });
    group.add(&row);
    if std::env::var(SMOKE_IMPORT_ENV).is_ok() {
        let weak = Rc::downgrade(context);
        glib::idle_add_local_once(move || {
            if let Some(context) = weak.upgrade() {
                let surface = build_import_page();
                context.start_rhythmbox_import(
                    &surface.import_button,
                    false,
                    RhythmboxImportChoices {
                        ratings: true,
                        play_counts_and_last_played: true,
                        added_at: true,
                    },
                    true,
                );
            }
        });
    }
}

pub(super) fn push_import_page(context: &Rc<PreferencesContext>, navigation: &adw::NavigationView) {
    let surface = build_import_page();
    let weak = Rc::downgrade(context);
    let rows = surface.rows;
    surface.import_button.connect_clicked(move |button| {
        let Some(context) = weak.upgrade() else {
            return;
        };
        context.start_rhythmbox_import(
            button,
            rows[0].is_active(),
            RhythmboxImportChoices {
                ratings: rows[1].is_active(),
                play_counts_and_last_played: rows[2].is_active(),
                added_at: rows[3].is_active(),
            },
            rows[4].is_active(),
        );
    });
    navigation.push(&surface.page);
}

impl PreferencesContext {
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
                let tracks = if choices.ratings
                    || choices.play_counts_and_last_played
                    || choices.added_at
                {
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
                    last_played = imported
                        .stats
                        .map_or(0, |summary| summary.last_played_imported),
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
                        summary.last_played_imported,
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
        if let Some(preferences_window) = self.preferences_window() {
            dialog.present(Some(&preferences_window));
        } else {
            dialog.present(Some(&self.window));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn import_row_requires_a_detected_rhythmdb_file() {
        let dir = tempdir().unwrap();
        let rhythmdb = dir.path().join("rhythmdb.xml");

        assert!(!rhythmbox_data_available(&rhythmdb));
        fs::write(&rhythmdb, "<rhythmdb/>").unwrap();
        assert!(rhythmbox_data_available(&rhythmdb));
        assert!(!rhythmbox_data_available(dir.path()));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn detected_rhythmdb_builds_the_import_row() {
        gtk4::init().unwrap();
        let dir = tempdir().unwrap();
        let rhythmdb = dir.path().join("rhythmdb.xml");
        assert!(build_import_row(&rhythmdb).is_none());

        fs::write(&rhythmdb, "<rhythmdb/>").unwrap();
        let surface = build_import_row(&rhythmdb).unwrap();
        assert_eq!(surface.row.title(), "Import from Rhythmbox");
        assert!(surface.row.is_activatable());
        let widgets = descendants(surface.row.upcast_ref());
        assert!(widgets
            .iter()
            .filter_map(|widget| widget.clone().downcast::<gtk4::Image>().ok())
            .any(|image| image.icon_name().as_deref() == Some("go-next-symbolic")));
        assert!(!widgets
            .iter()
            .any(gtk4::prelude::ObjectExt::is::<gtk4::Button>));
    }

    #[test]
    fn statistics_are_selected_but_column_layout_requires_opt_in() {
        let options = import_option_specs();
        assert_eq!(options.len(), 5);
        assert_eq!(options[0].id, RhythmboxOption::ColumnLayout);
        assert!(!options[0].selected);
        assert_eq!(options[1].id, RhythmboxOption::Ratings);
        assert!(options[1].selected);
        assert_eq!(options[2].id, RhythmboxOption::PlayCountsAndLastPlayed);
        assert!(options[2].selected);
        assert_eq!(options[3].id, RhythmboxOption::DateAdded);
        assert!(options[3].selected);
        assert_eq!(options[4].id, RhythmboxOption::Playlists);
        assert!(options[4].selected);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn import_page_exposes_all_five_explicit_choices() {
        gtk4::init().unwrap();
        let surface = build_import_page();
        assert_eq!(surface.page.title(), "Import from Rhythmbox");
        assert!(surface.page.can_pop());
        assert!(surface
            .page
            .child()
            .is_some_and(|child| child.is::<adw::ToolbarView>()));
        assert_eq!(surface.import_button.label().as_deref(), Some("Import"));
        let root = adw::NavigationPage::with_tag(
            &gtk4::Box::new(gtk4::Orientation::Vertical, 0),
            "Preferences",
            "preferences",
        );
        let navigation = adw::NavigationView::new();
        navigation.add(&root);
        navigation.push(&surface.page);
        assert_eq!(navigation.visible_page().as_ref(), Some(&surface.page));
        assert!(navigation.pop());
        assert_eq!(navigation.visible_page().as_ref(), Some(&root));
        assert_eq!(surface.rows.len(), 5);
        assert_eq!(surface.rows[0].title(), "Column layout");
        assert_eq!(surface.rows[1].title(), "Ratings");
        assert_eq!(surface.rows[2].title(), "Play counts & last played");
        assert_eq!(surface.rows[3].title(), "Date added");
        assert_eq!(surface.rows[4].title(), "Playlists");
        assert!(!surface.rows[0].is_active());
        assert!(surface.rows[1].is_active());
        assert!(surface.rows[2].is_active());
        assert!(surface.rows[3].is_active());
        assert!(surface.rows[4].is_active());
    }

    fn descendants(root: &gtk4::Widget) -> Vec<gtk4::Widget> {
        let mut found = Vec::new();
        collect_descendants(root, &mut found);
        found
    }

    fn collect_descendants(root: &gtk4::Widget, found: &mut Vec<gtk4::Widget>) {
        let mut child = root.first_child();
        while let Some(widget) = child {
            found.push(widget.clone());
            collect_descendants(&widget, found);
            child = widget.next_sibling();
        }
    }
}
