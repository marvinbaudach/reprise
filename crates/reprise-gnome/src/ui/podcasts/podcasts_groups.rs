//! Channel/show-grouped podcast and YouTube rows.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use chrono::Local;
use gtk4::glib::variant::ToVariant;
use gtk4::prelude::*;
use reprise_core::podcasts::download_state::DownloadState;
use reprise_core::podcasts::{EpisodeRow, PodcastKind, SourceGroup};

use super::podcasts_context_menu;
use super::podcasts_presentation::{
    duration, file_size, relative_date, status_pill, RenderedSourceGroup, SourceSummary,
};
use crate::ui::strings;

#[derive(Clone)]
pub(super) struct DownloadRowWidgets {
    pub(super) status: gtk4::Box,
    pub(super) action: gtk4::Button,
}

struct GroupRenderContext<'a> {
    playing_episode: Option<i64>,
    expanded_sources: &'a Rc<RefCell<BTreeSet<i64>>>,
    download_states: &'a BTreeMap<i64, DownloadState>,
    connected_devices: &'a [podcasts_context_menu::PodcastSyncDevice],
    selected_devices: &'a BTreeMap<i64, Vec<String>>,
}

pub(super) fn replace(
    container: &gtk4::Box,
    groups: &[RenderedSourceGroup],
    playing_episode: Option<i64>,
    expanded_sources: &Rc<RefCell<BTreeSet<i64>>>,
    download_states: &BTreeMap<i64, DownloadState>,
    connected_devices: &[podcasts_context_menu::PodcastSyncDevice],
    selected_devices: &BTreeMap<i64, Vec<String>>,
) -> BTreeMap<i64, DownloadRowWidgets> {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
    let mut download_widgets = BTreeMap::new();
    let context = GroupRenderContext {
        playing_episode,
        expanded_sources,
        download_states,
        connected_devices,
        selected_devices,
    };
    for rendered in groups {
        container.append(&build_group(rendered, &context, &mut download_widgets));
    }
    download_widgets
}

fn build_group(
    rendered: &RenderedSourceGroup,
    context: &GroupRenderContext<'_>,
    download_widgets: &mut BTreeMap<i64, DownloadRowWidgets>,
) -> gtk4::Expander {
    let group = &rendered.group;
    let expander = gtk4::Expander::new(None);
    let expanded = context
        .expanded_sources
        .borrow()
        .contains(&group.subscription_id);
    expander.set_expanded(expanded);
    let subscription_id = group.subscription_id;
    let expanded_sources = context.expanded_sources.clone();
    expander.connect_expanded_notify(move |expander| {
        if expander.is_expanded() {
            expanded_sources.borrow_mut().insert(subscription_id);
        } else {
            expanded_sources.borrow_mut().remove(&subscription_id);
        }
    });
    expander.add_css_class("reprise-podcast-group");
    expander.set_label_widget(Some(&group_header(
        group,
        &rendered.summary,
        context.connected_devices,
        context
            .selected_devices
            .get(&group.subscription_id)
            .map_or(&[], Vec::as_slice),
    )));

    let episodes = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    episodes.add_css_class("reprise-podcast-episodes");
    for episode in &group.episodes {
        let state = context
            .download_states
            .get(&episode.id)
            .cloned()
            .unwrap_or(DownloadState::NotDownloaded);
        episodes.append(&episode_row(
            episode,
            context.playing_episode == Some(episode.id),
            &state,
            download_widgets,
        ));
    }
    expander.set_child(Some(&episodes));
    expander
}

