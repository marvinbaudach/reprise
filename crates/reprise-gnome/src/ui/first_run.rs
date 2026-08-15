//! Native first-run wizard reusing the normal window actions and scan button.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::db::Db;
use reprise_core::library::settings;
use reprise_core::online_sources::{self, WizardSourceSelection};

use crate::ui::{
    first_run_sources::{self, SourceWidgets},
    preference_rhythmbox,
    scan_flow::ScanControls,
    strings,
};

pub(super) const SMOKE_ENV: &str = "REPRISE_SMOKE_FIRST_RUN";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FirstRunDecision {
    ShowWizard,
    ExistingLibrary,
    AlreadyCompleted,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct CompletionOptions {
    rhythmbox_import: bool,
    sources: WizardSourceSelection,
}

struct RhythmboxImportWidgets {
    group: adw::PreferencesGroup,
    import_data: adw::SwitchRow,
}

struct LibraryFolderWidgets {
    group: adw::PreferencesGroup,
    row: adw::ActionRow,
    choose: gtk4::Button,
}

struct WizardContentWidgets {
    root: gtk4::Box,
    library: Option<LibraryFolderWidgets>,
    rhythmbox: Option<RhythmboxImportWidgets>,
    sources: SourceWidgets,
    skip: gtk4::Button,
    setup: gtk4::Button,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompletionResponse {
    Skip,
    SetUp,
}

fn requested_actions(options: CompletionOptions) -> bool {
    options.rhythmbox_import
}

/// Everything the wizard persists, on both exits. `NET-4`: the wizard
/// *replaces* the discovery banner's question for a fresh install, so it
/// closes the banner too — otherwise the same question arrives twice.
fn persist_completion(db: &Db, options: CompletionOptions) {
    if let Err(error) = settings::set_onboarding_completed(db, true) {
        tracing::warn!(%error, "could not persist onboarding completion");
    }
    if let Err(error) = online_sources::apply_wizard_selection(db, options.sources) {
        tracing::warn!(%error, "could not persist first-run source selection");
    }
    if let Err(error) = settings::set_online_discovery_banner_completed(db, true) {
        tracing::warn!(%error, "could not close the discovery banner");
    }
}

/// `~/Music` reads as a place; `/home/someone/Music` reads as a machine.
/// Only an exact prefix match is folded — a sibling like `/home/someone2`
/// must not become `~2`.
fn tilde_path(path: &Path, home: &Path) -> String {
    let Ok(relative) = path.strip_prefix(home) else {
        return path.display().to_string();
    };
    if relative.as_os_str().is_empty() {
        return "~".to_owned();
    }
    PathBuf::from("~").join(relative).display().to_string()
}

/// Which folder path the wizard takes on the way out.
///
/// `Skip` keeps a folder the user typed into the dialog: skipping means
/// skipping what was never asked for — sources and import — not the one
/// thing the user filled in themselves. A folder that shows in the row and
/// vanishes on click reads as a bug.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FolderOutcome {
    /// Nothing chosen, and the user asked to set up: open the picker.
    OpenPicker,
    /// A folder is remembered: scan it, on either exit.
    ScanChosen,
    /// Skipped without choosing anything.
    Nothing,
}

fn folder_outcome(response: CompletionResponse, folder_chosen: bool) -> FolderOutcome {
    match (response, folder_chosen) {
        (_, true) => FolderOutcome::ScanChosen,
        (CompletionResponse::SetUp, false) => FolderOutcome::OpenPicker,
        (CompletionResponse::Skip, false) => FolderOutcome::Nothing,
    }
}

fn rhythmbox_offer(decision: FirstRunDecision, available: bool) -> Option<bool> {
    (decision == FirstRunDecision::ShowWizard && available).then_some(false)
}

fn take_completed_library_import(presented: &Cell<bool>, library_root: Option<&str>) -> bool {
    if presented.get() || library_root.is_none_or(|root| root.trim().is_empty()) {
        return false;
    }
    presented.set(true);
    true
}

fn build_rhythmbox_import_group(active: bool) -> RhythmboxImportWidgets {
    let group = adw::PreferencesGroup::builder()
        .title(strings::text(strings::ONBOARDING_GROUP_IMPORT))
        .build();
    let import_data = adw::SwitchRow::builder()
        .title(strings::text(strings::ONBOARDING_IMPORT_FROM_RHYTHMBOX))
        .subtitle(strings::text(
            strings::ONBOARDING_IMPORT_FROM_RHYTHMBOX_DESCRIPTION,
        ))
        .active(active)
        .use_markup(false)
        .build();
    group.add(&import_data);
    RhythmboxImportWidgets { group, import_data }
}

fn build_library_folder_group(
    library_root: Option<&str>,
    music_dir: Option<&Path>,
    home: &Path,
) -> Option<LibraryFolderWidgets> {
    if library_root.is_some_and(|root| !root.trim().is_empty()) {
        return None;
    }

    let group = adw::PreferencesGroup::builder()
        .title(strings::text(strings::ONBOARDING_GROUP_LIBRARY_FOLDER))
        .build();
    let row = adw::ActionRow::builder()
        .title(strings::text(strings::NO_LIBRARY_FOLDER))
        .build();
    if let Some(music_dir) = music_dir {
        let display = tilde_path(music_dir, home);
        row.set_subtitle(&strings::onboarding_no_library_yet_in(&display));
    }
    let choose = gtk4::Button::with_label(&strings::text(strings::CHOOSE_FOLDER));
    choose.set_valign(gtk4::Align::Center);
    row.add_suffix(&choose);
    group.add(&row);

    Some(LibraryFolderWidgets { group, row, choose })
}

fn show_chosen_folder(row: &adw::ActionRow, choose: &gtk4::Button, folder: &Path) {
    row.set_title(&strings::text(strings::LIBRARY_FOLDER));
    row.set_subtitle(&folder.display().to_string());
    choose.set_label(&strings::text(strings::ONBOARDING_CHANGE_FOLDER));
}

fn build_wizard_content(
    library_root: Option<&str>,
    music_dir: Option<&Path>,
    home: &Path,
    rhythmbox: Option<bool>,
    selection: WizardSourceSelection,
) -> WizardContentWidgets {
    let privacy = gtk4::Label::builder()
        .label(strings::text(strings::ONBOARDING_PRIVACY))
        .wrap(true)
        .xalign(0.0)
        .build();
    let library = build_library_folder_group(library_root, music_dir, home);
    let rhythmbox = rhythmbox.map(build_rhythmbox_import_group);
    let sources = first_run_sources::build_source_group(selection);
    let skip = gtk4::Button::with_label(&strings::text(strings::ONBOARDING_SKIP));
    let setup = gtk4::Button::with_label(&strings::text(strings::ONBOARDING_SET_UP));
    setup.add_css_class("suggested-action");
    let buttons = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    buttons.set_halign(gtk4::Align::End);
    buttons.append(&skip);
    buttons.append(&setup);

    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 18);
    root.set_margin_top(18);
    root.set_margin_bottom(18);
    root.set_margin_start(18);
    root.set_margin_end(18);
    root.append(&privacy);
    if let Some(library) = &library {
        root.append(&library.group);
    }
    if let Some(rhythmbox) = &rhythmbox {
        root.append(&rhythmbox.group);
    }
    root.append(&sources.group);
    root.append(&sources.footer);
    root.append(&buttons);

    WizardContentWidgets {
        root,
        library,
        rhythmbox,
        sources,
        skip,
        setup,
    }
}

