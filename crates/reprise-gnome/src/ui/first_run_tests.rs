use super::*;
use reprise_core::online_sources::WizardSourceSelection;

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
        sources: WizardSourceSelection::default(),
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

#[test]
fn both_exits_close_onboarding_and_the_discovery_banner() {
    for options in [
        CompletionOptions::default(),
        CompletionOptions {
            rhythmbox_import: true,
            sources: WizardSourceSelection::from_first_enable_defaults(),
        },
    ] {
        let db = Db::open_in_memory().unwrap();
        persist_completion(&db, options);
        assert!(settings::get_onboarding_completed(&db).unwrap());
        assert!(settings::get_online_discovery_banner_completed(&db).unwrap());
    }
}

#[test]
fn skipping_the_wizard_leaves_the_network_gate_shut() {
    let db = Db::open_in_memory().unwrap();
    persist_completion(&db, CompletionOptions::default());
    assert!(!reprise_core::online_sources::is_enabled(&db).unwrap());
}

#[test]
fn a_completed_wizard_leaves_no_banner_to_show() {
    // `build` returns before it touches a widget when the banner is done, so
    // this needs no display.
    let db = Rc::new(Db::open_in_memory().unwrap());
    persist_completion(&db, CompletionOptions::default());
    assert!(crate::ui::online_discovery_banner::build(&db, || {}).is_none());
}
