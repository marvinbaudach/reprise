//! Result and preview rows for the podcast add dialog.

use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::db::Db;
use reprise_core::podcasts::discovery::Candidate;
use reprise_core::podcasts::PodcastKind;

use crate::ui::source_add_action;
use crate::ui::strings;

use super::add_dialog::OnAdded;
use super::add_dialog_results::{clear, search_result_markup};
use super::add_dialog_subscription::{baseline_for_import_choice, subscribe};

#[derive(Clone)]
pub(super) struct Preview {
    pub(super) kind: PodcastKind,
    pub(super) title: String,
    pub(super) author: Option<String>,
    pub(super) image_url: Option<String>,
    pub(super) count: usize,
    pub(super) url: String,
    pub(super) guids: Vec<String>,
}

#[derive(Clone)]
pub(super) struct CandidateRow {
    pub(super) root: gtk4::Widget,
    pub(super) subtitle: gtk4::Label,
}

pub(super) fn append_heading(parent: &gtk4::Box, text: &str) {
    let label = gtk4::Label::new(Some(text));
    label.add_css_class("caption");
    label.add_css_class("reprise-text-secondary");
    label.set_xalign(0.0);
    parent.append(&label);
}

pub(super) fn append_candidate(
    parent: &gtk4::Box,
    candidate: Candidate,
    query: Option<&str>,
    conn: &Rc<Db>,
    on_added: &OnAdded,
    auto_download_default: bool,
) -> CandidateRow {
    let handle = candidate_row(
        &candidate.title,
        &candidate.subtitle,
        candidate.author.as_deref(),
        query,
        candidate.kind,
        candidate.image_url.as_deref(),
        images_allowed(conn),
    );
    let row = handle
        .root
        .clone()
        .downcast::<gtk4::Box>()
        .expect("candidate rows are boxes");
    // SRC-7: the same compact action every discovery row uses.
    let title = candidate.title.clone();
    let button = source_add_action::add_button(source_add_action::AddActionKind::Subscribe, &title);
    let conn = conn.clone();
    let on_added = on_added.clone();
    button.connect_clicked(move |button| {
        let result = subscribe(&conn, &candidate, auto_download_default, None);
        match result {
            Ok(subscription_id) => {
                on_added(subscription_id, true);
                // SRC-5/SRC-7: acknowledge in place; only the next submitted
                // search drops the row.
                source_add_action::mark_added(
                    button,
                    source_add_action::AddActionKind::Subscribe,
                    &title,
                );
            }
            Err(error) => {
                tracing::warn!(%error, "could not subscribe to podcast");
                button.set_tooltip_text(Some(&strings::text(strings::PODCAST_SUBSCRIBE_FAILED)));
            }
        }
    });
    row.append(&button);
    parent.append(&handle.root);
    handle
}

pub(super) fn append_preview(
    parent: &gtk4::Box,
    preview: Preview,
    import_count: usize,
    auto_download_default: bool,
    conn: &Rc<Db>,
    on_added: &OnAdded,
) {
    clear(parent);
    let subtitle = strings::podcast_episode_count(preview.count);
    let row = candidate_row(
        &preview.title,
        &subtitle,
        None,
        None,
        preview.kind,
        preview.image_url.as_deref(),
        images_allowed(conn),
    );
    parent.append(&row.root);
    let import = gtk4::CheckButton::with_label(&strings::podcast_import_latest_count(import_count));
    import.set_active(true);
    parent.append(&import);
    let auto_download =
        gtk4::CheckButton::with_label(&strings::text(strings::PODCAST_AUTO_DOWNLOAD));
    auto_download.set_active(auto_download_default);
    parent.append(&auto_download);
    let subscribe_button = gtk4::Button::with_label(&strings::text(strings::PODCAST_SUBSCRIBE));
    subscribe_button.add_css_class("suggested-action");
    let candidate = Candidate {
        kind: preview.kind,
        title: preview.title,
        subtitle,
        author: preview.author,
        image_url: preview.image_url,
        url: preview.url,
        identity_guids: preview.guids.clone(),
        follower_count: None,
        channel_id: None,
        matching_video_count: None,
    };
    let conn = conn.clone();
    let on_added = on_added.clone();
    let preview_guids = preview.guids;
    let parent_weak = parent.downgrade();
    subscribe_button.connect_clicked(move |button| {
        let baseline = baseline_for_import_choice(import.is_active(), &preview_guids);
        let result = subscribe(
            &conn,
            &candidate,
            auto_download.is_active(),
            baseline.as_deref(),
        );
        match result {
            Ok(subscription_id) => {
                on_added(subscription_id, import.is_active());
                if let Some(parent) = parent_weak.upgrade() {
                    clear(&parent);
                }
            }
            Err(error) => {
                tracing::warn!(%error, "could not subscribe to podcast preview");
                button.set_tooltip_text(Some(&strings::text(strings::PODCAST_SUBSCRIBE_FAILED)));
            }
        }
    });
    parent.append(&subscribe_button);
}

