use gtk4::glib;
use reprise_core::podcasts::PodcastKind;

use super::*;

fn episode(id: i64, played: bool) -> EpisodeRow {
    EpisodeRow {
        id,
        subscription_id: 7,
        guid: format!("episode-{id}"),
        title: format!("Episode {id}"),
        show: "Show".into(),
        show_image_url: None,
        image_url: None,
        kind: PodcastKind::Rss,
        audio_url: format!("https://example.test/{id}.mp3"),
        page_url: None,
        published_at: None,
        duration_secs: None,
        downloaded_path: None,
        downloaded_bytes: None,
        played_at: played.then_some(10),
        position_ms: 0,
        first_seen_at: 1,
        is_new: false,
        media_category: None,
    }
}

fn collect_entries(model: &gio::MenuModel, entries: &mut Vec<(String, String)>) {
    for item in 0..model.n_items() {
        let label = model
            .item_attribute_value(item, "label", Some(glib::VariantTy::STRING))
            .and_then(|value| value.get::<String>());
        let action = model
            .item_attribute_value(item, "action", Some(glib::VariantTy::STRING))
            .and_then(|value| value.get::<String>());
        if let (Some(label), Some(action)) = (label, action) {
            entries.push((label, action));
        }
        if let Some(section) = model.item_link(item, "section") {
            collect_entries(&section, entries);
        }
    }
}

fn menu_entries(menu: &gio::Menu) -> Vec<(String, String)> {
    let mut entries = Vec::new();
    collect_entries(menu.upcast_ref(), &mut entries);
    entries
}

fn build_menu(
    row: &EpisodeRow,
    selected_ids: &[i64],
    unavailable_episode: Option<i64>,
) -> gio::Menu {
    let paths = EpisodePaths::from_rows(&[]);
    build_for_selection(row, selected_ids, unavailable_episode, &paths)
}

fn collect_targets(model: &gio::MenuModel, targets: &mut Vec<(String, Vec<i64>)>) {
    for item in 0..model.n_items() {
        let action = model
            .item_attribute_value(item, "action", Some(glib::VariantTy::STRING))
            .and_then(|value| value.get::<String>());
        // Single-episode actions carry a bare `i64`, batch actions an
        // array of them. Both answer the same question here: which
        // episodes would this entry act on?
        let target = model
            .item_attribute_value(item, "target", None)
            .and_then(|target| {
                target
                    .get::<i64>()
                    .map(|episode_id| vec![episode_id])
                    .or_else(|| target.get::<Vec<i64>>())
            });
        if let (Some(action), Some(target)) = (action, target) {
            targets.push((action, target));
        }
        if let Some(section) = model.item_link(item, "section") {
            collect_targets(&section, targets);
        }
    }
}

/// Every menu entry that acts on episodes, with the episodes it acts on.
/// Unsubscribe is excluded because its target is a `subscription_id`, which
/// is a different thing that happens to be an `i64`.
fn episode_targets(menu: &gio::Menu) -> Vec<(String, Vec<i64>)> {
    let mut targets = Vec::new();
    collect_targets(menu.upcast_ref(), &mut targets);
    targets.retain(|(action, _)| !action.contains(ACTION_UNSUBSCRIBE) && !action.contains("sync"));
    targets
}

#[test]
fn src_12b_a_menu_on_a_row_outside_the_selection_acts_on_that_row_alone() {
    let row = episode(3, false);

    let menu = build_menu(&row, &[1, 2], None);

    let actions = menu_entries(&menu)
        .into_iter()
        .map(|(_, action)| action)
        .collect::<Vec<_>>();
    assert!(
        actions.contains(&"podcasts.play".to_owned()),
        "a row outside the selection gets its own single-row menu: {actions:?}"
    );
    let targets = episode_targets(&menu);
    assert!(
        !targets.is_empty(),
        "the menu carries episode targets at all"
    );
    for (action, target) in targets {
        assert_eq!(
            target,
            vec![3],
            "`{action}` must never reach episodes the menu was not opened on"
        );
    }
}

#[test]
fn src_12b_a_menu_on_a_selected_row_acts_on_the_whole_selection() {
    let row = episode(2, false);

    let menu = build_menu(&row, &[1, 2, 3], None);

    let targets = episode_targets(&menu);
    assert!(
        !targets.is_empty(),
        "the menu carries episode targets at all"
    );
    for (action, target) in targets {
        assert_eq!(target, vec![1, 2, 3], "`{action}` acts on the selection");
    }
}

#[test]
fn src_4b_single_selection_keeps_existing_actions_and_adds_queue_routes() {
    let row = episode(1, false);

    let entries = menu_entries(&build_menu(&row, &[row.id], None));

    assert_eq!(
        entries,
        vec![
            (strings::text(strings::PODCAST_PLAY), "podcasts.play".into()),
            (
                strings::text(strings::PODCAST_COPY_URL),
                "podcasts.copy-url".into(),
            ),
            (
                strings::text(strings::CONTEXT_MENU_PLAY_NEXT),
                "podcasts.play-next".into(),
            ),
            (
                strings::text(strings::CONTEXT_MENU_ADD_TO_QUEUE),
                "podcasts.add-to-queue".into(),
            ),
            (
                strings::text(strings::PODCAST_MARK_PLAYED),
                "podcasts.toggle-played".into(),
            ),
            (
                strings::text(strings::PODCAST_DOWNLOAD),
                "podcasts.toggle-download".into(),
            ),
            (
                strings::text(strings::PODCAST_REMOVE_EPISODE),
                "podcasts.remove-episode".into(),
            ),
            (
                strings::podcast_unsubscribe_from("Show"),
                "podcasts.unsubscribe".into(),
            ),
        ]
    );
}

