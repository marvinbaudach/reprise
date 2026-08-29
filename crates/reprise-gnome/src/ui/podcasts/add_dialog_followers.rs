//! Second-wave subscriber enrichment and ordering for Add Channel results.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use gtk4::prelude::*;
use reprise_core::db::Db;
use reprise_core::podcasts::config::YoutubeBrowser;
use reprise_core::podcasts::discovery::Candidate;
use reprise_core::podcasts::{self, PodcastKind};

use crate::ui::one_shot_task;
use crate::ui::strings;

use super::add_dialog_results::{search_result_markup, subscriber_order, youtube_subtitle};
use super::add_dialog_rows::CandidateRow;

pub(super) struct YoutubeFollowerRequest {
    pub(super) ytdlp_path: Option<String>,
    pub(super) youtube_browser: Option<YoutubeBrowser>,
    pub(super) cancelled: Arc<AtomicBool>,
}

#[derive(Clone)]
struct RenderedCandidate {
    candidate: Candidate,
    row: CandidateRow,
}

#[derive(Clone)]
pub(super) struct YoutubeResults {
    parent: gtk4::Box,
    rows: Rc<RefCell<Vec<RenderedCandidate>>>,
    counts_ready: Rc<Cell<bool>>,
    largest_first: gtk4::ToggleButton,
    query: Option<String>,
}

impl YoutubeResults {
    pub(super) fn new(parent: &gtk4::Box, heading: &str, query: Option<String>) -> Self {
        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        let label = gtk4::Label::new(Some(heading));
        label.add_css_class("caption");
        label.add_css_class("reprise-text-secondary");
        label.set_xalign(0.0);
        label.set_hexpand(true);
        header.append(&label);

        let largest_first =
            gtk4::ToggleButton::with_label(&strings::text(strings::YOUTUBE_LARGEST_FIRST));
        largest_first.add_css_class("pill");
        let accessible_label = strings::text(strings::YOUTUBE_LARGEST_FIRST);
        largest_first.update_property(&[gtk4::accessible::Property::Label(&accessible_label)]);
        // a11y-semantics: role=toggle-button name=largest-first state=focusable/checked action=toggle
        header.append(&largest_first);
        parent.append(&header);

        let rows = Rc::new(RefCell::new(Vec::<RenderedCandidate>::new()));
        let counts_ready = Rc::new(Cell::new(false));
        let parent_weak = parent.downgrade();
        let rows_for_toggle = rows.clone();
        let counts_for_toggle = counts_ready.clone();
        largest_first.connect_toggled(move |button| {
            if !counts_for_toggle.get() {
                return;
            }
            let Some(parent) = parent_weak.upgrade() else {
                return;
            };
            let rows = rows_for_toggle.borrow().clone();
            reorder(&parent, &rows, button.is_active());
        });

        Self {
            parent: parent.clone(),
            rows,
            counts_ready,
            largest_first,
            query,
        }
    }

    pub(super) fn push(&self, candidate: Candidate, row: CandidateRow) {
        self.rows
            .borrow_mut()
            .push(RenderedCandidate { candidate, row });
    }

    fn targets(&self) -> Vec<podcasts::ytdlp::YtDlpChannel> {
        self.rows
            .borrow()
            .iter()
            .filter_map(|rendered| {
                Some(podcasts::ytdlp::YtDlpChannel {
                    id: rendered.candidate.channel_id.clone()?,
                    title: rendered.candidate.title.clone(),
                    url: rendered.candidate.url.clone(),
                    image_url: rendered.candidate.image_url.clone(),
                    matching_video_count: rendered.candidate.matching_video_count?,
                    matching_video_ids: rendered.candidate.identity_guids.clone(),
                    follower_count: rendered.candidate.follower_count,
                })
            })
            .collect()
    }

    fn apply(&self, counts: &[(String, Option<u64>)]) {
        let (rows, subtitle_updates) = {
            let mut rows = self.rows.borrow_mut();
            let mut subtitle_updates = Vec::new();
            for rendered in rows.iter_mut() {
                let Some(channel_id) = rendered.candidate.channel_id.as_deref() else {
                    continue;
                };
                let Some((_, follower_count)) = counts.iter().find(|(id, _)| id == channel_id)
                else {
                    continue;
                };
                rendered.candidate.follower_count = *follower_count;
                let subtitle = youtube_subtitle(
                    rendered.candidate.matching_video_count.unwrap_or_default(),
                    *follower_count,
                );
                rendered.candidate.subtitle.clone_from(&subtitle);
                subtitle_updates.push((
                    rendered.row.subtitle.clone(),
                    rendered.candidate.title.clone(),
                    subtitle,
                ));
            }
            (rows.clone(), subtitle_updates)
        };
        for (label, title, subtitle) in subtitle_updates {
            let palette = crate::ui::search_highlight::accent_palette(&label);
            let markup = search_result_markup(
                PodcastKind::Youtube,
                &title,
                &subtitle,
                None,
                self.query.as_deref(),
                Some(&palette),
            );
            label.set_markup(&markup.subtitle);
        }
        self.counts_ready.set(true);
        if self.largest_first.is_active() {
            reorder(&self.parent, &rows, true);
        }
    }

