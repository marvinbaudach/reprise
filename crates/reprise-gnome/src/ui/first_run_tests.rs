use super::*;
use reprise_core::online_sources::WizardSourceSelection;
use std::cell::RefCell;
use std::path::{Path, PathBuf};

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

#[test]
fn tilde_path_folds_only_an_exact_home_component_prefix() {
    let home = Path::new("/home/someone");

    assert_eq!(
        tilde_path(Path::new("/home/someone/Music"), home),
        "~/Music"
    );
    assert_eq!(
        tilde_path(Path::new("/home/someone2/Music"), home),
        "/home/someone2/Music"
    );
    assert_eq!(tilde_path(home, home), "~");
    assert_eq!(tilde_path(Path::new("/srv/music"), home), "/srv/music");
}

#[test]
fn a_chosen_folder_is_scanned_on_both_exits() {
    assert_eq!(
        folder_outcome(CompletionResponse::SetUp, true),
        FolderOutcome::ScanChosen
    );
    assert_eq!(
        folder_outcome(CompletionResponse::Skip, true),
        FolderOutcome::ScanChosen
    );
    assert_eq!(
        folder_outcome(CompletionResponse::SetUp, false),
        FolderOutcome::OpenPicker
    );
    assert_eq!(
        folder_outcome(CompletionResponse::Skip, false),
        FolderOutcome::Nothing
    );
}

#[test]
fn skipping_with_a_chosen_folder_scans_it_and_keeps_the_gate_shut() {
    let db = Db::open_in_memory().unwrap();
    let scanned: Rc<RefCell<Vec<PathBuf>>> = Rc::default();
    // Same shape the dialog uses; the callback is the unit under test.
    let start_scan_of: Rc<dyn Fn(PathBuf)> = {
        let scanned = scanned.clone();
        Rc::new(move |folder| scanned.borrow_mut().push(folder))
    };

    persist_completion(&db, CompletionOptions::default());
    if folder_outcome(CompletionResponse::Skip, true) == FolderOutcome::ScanChosen {
        start_scan_of(PathBuf::from("/music"));
    }

    assert_eq!(scanned.borrow().as_slice(), [PathBuf::from("/music")]);
    assert!(!reprise_core::online_sources::is_enabled(&db).unwrap());
}
