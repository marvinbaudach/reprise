use super::*;
use reprise_core::modules;
use reprise_core::online_sources::{self, WizardSourceSelection};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, OnceLock};

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

    let visible_sources = WizardSourceSelection {
        radio: false,
        podcasts: true,
        youtube: false,
    };
    for response in [CompletionResponse::Skip, CompletionResponse::SetUp] {
        assert_eq!(
            completion_options(response, true, visible_sources).sources,
            visible_sources
        );
    }
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
    for response in [CompletionResponse::Skip, CompletionResponse::SetUp] {
        let db = Db::open_in_memory().unwrap();
        let options = completion_options(response, true, WizardSourceSelection::default());
        persist_completion(&db, options);
        assert!(settings::get_onboarding_completed(&db).unwrap());
        assert!(settings::get_online_discovery_banner_completed(&db).unwrap());
        assert_eq!(options.sources, WizardSourceSelection::default());
    }
}

fn open_gate_with_podcasts(db: &Db) {
    online_sources::set_enabled(db, true).unwrap();
    modules::set_enabled(db, &modules::PODCASTS_MODULE, true).unwrap();
}

fn assert_gate_and_wizard_sources_are_off(db: &Db) {
    assert!(!online_sources::is_enabled(db).unwrap());
    for module in [
        &modules::RADIO_MODULE,
        &modules::PODCASTS_MODULE,
        &modules::YOUTUBE_MODULE,
    ] {
        assert!(!modules::is_enabled(db, module).unwrap(), "{}", module.id);
    }
}

#[test]
fn skipping_with_nothing_chosen_revokes_an_open_gate() {
    let db = Db::open_in_memory().unwrap();
    open_gate_with_podcasts(&db);

    let options = completion_options(
        CompletionResponse::Skip,
        false,
        WizardSourceSelection::default(),
    );
    persist_completion(&db, options);

    assert_gate_and_wizard_sources_are_off(&db);
}

#[test]
fn setting_up_with_nothing_chosen_revokes_an_open_gate() {
    let db = Db::open_in_memory().unwrap();
    open_gate_with_podcasts(&db);

    let options = completion_options(
        CompletionResponse::SetUp,
        false,
        WizardSourceSelection::default(),
    );
    persist_completion(&db, options);

    assert_gate_and_wizard_sources_are_off(&db);
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
    let start_scan_of: Rc<dyn Fn(PathBuf)> = {
        let scanned = scanned.clone();
        Rc::new(move |folder| scanned.borrow_mut().push(folder))
    };

    persist_completion(&db, CompletionOptions::default());
    let open_picker = || panic!("Skip with a chosen folder must not open the picker");
    dispatch_folder_outcome(
        CompletionResponse::Skip,
        Some(PathBuf::from("/music")),
        &open_picker,
        start_scan_of.as_ref(),
    );

    assert_eq!(scanned.borrow().as_slice(), [PathBuf::from("/music")]);
    assert!(!reprise_core::online_sources::is_enabled(&db).unwrap());
}

fn direct_preferences_groups(root: &gtk4::Box) -> Vec<adw::PreferencesGroup> {
    let mut groups = Vec::new();
    let mut child = root.first_child();
    while let Some(current) = child {
        if let Ok(group) = current.clone().downcast::<adw::PreferencesGroup>() {
            groups.push(group);
        }
        child = current.next_sibling();
    }
    groups
}

fn descendant_switch_rows(widget: &gtk4::Widget) -> Vec<adw::SwitchRow> {
    let mut rows = widget
        .clone()
        .downcast::<adw::SwitchRow>()
        .ok()
        .into_iter()
        .collect::<Vec<_>>();
    let mut child = widget.first_child();
    while let Some(current) = child {
        rows.extend(descendant_switch_rows(&current));
        child = current.next_sibling();
    }
    rows
}

#[derive(Clone, Copy)]
enum Net4aCase {
    DefaultWizard,
    StoredSources,
}