    fn finish_without_counts(&self) {
        self.largest_first.set_active(false);
        self.largest_first.set_sensitive(false);
    }

    fn apply_if_current(
        &self,
        counts: &[(String, Option<u64>)],
        cancelled: bool,
        current_generation: u64,
        request_generation: u64,
    ) -> bool {
        if cancelled || current_generation != request_generation {
            return false;
        }
        self.apply(counts);
        true
    }
}

fn reorder(parent: &gtk4::Box, rows: &[RenderedCandidate], largest_first: bool) {
    if !largest_first {
        let mut previous = parent.first_child();
        for rendered in rows {
            parent.reorder_child_after(&rendered.row.root, previous.as_ref());
            previous = Some(rendered.row.root.clone());
        }
        return;
    }
    let counts = rows
        .iter()
        .map(|rendered| rendered.candidate.follower_count)
        .collect::<Vec<_>>();
    let mut previous = parent.first_child();
    for index in subscriber_order(&counts) {
        let root = &rows[index].row.root;
        parent.reorder_child_after(root, previous.as_ref());
        previous = Some(root.clone());
    }
}

pub(super) fn start(
    results: YoutubeResults,
    request: YoutubeFollowerRequest,
    conn: &Db,
    generation: Rc<Cell<u64>>,
    request_generation: u64,
) {
    // `NET-1a`: permission is re-read after wave 1. It can change while the
    // first search is running, and an earlier allow is not authority for a
    // second set of provider requests.
    let youtube_allowed =
        reprise_core::online_sources::network_allowed(conn, &reprise_core::modules::YOUTUBE_MODULE)
            .unwrap_or(false);
    if !youtube_allowed || request.cancelled.load(Ordering::Acquire) {
        request.cancelled.store(true, Ordering::Release);
        results.finish_without_counts();
        return;
    }

    let mut channels = results.targets();
    let cancelled = request.cancelled.clone();
    let task_cancelled = cancelled.clone();
    let receiver = one_shot_task::spawn("reprise-youtube-followers", move || {
        let ytdlp = super::metadata_ytdlp(request.ytdlp_path.as_deref(), request.youtube_browser);
        ytdlp.enrich_follower_counts(&mut channels, &task_cancelled);
        channels
            .into_iter()
            .map(|channel| (channel.id, channel.follower_count))
            .collect::<Vec<_>>()
    });
    gtk4::glib::spawn_future_local(async move {
        let counts = match receiver {
            Ok(receiver) => match receiver.recv().await {
                Ok(counts) => counts,
                Err(_) => {
                    results.finish_without_counts();
                    return;
                }
            },
            Err(error) => {
                tracing::warn!(%error, "could not start YouTube follower enrichment");
                results.finish_without_counts();
                return;
            }
        };
        results.apply_if_current(
            &counts,
            cancelled.load(Ordering::Acquire),
            generation.get(),
            request_generation,
        );
    });
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::time::Duration;

    use reprise_core::podcasts::ytdlp::{YtDlp, YtDlpTimeouts};

    use super::*;

    fn candidate(id: &str, title: &str, url: &str, matching_video_count: usize) -> Candidate {
        Candidate {
            kind: PodcastKind::Youtube,
            title: title.into(),
            subtitle: youtube_subtitle(matching_video_count, None),
            author: None,
            image_url: None,
            url: url.into(),
            identity_guids: Vec::new(),
            follower_count: None,
            channel_id: Some(id.into()),
            matching_video_count: Some(matching_video_count),
        }
    }

    #[test]
    fn src_9_the_two_argv_search_path_reaches_the_channel_subtitle() {
        let directory = tempfile::tempdir().unwrap();
        let binary = directory.path().join("fake-yt-dlp");
        fs::write(
            &binary,
            r#"#!/bin/sh
set -eu
case "$*" in
  "--no-warnings --flat-playlist -J ytsearch20:metal")
    printf '%s\n' '{"entries":[{"id":"video-1","channel_id":"UC-visible","channel":"Visible"}]}' ;;
  "--no-warnings --flat-playlist -I 0 -J https://www.youtube.com/channel/UC-visible")
    printf '%s\n' '{"channel_follower_count":62400}' ;;
  *) printf '%s\n' "unexpected arguments: $*" >&2; exit 2 ;;
