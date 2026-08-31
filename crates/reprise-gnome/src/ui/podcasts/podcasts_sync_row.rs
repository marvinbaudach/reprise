//! The three-step initial-sync projection owned by one podcast source row.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::glib::variant::ToVariant;
use gtk4::prelude::*;
use libadwaita::prelude::AnimationExt;
use reprise_core::podcasts::PodcastKind;

use super::podcasts_sync_state::{SyncRowState, SyncStep};
use crate::ui::strings;

const PROGRESS_PAGE: &str = "progress";
const IDLE_PAGE: &str = "idle";
const CANCEL_PAGE: &str = "cancel";
const RETRY_PAGE: &str = "retry";
const PENDING_PAGE: &str = "pending";
const ACTIVE_PAGE: &str = "active";
const ACTIVE_STATIC_PAGE: &str = "active-static";
const DONE_PAGE: &str = "done";
const ERROR_PAGE: &str = "error";
type Completion = Rc<RefCell<Option<Box<dyn FnOnce()>>>>;

#[derive(Clone)]
struct StepWidgets {
    indicator: gtk4::Stack,
    label: gtk4::Label,
    row: gtk4::Box,
}

#[derive(Clone)]
pub(super) struct SyncRowWidgets {
    pub(super) root: gtk4::Box,
    pub(super) expander: gtk4::Expander,
    pub(super) progress_stack: gtk4::Stack,
    pub(super) action_stack: gtk4::Stack,
    pub(super) step_rows: Vec<gtk4::Box>,
    pub(super) step_labels: Vec<gtk4::Label>,
    kind: PodcastKind,
    steps: Vec<StepWidgets>,
    animations: Rc<RefCell<Vec<libadwaita::TimedAnimation>>>,
}

pub(super) fn attach(
    skeleton: &crate::ui::source_row::Skeleton,
    expander: &gtk4::Expander,
    facts: &gtk4::Label,
    subscription_id: i64,
    kind: PodcastKind,
    state: &SyncRowState,
) -> SyncRowWidgets {
    skeleton.root.add_css_class("reprise-podcast-sync-row");
    expander.add_css_class("reprise-podcast-group-syncing");
    install_loading_cover(&skeleton.media, kind);

    let progress = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    progress.add_css_class("reprise-podcast-sync-steps");
    let steps = (0..3).map(|_| step_row()).collect::<Vec<_>>();
    for step in &steps {
        progress.append(&step.row);
    }
    let idle = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let progress_stack = gtk4::Stack::builder()
        .transition_type(gtk4::StackTransitionType::Crossfade)
        .transition_duration(crate::ui::motion::STANDARD_MS)
        .vhomogeneous(false)
        .hhomogeneous(false)
        .build();
    progress_stack.add_named(&progress, Some(PROGRESS_PAGE));
    progress_stack.add_named(&idle, Some(IDLE_PAGE));
    progress_stack.set_visible_child_name(PROGRESS_PAGE);
    skeleton.identity.append(&progress_stack);

    let cancel = gtk4::Button::with_label(&strings::text(strings::PODCAST_CANCEL));
    cancel.add_css_class("flat");
    cancel.set_action_name(Some("podcasts.cancel-sync"));
    cancel.set_action_target_value(Some(&subscription_id.to_variant()));
    let retry = gtk4::Button::with_label(&strings::text(strings::PODCAST_SYNC_RETRY));
    retry.add_css_class("flat");
    retry.set_action_name(Some("podcasts.retry-sync"));
    retry.set_action_target_value(Some(&subscription_id.to_variant()));
    let action_stack = gtk4::Stack::builder()
        .transition_type(gtk4::StackTransitionType::Crossfade)
        .transition_duration(crate::ui::motion::STANDARD_MS)
        .vhomogeneous(false)
        .hhomogeneous(false)
        .build();
    action_stack.add_named(facts, Some(IDLE_PAGE));
    action_stack.add_named(&cancel, Some(CANCEL_PAGE));
    action_stack.add_named(&retry, Some(RETRY_PAGE));
    skeleton.trailing.append(&action_stack);

    let widgets = SyncRowWidgets {
        root: skeleton.root.clone(),
        expander: expander.clone(),
        progress_stack,
        action_stack,
        step_rows: steps.iter().map(|step| step.row.clone()).collect(),
        step_labels: steps.iter().map(|step| step.label.clone()).collect(),
        kind,
        steps,
        animations: Rc::new(RefCell::new(Vec::new())),
    };
    update(&widgets, state);
    widgets
}

fn install_loading_cover(host: &gtk4::Box, kind: PodcastKind) {
    while let Some(child) = host.first_child() {
        host.remove(&child);
    }
    let icon_name = match kind {
        PodcastKind::Rss => "audio-input-microphone-symbolic",
        PodcastKind::Youtube => "video-x-generic-symbolic",
    };
    let icon = gtk4::Image::from_icon_name(icon_name);
    icon.set_pixel_size(22);
    icon.add_css_class("reprise-podcast-sync-cover-icon");
    let cover = gtk4::Overlay::new();
    cover.add_css_class("reprise-podcast-sync-cover");
    cover.set_overflow(gtk4::Overflow::Hidden);
    cover.set_child(Some(&icon));
    if crate::ui::motion::animations_enabled() {
        icon.add_css_class("reprise-podcast-sync-breathe");
        let shimmer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        shimmer.add_css_class("reprise-podcast-sync-shimmer");
        shimmer.set_halign(gtk4::Align::Start);
        shimmer.set_valign(gtk4::Align::Fill);
        cover.add_overlay(&shimmer);
    }
    host.append(&crate::ui::source_row::media(
        &cover,
        crate::ui::source_row::MediaShape::SourceSquare,
    ));
}

