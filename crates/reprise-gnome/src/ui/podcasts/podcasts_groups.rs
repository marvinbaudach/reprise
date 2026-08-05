//! Channel/show-grouped podcast and YouTube rows.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use chrono::Local;
use gtk4::glib::variant::ToVariant;
use gtk4::prelude::*;
use reprise_core::connectivity::Connectivity;
use reprise_core::podcasts::download_state::DownloadState;
use reprise_core::podcasts::{EpisodeRow, PodcastKind, SourceGroup};

use super::podcasts_context_menu;
use super::podcasts_context_surface;
use super::podcasts_playback::EpisodeMark;
use super::podcasts_presentation::{
    duration, file_size, on_phone, relative_date, source_header, status_pill, RenderedSourceGroup,
    SourceSummary,
};
use super::podcasts_row_interaction::{
    episode_thumbnail, install_row_interaction, SELECT_ROW_ACTION,
};
use super::podcasts_row_state::{download_status, RowNetworkState};
use super::podcasts_selection::PodcastSelection;
use super::podcasts_title::TitleParts;
use crate::ui::playing_marker;
use crate::ui::strings;

/// `SRC-14`: the look of a selected row. Applied here at build time and by
/// `PodcastsView::apply_selection` afterwards.
pub(super) const SELECTED_ROW_CLASS: &str = "reprise-podcast-episode-selected";

#[derive(Clone)]
pub(super) struct DownloadRowWidgets {
    pub(super) root: gtk4::Box,
    pub(super) status: gtk4::Box,
    pub(super) action: gtk4::Button,
    pub(super) marker: gtk4::Box,
}

/// The row a selection change has to touch, held per episode so a selection can
/// be applied without rebuilding the list — see `PodcastsView::apply_selection`.
pub(super) struct SelectionRowWidgets {
    pub(super) row: gtk4::Box,
    pub(super) media: Option<Rc<crate::ui::source_row::MediaColumn>>,
    pub(super) reveal: Option<Rc<crate::ui::source_row::Reveal>>,
}

/// Everything `replace` hands back for later targeted updates.
pub(super) struct RenderedRowWidgets {
    pub(super) downloads: BTreeMap<i64, DownloadRowWidgets>,
    pub(super) selection: BTreeMap<i64, SelectionRowWidgets>,
}

struct GroupRenderContext<'a> {
    playing_episode: Option<EpisodeMark>,
    expanded_sources: &'a Rc<RefCell<BTreeSet<i64>>>,
    expanded_episode_sources: &'a Rc<RefCell<BTreeSet<i64>>>,
    download_states: &'a BTreeMap<i64, DownloadState>,
    connected_devices: &'a [podcasts_context_menu::PodcastSyncDevice],
    selected_devices: &'a BTreeMap<i64, Vec<String>>,
    /// `NET-1a` / `C1`: `online_sources::network_allowed(conn,
    /// &modules::SOURCE_IMAGES_MODULE)`, computed once per render pass by
    /// the caller — this module never reads settings itself.
    images_allowed: bool,
    connectivity: Connectivity,
    unavailable_episode: Option<i64>,
    selection: &'a Rc<RefCell<PodcastSelection>>,
    selected_ids: Vec<i64>,
}

struct EpisodeRenderContext<'a> {
    mark: Option<EpisodeMark>,
    download_state: &'a DownloadState,
    images_allowed: bool,
    network: RowNetworkState,
    selection: &'a Rc<RefCell<PodcastSelection>>,
    selected_ids: &'a [i64],
    unavailable_episode: Option<i64>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn replace(
    container: &gtk4::Box,
    groups: &[RenderedSourceGroup],
    playing_episode: Option<EpisodeMark>,
    expanded_sources: &Rc<RefCell<BTreeSet<i64>>>,
    expanded_episode_sources: &Rc<RefCell<BTreeSet<i64>>>,
    download_states: &BTreeMap<i64, DownloadState>,
    connected_devices: &[podcasts_context_menu::PodcastSyncDevice],
    selected_devices: &BTreeMap<i64, Vec<String>>,
    images_allowed: bool,
    connectivity: Connectivity,
    unavailable_episode: Option<i64>,
    selection: &Rc<RefCell<PodcastSelection>>,
) -> RenderedRowWidgets {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
    let mut widgets = RenderedRowWidgets {
        downloads: BTreeMap::new(),
        selection: BTreeMap::new(),
    };
    let selected_ids = selection.borrow().selected_ids();
    let context = GroupRenderContext {
        playing_episode,
        expanded_sources,
        expanded_episode_sources,
        download_states,
        connected_devices,
        selected_devices,
        images_allowed,
        connectivity,
        unavailable_episode,
        selection,
        selected_ids,
    };
    for rendered in groups {
        container.append(&build_group(rendered, &context, &mut widgets));
    }
    widgets
}

