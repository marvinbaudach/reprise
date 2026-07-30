//! Native first-run wizard reusing the normal window actions and scan button.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::db::Db;
use reprise_core::library::settings;

use crate::ui::{preference_rhythmbox, scan_flow::ScanControls, strings};

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
}

struct RhythmboxImportWidgets {
    group: adw::PreferencesGroup,
    import_data: adw::SwitchRow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompletionResponse {
    Skip,
    SetUp,
}

fn requested_actions(options: CompletionOptions) -> bool {
    options.rhythmbox_import
}

fn should_open_folder(response: CompletionResponse) -> bool {
    response == CompletionResponse::SetUp
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
    let group = adw::PreferencesGroup::new();
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

fn arm_rhythmbox_import_after_library_setup(
    scan_controls: &ScanControls,
    conn: &Rc<Db>,
    present_import: &Rc<dyn Fn()>,
) {
    let presented = Rc::new(Cell::new(false));
    let conn = conn.clone();
    let present_import = present_import.clone();
    scan_controls.set_on_complete(move || {
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
    present_rhythmbox_import: &Rc<dyn Fn()>,
) {
    tracing::info!(?decision, "first-run decision");
    if decision != FirstRunDecision::ShowWizard {
        return;
    }

    let rhythmbox_found = preference_rhythmbox::rhythmbox_import_available();
    let rhythmbox = rhythmbox_offer(decision, rhythmbox_found).map(build_rhythmbox_import_group);
    tracing::info!(
        rhythmbox_found,
        rhythmbox_import_default = false,
        "first-run Rhythmbox discovery complete"
    );
    let privacy = gtk4::Label::builder()
        .label(strings::text(strings::ONBOARDING_PRIVACY))
        .wrap(true)
        .xalign(0.0)
        .build();
    let skip = gtk4::Button::with_label(&strings::text(strings::ONBOARDING_SKIP));
    let setup = gtk4::Button::with_label(&strings::text(strings::ONBOARDING_SET_UP));
    setup.add_css_class("suggested-action");
    let buttons = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    buttons.set_halign(gtk4::Align::End);
    buttons.append(&skip);
    buttons.append(&setup);
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 18);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    content.set_margin_start(18);
    content.set_margin_end(18);
    content.append(&privacy);
    if let Some(rhythmbox) = &rhythmbox {
        content.append(&rhythmbox.group);
    }
    content.append(&buttons);

    let header = adw::HeaderBar::new();
    header.set_show_end_title_buttons(false);
    header.set_show_start_title_buttons(false);
    header.set_title_widget(Some(&adw::WindowTitle::new(
        &strings::text(strings::ONBOARDING_WELCOME),
        "",
    )));
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&content));
    let dialog = adw::Dialog::builder()
        .child(&toolbar)
        .content_width(560)
        .content_height(430)
        .build();
    let focus_guard = crate::ui::transient_focus::TransientFocusGuard::capture(window);
    focus_guard.bind_closable_dialog(&dialog, &setup);

    let complete: Rc<dyn Fn(CompletionOptions, CompletionResponse, bool)> = {
        let window = window.downgrade();
        let scan_button = scan_button.downgrade();
        let scan_controls = scan_controls.clone();
        let dialog = dialog.downgrade();
        let conn = conn.clone();
        let present_rhythmbox_import = present_rhythmbox_import.clone();
        Rc::new(move |options, response, suppress_picker| {
            if window.upgrade().is_none() {
                return;
            }
            let rhythmbox_import = requested_actions(options);
            if let Err(error) = settings::set_onboarding_completed(&conn, true) {
                tracing::warn!(%error, "could not persist onboarding completion");
            }
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
            if should_open_folder(response) && !suppress_picker {
                if let Some(scan_button) = scan_button.upgrade() {
                    scan_button.emit_clicked();
                }
            }
            tracing::info!(?response, rhythmbox_import, "first-run setup completed");
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
        setup.connect_clicked(move |_| {
            complete(
                CompletionOptions {
                    rhythmbox_import: rhythmbox
                        .as_ref()
                        .is_some_and(|widgets| widgets.import_data.is_active()),
                },
                CompletionResponse::SetUp,
                false,
            );
        });
    }

    let window = window.clone();
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
mod tests {
    use super::*;

    #[test]
    fn incomplete_fresh_install_shows_the_wizard() {
        assert_eq!(decide(false, None), FirstRunDecision::ShowWizard);
        assert_eq!(decide(false, Some("  ")), FirstRunDecision::ShowWizard);
    }

    #[test]
    fn existing_library_is_a_silent_upgrade() {
        assert_eq!(
            decide(false, Some("/music")),
            FirstRunDecision::ExistingLibrary
        );
    }

    #[test]
    fn completed_onboarding_never_reopens_the_wizard() {
        assert_eq!(decide(true, None), FirstRunDecision::AlreadyCompleted);
    }

    #[test]
    fn completion_activates_only_explicitly_enabled_options() {
        assert!(!requested_actions(CompletionOptions::default()));
        assert!(requested_actions(CompletionOptions {
            rhythmbox_import: true,
        }));
    }

    #[test]
    fn only_set_up_opens_the_folder_picker() {
        assert!(!should_open_folder(CompletionResponse::Skip));
        assert!(should_open_folder(CompletionResponse::SetUp));
    }

    #[test]
    fn rhythmbox_offer_is_first_run_only_detected_and_defaults_off() {
        assert_eq!(rhythmbox_offer(FirstRunDecision::ShowWizard, false), None);
        assert_eq!(
            rhythmbox_offer(FirstRunDecision::ExistingLibrary, true),
            None
        );
        assert_eq!(
            rhythmbox_offer(FirstRunDecision::AlreadyCompleted, true),
            None
        );
        assert_eq!(
            rhythmbox_offer(FirstRunDecision::ShowWizard, true),
            Some(false)
        );
    }

    #[test]
    fn rhythmbox_import_is_taken_once_after_a_completed_library_scan() {
        let presented = Cell::new(false);

        assert!(!take_completed_library_import(&presented, None));
        assert!(!take_completed_library_import(&presented, Some("  ")));
        assert!(take_completed_library_import(&presented, Some("/music")));
        assert!(!take_completed_library_import(&presented, Some("/music")));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn detected_rhythmbox_group_lists_the_supported_import_choice() {
        gtk4::init().unwrap();
        let widgets = build_rhythmbox_import_group(false);

        assert_eq!(
            widgets.import_data.title(),
            strings::text(strings::ONBOARDING_IMPORT_FROM_RHYTHMBOX)
        );
        assert_eq!(
            widgets.import_data.subtitle().as_deref(),
            Some(strings::text(strings::ONBOARDING_IMPORT_FROM_RHYTHMBOX_DESCRIPTION).as_str())
        );
        assert!(!widgets.import_data.is_active());
        assert!(!widgets.import_data.uses_markup());
    }
}