fn arm_rhythmbox_import_after_library_setup(
    scan_controls: &ScanControls,
    conn: &Rc<Db>,
    present_import: &Rc<dyn Fn()>,
) {
    let presented = Rc::new(Cell::new(false));
    let conn = conn.clone();
    let present_import = present_import.clone();
    scan_controls.add_on_complete(move || {
        let library_root = match settings::get_library_root(&conn) {
            Ok(root) => root,
            Err(error) => {
                tracing::warn!(%error, "could not read library root before Rhythmbox import");
                return;
            }
        };
        if !take_completed_library_import(&presented, library_root.as_deref()) {
            return;
        }
        present_import();
    });
}

pub(super) fn decide(completed: bool, library_root: Option<&str>) -> FirstRunDecision {
    if completed {
        return FirstRunDecision::AlreadyCompleted;
    }
    if library_root.is_some_and(|root| !root.trim().is_empty()) {
        return FirstRunDecision::ExistingLibrary;
    }
    FirstRunDecision::ShowWizard
}

pub(super) fn initial_decision(db: &Db) -> FirstRunDecision {
    let completed = match settings::get_onboarding_completed(db) {
        Ok(completed) => completed,
        Err(error) => {
            tracing::warn!(%error, "could not read onboarding state; showing setup");
            return FirstRunDecision::ShowWizard;
        }
    };
    let library_root = match settings::get_library_root(db) {
        Ok(root) => root,
        Err(error) => {
            tracing::warn!(%error, "could not read library root for onboarding; showing setup");
            return FirstRunDecision::ShowWizard;
        }
    };
    let decision = decide(completed, library_root.as_deref());
    if decision == FirstRunDecision::ExistingLibrary {
        if let Err(error) = settings::set_onboarding_completed(db, true) {
            tracing::warn!(%error, "could not mark existing-library onboarding complete");
        }
    }
    decision
}