fn build_group(
    rendered: &RenderedSourceGroup,
    context: &GroupRenderContext<'_>,
    widgets: &mut RenderedRowWidgets,
) -> gtk4::Expander {
    let group = &rendered.group;
    let expander = gtk4::Expander::new(None);
    // The title lives in the header widget, which leaves the expander itself
    // nameless in the accessibility tree — a screen reader announces "toggle
    // button" with nothing to say which show it opens. Naming it is also what
    // lets a keyboard or assistive user address one show among several.
    expander.update_property(&[gtk4::accessible::Property::Label(&group.title)]);
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
    let header = group_header(
        group,
        &rendered.summary,
        context.connected_devices,
        context
            .selected_devices
            .get(&group.subscription_id)
            .map_or(&[], Vec::as_slice),
        context.images_allowed,
    );
    expander.set_label_widget(Some(&header));

    let episodes = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    episodes.add_css_class("reprise-podcast-episodes");
    let titles = group
        .episodes
        .iter()
        .map(|episode| episode.title.as_str())
        .collect::<Vec<_>>();
    let all_episodes_visible = context
        .expanded_episode_sources
        .borrow()
        .contains(&group.subscription_id);
    let visible_count =
        super::podcasts_episode_window::visible_count(group.episodes.len(), all_episodes_visible);
    for episode in group.episodes.iter().take(visible_count) {
        let state = context
            .download_states
            .get(&episode.id)
            .cloned()
            .unwrap_or(DownloadState::NotDownloaded);
        let title_parts = super::podcasts_title::for_group(
            &titles,
            &episode.title,
            group.kind == PodcastKind::Youtube,
        );
        episodes.append(&episode_row(
            episode,
            &title_parts,
            widgets,
            &EpisodeRenderContext {
                mark: context.playing_episode.filter(|mark| mark.id == episode.id),
                download_state: &state,
                images_allowed: context.images_allowed,
                network: RowNetworkState {
                    connectivity: context.connectivity,
                    unavailable_now: context.unavailable_episode == Some(episode.id),
                },
                selection: context.selection,
                selected_ids: &context.selected_ids,
                unavailable_episode: context.unavailable_episode,
            },
        ));
    }
    if visible_count < group.episodes.len() {
        let show_all =
            gtk4::Button::with_label(&strings::podcast_show_all_episodes(group.episodes.len()));
        show_all.add_css_class("flat");
        show_all.set_action_name(Some("podcasts.show-all-episodes"));
        show_all.set_action_target_value(Some(&group.subscription_id.to_variant()));
        episodes.append(&show_all);
    }
    expander.set_child(Some(&episodes));
    expander
}

