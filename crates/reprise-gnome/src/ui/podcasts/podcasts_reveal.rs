//! Revealing the loaded episode in the grouped list.
//!
//! `SRC-13`'s "how" for the podcast/YouTube surface. *Whether* to reveal is
//! decided in `crate::ui::source_reveal`; this module answers where the
//! episode is and what has to open before it exists as a widget at all.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::AnimationExt;
use reprise_core::podcasts::SourceGroup;

use super::podcasts_episode_window::visible_count;

/// Frames to wait for the freshly rebuilt tree to allocate before giving up.
/// A `render()` replaces every widget, so the first frames report zero
/// geometry; bailing out after a bounded number of frames keeps a row that
/// never allocates (filtered away mid-flight, view hidden again) from leaving
/// a tick callback spinning for the process lifetime.
const MAX_LAYOUT_FRAMES: u32 = 60;

/// What has to change about the list's expansion state before the loaded
/// episode is a rendered row that can be scrolled to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RevealTarget {
    /// The group that has to be expanded.
    pub(super) subscription_id: i64,
    /// Whether the group's ten-episode preview window also has to be opened —
    /// true when the episode sits past the preview, where no row is built.
    pub(super) needs_full_window: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RevealRequest {
    Episode(i64),
    Channel(i64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RevealOutcome {
    Reveal(RevealRequest),
    NotListed,
}

/// Resolves an explicit request against the unfiltered source groups. The
/// episode must belong to the addressed subscription, so stale or mismatched
/// identities are reported rather than silently aimed at another group.
pub(super) fn reveal_outcome(
    groups: &[SourceGroup],
    subscription_id: i64,
    episode_id: Option<i64>,
) -> RevealOutcome {
    let Some(group) = groups
        .iter()
        .find(|group| group.subscription_id == subscription_id)
    else {
        return RevealOutcome::NotListed;
    };
    match episode_id {
        Some(episode_id) if group.episodes.iter().any(|row| row.id == episode_id) => {
            RevealOutcome::Reveal(RevealRequest::Episode(episode_id))
        }
        Some(_) => RevealOutcome::NotListed,
        None => RevealOutcome::Reveal(RevealRequest::Channel(subscription_id)),
    }
}

/// Locates `episode_id` in the rendered groups and reports what must open for
/// it to become visible. `None` when the episode is not in this list at all —
/// a filtered-out episode, or the other kind's view.
pub(super) fn reveal_target(
    groups: &[SourceGroup],
    episode_id: i64,
    window_already_expanded: bool,
) -> Option<RevealTarget> {
    let group = groups
        .iter()
        .find(|group| group.episodes.iter().any(|row| row.id == episode_id))?;
    let index = group.episodes.iter().position(|row| row.id == episode_id)?;
    let rendered = visible_count(group.episodes.len(), window_already_expanded);
    Some(RevealTarget {
        subscription_id: group.subscription_id,
        needs_full_window: index >= rendered,
    })
}

/// Locates a channel in the rendered groups. `needs_full_window` is always
/// false: whoever jumps to the channel wants to see it from the top, not a row
/// in the middle of its episode list (Spec A.2).
///
/// Measured on 2026-08-05 in the first post-idle Xvfb tick: the header was
/// 40 px high both collapsed and expanded; the respective expanders were 60
/// and 102 px high. The header is therefore the stable centering target in
/// both states, while the expanded expander would include episode rows.
pub(super) fn channel_reveal_target(
    groups: &[SourceGroup],
    subscription_id: i64,
) -> Option<RevealTarget> {
    groups
        .iter()
        .any(|group| group.subscription_id == subscription_id)
        .then_some(RevealTarget {
            subscription_id,
            needs_full_window: false,
        })
}

/// Adjustment value that vertically centers a row spanning
/// `row_top..row_top + row_height` in the scrolled content. `row_top` is
/// measured against the content, not the viewport. Returns `None` while the
/// geometry is not usable yet — nothing allocated, or the content fits the
/// viewport entirely, in which case there is nothing to center.
pub(super) fn centered_value(
    row_top: f64,
    row_height: f64,
    page_size: f64,
    upper: f64,
) -> Option<f64> {
    if row_height <= 0.0 || page_size <= 0.0 || upper <= page_size {
        return None;
    }
    let target = row_top + row_height / 2.0 - page_size / 2.0;
    Some(target.clamp(0.0, upper - page_size))
}

fn centered_target(scroller: &gtk4::ScrolledWindow, row: &gtk4::Widget) -> Option<f64> {
    let adjustment = scroller.vadjustment();
    let point = row.compute_point(scroller, &gtk4::graphene::Point::new(0.0, 0.0))?;
    // `compute_point` gives the row's offset inside the *viewport*; adding the
    // current scroll offset lifts it into content coordinates, which is what
    // the adjustment speaks.
    let row_top = f64::from(point.y()) + adjustment.value();
    centered_value(
        row_top,
        f64::from(row.height()),
        adjustment.page_size(),
        adjustment.upper(),
    )
}

fn apply(
    scroller: &gtk4::ScrolledWindow,
    target: f64,
    animation_slot: &Rc<RefCell<Option<adw::TimedAnimation>>>,
) {
    let adjustment = scroller.vadjustment();
    // `MOT-7`: the animation gate is honoured by jumping, not by animating
    // faster.
    if !crate::ui::motion::animations_enabled() {
        adjustment.set_value(target);
        return;
    }
    if (adjustment.value() - target).abs() < f64::EPSILON {
        return;
    }
    let animation_target = adw::CallbackAnimationTarget::new({
        let adjustment = adjustment.clone();
        move |value| adjustment.set_value(value)
    });
    let animation = crate::ui::motion::timed(
        scroller,
        adjustment.value(),
        target,
        crate::ui::motion::STANDARD,
        animation_target,
    );
    // The animation must outlive this call, and a second reveal must replace
    // the first rather than race it.
    crate::ui::motion::replace_animation(animation_slot, animation.clone());
    animation.play();
}

/// Centers `row` in `scroller` once the layout after a `render()` has settled.
/// Never touches focus or selection — `SRC-13` reveals the viewport only.
pub(super) fn center_row(
    scroller: &gtk4::ScrolledWindow,
    row: &gtk4::Widget,
    animation_slot: &Rc<RefCell<Option<adw::TimedAnimation>>>,
) {
    let scroller = scroller.clone();
    let row = row.clone();
    let animation_slot = animation_slot.clone();
    // Out of the caller's signal handler first: `render()` has only just
    // rebuilt the tree, so nothing is allocated in this main-loop turn.
    gtk4::glib::idle_add_local_once(move || {
        let frames = std::cell::Cell::new(0_u32);
        let target_scroller = scroller.clone();
        scroller.add_tick_callback(move |_, _| {
            if let Some(value) = centered_target(&target_scroller, &row) {
                apply(&target_scroller, value, &animation_slot);
                return gtk4::glib::ControlFlow::Break;
            }
            let seen = frames.replace(frames.get() + 1);
            if seen >= MAX_LAYOUT_FRAMES {
                tracing::debug!("episode reveal gave up waiting for layout");
                return gtk4::glib::ControlFlow::Break;
            }
            gtk4::glib::ControlFlow::Continue
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::podcasts::{EpisodeRow, PodcastKind, SourceGroup};

    fn episode(id: i64, subscription_id: i64) -> EpisodeRow {
        EpisodeRow {
            id,
            subscription_id,
            guid: format!("episode-{id}"),
            title: format!("Episode {id}"),
            show: "Show".into(),
            show_image_url: None,
            image_url: None,
            kind: PodcastKind::Youtube,
            audio_url: "https://example.test/e.mp3".into(),
            page_url: None,
            published_at: None,
            duration_secs: None,
            downloaded_path: None,
            downloaded_bytes: None,
            played_at: None,
            position_ms: 0,
            first_seen_at: 1,
            is_new: false,
            media_category: None,
        }
    }

    fn group(subscription_id: i64, episode_ids: &[i64]) -> SourceGroup {
        SourceGroup {
            subscription_id,
            title: format!("Channel {subscription_id}"),
            author: None,
            image_url: None,
            kind: PodcastKind::Youtube,
            episodes: episode_ids
                .iter()
                .map(|id| episode(*id, subscription_id))
                .collect(),
        }
    }

    #[test]
    fn an_episode_inside_the_preview_window_only_needs_its_group_expanded() {
        let groups = [group(7, &[1, 2, 3])];

        assert_eq!(
            reveal_target(&groups, 2, false),
            Some(RevealTarget {
                subscription_id: 7,
                needs_full_window: false,
            })
        );
    }

    #[test]
    fn an_episode_beyond_the_preview_window_needs_the_window_opened() {
        let ids = (1..=15).collect::<Vec<_>>();
        let groups = [group(7, &ids)];

        // Index 12 (episode 13) sits past the ten-episode preview.
        assert_eq!(
            reveal_target(&groups, 13, false),
            Some(RevealTarget {
                subscription_id: 7,
                needs_full_window: true,
            })
        );
    }

    #[test]
    fn an_already_expanded_window_never_asks_to_be_expanded_again() {
        let ids = (1..=15).collect::<Vec<_>>();
        let groups = [group(7, &ids)];

        assert_eq!(
            reveal_target(&groups, 13, true),
            Some(RevealTarget {
                subscription_id: 7,
                needs_full_window: false,
            })
        );
    }

    #[test]
    fn the_right_group_is_picked_out_of_several() {
        let groups = [group(7, &[1, 2]), group(8, &[3, 4])];

        assert_eq!(
            reveal_target(&groups, 4, false).map(|target| target.subscription_id),
            Some(8)
        );
    }

    #[test]
    fn an_episode_that_is_not_listed_has_nothing_to_reveal() {
        let groups = [group(7, &[1, 2])];

        assert_eq!(reveal_target(&groups, 99, false), None);
        assert_eq!(reveal_target(&[], 1, false), None);
    }

    #[test]
    fn src_13_a_channel_reveal_only_expands_its_group() {
        let groups = [group(7, &[1, 2, 3])];

        assert_eq!(
            channel_reveal_target(&groups, 7),
            Some(RevealTarget {
                subscription_id: 7,
                needs_full_window: false,
            })
        );
    }

    #[test]
    fn src_13_a_channel_reveal_leaves_the_episode_window_closed() {
        let ids = (1..=15).collect::<Vec<_>>();
        let groups = [group(7, &ids)];

        assert_eq!(
            channel_reveal_target(&groups, 7),
            Some(RevealTarget {
                subscription_id: 7,
                needs_full_window: false,
            })
        );
    }

    #[test]
    fn src_13_a_channel_that_is_not_listed_has_nothing_to_reveal() {
        let groups = [group(7, &[1, 2])];

        assert_eq!(channel_reveal_target(&groups, 99), None);
        assert_eq!(channel_reveal_target(&[], 7), None);
    }

    #[test]
    fn src_13_an_unlisted_episode_is_reported_instead_of_ignored() {
        let groups = [group(7, &[1, 2])];

        assert_eq!(
            reveal_outcome(&groups, 7, Some(99)),
            RevealOutcome::NotListed
        );
        assert_eq!(reveal_outcome(&groups, 99, None), RevealOutcome::NotListed);
        assert_eq!(
            reveal_outcome(&groups, 7, Some(2)),
            RevealOutcome::Reveal(RevealRequest::Episode(2))
        );
    }

    #[test]
    fn centering_puts_the_row_middle_at_the_viewport_middle() {
        // Row spans 500..540 in a 1000px-tall content, 200px viewport.
        // Its middle is 520; centering puts the viewport at 520 - 100 = 420.
        assert_eq!(centered_value(500.0, 40.0, 200.0, 1000.0), Some(420.0));
    }

    #[test]
    fn centering_clamps_at_both_ends_instead_of_overscrolling() {
        assert_eq!(centered_value(0.0, 40.0, 200.0, 1000.0), Some(0.0));
        assert_eq!(centered_value(960.0, 40.0, 200.0, 1000.0), Some(800.0));
    }

    #[test]
    fn centering_skips_geometry_that_is_not_ready_or_not_scrollable() {
        // Not allocated yet.
        assert_eq!(centered_value(0.0, 0.0, 0.0, 0.0), None);
        // Row not allocated yet, viewport is.
        assert_eq!(centered_value(100.0, 0.0, 200.0, 1000.0), None);
        // Whole list fits: nothing to scroll.
        assert_eq!(centered_value(10.0, 40.0, 1000.0, 500.0), None);
    }
}
