use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::connectivity::Connectivity;
use reprise_core::podcasts::pipeline::SyncAbort;
use reprise_core::podcasts::{PodcastKind, SourceGroup};

use super::podcasts_groups::replace_with_sync;
use super::podcasts_presentation::{RenderedSourceGroup, SourceSummary};
use super::podcasts_selection::PodcastSelection;
use super::podcasts_sync_state::{SyncRowState, SyncStep};

fn descendants(widget: &gtk4::Widget) -> Vec<gtk4::Widget> {
    let mut result = Vec::new();
    let mut child = widget.first_child();
    while let Some(current) = child {
        result.push(current.clone());
        result.extend(descendants(&current));
        child = current.next_sibling();
    }
    result
}

fn rendered_group(id: i64) -> RenderedSourceGroup {
    RenderedSourceGroup {
        summary: SourceSummary {
            episode_count: 0,
            new_count: 0,
            downloaded_bytes: 0,
            latest_published_at: None,
        },
        group: SourceGroup {
            subscription_id: id,
            title: format!("Channel {id}"),
            author: None,
            image_url: None,
            kind: PodcastKind::Youtube,
            episodes: Vec::new(),
        },
    }
}

fn state(step: SyncStep, episodes_found: usize) -> SyncRowState {
    SyncRowState {
        step,
        episodes_found,
        abort: SyncAbort::new(),
    }
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn pod_26_each_loading_row_names_three_stable_steps_and_owns_its_failure() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    crate::ui::style::install_css_string_for_test(&crate::ui::style::app_css_for_test());

    let groups = (1..=4).map(rendered_group).collect::<Vec<_>>();
    let syncing = HashMap::from([
        (1, state(SyncStep::Added, 0)),
        (2, state(SyncStep::ReadingFeed, 7)),
        (3, state(SyncStep::DownloadingArtwork, 7)),
        (4, state(SyncStep::Failed, 0)),
    ]);
    let container = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    let widgets = replace_with_sync(
        &container,
        &groups,
        None,
        &Rc::new(RefCell::new(BTreeSet::new())),
        &Rc::new(RefCell::new(BTreeSet::new())),
        &BTreeMap::new(),
        false,
        &Rc::new(crate::test_db::open().unwrap()),
        Connectivity::Online,
        None,
        &Rc::new(RefCell::new(PodcastSelection::default())),
        "",
        &syncing,
    );
    let window = gtk4::Window::builder()
        .default_width(900)
        .default_height(700)
        .child(&container)
        .build();
    window.present();
    crate::ui::source_context_surface::settle_layout();

    assert_eq!(widgets.syncs.len(), 4, "subscriptions must not aggregate");
    let heights = (1..=4)
        .map(|id| widgets.syncs[&id].root.height())
        .collect::<Vec<_>>();
    assert!(
        heights.iter().all(|height| *height == heights[0]),
        "loading and failure heights must stay stable: {heights:?}"
    );
    assert!(heights[0] > crate::ui::source_row::ROW_MIN_HEIGHT);
    for sync in widgets.syncs.values() {
        assert_eq!(sync.step_rows.len(), 3);
        assert!(!sync.expander.is_expanded());
    }
    let cancel = descendants(widgets.syncs[&2].root.upcast_ref())
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk4::Button>().ok())
        .find(|button| button.action_name().as_deref() == Some("podcasts.cancel-sync"))
        .expect("a loading row owns its Cancel action");
    assert_eq!(
        cancel
            .action_target_value()
            .and_then(|target| target.get::<i64>()),
        Some(2)
    );
    assert_eq!(
        widgets.syncs[&4]
            .action_stack
            .visible_child_name()
            .as_deref(),
        Some("retry")
    );
    assert_eq!(
        widgets.syncs[&4].step_labels[1].text().as_str(),
        "Couldn't read feed"
    );
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn pod_26_completion_crossfades_before_the_row_shrinks_to_the_shared_height() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let settings = gtk4::Settings::default().expect("GTK settings");
    let previous = settings.is_gtk_enable_animations();
    settings.set_gtk_enable_animations(true);
    crate::ui::style::install_css_string_for_test(&crate::ui::style::app_css_for_test());

    let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let syncing = HashMap::from([(1, state(SyncStep::DownloadingArtwork, 7))]);
    let widgets = replace_with_sync(
        &container,
        &[rendered_group(1)],
        None,
        &Rc::new(RefCell::new(BTreeSet::new())),
        &Rc::new(RefCell::new(BTreeSet::new())),
        &BTreeMap::new(),
        false,
        &Rc::new(crate::test_db::open().unwrap()),
        Connectivity::Online,
        None,
        &Rc::new(RefCell::new(PodcastSelection::default())),
        "",
        &syncing,
    );
    let sync = widgets.syncs[&1].clone();
    let window = gtk4::Window::builder()
        .default_width(900)
        .child(&container)
        .build();
    window.present();
    crate::ui::source_context_surface::settle_layout();
    let tall = sync.root.height();
    assert!(tall > crate::ui::source_row::ROW_MIN_HEIGHT);

    let done = Rc::new(std::cell::Cell::new(false));
    let done_for_callback = done.clone();
    super::podcasts_sync_row::complete(&sync, move || done_for_callback.set(true));
    assert_eq!(
        sync.root.height_request(),
        tall,
        "the crossfade must pin the tall allocation before the progress child disappears"
    );
    assert_eq!(
        sync.progress_stack.transition_type(),
        gtk4::StackTransitionType::Crossfade
    );
    assert_eq!(
        sync.progress_stack.transition_duration(),
        crate::ui::motion::STANDARD_MS
    );
    assert!(!done.get(), "completion must not jump to its final state");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while !done.get() && std::time::Instant::now() < deadline {
        gtk4::glib::MainContext::default().block_on(gtk4::glib::timeout_future(
            std::time::Duration::from_millis(20),
        ));
    }
    assert!(done.get(), "the crossfade and shrink must finish");
    assert_eq!(
        sync.root.height_request(),
        crate::ui::source_row::ROW_MIN_HEIGHT
    );
    window.close();
    settings.set_gtk_enable_animations(previous);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn pod_26_reduced_motion_uses_a_static_indicator_and_no_cover_motion() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let settings = gtk4::Settings::default().expect("GTK settings");
    let previous = settings.is_gtk_enable_animations();
    settings.set_gtk_enable_animations(false);

    let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let syncing = HashMap::from([(1, state(SyncStep::ReadingFeed, 2))]);
    let widgets = replace_with_sync(
        &container,
        &[rendered_group(1)],
        None,
        &Rc::new(RefCell::new(BTreeSet::new())),
        &Rc::new(RefCell::new(BTreeSet::new())),
        &BTreeMap::new(),
        false,
        &Rc::new(crate::test_db::open().unwrap()),
        Connectivity::Online,
        None,
        &Rc::new(RefCell::new(PodcastSelection::default())),
        "",
        &syncing,
    );
    let sync = &widgets.syncs[&1];
    let active_indicator = descendants(sync.step_rows[1].upcast_ref())
        .into_iter()
        .find_map(|widget| widget.downcast::<gtk4::Stack>().ok())
        .expect("step indicator stack");
    assert_eq!(
        active_indicator.visible_child_name().as_deref(),
        Some("active-static")
    );
    let row_descendants = descendants(sync.root.upcast_ref());
    assert!(!row_descendants
        .iter()
        .any(|widget| widget.has_css_class("reprise-podcast-sync-shimmer")));
    assert!(!row_descendants
        .iter()
        .any(|widget| widget.has_css_class("reprise-podcast-sync-breathe")));
    settings.set_gtk_enable_animations(previous);
}