fn group_header(
    group: &SourceGroup,
    summary: &SourceSummary,
    connected_devices: &[podcasts_context_menu::PodcastSyncDevice],
    selected_device_ids: &[String],
) -> gtk4::Widget {
    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    header.set_hexpand(true);
    header.set_margin_top(10);
    header.set_margin_bottom(10);
    header.set_margin_start(6);
    header.set_margin_end(6);

    let artwork = super::source_image::SourceImage::new(
        group.image_url.as_deref(),
        match group.kind {
            PodcastKind::Rss => "audio-input-microphone-symbolic",
            PodcastKind::Youtube => "video-x-generic-symbolic",
        },
        40,
    );
    artwork
        .widget()
        .add_css_class("reprise-podcast-group-artwork");
    header.append(artwork.widget());

    let identity = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    identity.set_hexpand(true);
    let title = gtk4::Label::new(Some(&group.title));
    title.set_xalign(0.0);
    title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    title.add_css_class("heading");
    identity.append(&title);
    if let Some(author) = group
        .author
        .as_deref()
        .filter(|author| !author.trim().is_empty())
    {
        let author = gtk4::Label::new(Some(author));
        author.set_xalign(0.0);
        author.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        author.add_css_class("caption");
        author.add_css_class("dim-label");
        identity.append(&author);
    }
    header.append(&identity);

    let facts = strings::podcast_group_facts(
        &strings::podcast_episode_count(summary.episode_count),
        summary.unplayed_count,
        &relative_date(summary.latest_published_at, Local::now().date_naive()),
        &file_size(Some(summary.downloaded_bytes)),
    );
    let facts = gtk4::Label::new(Some(&facts));
    facts.add_css_class("caption");
    facts.add_css_class("dim-label");
    header.append(&facts);
    // RSS and YouTube sources sync to their own device target folder alike
    // (`POD-12`), so the phone-sync indicator is not kind-restricted.
    if connected_devices
        .iter()
        .any(|device| selected_device_ids.contains(&device.id))
    {
        let sync = gtk4::Image::from_icon_name("phone-symbolic");
        sync.set_tooltip_text(Some(&strings::text(strings::PODCAST_SYNC_PHONE)));
        header.append(&sync);
    }
    if group.kind == PodcastKind::Youtube {
        let open = gtk4::Button::from_icon_name("go-next-symbolic");
        open.add_css_class("flat");
        open.set_tooltip_text(Some(&strings::text(strings::YOUTUBE_OPEN_CHANNEL)));
        open.set_action_name(Some("podcasts.open-channel"));
        open.set_action_target_value(Some(&group.subscription_id.to_variant()));
        header.append(&open);
    }
    let unsubscribe = gtk4::Button::from_icon_name("starred-symbolic");
    unsubscribe.add_css_class("flat");
    unsubscribe.add_css_class("accent");
    unsubscribe.set_focusable(false);
    unsubscribe.set_opacity(0.0);
    unsubscribe.set_tooltip_text(Some(&strings::text(strings::PODCAST_UNSUBSCRIBE)));
    unsubscribe.set_action_name(Some("podcasts.unsubscribe"));
    unsubscribe.set_action_target_value(Some(&group.subscription_id.to_variant()));
    header.append(&unsubscribe);
    let hover = gtk4::EventControllerMotion::new();
    let hovered_unsubscribe = unsubscribe.downgrade();
    hover.connect_enter(move |_, _, _| {
        if let Some(unsubscribe) = hovered_unsubscribe.upgrade() {
            unsubscribe.set_opacity(1.0);
        }
    });
    let hovered_unsubscribe = unsubscribe.downgrade();
    hover.connect_leave(move |_| {
        if let Some(unsubscribe) = hovered_unsubscribe.upgrade() {
            unsubscribe.set_opacity(0.0);
        }
    });
    header.add_controller(hover);

    let menu = gtk4::MenuButton::builder()
        .icon_name("view-more-symbolic")
        .menu_model(&podcasts_context_menu::build_source(
            group,
            connected_devices,
            selected_device_ids,
        ))
        .build();
    menu.add_css_class("flat");
    menu.set_tooltip_text(Some(&strings::text(strings::PODCAST_MORE_SOURCE_OPTIONS)));
    header.append(&menu);
    header.upcast()
}

fn episode_row(
    row: &EpisodeRow,
    playing: bool,
    download_state: &DownloadState,
    download_widgets: &mut BTreeMap<i64, DownloadRowWidgets>,
) -> gtk4::Widget {
    let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    root.add_css_class("reprise-podcast-episode-row");
    if playing {
        root.add_css_class("reprise-podcast-playing");
    }
    root.set_margin_start(20);
    root.set_margin_end(8);
    root.set_margin_top(7);
    root.set_margin_bottom(7);

    let play = gtk4::Button::from_icon_name(if playing {
        "media-playback-pause-symbolic"
    } else {
        "media-playback-start-symbolic"
    });
    play.add_css_class("flat");
    play.set_tooltip_text(Some(&strings::text(strings::PLAY_OR_PAUSE)));
    play.set_action_name(Some("podcasts.play"));
    play.set_action_target_value(Some(&row.id.to_variant()));
    root.append(&play);

    let identity = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    identity.set_hexpand(true);
    let title = gtk4::Label::new(Some(&row.title));
    title.set_xalign(0.0);
    title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    identity.append(&title);
    let detail = format!(
        "{} · {} · {}",
        relative_date(row.published_at, Local::now().date_naive()),
        duration(row.duration_secs),
        status_pill(row).label
    );
    let detail = gtk4::Label::new(Some(&detail));
    detail.set_xalign(0.0);
    detail.add_css_class("caption");
    detail.add_css_class("dim-label");
    identity.append(&detail);
    root.append(&identity);

    let status = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    status.append(&download_status(download_state));
    root.append(&status);

    let download = gtk4::Button::new();
    download.add_css_class("flat");
    download.set_action_name(Some("podcasts.toggle-download"));
    download.set_action_target_value(Some(&row.id.to_variant()));
    let widgets = DownloadRowWidgets {
        status,
        action: download.clone(),
    };
    update_download_state(&widgets, download_state);
    download_widgets.insert(row.id, widgets);
    root.append(&download);

    let menu = gtk4::MenuButton::builder()
        .icon_name("view-more-symbolic")
        .menu_model(&podcasts_context_menu::build(row))
        .build();
    menu.add_css_class("flat");
    menu.set_tooltip_text(Some(&strings::text(strings::PODCAST_MORE_OPTIONS)));
    root.append(&menu);
    root.upcast()
}

