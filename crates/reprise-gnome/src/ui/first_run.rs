//! Native first-run wizard reusing the normal window actions and scan button.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gio::prelude::*;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library::settings;
use rusqlite::Connection;

use crate::ui::{column_layout, primary_menu, strings};

pub(super) const SMOKE_ENV: &str = "REPRISE_SMOKE_FIRST_RUN";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FirstRunDecision {
    ShowWizard,
    ExistingLibrary,
    AlreadyCompleted,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct CompletionOptions {
    cover_download: bool,
    rhythmbox_columns: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompletionResponse {
    Skip,
    SetUp,
}

fn requested_actions(options: CompletionOptions) -> (bool, bool) {
    (options.cover_download, options.rhythmbox_columns)
}

fn should_open_folder(response: CompletionResponse) -> bool {
    response == CompletionResponse::SetUp
}

fn rhythmbox_offer(available: bool) -> Option<bool> {
    column_layout::should_offer_rhythmbox_import(available).then_some(false)
}

fn rhythmbox_layout_available() -> bool {
    std::env::var(primary_menu::SMOKE_RHYTHMBOX_COLUMNS_ENV_VAR).is_ok()
        || column_layout::read_rhythmbox_visible_columns().is_ok()
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

pub(super) fn initial_decision(conn: &Connection) -> FirstRunDecision {
    let completed = match settings::get_onboarding_completed(conn) {
        Ok(completed) => completed,
        Err(error) => {
            tracing::warn!(%error, "could not read onboarding state; showing setup");
            return FirstRunDecision::ShowWizard;
        }
    };
    let library_root = match settings::get_library_root(conn) {
        Ok(root) => root,
        Err(error) => {
            tracing::warn!(%error, "could not read library root for onboarding; showing setup");
            return FirstRunDecision::ShowWizard;
        }
    };
    let decision = decide(completed, library_root.as_deref());
    if decision == FirstRunDecision::ExistingLibrary {
        if let Err(error) = settings::set_onboarding_completed(conn, true) {
            tracing::warn!(%error, "could not mark existing-library onboarding complete");
        }
    }
    decision
}

pub(super) fn run(
    window: &adw::ApplicationWindow,
    scan_button: &gtk4::Button,
    conn: &Rc<RefCell<Connection>>,
) {
    let decision = initial_decision(&conn.borrow());
    tracing::info!(?decision, "first-run decision");
    if decision != FirstRunDecision::ShowWizard {
        return;
    }

    let cover = adw::SwitchRow::builder()
        .title(strings::text(strings::ONBOARDING_COVERS))
        .subtitle(strings::text(strings::ONBOARDING_COVERS_SUBTITLE))
        .build();
    let rhythmbox_found = rhythmbox_layout_available();
    let rhythmbox = rhythmbox_offer(rhythmbox_found).map(|active| {
        adw::SwitchRow::builder()
            .title(strings::text(strings::ONBOARDING_RHYTHMBOX_FOUND))
            .subtitle(strings::text(strings::ONBOARDING_RHYTHMBOX_FOUND_SUBTITLE))
            .active(active)
            .build()
    });
    tracing::info!(
        rhythmbox_found,
        rhythmbox_import_default = false,
        "first-run Rhythmbox discovery complete"
    );
    let group = adw::PreferencesGroup::new();
    group.add(&cover);
    if let Some(rhythmbox) = &rhythmbox {
        group.add(rhythmbox);
    }

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
    content.append(&group);
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

    let complete: Rc<dyn Fn(CompletionOptions, CompletionResponse, bool)> = {
        let window = window.downgrade();
        let scan_button = scan_button.downgrade();
        let dialog = dialog.downgrade();
        let conn = conn.clone();
        Rc::new(move |options, response, suppress_picker| {
            let Some(window) = window.upgrade() else {
                return;
            };
            let (cover_download, rhythmbox_columns) = requested_actions(options);
            if cover_download {
                if let Some(action) =
                    window.lookup_action(primary_menu::ACTION_DOWNLOAD_MISSING_COVERS)
                {
                    action.change_state(&true.to_variant());
                }
            }
            if rhythmbox_columns {
                if let Some(action) =
                    window.lookup_action(primary_menu::ACTION_IMPORT_RHYTHMBOX_COLUMNS)
                {
                    action.activate(None);
                }
            }
            if let Err(error) = settings::set_onboarding_completed(&conn.borrow(), true) {
                tracing::warn!(%error, "could not persist onboarding completion");
            }
            if let Some(dialog) = dialog.upgrade() {
                dialog.close();
            }
            if should_open_folder(response) && !suppress_picker {
                if let Some(scan_button) = scan_button.upgrade() {
                    scan_button.emit_clicked();
                }
            }
            log_smoke_result(&conn.borrow());
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
                    cover_download: cover.is_active(),
                    rhythmbox_columns: rhythmbox.as_ref().is_some_and(adw::SwitchRow::is_active),
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
                    cover_download: true,
                    rhythmbox_columns: true,
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

fn log_smoke_result(conn: &Connection) {
    if std::env::var(SMOKE_ENV).is_err() {
        return;
    }
    let completed = settings::get_onboarding_completed(conn).unwrap_or(false);
    let cover_download =
        reprise_core::modules::is_enabled(conn, &reprise_core::modules::COVER_DOWNLOAD_MODULE)
            .unwrap_or(false);
    let column_layout = settings::get_setting(conn, settings::COLUMN_LAYOUT_KEY)
        .ok()
        .flatten();
    tracing::info!(
        completed,
        cover_download,
        ?column_layout,
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
        assert_eq!(
            requested_actions(CompletionOptions {
                cover_download: true,
                rhythmbox_columns: false,
            }),
            (true, false)
        );
    }

    #[test]
    fn only_set_up_opens_the_folder_picker() {
        assert!(!should_open_folder(CompletionResponse::Skip));
        assert!(should_open_folder(CompletionResponse::SetUp));
    }

    #[test]
    fn rhythmbox_offer_is_visible_only_when_detected_and_defaults_off() {
        assert_eq!(rhythmbox_offer(false), None);
        assert_eq!(rhythmbox_offer(true), Some(false));
    }
}
