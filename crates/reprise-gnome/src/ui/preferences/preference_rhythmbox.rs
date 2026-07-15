use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library::rhythmbox_import::{
    self, RhythmboxImportChoices, RhythmboxPlaylist, RhythmboxPrescanResult, RhythmboxRollback,
    RhythmboxTrackStats,
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

fn option_title(option: RhythmboxOption) -> String {
    strings::text(match option {
        RhythmboxOption::ColumnLayout => strings::ONBOARDING_RHYTHMBOX_COLUMN_LAYOUT,
        RhythmboxOption::Ratings => strings::RHYTHMBOX_IMPORT_RATINGS,
        RhythmboxOption::PlayCountsAndLastPlayed => strings::RHYTHMBOX_PLAY_COUNTS_AND_LAST_PLAYED,
        RhythmboxOption::DateAdded => strings::RHYTHMBOX_IMPORT_DATE_ADDED,
        RhythmboxOption::Playlists => strings::RHYTHMBOX_IMPORT_PLAYLISTS,
    })
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
                context.present_rhythmbox_import_dialog();
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Three-state import dialog
// ---------------------------------------------------------------------------

struct ImportDialogWidgets {
    dialog: adw::Dialog,
    stack: gtk4::Stack,
    // Selection state
    info_subtitle: gtk4::Label,
    match_label: gtk4::Label,
    warning_row: adw::ActionRow,
    import_button: gtk4::Button,
    rows: Vec<adw::SwitchRow>,
    // Progress state
    progress_bar: gtk4::ProgressBar,
    progress_label: gtk4::Label,
    // Complete state
    complete_subtitle: gtk4::Label,
    ratings_result: adw::ActionRow,
    play_counts_result: adw::ActionRow,
    dates_result: adw::ActionRow,
    playlists_result: adw::ActionRow,
    skipped_expander: adw::ExpanderRow,
    skip_outside: adw::ActionRow,
    skip_missing: adw::ActionRow,
    skip_non_song: adw::ActionRow,
    undo_button: gtk4::Button,
    done_button: gtk4::Button,
}

fn build_import_dialog() -> ImportDialogWidgets {
    // === Selection state ===
    let info_icon = gtk4::Image::from_icon_name("emblem-ok-symbolic");
    info_icon.set_pixel_size(24);
    let info_title = gtk4::Label::new(Some(&strings::text(strings::RHYTHMBOX_LIBRARY_FOUND)));
    info_title.add_css_class("heading");
    let info_subtitle = gtk4::Label::new(None);
    info_subtitle.add_css_class("dim-label");
    info_subtitle.set_wrap(true);
    info_subtitle.set_xalign(0.0);
    let match_label = gtk4::Label::new(None);
    match_label.add_css_class("dim-label");
    match_label.set_xalign(0.0);

    let info_text = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    info_text.append(&info_title);
    info_text.append(&info_subtitle);
    info_text.append(&match_label);
    let info_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    info_box.append(&info_icon);
    info_box.append(&info_text);
    info_box.set_margin_bottom(12);

    let body_label = gtk4::Label::new(Some(&strings::text(strings::RHYTHMBOX_IMPORT_BODY_RICH)));
    body_label.set_wrap(true);
    body_label.set_xalign(0.0);
    body_label.add_css_class("dim-label");
    body_label.set_margin_bottom(12);

    let options_group = adw::PreferencesGroup::new();
    let specs = import_option_specs();
    let rows: Vec<adw::SwitchRow> = specs
        .into_iter()
        .map(|spec| {
            let row = adw::SwitchRow::builder()
                .title(option_title(spec.id))
                .active(spec.selected)
                .build();
            options_group.add(&row);
            row
        })
        .collect();

    let warning_row = adw::ActionRow::builder().title("").build();
    warning_row.add_prefix(&gtk4::Image::from_icon_name("dialog-warning-symbolic"));
    warning_row.set_visible(false);

    let selection_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    selection_box.set_margin_top(18);
    selection_box.set_margin_bottom(18);
    selection_box.set_margin_start(18);
    selection_box.set_margin_end(18);
    selection_box.append(&info_box);
    selection_box.append(&body_label);
    selection_box.append(&options_group);
    selection_box.append(&warning_row);

    // === Progress state ===
    let progress_title = gtk4::Label::new(Some(&strings::text(strings::RHYTHMBOX_IMPORTING)));
    progress_title.add_css_class("title-3");
    let progress_bar = gtk4::ProgressBar::new();
    let progress_label = gtk4::Label::new(None);
    progress_label.add_css_class("dim-label");

    let progress_box = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    progress_box.set_margin_top(48);
    progress_box.set_margin_bottom(48);
    progress_box.set_margin_start(24);
    progress_box.set_margin_end(24);
    progress_box.set_valign(gtk4::Align::Center);
    progress_box.append(&progress_title);
    progress_box.append(&progress_bar);
    progress_box.append(&progress_label);

    // === Complete state ===
    let complete_icon = gtk4::Image::from_icon_name("emblem-ok-symbolic");
    complete_icon.set_pixel_size(48);
    complete_icon.set_halign(gtk4::Align::Center);
    complete_icon.set_margin_bottom(12);
    let complete_heading = gtk4::Label::new(Some(&strings::text(
        strings::RHYTHMBOX_IMPORT_COMPLETE_HEADING,
    )));
    complete_heading.add_css_class("title-2");
    complete_heading.set_halign(gtk4::Align::Center);
    let complete_subtitle = gtk4::Label::new(None);
    complete_subtitle.add_css_class("dim-label");
    complete_subtitle.set_halign(gtk4::Align::Center);
    complete_subtitle.set_margin_bottom(18);

    let results_group = adw::PreferencesGroup::new();
    let ratings_result = adw::ActionRow::builder()
        .title(strings::text(strings::RHYTHMBOX_IMPORT_RATINGS))
        .build();
    let play_counts_result = adw::ActionRow::builder()
        .title(strings::text(strings::RHYTHMBOX_PLAY_COUNTS_AND_LAST_PLAYED))
        .build();
    let dates_result = adw::ActionRow::builder()
        .title(strings::text(strings::RHYTHMBOX_IMPORT_DATE_ADDED))
        .build();
    let playlists_result = adw::ActionRow::builder()
        .title(strings::text(strings::RHYTHMBOX_IMPORT_PLAYLISTS))
        .build();
    results_group.add(&ratings_result);
    results_group.add(&play_counts_result);
    results_group.add(&dates_result);
    results_group.add(&playlists_result);

    let skipped_expander = adw::ExpanderRow::builder()
        .title("")
        .show_enable_switch(false)
        .build();
    skipped_expander.add_prefix(&gtk4::Image::from_icon_name("dialog-warning-symbolic"));
    let skip_outside = adw::ActionRow::builder()
        .title(strings::text(strings::RHYTHMBOX_SKIP_OUTSIDE_LIBRARY))
        .build();
    let skip_missing = adw::ActionRow::builder()
        .title(strings::text(strings::RHYTHMBOX_SKIP_MISSING_ON_DISK))
        .build();
    let skip_non_song = adw::ActionRow::builder()
        .title(strings::text(strings::RHYTHMBOX_SKIP_NON_SONG))
        .build();
    skipped_expander.add_row(&skip_outside);
    skipped_expander.add_row(&skip_missing);
    skipped_expander.add_row(&skip_non_song);
    let skipped_group = adw::PreferencesGroup::new();
    skipped_group.add(&skipped_expander);

    let undo_button = gtk4::Button::with_label(&strings::text(strings::RHYTHMBOX_UNDO_IMPORT));
    undo_button.add_css_class("flat");
    undo_button.add_css_class("destructive-action");
    let done_button = gtk4::Button::with_label(&strings::text(strings::RHYTHMBOX_DONE));
    done_button.add_css_class("suggested-action");
    let button_bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    button_bar.set_halign(gtk4::Align::End);
    button_bar.set_margin_top(18);
    button_bar.append(&undo_button);
    button_bar.append(&done_button);

    let complete_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    complete_box.set_margin_top(24);
    complete_box.set_margin_bottom(18);
    complete_box.set_margin_start(18);
    complete_box.set_margin_end(18);
    complete_box.append(&complete_icon);
    complete_box.append(&complete_heading);
    complete_box.append(&complete_subtitle);
    complete_box.append(&results_group);
    complete_box.append(&skipped_group);
    complete_box.append(&button_bar);

    // === Stack ===
    let stack = gtk4::Stack::new();
    stack.set_transition_type(gtk4::StackTransitionType::SlideLeft);
    stack.add_named(&selection_box, Some("selection"));
    stack.add_named(&progress_box, Some("progress"));
    stack.add_named(&complete_box, Some("complete"));

    // === Dialog ===
    let cancel_button = gtk4::Button::with_label(&strings::text(strings::RHYTHMBOX_CANCEL));
    let import_button = gtk4::Button::with_label(&strings::text(strings::RHYTHMBOX_IMPORT_START));
    import_button.add_css_class("suggested-action");
    import_button.set_sensitive(false); // enabled after prescan

    let header = adw::HeaderBar::new();
    header.pack_start(&cancel_button);
    header.pack_end(&import_button);
    header.set_show_end_title_buttons(false);
    header.set_show_start_title_buttons(false);
    header.set_title_widget(Some(&adw::WindowTitle::new(
        &strings::text(strings::ONBOARDING_IMPORT_FROM_RHYTHMBOX),
        "",
    )));

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&stack));
    let dialog = adw::Dialog::builder()
        .child(&toolbar)
        .content_width(560)
        .build();

    cancel_button.connect_clicked({
        let dialog = dialog.clone();
        move |_| {
            dialog.close();
        }
    });

    ImportDialogWidgets {
        dialog,
        stack,
        info_subtitle,
        match_label,
        warning_row,
        import_button,
        rows,
        progress_bar,
        progress_label,
        complete_subtitle,
        ratings_result,
        play_counts_result,
        dates_result,
        playlists_result,
        skipped_expander,
        skip_outside,
        skip_missing,
        skip_non_song,
        undo_button,
        done_button,
    }
}