pub(super) fn update_download_state(widgets: &DownloadRowWidgets, state: &DownloadState) {
    while let Some(child) = widgets.status.first_child() {
        widgets.status.remove(&child);
    }
    widgets.status.append(&download_status(state));
    widgets
        .action
        .set_icon_name(if matches!(state, DownloadState::Downloaded { .. }) {
            "object-select-symbolic"
        } else {
            "folder-download-symbolic"
        });
    widgets.action.set_tooltip_text(Some(&strings::text(
        if matches!(state, DownloadState::Downloaded { .. }) {
            strings::PODCAST_DELETE_DOWNLOAD
        } else {
            strings::PODCAST_DOWNLOAD
        },
    )));
    widgets.action.set_sensitive(!matches!(
        state,
        DownloadState::Queued | DownloadState::Downloading { .. }
    ));
}

fn download_status(state: &DownloadState) -> gtk4::Widget {
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 3);
    root.set_size_request(130, -1);

    let label = gtk4::Label::new(None);
    label.set_xalign(1.0);
    label.add_css_class("caption");
    label.add_css_class("dim-label");
    match state {
        DownloadState::NotDownloaded => {
            label.set_text(&strings::text(strings::PODCAST_NOT_DOWNLOADED));
        }
        DownloadState::Queued => {
            label.set_text(&strings::text(strings::PODCAST_DOWNLOAD_QUEUED));
        }
        DownloadState::Downloading {
            received_bytes,
            total_bytes,
        } => {
            label.set_text(&format!(
                "{} · {}",
                strings::text(strings::PODCAST_DOWNLOADING),
                strings::compact_file_size(*received_bytes)
            ));
            match total_bytes {
                Some(total) if *total > 0 => {
                    let progress = gtk4::ProgressBar::new();
                    progress.set_fraction((*received_bytes as f64 / *total as f64).clamp(0.0, 1.0));
                    root.append(&progress);
                }
                _ => {
                    let spinner = gtk4::Spinner::new();
                    spinner.start();
                    spinner.set_halign(gtk4::Align::End);
                    root.append(&spinner);
                }
            }
        }
        DownloadState::Downloaded { bytes } => {
            // POD-11: the file exists, so its compact, truthful size is shown.
            label.set_text(&strings::compact_file_size(*bytes));
        }
        DownloadState::Missing => {
            label.set_text(&strings::text(strings::PODCAST_DOWNLOAD_MISSING));
        }
        DownloadState::Failed { message } => {
            label.set_text(&strings::text(strings::PODCAST_DOWNLOAD_FAILED));
            label.set_tooltip_text(Some(message));
        }
    }
    root.prepend(&label);
    root.upcast()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn src_5_one_expander_is_rendered_per_source_group() {
        gtk4::init().unwrap();
        let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let group = SourceGroup {
            subscription_id: 1,
            title: "Show".into(),
            author: None,
            image_url: None,
            kind: PodcastKind::Rss,
            sync_to_phone: false,
            episodes: Vec::new(),
        };
        let rendered = RenderedSourceGroup {
            summary: SourceSummary {
                episode_count: 0,
                unplayed_count: 0,
                downloaded_bytes: 0,
                latest_published_at: None,
            },
            group,
        };
        let widgets = replace(
            &container,
            &[rendered],
            None,
            &Rc::new(RefCell::new(BTreeSet::new())),
            &BTreeMap::new(),
            &[],
            &BTreeMap::new(),
        );
        assert!(widgets.is_empty());
        assert!(container.first_child().is_some());
        assert!(container
            .first_child()
            .and_downcast::<gtk4::Expander>()
            .is_some());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn src_4_grouped_source_keeps_the_hover_star_unsubscribe_action() {
        gtk4::init().unwrap();
        let group = SourceGroup {
            subscription_id: 7,
            title: "Show".into(),
            author: None,
            image_url: None,
            kind: PodcastKind::Rss,
            sync_to_phone: false,
            episodes: Vec::new(),
        };
        let header = group_header(
            &group,
            &SourceSummary {
                episode_count: 0,
                unplayed_count: 0,
                downloaded_bytes: 0,
                latest_published_at: None,
            },
            &[],
            &[],
        )
        .downcast::<gtk4::Box>()
        .unwrap();
        let menu = header
            .last_child()
            .and_downcast::<gtk4::MenuButton>()
            .unwrap();
        let star = menu.prev_sibling().and_downcast::<gtk4::Button>().unwrap();
        let icon = star.child().and_downcast::<gtk4::Image>().unwrap();

        assert_eq!(icon.icon_name().as_deref(), Some("starred-symbolic"));
        assert_eq!(star.opacity(), 0.0);
        assert!(star.has_css_class("accent"));
        assert_eq!(star.action_name().as_deref(), Some("podcasts.unsubscribe"));
    }
}