pub(super) fn run(
    window: &adw::ApplicationWindow,
    scan_button: &gtk4::Button,
    scan_controls: &ScanControls,
    conn: &Rc<Db>,
    decision: FirstRunDecision,
    start_scan_of: &Rc<dyn Fn(PathBuf)>,
    present_rhythmbox_import: &Rc<dyn Fn()>,
) {
    tracing::info!(?decision, "first-run decision");
    if decision != FirstRunDecision::ShowWizard {
        return;
    }

    let rhythmbox_found = preference_rhythmbox::rhythmbox_import_available();
    let rhythmbox_offer = rhythmbox_offer(decision, rhythmbox_found);
    tracing::info!(
        rhythmbox_found,
        rhythmbox_import_default = false,
        "first-run Rhythmbox discovery complete"
    );
    let selection = WizardSourceSelection::current_or_first_enable_defaults(conn).unwrap_or_else(
        |error| {
            tracing::warn!(%error, "could not read online source state; showing first-enable defaults");
            WizardSourceSelection::from_first_enable_defaults()
        },
    );
    let library_root = settings::get_library_root(conn).unwrap_or_else(|error| {
        tracing::warn!(%error, "could not read library root for first-run folder group");
        None
    });
    let music_dir = glib::user_special_dir(glib::UserDirectory::Music);
    let home = glib::home_dir();
    let WizardContentWidgets {
        root,
        library,
        rhythmbox,
        sources,
        skip,
        setup,
    } = build_wizard_content(
        library_root.as_deref(),
        music_dir.as_deref(),
        &home,
        rhythmbox_offer,
        selection,
    );
    let sources = Rc::new(sources);
    let remembered_folder = Rc::new(RefCell::<Option<PathBuf>>::new(None));

    if let Some(library) = library {
        let window = window.clone();
        let row = library.row.clone();
        let choose = library.choose.clone();
        let remembered_folder = remembered_folder.clone();
        library.choose.connect_clicked(move |button| {
            button.set_sensitive(false);
            let dialog = gtk4::FileDialog::builder()
                .title(strings::text(strings::SCAN_DIALOG_TITLE))
                .modal(true)
                .build();
            let window = window.clone();
            let row = row.clone();
            let choose = choose.clone();
            let remembered_folder = remembered_folder.clone();
            glib::spawn_future_local(async move {
                let folder = match dialog.select_folder_future(Some(&window)).await {
                    Ok(folder) => folder,
                    Err(error) => {
                        if error.matches(gtk4::DialogError::Dismissed)
                            || error.matches(gtk4::DialogError::Cancelled)
                        {
                            tracing::debug!("first-run folder dialog dismissed");
                        } else {
                            tracing::error!(%error, "first-run folder dialog failed");
                        }
                        choose.set_sensitive(true);
                        return;
                    }
                };
                let Some(path) = folder.path() else {
                    tracing::warn!("selected folder has no local filesystem path; cannot scan");
                    choose.set_sensitive(true);
                    return;
                };
                *remembered_folder.borrow_mut() = Some(path.clone());
                show_chosen_folder(&row, &choose, &path);
                choose.set_sensitive(true);
            });
        });
    }

    let header = adw::HeaderBar::new();
    header.set_show_end_title_buttons(false);
    header.set_show_start_title_buttons(false);
    header.set_title_widget(Some(&adw::WindowTitle::new(
        &strings::text(strings::ONBOARDING_WELCOME),
        "",
    )));
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    let scrolled = gtk4::ScrolledWindow::builder()
        .child(&root)
        .propagate_natural_height(true)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .build();
    toolbar.set_content(Some(&scrolled));
    let dialog = adw::Dialog::builder()
        .child(&toolbar)
        .content_width(560)
        .content_height(620)
        .build();
    let focus_guard = crate::ui::transient_focus::TransientFocusGuard::capture(window);
    focus_guard.bind_closable_dialog(&dialog, &setup);

    let complete: Rc<dyn Fn(CompletionOptions, CompletionResponse, bool)> = {
        let window = window.downgrade();
        let scan_button = scan_button.downgrade();
        let scan_controls = scan_controls.clone();
        let dialog = dialog.downgrade();
        let conn = conn.clone();
        let remembered_folder = remembered_folder.clone();
        let start_scan_of = start_scan_of.clone();
        let present_rhythmbox_import = present_rhythmbox_import.clone();
        Rc::new(move |options, response, suppress_picker| {
            if window.upgrade().is_none() {
                return;
            }
            let rhythmbox_import = requested_actions(options);
            let chosen_folder = remembered_folder.borrow().clone();
            let outcome = folder_outcome(response, chosen_folder.is_some());
            persist_completion(&conn, options);
            if let Some(dialog) = dialog.upgrade() {
                dialog.close();
            }
            if rhythmbox_import {
                if suppress_picker {
                    present_rhythmbox_import();
                } else {
                    arm_rhythmbox_import_after_library_setup(
                        &scan_controls,
                        &conn,
                        &present_rhythmbox_import,
                    );
                }
            }
            if !suppress_picker {
                match outcome {
                    FolderOutcome::OpenPicker => {
                        if let Some(scan_button) = scan_button.upgrade() {
                            scan_button.emit_clicked();
                        }
                    }
                    FolderOutcome::ScanChosen => {
                        if let Some(folder) = chosen_folder {
                            start_scan_of(folder);
                        }
                    }
                    FolderOutcome::Nothing => {}
                }
            }
            tracing::info!(
                ?response,
                ?outcome,
                rhythmbox_import,
                "first-run setup completed"
            );
            log_smoke_result(&conn);
        })
    };

    {
        let complete = complete.clone();
        skip.connect_clicked(move |_| {
            complete(
                CompletionOptions::default(),
                CompletionResponse::Skip,
                false,
            );
        });
    }
    {
        let complete = complete.clone();
        let import_data = rhythmbox.map(|widgets| widgets.import_data);
        let sources = sources.clone();
        setup.connect_clicked(move |_| {
            complete(
                CompletionOptions {
                    rhythmbox_import: import_data.as_ref().is_some_and(adw::SwitchRow::is_active),
                    sources: sources.selection(),
                },
                CompletionResponse::SetUp,
                false,
            );
        });
    }

    let window = window.clone();
    let sources = sources.clone();
    glib::idle_add_local_once(move || {
        dialog.present(Some(&window));
        tracing::info!(presentations = 1, "first-run wizard presented");
        let Ok(smoke) = std::env::var(SMOKE_ENV) else {
            return;
        };
        let (options, response) = match smoke.as_str() {
            "skip" => (CompletionOptions::default(), CompletionResponse::Skip),
            "setup-options" => (
                CompletionOptions {
                    rhythmbox_import: true,
                    sources: sources.selection(),
                },
                CompletionResponse::SetUp,
            ),
            _ => {
                tracing::warn!(smoke, "invalid first-run smoke response");
                return;
            }
        };
        complete(options, response, true);
    });
}

fn log_smoke_result(db: &Db) {
    if std::env::var(SMOKE_ENV).is_err() {
        return;
    }
    let completed = settings::get_onboarding_completed(db).unwrap_or(false);
    tracing::info!(
        completed,
        cover_download = true,
        "first-run smoke completed"
    );
}

#[cfg(test)]
#[path = "first_run_tests.rs"]
mod tests;