// ---------------------------------------------------------------------------
// PreferencesContext integration
// ---------------------------------------------------------------------------

impl PreferencesContext {
    pub(super) fn present_rhythmbox_import_dialog(self: &Rc<Self>) {
        let widgets = build_import_dialog();
        let rhythmdb_path = default_rhythmdb_path();
        let playlists_path = default_playlists_path(&rhythmdb_path);
        let library_root = {
            let conn = self.conn.borrow();
            reprise_core::library::settings::get_library_root(&conn)
                .ok()
                .flatten()
        };

        // For prescan, open a separate read-only connection off-main.
        let root_clone = library_root.clone();
        let rhythmdb_clone = rhythmdb_path.clone();
        let playlists_clone = playlists_path.clone();
        let conn_for_prescan = {
            let conn = self.conn.borrow();
            conn.path().map(|p| p.to_owned())
        };

        let info_subtitle = widgets.info_subtitle.clone();
        let match_label = widgets.match_label.clone();
        let warning_row = widgets.warning_row.clone();
        let import_button = widgets.import_button.clone();
        let rows = widgets.rows.clone();
        let prescan_result: Rc<RefCell<Option<RhythmboxPrescanResult>>> =
            Rc::new(RefCell::new(None));
        let prescan_for_import = prescan_result.clone();

        // Spawn prescan in background
        glib::spawn_future_local({
            let prescan_result = prescan_result.clone();
            async move {
                let result = gio::spawn_blocking(move || {
                    let conn_path = conn_for_prescan.unwrap_or_default();
                    if conn_path.is_empty() {
                        return Err("no database path".to_string());
                    }
                    let conn = rusqlite::Connection::open_with_flags(
                        &conn_path,
                        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                    )
                    .map_err(|e| e.to_string())?;
                    rhythmbox_import::prescan_rhythmdb(
                        &rhythmdb_clone,
                        &playlists_clone,
                        &conn,
                        root_clone.as_deref(),
                    )
                    .map_err(|e| e.to_string())
                })
                .await;
                let result = match result {
                    Ok(Ok(r)) => r,
                    Ok(Err(e)) => {
                        tracing::warn!(error = %e, "prescan failed");
                        import_button.set_sensitive(false);
                        return;
                    }
                    Err(_) => {
                        tracing::warn!("prescan worker panicked");
                        import_button.set_sensitive(false);
                        return;
                    }
                };

                // Fill in selection state
                let days_ago = result
                    .last_modified
                    .and_then(|m| m.elapsed().ok().map(|d| d.as_secs() / 86400));
                info_subtitle
                    .set_label(&strings::rhythmbox_prescan_info(result.song_entries, days_ago));
                match_label.set_label(&strings::rhythmbox_match_count(result.matched));

                // Set subtitles on rows based on prescan
                if rows.len() >= 5 {
                    rows[1].set_subtitle(&strings::rhythmbox_rated_subtitle(result.rated_tracks));
                    rows[2].set_subtitle(&strings::rhythmbox_history_subtitle(
                        result.tracks_with_history,
                    ));
                    rows[3].set_subtitle(&strings::rhythmbox_date_added_subtitle());
                    rows[4].set_subtitle(&strings::rhythmbox_playlists_subtitle(
                        result.playlist_count,
                        result.playlist_track_count,
                    ));
                }

                let total_skipped =
                    result.outside_library + result.missing_on_disk + result.non_song_entries;
                if total_skipped > 0 {
                    warning_row.set_title(&strings::rhythmbox_skipped_warning(total_skipped));
                    warning_row.set_visible(true);
                }

                import_button.set_sensitive(true);
                *prescan_result.borrow_mut() = Some(result);
                tracing::info!("Rhythmbox prescan complete, dialog ready");
            }
        });

        // Wire import button
        let weak_import = Rc::downgrade(self);
        let stack = widgets.stack.clone();
        let progress_bar = widgets.progress_bar.clone();
        let progress_label = widgets.progress_label.clone();
        let complete_subtitle = widgets.complete_subtitle.clone();
        let ratings_result = widgets.ratings_result.clone();
        let play_counts_result = widgets.play_counts_result.clone();
        let dates_result = widgets.dates_result.clone();
        let playlists_result = widgets.playlists_result.clone();
        let skipped_expander = widgets.skipped_expander.clone();
        let skip_outside = widgets.skip_outside.clone();
        let skip_missing = widgets.skip_missing.clone();
        let skip_non_song = widgets.skip_non_song.clone();
        let rollback_holder: Rc<RefCell<Option<RhythmboxRollback>>> =
            Rc::new(RefCell::new(None));
        let rollback_for_import = rollback_holder.clone();
        let rows_for_import = widgets.rows.clone();

        widgets.import_button.connect_clicked(move |button| {
            let Some(context) = weak_import.upgrade() else {
                return;
            };
            button.set_sensitive(false);
            stack.set_visible_child_name("progress");

            let column_layout = rows_for_import[0].is_active();
            let choices = RhythmboxImportChoices {
                ratings: rows_for_import[1].is_active(),
                play_counts_and_last_played: rows_for_import[2].is_active(),
                added_at: rows_for_import[3].is_active(),
            };
            let import_playlists = rows_for_import[4].is_active();
            let any_stats =
                choices.ratings || choices.play_counts_and_last_played || choices.added_at;

            let progress_bar_c = progress_bar.clone();
            let progress_label_c = progress_label.clone();

            let stack_c = stack.clone();
            let complete_subtitle_c = complete_subtitle.clone();
            let ratings_result_c = ratings_result.clone();
            let play_counts_result_c = play_counts_result.clone();
            let dates_result_c = dates_result.clone();
            let playlists_result_c = playlists_result.clone();
            let skipped_expander_c = skipped_expander.clone();
            let skip_outside_c = skip_outside.clone();
            let skip_missing_c = skip_missing.clone();
            let skip_non_song_c = skip_non_song.clone();
            let rollback_c = rollback_for_import.clone();
            let prescan_for_complete = prescan_for_import.clone();
            let context_c = context.clone();

            // Pulse the progress bar while parsing runs off-thread
            progress_bar_c.pulse();

            glib::spawn_future_local(async move {
                let parsed = gio::spawn_blocking(move || -> Result<ParsedImport, String> {
                    let rhythmdb_path = default_rhythmdb_path();
                    let playlists_path = default_playlists_path(&rhythmdb_path);
                    let tracks = if any_stats {
                        Some(
                            rhythmbox_import::parse_rhythmdb(&rhythmdb_path)
                                .map_err(|e| e.to_string())?,
                        )
                    } else {
                        None
                    };
                    let playlists = import_playlists.then(|| {
                        rhythmbox_import::parse_playlists(&playlists_path)
                            .map_err(|e| e.to_string())
                    });
                    Ok(ParsedImport { tracks, playlists })
                })
                .await;

                let parsed = match parsed {
                    Ok(Ok(p)) => p,
                    _ => {
                        tracing::warn!("Rhythmbox import parse failed");
                        stack_c.set_visible_child_name("selection");
                        return;
                    }
                };

                // Merge on main thread (we need conn)
                let mut conn = context_c.conn.borrow_mut();
                let total_tracks = parsed
                    .tracks
                    .as_ref()
                    .map_or(0usize, |t| t.len());
                let stats = parsed.tracks.map(|tracks| {
                    rhythmbox_import::merge_stats(
                        &mut conn,
                        &tracks,
                        choices,
                        None,
                    )
                });
                let (summary, rollback) = match stats {
                    Some(Ok((s, r))) => (Some(s), Some(r)),
                    Some(Err(e)) => {
                        tracing::warn!(%e, "merge_stats failed");
                        (None, None)
                    }
                    None => (None, None),
                };

                let playlist_summary = match parsed.playlists {
                    Some(Ok(playlists)) => Some(
                        rhythmbox_import::merge_playlists(&mut conn, &playlists)
                            .map_err(|e| e.to_string()),
                    ),
                    Some(Err(e)) => {
                        tracing::warn!(%e, "playlist parse failed");
                        None
                    }
                    None => None,
                };
                drop(conn);

                // Update progress to complete
                progress_bar_c.set_fraction(1.0);
                progress_label_c
                    .set_label(&strings::rhythmbox_progress_count(total_tracks, total_tracks));

                if column_layout {
                    context_c.import_rhythmbox_column_layout();
                }
                if summary.is_some() {
                    context_c.track_list.reload();
                }
                if playlist_summary.as_ref().is_some_and(|r| r.is_ok()) {
                    context_c.sidebar.refresh("Rhythmbox playlist import");
                }

                // Store rollback
                *rollback_c.borrow_mut() = rollback;

                // Fill complete state
                let prescan = prescan_for_complete.borrow();
                let matched = summary.map_or(0, |s| s.matched);
                let total = prescan.as_ref().map_or(0, |p| p.song_entries);
                complete_subtitle_c
                    .set_label(&strings::rhythmbox_entries_matched(matched, total));

                if let Some(ref s) = summary {
                    ratings_result_c
                        .set_subtitle(&strings::rhythmbox_result_ratings(s.ratings_imported));
                    play_counts_result_c
                        .set_subtitle(&strings::rhythmbox_result_play_counts(s.play_counts_raised));
                    dates_result_c.set_subtitle(&strings::rhythmbox_result_dates(
                        s.dates_imported,
                        s.last_played_imported,
                    ));
                }
                if let Some(Ok(ref ps)) = playlist_summary {
                    playlists_result_c
                        .set_subtitle(&strings::rhythmbox_result_playlists(ps.imported));
                }

                // Skipped breakdown
                let outside = prescan.as_ref().map_or(0, |p| p.outside_library);
                let missing = prescan.as_ref().map_or(0, |p| p.missing_on_disk);
                let non_song = prescan.as_ref().map_or(0, |p| p.non_song_entries);
                let total_skipped = outside + missing + non_song;
                if total_skipped > 0 {
                    skipped_expander_c
                        .set_title(&strings::rhythmbox_entries_skipped(total_skipped));
                    skip_outside_c.set_subtitle(&outside.to_string());
                    skip_missing_c.set_subtitle(&missing.to_string());
                    skip_non_song_c.set_subtitle(&non_song.to_string());
                    skipped_expander_c.set_visible(true);
                } else {
                    skipped_expander_c.set_visible(false);
                }

                tracing::info!(
                    matched,
                    ratings = summary.map_or(0, |s| s.ratings_imported),
                    play_counts = summary.map_or(0, |s| s.play_counts_raised),
                    dates = summary.map_or(0, |s| s.dates_imported),
                    last_played = summary.map_or(0, |s| s.last_played_imported),
                    "Rhythmbox import finished"
                );

                stack_c.set_visible_child_name("complete");
            });
        });

        // Wire undo button
        let rollback_for_undo = rollback_holder.clone();
        let weak_undo = Rc::downgrade(self);
        let dialog_for_undo = widgets.dialog.clone();
        widgets.undo_button.connect_clicked(move |_| {
            let Some(context) = weak_undo.upgrade() else {
                return;
            };
            let rollback = rollback_for_undo.borrow_mut().take();
            if let Some(rollback) = rollback {
                let mut conn = context.conn.borrow_mut();
                match rhythmbox_import::undo_rhythmbox_import(&mut conn, &rollback) {
                    Ok(restored) => {
                        tracing::info!(restored, "Rhythmbox import undone");
                        drop(conn);
                        context.track_list.reload();
                    }
                    Err(e) => tracing::warn!(%e, "could not undo Rhythmbox import"),
                }
            }
            dialog_for_undo.close();
        });

        // Wire done button
        let dialog_for_done = widgets.dialog.clone();
        widgets.done_button.connect_clicked(move |_| {
            dialog_for_done.close();
        });

        // Present
        let parent = self.preferences_parent();
        widgets.dialog.present(Some(&parent));
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
    fn import_dialog_has_three_stack_states_and_five_option_rows() {
        gtk4::init().unwrap();
        let widgets = build_import_dialog();

        // Stack has three children
        assert!(widgets.stack.child_by_name("selection").is_some());
        assert!(widgets.stack.child_by_name("progress").is_some());
        assert!(widgets.stack.child_by_name("complete").is_some());

        // Five option rows with correct titles and defaults
        assert_eq!(widgets.rows.len(), 5);
        assert_eq!(widgets.rows[0].title(), "Column layout");
        assert_eq!(widgets.rows[1].title(), "Ratings");
        assert_eq!(widgets.rows[2].title(), "Play counts & last played");
        assert_eq!(widgets.rows[3].title(), "Date added");
        assert_eq!(widgets.rows[4].title(), "Playlists");
        assert!(!widgets.rows[0].is_active());
        assert!(widgets.rows[1].is_active());
        assert!(widgets.rows[2].is_active());
        assert!(widgets.rows[3].is_active());
        assert!(widgets.rows[4].is_active());

        // Import button starts insensitive (needs prescan)
        assert!(!widgets.import_button.is_sensitive());
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