fn run_net_4a_case(case: Net4aCase) {
    type GtkCase = (Net4aCase, mpsc::Sender<std::thread::Result<()>>);
    static GTK_THREAD: OnceLock<mpsc::Sender<GtkCase>> = OnceLock::new();
    let sender = GTK_THREAD.get_or_init(|| {
        let (sender, receiver) = mpsc::channel::<GtkCase>();
        std::thread::spawn(move || {
            let _main_context = crate::ui::test_main_context::lock_main_context();
            let gtk_ready = gtk4::init().is_ok();
            while let Ok((case, reply)) = receiver.recv() {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    if !gtk_ready {
                        return;
                    }
                    match case {
                        Net4aCase::DefaultWizard => assert_default_wizard_tree(),
                        Net4aCase::StoredSources => assert_stored_source_tree(),
                    }
                }));
                let _ = reply.send(result);
            }
        });
        sender
    });
    let (reply, result) = mpsc::channel();
    sender
        .send((case, reply))
        .expect("GTK test thread is alive");
    match result.recv().expect("GTK test thread returned a result") {
        Ok(()) => {}
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn net_4a_the_wizard_asks_folder_import_and_sources_in_one_dialog() {
    run_net_4a_case(Net4aCase::DefaultWizard);
}

fn assert_default_wizard_tree() {
    let widgets = build_wizard_content(
        None,
        Some(Path::new("/home/test/Music")),
        Path::new("/home/test"),
        Some(false),
        WizardSourceSelection::default(),
    );
    let groups = direct_preferences_groups(&widgets.root);
    assert_eq!(
        groups
            .iter()
            .map(adw::PreferencesGroup::title)
            .collect::<Vec<_>>(),
        [
            strings::text(strings::ONBOARDING_GROUP_LIBRARY_FOLDER),
            strings::text(strings::ONBOARDING_GROUP_IMPORT),
            strings::text(strings::PREFERENCES_ONLINE_SOURCES),
        ]
    );

    let source_rows = descendant_switch_rows(widgets.sources.group.upcast_ref());
    assert_eq!(source_rows.len(), 3);
    for (row, title, subtitle, active) in [
        (
            &source_rows[0],
            strings::ONLINE_SOURCES_USE_RADIO,
            strings::ONLINE_SOURCES_RADIO_SUBTITLE,
            false,
        ),
        (
            &source_rows[1],
            strings::ONLINE_SOURCES_USE_PODCASTS,
            strings::ONLINE_SOURCES_PODCASTS_SUBTITLE,
            false,
        ),
        (
            &source_rows[2],
            strings::ONLINE_SOURCES_USE_YOUTUBE,
            strings::ONLINE_SOURCES_YOUTUBE_SUBTITLE,
            false,
        ),
    ] {
        assert_eq!(row.title(), strings::text(title));
        assert_eq!(
            row.subtitle().as_deref(),
            Some(strings::text(subtitle).as_str())
        );
        assert_eq!(row.is_active(), active);
        assert!(!row.uses_markup());
    }

    let rhythmbox = widgets.rhythmbox.expect("Rhythmbox group is present");
    assert!(!rhythmbox.import_data.is_active());
    let library = widgets.library.expect("Library folder group is present");
    assert_eq!(
        library.row.title(),
        strings::text(strings::NO_LIBRARY_FOLDER)
    );
    assert_eq!(
        library.choose.label().as_deref(),
        Some(strings::text(strings::CHOOSE_FOLDER).as_str())
    );
    assert_eq!(
        widgets.sources.footer.label(),
        strings::text(strings::ONBOARDING_ONLINE_SOURCES_FOOTER)
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn net_4a_an_open_gate_makes_the_wizard_show_the_stored_sources() {
    run_net_4a_case(Net4aCase::StoredSources);
}

fn assert_stored_source_tree() {
    let db = Db::open_in_memory().unwrap();
    online_sources::set_enabled(&db, true).unwrap();
    modules::set_enabled(&db, &modules::RADIO_MODULE, false).unwrap();
    modules::set_enabled(&db, &modules::PODCASTS_MODULE, true).unwrap();
    let selection = WizardSourceSelection::current_or_first_enable_defaults(&db).unwrap();
    let sources = crate::ui::first_run_sources::build_source_group(selection);
    let rows = descendant_switch_rows(sources.group.upcast_ref());

    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows[0].title(),
        strings::text(strings::ONLINE_SOURCES_USE_RADIO)
    );
    assert!(!rows[0].is_active());
    assert_eq!(
        rows[1].title(),
        strings::text(strings::ONLINE_SOURCES_USE_PODCASTS)
    );
    assert!(rows[1].is_active());
    assert_eq!(
        rows[2].title(),
        strings::text(strings::ONLINE_SOURCES_USE_YOUTUBE)
    );
    assert!(!rows[2].is_active());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn library_folder_block_is_absent_when_a_root_is_already_set() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    if gtk4::init().is_err() {
        return;
    }

    let widgets = build_wizard_content(
        Some("/music"),
        None,
        Path::new("/home/test"),
        Some(false),
        WizardSourceSelection::default(),
    );
    assert!(widgets.library.is_none());
    assert!(direct_preferences_groups(&widgets.root)
        .iter()
        .all(|group| group.title() != strings::text(strings::ONBOARDING_GROUP_LIBRARY_FOLDER)));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn rhythmbox_block_is_absent_when_no_import_is_found() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    if gtk4::init().is_err() {
        return;
    }

    let offer = rhythmbox_offer(FirstRunDecision::ShowWizard, false);
    assert_eq!(offer, None);
    let widgets = build_wizard_content(
        None,
        None,
        Path::new("/home/test"),
        offer,
        WizardSourceSelection::default(),
    );
    assert!(widgets.rhythmbox.is_none());
    assert!(direct_preferences_groups(&widgets.root)
        .iter()
        .all(|group| group.title() != strings::text(strings::ONBOARDING_GROUP_IMPORT)));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn existing_library_keeps_the_online_discovery_banner() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    if gtk4::init().is_err() {
        return;
    }

    let db = Rc::new(Db::open_in_memory().unwrap());
    settings::set_library_root(&db, "/music").unwrap();
    assert_eq!(initial_decision(&db), FirstRunDecision::ExistingLibrary);
    assert!(settings::get_onboarding_completed(&db).unwrap());
    assert!(!settings::get_online_discovery_banner_completed(&db).unwrap());
    assert!(crate::ui::online_discovery_banner::build(&db, || {}).is_some());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn chosen_folder_row_matches_the_preferences_folder_pattern() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    if gtk4::init().is_err() {
        return;
    }

    let widgets = build_library_folder_group(None, None, Path::new("/home/test"))
        .expect("fresh install gets a folder group");
    show_chosen_folder(&widgets.row, &widgets.choose, Path::new("/srv/Music"));
    assert_eq!(widgets.row.title(), strings::text(strings::LIBRARY_FOLDER));
    assert_eq!(widgets.row.subtitle().as_deref(), Some("/srv/Music"));
    assert_eq!(
        widgets.choose.label().as_deref(),
        Some(strings::text(strings::ONBOARDING_CHANGE_FOLDER).as_str())
    );
}