fn group_header(
    group: &SourceGroup,
    summary: &SourceSummary,
    connected_devices: &[podcasts_context_menu::PodcastSyncDevice],
    selected_device_ids: &[String],
    images_allowed: bool,
) -> gtk4::Widget {
    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    header.set_hexpand(true);
    header.set_margin_top(10);
    header.set_margin_bottom(10);
    header.set_margin_start(6);
    header.set_margin_end(6);

    let artwork = super::source_image::SourceImage::new(
        group_image_url(group),
        match group.kind {
            PodcastKind::Rss => "audio-input-microphone-symbolic",
            PodcastKind::Youtube => "video-x-generic-symbolic",
        },
        40,
        images_allowed,
    );
    artwork
        .widget()
        .add_css_class("reprise-podcast-group-artwork");
    header.append(artwork.widget());

    let identity = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    identity.set_hexpand(true);
    identity.set_valign(gtk4::Align::Center);
    let source = source_header(group.kind, &group.title, group.author.as_deref());
    let title = gtk4::Label::new(Some(source.title));
    title.set_xalign(0.0);
    title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    title.add_css_class("heading");
    identity.append(&title);
    if let Some(author) = source.subtitle {
        let author = gtk4::Label::new(Some(author));
        author.set_xalign(0.0);
        author.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        author.add_css_class("caption");
        author.add_css_class("dim-label");
        identity.append(&author);
    }
    header.append(&identity);

    let downloaded = file_size(Some(summary.downloaded_bytes));
    let facts = strings::podcast_group_facts(
        &strings::podcast_episode_count(summary.episode_count),
        summary.new_count,
        &relative_date(summary.latest_published_at, Local::now().date_naive()),
        downloaded.as_deref().unwrap_or_default(),
    );
    let facts = gtk4::Label::new(Some(&facts));
    facts.add_css_class("caption");
    facts.add_css_class("dim-label");
    header.append(&facts);
    // RSS and YouTube sources sync to their own device target folder alike
    // (`POD-12`), so the phone-sync indicator is not kind-restricted.
    if on_phone(connected_devices, selected_device_ids) {
        let sync = gtk4::Image::from_icon_name("phone-symbolic");
        sync.set_tooltip_text(Some(&strings::text(strings::PODCAST_SYNC_PHONE)));
        header.append(&sync);
    }
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
    podcasts_context_surface::wire_source_header(
        &header,
        group,
        connected_devices,
        selected_device_ids,
    );
    header.upcast()
}

fn group_image_url(group: &SourceGroup) -> Option<&str> {
    group.image_url.as_deref().or_else(|| match group.kind {
        PodcastKind::Rss => None,
        PodcastKind::Youtube => group
            .episodes
            .first()
            .and_then(|episode| episode.image_url.as_deref()),
    })
}