/// `NET-1a` / `C1`: `online_sources::network_allowed(conn,
/// &modules::ARTWORK_MODULE)`, computed once by each caller of
/// [`candidate_row`] — this dialog never lets the widget read settings
/// itself.
pub(super) fn images_allowed(conn: &Db) -> bool {
    reprise_core::online_sources::network_allowed(conn, &reprise_core::modules::ARTWORK_MODULE)
        .unwrap_or(false)
}

pub(super) fn candidate_row(
    title: &str,
    subtitle: &str,
    author: Option<&str>,
    query: Option<&str>,
    kind: PodcastKind,
    image_url: Option<&str>,
    images_allowed: bool,
) -> CandidateRow {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    row.add_css_class("reprise-podcast-result");
    let image = super::source_image::SourceImage::new(
        image_url,
        match kind {
            PodcastKind::Rss => "audio-input-microphone-symbolic",
            PodcastKind::Youtube => "video-x-generic-symbolic",
        },
        40,
        images_allowed,
        reprise_core::remote_image::CacheScope::Transient,
    );
    row.append(image.widget());
    let labels = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    labels.set_hexpand(true);
    let title_line = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    title_line.set_hexpand(true);
    let title_label = gtk4::Label::new(Some(title));
    title_label.set_xalign(0.0);
    title_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    let subtitle_label = gtk4::Label::new(Some(subtitle));
    subtitle_label.add_css_class("caption");
    subtitle_label.add_css_class("reprise-text-secondary");
    subtitle_label.set_xalign(0.0);
    let unexplained_match = if query.is_some() {
        let palette = crate::ui::search_highlight::accent_palette(&title_label);
        let markup = search_result_markup(kind, title, subtitle, author, query, Some(&palette));
        title_label.set_markup(&markup.title);
        subtitle_label.set_markup(&markup.subtitle);
        markup.unexplained_match
    } else {
        false
    };
    title_line.append(&title_label);
    if unexplained_match {
        let explanation = strings::text(strings::PODCAST_SEARCH_MATCH_NOT_SHOWN);
        let marker = gtk4::Image::from_icon_name(crate::ui::icons::UNEXPLAINED_SEARCH_MATCH);
        marker.set_pixel_size(16);
        marker.set_valign(gtk4::Align::Center);
        marker.add_css_class("reprise-text-secondary");
        marker.set_tooltip_text(Some(&explanation));
        marker.update_property(&[gtk4::accessible::Property::Description(&explanation)]);
        title_line.append(&marker);
    }
    labels.append(&title_line);
    // SRC-8: the subtitle ellipsizes for the same reason the title does — a
    // long publisher name would otherwise raise the dialog's minimum width
    // and the window would change size between two searches.
    subtitle_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    labels.append(&subtitle_label);
    row.append(&labels);
    CandidateRow {
        root: row.upcast(),
        subtitle: subtitle_label,
    }
}