fn step_row() -> StepWidgets {
    let pending = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    pending.add_css_class("reprise-podcast-sync-dot");
    let active_static = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    active_static.add_css_class("reprise-podcast-sync-dot");
    active_static.add_css_class("reprise-podcast-sync-dot-active");
    let active = gtk4::Image::from_icon_name("view-refresh-symbolic");
    active.add_css_class("reprise-podcast-sync-spin");
    let done = gtk4::Image::from_icon_name(crate::ui::icons::DONE);
    let error = gtk4::Image::from_icon_name("dialog-warning-symbolic");
    let indicator = gtk4::Stack::new();
    indicator.set_size_request(16, 16);
    indicator.set_hhomogeneous(true);
    indicator.set_vhomogeneous(true);
    indicator.add_named(&pending, Some(PENDING_PAGE));
    indicator.add_named(&active, Some(ACTIVE_PAGE));
    indicator.add_named(&active_static, Some(ACTIVE_STATIC_PAGE));
    indicator.add_named(&done, Some(DONE_PAGE));
    indicator.add_named(&error, Some(ERROR_PAGE));

    let label = gtk4::Label::new(None);
    label.set_xalign(0.0);
    label.add_css_class("caption");
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    row.append(&indicator);
    row.append(&label);
    StepWidgets {
        indicator,
        label,
        row,
    }
}

pub(super) fn update(widgets: &SyncRowWidgets, state: &SyncRowState) {
    let labels = step_labels(widgets.kind, state.episodes_found);
    let active = match state.step {
        SyncStep::Added => 0,
        SyncStep::ReadingFeed | SyncStep::Failed => 1,
        SyncStep::DownloadingArtwork => 2,
    };
    for (index, step) in widgets.steps.iter().enumerate() {
        step.label.set_text(labels[index].as_str());
        for class in [
            "reprise-podcast-sync-step-done",
            "reprise-podcast-sync-step-active",
            "reprise-podcast-sync-step-pending",
            "reprise-podcast-sync-step-failed",
        ] {
            step.row.remove_css_class(class);
        }
        let failed = state.step == SyncStep::Failed && index == active;
        let (page, class) = if failed {
            step.label
                .set_text(&strings::text(strings::PODCAST_SYNC_FAILED));
            (ERROR_PAGE, "reprise-podcast-sync-step-failed")
        } else if index < active {
            (DONE_PAGE, "reprise-podcast-sync-step-done")
        } else if index == active {
            (
                if crate::ui::motion::animations_enabled() {
                    ACTIVE_PAGE
                } else {
                    ACTIVE_STATIC_PAGE
                },
                "reprise-podcast-sync-step-active",
            )
        } else {
            (PENDING_PAGE, "reprise-podcast-sync-step-pending")
        };
        step.indicator.set_visible_child_name(page);
        step.row.add_css_class(class);
    }
    widgets
        .action_stack
        .set_visible_child_name(if state.step == SyncStep::Failed {
            RETRY_PAGE
        } else {
            CANCEL_PAGE
        });
}

fn step_labels(kind: PodcastKind, episodes_found: usize) -> [String; 3] {
    let added = match kind {
        PodcastKind::Rss => strings::PODCAST_SYNC_ADDED,
        PodcastKind::Youtube => strings::YOUTUBE_SYNC_ADDED,
    };
    [
        strings::text(added),
        strings::podcast_sync_reading(episodes_found),
        strings::text(strings::PODCAST_SYNC_ARTWORK),
    ]
}

pub(super) fn complete(widgets: &SyncRowWidgets, on_done: impl FnOnce() + 'static) {
    let animated = crate::ui::motion::animations_enabled();
    let start_height = animated.then(|| {
        let height = widgets
            .root
            .height()
            .max(crate::ui::source_row::ROW_MIN_HEIGHT);
        widgets.root.set_height_request(height);
        height
    });
    widgets.progress_stack.set_visible_child_name(IDLE_PAGE);
    widgets.action_stack.set_visible_child_name(IDLE_PAGE);
    widgets
        .expander
        .remove_css_class("reprise-podcast-group-syncing");
    widgets.root.remove_css_class("reprise-podcast-sync-row");

    let Some(start_height) = start_height else {
        widgets
            .root
            .set_height_request(crate::ui::source_row::ROW_MIN_HEIGHT);
        on_done();
        return;
    };

    let root = widgets.root.clone();
    let animations = widgets.animations.clone();
    let on_done: Completion = Rc::new(RefCell::new(Some(Box::new(on_done))));
    gtk4::glib::timeout_add_local_once(
        Duration::from_millis(u64::from(crate::ui::motion::STANDARD_MS)),
        move || {
            let weak_root = root.downgrade();
            let target = libadwaita::CallbackAnimationTarget::new(move |progress| {
                let Some(root) = weak_root.upgrade() else {
                    return;
                };
                let target = crate::ui::source_row::ROW_MIN_HEIGHT;
                let height = f64::from(start_height) + f64::from(target - start_height) * progress;
                root.set_height_request(height.round() as i32);
            });
            let animation =
                crate::ui::motion::timed(&root, 0.0, 1.0, crate::ui::motion::STANDARD, target);
            let done_root = root.clone();
            let done_callback = on_done.clone();
            animation.connect_done(move |_| {
                done_root.set_height_request(crate::ui::source_row::ROW_MIN_HEIGHT);
                if let Some(callback) = done_callback.borrow_mut().take() {
                    callback();
                }
            });
            animation.play();
            animations.borrow_mut().push(animation);
        },
    );
}