fn episode_row(
    row: &EpisodeRow,
    title_parts: &TitleParts,
    widgets: &mut RenderedRowWidgets,
    context: &EpisodeRenderContext<'_>,
) -> gtk4::Widget {
    let loaded = context.mark.is_some();
    let playing = context.mark.is_some_and(|mark| mark.playing);
    let skeleton = crate::ui::source_row::skeleton();
    let root = skeleton.root.clone();
    root.add_css_class("reprise-podcast-episode-row");
    // `POD-20`: this plain Box needs the shared hover tint that ColumnView
    // rows receive from the platform stylesheet.
    root.add_css_class("reprise-hover");
    // a11y-semantics: role=button name=podcast-episode-row state=focusable action=activate
    root.set_focusable(true);
    // input-parity: ACC-8 keyboard=episode-row-enter-space
    root.set_cursor_from_name(Some("pointer"));
    root.set_accessible_role(gtk4::AccessibleRole::Button);
    // The name is what the row *is*, not what clicking it does: activating
    // this button plays the episode, while a plain click selects it. Naming
    // it "Select …" would tell a screen reader the opposite of what Enter
    // does. Selection is reported through the `Selected` state below, which
    // is where assistive technology expects to read it.
    root.update_property(&[gtk4::accessible::Property::Label(&row.title)]);
    if loaded {
        root.add_css_class("reprise-podcast-playing");
    }

    let is_selected = context.selection.borrow().contains(row.id);
    root.update_state(&[gtk4::accessible::State::Selected(Some(is_selected))]);
    if is_selected {
        root.add_css_class(SELECTED_ROW_CLASS);
    }
    let (artwork, shape) = episode_thumbnail(row, context.images_allowed);
    let media = Rc::new(crate::ui::source_row::MediaColumn::new(&artwork, shape));
    let marker = playing_marker::build();
    marker.add_css_class("reprise-podcast-episode-marker");
    playing_marker::set_playing(&marker, playing);
    media.set_state_overlay(&marker);
    media.set_loaded(loaded);
    skeleton.media.append(media.widget());
    install_row_interaction(&root, row.id, SELECT_ROW_ACTION);
    podcasts_context_surface::wire_episode_row(
        &root,
        row,
        context.selection,
        context.unavailable_episode,
        SELECT_ROW_ACTION,
    );
    super::podcasts_dnd::wire_episode_drag_source(&root, row.id, context.selection);

    let title = gtk4::Label::new(None);
    title.set_markup(&super::podcasts_title::markup(title_parts));
    title.set_xalign(0.0);
    title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    title.add_css_class("reprise-source-row-title");
    skeleton.identity.append(&title);
    let date = relative_date(row.published_at, Local::now().date_naive());
    let duration = duration(row.duration_secs);
    let detail_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    let facts = gtk4::Label::new(Some(&crate::ui::source_row::detail_line([
        date.as_str(),
        duration.as_str(),
    ])));
    facts.set_xalign(0.0);
    facts.add_css_class("caption");
    facts.add_css_class("dim-label");
    detail_row.append(&facts);
    if let Some(spec) = chip_spec(row) {
        detail_row.append(&crate::ui::source_row::chip(&spec));
    }
    skeleton.identity.append(&detail_row);

    let status = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    status.set_size_request(crate::ui::source_row::SIZE_SLOT_WIDTH, -1);
    status.add_css_class("reprise-source-row-size");
    status.append(&download_status(context.download_state));
    skeleton.trailing.append(&status);

    let download = gtk4::Button::new();
    download.add_css_class("flat");
    download.set_action_name(Some("podcasts.toggle-download"));
    download.set_action_target_value(Some(&row.id.to_variant()));
    let download_row = DownloadRowWidgets {
        root: root.clone(),
        status,
        action: download.clone(),
        marker,
    };
    update_download_state(&download_row, context.download_state);
    update_network_state(
        &download_row,
        context.download_state,
        context.network.connectivity,
        context.network.unavailable_now,
    );
    widgets.downloads.insert(row.id, download_row);
    skeleton.trailing.append(&download);

    let menu = podcasts_context_surface::episode_menu_button(
        row,
        context.selection,
        context.unavailable_episode,
        SELECT_ROW_ACTION,
    );
    skeleton.trailing.append(&menu);
    let reveal = Rc::new(crate::ui::source_row::Reveal::install(&root, &menu));
    reveal.set_selected(is_selected);
    let hover_media = media.clone();
    reveal.on_hover(move |hovered| hover_media.set_hovered(hovered));
    let focus_media = media.clone();
    root.connect_has_focus_notify(move |row| focus_media.set_focused(row.has_focus()));
    media.set_selection_mode(!context.selected_ids.is_empty());
    media.set_selected(is_selected);
    let checkbox_id = row.id;
    media.connect_toggled(move |checkbox| {
        let target = (
            checkbox_id,
            super::podcasts_selection::SelectMode::Toggle.as_u8(),
        )
            .to_variant();
        if let Err(error) = checkbox.activate_action(SELECT_ROW_ACTION, Some(&target)) {
            tracing::debug!(%error, checkbox_id, "selection checkbox did not reach the action");
        }
    });
    widgets.selection.insert(
        row.id,
        SelectionRowWidgets {
            row: root.clone(),
            media: Some(media),
            reveal: Some(reveal),
        },
    );
    root.upcast()
}

/// The one status chip a source row may carry.
pub(super) fn chip_spec(row: &EpisodeRow) -> Option<crate::ui::source_row::ChipSpec> {
    let pill = status_pill(row)?;
    let label = if pill.css_class == "reprise-podcast-status-resume" {
        strings::podcast_status_resume(crate::ui::source_row::resume_percent(
            row.position_ms,
            row.duration_secs,
        ))
    } else {
        strings::text(pill.label)
    };
    Some(crate::ui::source_row::ChipSpec {
        label,
        css_class: pill.css_class,
    })
}

pub(super) use super::podcasts_row_state::{update_download_state, update_network_state};

pub(super) fn update_playback_state(widgets: &DownloadRowWidgets, playing: bool) {
    playing_marker::set_playing(&widgets.marker, playing);
}

#[cfg(test)]
#[path = "podcasts_groups_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "podcasts_source_row_tests.rs"]
mod source_row_tests;