esac
"#,
        )
        .unwrap();
        let mut permissions = fs::metadata(&binary).unwrap().permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&binary, permissions).unwrap();
        let short = Duration::from_secs(2);
        let runner = YtDlp::with_binary_and_timeouts(
            binary,
            YtDlpTimeouts {
                version: short,
                update: short,
                list: short,
                search: short,
                channel_head: short,
                resolve: short,
                download: short,
            },
        );
        let mut channels = runner.search_channels("metal").unwrap();

        runner.enrich_follower_counts(&mut channels, &AtomicBool::new(false));
        let candidate = super::super::add_dialog_results::youtube_candidate(
            channels.into_iter().next().unwrap(),
        );

        assert!(
            candidate.subtitle.contains("62.4k"),
            "{}",
            candidate.subtitle
        );
        assert_eq!(candidate.follower_count, Some(62_400));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn src_23_largest_first_is_focusable_while_counts_are_pending() {
        gtk4::init().unwrap();
        let parent = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        let results = YoutubeResults::new(&parent, "YOUTUBE · audio only", Some("metal".into()));

        assert!(results.largest_first.is_sensitive());
        assert!(results.largest_first.has_css_class("pill"));
        results.largest_first.set_active(true);
        assert!(results.largest_first.is_active());
        assert!(!results.counts_ready.get());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn src_23_terminal_enrichment_failure_makes_largest_first_unavailable() {
        gtk4::init().unwrap();
        let parent = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        let results = YoutubeResults::new(&parent, "YOUTUBE · audio only", Some("metal".into()));
        results.largest_first.set_active(true);

        results.finish_without_counts();

        assert!(!results.largest_first.is_active());
        assert!(!results.largest_first.is_sensitive());
        assert!(!results.counts_ready.get());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn src_9_wave_two_joins_by_channel_id_and_preserves_query_highlighting() {
        gtk4::init().unwrap();
        let parent = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        let results = YoutubeResults::new(&parent, "YOUTUBE · audio only", Some("metal".into()));
        let candidate = candidate(
            "UC-stable",
            "Metal Channel",
            "https://www.youtube.com/@rewritten-downstream",
            3,
        );
        let row = super::super::add_dialog_rows::candidate_row(
            &candidate.title,
            &candidate.subtitle,
            None,
            Some("metal"),
            PodcastKind::Youtube,
            None,
            false,
        );
        let title = row
            .root
            .first_child()
            .and_then(|child| child.next_sibling())
            .and_downcast::<gtk4::Box>()
            .and_then(|labels| labels.first_child())
            .and_downcast::<gtk4::Box>()
            .and_then(|title_line| title_line.first_child())
            .and_downcast::<gtk4::Label>()
            .expect("result title");
        let highlighted_title = title.label();
        let subtitle = row.subtitle.clone();
        results.push(candidate, row);

        results.apply(&[("UC-stable".into(), Some(62_400))]);

        assert_eq!(
            subtitle.text(),
            "3 matching videos · audio only · 62.4k subscribers"
        );
        assert!(highlighted_title.contains("weight=\"bold\""));
        assert_eq!(
            title.label(),
            highlighted_title,
            "the query mark must survive"
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn a_second_search_discards_the_first_searchs_follower_result() {
        gtk4::init().unwrap();
        let parent = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        let results = YoutubeResults::new(&parent, "YOUTUBE · audio only", Some("metal".into()));
        let candidate = candidate(
            "UC-stale",
            "Stale Channel",
            "https://www.youtube.com/channel/UC-stale",
            3,
        );
        let row = super::super::add_dialog_rows::candidate_row(
            &candidate.title,
            &candidate.subtitle,
            None,
            Some("metal"),
            PodcastKind::Youtube,
            None,
            false,
        );
        let subtitle = row.subtitle.clone();
        results.push(candidate, row);

        let applied = results.apply_if_current(&[("UC-stale".into(), Some(62_400))], false, 2, 1);

        assert!(!applied);
        assert_eq!(subtitle.text(), "3 matching videos · audio only");
        assert!(!results.counts_ready.get());
        assert_eq!(results.rows.borrow()[0].candidate.follower_count, None);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn src_23_pending_largest_first_reorders_once_when_counts_arrive() {
        gtk4::init().unwrap();
        let parent = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        let results = YoutubeResults::new(&parent, "YOUTUBE · audio only", None);
        let candidates = [
            candidate(
                "UC-missing",
                "Missing",
                "https://www.youtube.com/channel/UC-missing",
                1,
            ),
            candidate(
                "UC-largest",
                "Largest",
                "https://www.youtube.com/channel/UC-largest",
                1,
            ),
            candidate(
                "UC-smaller",
                "Smaller",
                "https://www.youtube.com/channel/UC-smaller",
                1,
            ),
        ];
        let mut roots = Vec::new();
        for candidate in candidates {
            let row = super::super::add_dialog_rows::candidate_row(
                &candidate.title,
                &candidate.subtitle,
                None,
                None,
                PodcastKind::Youtube,
                None,
                false,
            );
            parent.append(&row.root);
            roots.push(row.root.clone());
            results.push(candidate, row);
        }

        results.largest_first.set_active(true);
        assert_eq!(displayed_row_order(&parent, &roots), vec![0, 1, 2]);

        results.apply(&[
            ("UC-missing".into(), None),
            ("UC-largest".into(), Some(200)),
            ("UC-smaller".into(), Some(100)),
        ]);

        assert_eq!(displayed_row_order(&parent, &roots), vec![1, 2, 0]);
    }

    fn displayed_row_order(parent: &gtk4::Box, roots: &[gtk4::Widget]) -> Vec<usize> {
        let mut order = Vec::new();
        let mut child = parent.first_child();
        while let Some(widget) = child {
            if let Some(index) = roots.iter().position(|root| root == &widget) {
                order.push(index);
            }
            child = widget.next_sibling();
        }
        order
    }
}