#[test]
fn src_12b_multi_selection_hides_single_targets_and_offers_explicit_played_states() {
    let mut entries = multi_selection_primary_entries();
    entries.push(multi_selection_destructive_entry());
    let actions = entries.iter().map(|entry| entry.action).collect::<Vec<_>>();

    assert!(!actions.contains(&ACTION_PLAY));
    assert!(!actions.contains(&ACTION_COPY_URL));
    assert!(entries.iter().any(|entry| {
        entry.action == ACTION_MARK_PLAYED_SELECTED
            && entry.label == strings::text(strings::PODCAST_MARK_PLAYED)
    }));
    assert!(entries.iter().any(|entry| {
        entry.action == ACTION_MARK_UNPLAYED_SELECTED
            && entry.label == strings::text(strings::PODCAST_MARK_UNPLAYED)
    }));
    assert!(actions.contains(&ACTION_DOWNLOAD_SELECTED));
    assert!(actions.contains(&ACTION_DELETE_DOWNLOADS_SELECTED));
    assert!(actions.contains(&ACTION_REMOVE_SELECTED));
    assert!(!actions.contains(&ACTION_UNSUBSCRIBE));
    // The destructive entry is the last one and sits alone in its section;
    // the split is a property of the two builders, not of an index.
    assert_eq!(
        multi_selection_destructive_entry().action,
        ACTION_REMOVE_SELECTED
    );
    assert!(!multi_selection_primary_entries()
        .iter()
        .any(|entry| entry.action == ACTION_REMOVE_SELECTED));
}

#[test]
fn src_4b_podcast_context_menu_exposes_queue_membership_actions() {
    assert!(ACTIONS.contains(&ACTION_PLAY_NEXT));
    assert!(ACTIONS.contains(&ACTION_ADD_TO_QUEUE));
}

#[test]
fn acc_8_episode_menu_queue_actions_are_the_keyboard_partner_for_drag() {
    let row = episode(1, false);
    let entries = menu_entries(&build_menu(&row, &[1, 2], None));
    let actions = entries
        .iter()
        .map(|(_, action)| action.as_str())
        .collect::<Vec<_>>();

    assert!(actions.contains(&"podcasts.play-next"));
    assert!(actions.contains(&"podcasts.add-to-queue"));
}

#[test]
fn ctx_12_unresolvable_episode_routes_to_disabled_queue_actions() {
    let row = episode(1, false);
    let entries = menu_entries(&build_menu(&row, &[1, 2], Some(2)));
    let actions = entries
        .iter()
        .map(|(_, action)| action.as_str())
        .collect::<Vec<_>>();

    assert!(actions.contains(&"podcasts.play-next-unavailable"));
    assert!(actions.contains(&"podcasts.add-to-queue-unavailable"));

    let group = gio::SimpleActionGroup::new();
    install_disabled_queue_actions(&group);
    assert!(!group
        .lookup_action(ACTION_PLAY_NEXT_UNAVAILABLE)
        .expect("play-next unavailable action")
        .is_enabled());
    assert!(!group
        .lookup_action(ACTION_ADD_TO_QUEUE_UNAVAILABLE)
        .expect("add-to-queue unavailable action")
        .is_enabled());
}

#[test]
fn ctx_13_single_file_uses_show_in_files_with_a_selection_target() {
    let temp = tempfile::tempdir().expect("temp directory");
    let path = temp.path().join("episode.mp3");
    std::fs::write(&path, b"episode").expect("episode file");
    let mut row = episode(1, false);
    row.downloaded_path = Some(path.to_string_lossy().into_owned());
    let paths = EpisodePaths::from_rows(std::slice::from_ref(&row));

    let menu = build_for_selection(&row, &[row.id], None, &paths);

    assert!(menu_entries(&menu).contains(&(
        strings::text(strings::CONTEXT_MENU_SHOW_IN_FILES),
        "podcasts.show-in-files".into(),
    )));
    assert!(episode_targets(&menu).contains(&("podcasts.show-in-files".into(), vec![row.id],)));
}

#[test]
fn ctx_13_multi_file_selection_uses_open_folder() {
    let temp = tempfile::tempdir().expect("temp directory");
    let mut rows = [episode(1, false), episode(2, false)];
    for row in &mut rows {
        let path = temp.path().join(format!("{}.mp3", row.id));
        std::fs::write(&path, b"episode").expect("episode file");
        row.downloaded_path = Some(path.to_string_lossy().into_owned());
    }
    let paths = EpisodePaths::from_rows(&rows);

    let menu = build_for_selection(&rows[0], &[1, 2], None, &paths);

    assert!(menu_entries(&menu).contains(&(
        strings::text(strings::PODCAST_OPEN_FOLDER),
        "podcasts.show-in-files".into(),
    )));
    assert!(episode_targets(&menu).contains(&("podcasts.show-in-files".into(), vec![1, 2],)));
}

#[test]
fn pod_6_context_menu_exposes_individual_episode_removal() {
    assert!(ACTIONS.contains(&ACTION_REMOVE_EPISODE));
}

#[test]
fn source_menu_has_no_phone_destination() {
    let group = SourceGroup {
        subscription_id: 1,
        title: "Channel".into(),
        author: None,
        image_url: None,
        kind: PodcastKind::Youtube,
        sync_to_phone: false,
        episodes: Vec::new(),
    };
    let menu = build_source(&group);
    // Open channel and Unsubscribe. Nothing in this menu targets a phone.
    assert_eq!(menu.n_items(), 2);
    assert!(
        menu_entries(&menu)
            .iter()
            .all(|(label, action)| !label.to_lowercase().contains("phone")
                && !action.contains("sync"))
    );
}
