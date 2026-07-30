//! Shared device-content picker used by playlists, YouTube, and podcasts.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::connectivity::LocalAvailability;
use reprise_core::device_sync::{
    select_episodes, summarize_picker_selection, EpisodeSelectionCandidate, EpisodeSelectionRule,
    PickerSelectionItem, SyncTargetKind, EVERYTHING_SOURCE,
};

use super::device_sync_runtime::{
    DeviceSyncRuntime, PickerEpisodeGroup, PickerSave, PickerSnapshot,
};
use super::device_sync_strings;

struct PickerDraft {
    original: PickerSnapshot,
    current: PickerSnapshot,
    filter: String,
}

type RefreshFn = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

pub(super) fn present(
    parent: &impl IsA<gtk4::Widget>,
    runtime: &Rc<DeviceSyncRuntime>,
    device_id: &str,
    kind: SyncTargetKind,
) {
    let Ok(snapshot) = runtime.picker_snapshot(device_id, kind) else {
        return;
    };
    let draft = Rc::new(RefCell::new(PickerDraft {
        original: snapshot.clone(),
        current: snapshot,
        filter: String::new(),
    }));

    let rule_box = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    rule_box.add_css_class("card");
    rule_box.set_margin_top(6);
    if kind != SyncTargetKind::Playlists {
        let rule_label = picker_label(&rule_text(kind));
        rule_label.add_css_class("heading");
        rule_box.append(&rule_label);
    }
    let rule_control = build_rule_control(kind, &draft);
    if let Some(control) = &rule_control {
        rule_box.append(control);
    }
    if kind == SyncTargetKind::PodcastEpisodes {
        rule_box.append(&picker_label(&device_sync_strings::text(
            device_sync_strings::PODCAST_REMOVAL_NOTE,
        )));
    }

    let filter = gtk4::SearchEntry::new();
    filter.set_placeholder_text(Some(&device_sync_strings::text(
        device_sync_strings::FILTER_SYNC_CONTENT,
    )));
    filter.set_hexpand(true);
    let select_all =
        gtk4::Button::with_label(&device_sync_strings::text(device_sync_strings::SELECT_ALL));
    let filter_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    filter_row.append(&filter);
    filter_row.append(&select_all);

    let list = gtk4::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk4::SelectionMode::None);
    let scroller = gtk4::ScrolledWindow::builder()
        .child(&list)
        .min_content_height(300)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vexpand(true)
        .build();

    let footer_label = picker_label("");
    let preparation_link = gtk4::Button::with_label(&device_sync_strings::text(
        device_sync_strings::PREPARATION_LINK,
    ));
    preparation_link.add_css_class("flat");
    preparation_link.set_halign(gtk4::Align::Start);
    preparation_link.set_tooltip_text(Some(&device_sync_strings::text(
        device_sync_strings::SHOW_PREPARATION_PHASE,
    )));
    let error_label = picker_label("");
    error_label.add_css_class("error");
    error_label.set_visible(false);
    let cancel = gtk4::Button::with_label(&device_sync_strings::text(device_sync_strings::CANCEL));
    let save = gtk4::Button::with_label(&device_sync_strings::text(device_sync_strings::SAVE));
    save.add_css_class("suggested-action");
    let footer = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let footer_copy = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    footer_copy.set_hexpand(true);
    footer_copy.append(&footer_label);
    footer_copy.append(&preparation_link);
    footer.append(&footer_copy);
    footer.append(&cancel);
    footer.append(&save);

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(16);
    content.set_margin_end(16);
    content.append(&rule_box);
    content.append(&filter_row);
    content.append(&scroller);
    content.append(&error_label);
    content.append(&footer);

    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new(
        &device_sync_strings::choose_category(device_sync_strings::category_name(kind)),
        "",
    )));
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&content));
    let dialog = adw::Dialog::builder()
        .child(&toolbar)
        .content_width(620)
        .content_height(680)
        .build();

    let refresh: RefreshFn = Rc::new(RefCell::new(None));
    let refresh_impl: Rc<dyn Fn()> = {
        let draft = draft.clone();
        let list = list.clone();
        let footer_label = footer_label.clone();
        let preparation_link = preparation_link.clone();
        let refresh = refresh.clone();
        Rc::new(move || {
            rebuild_list(&list, &draft, &refresh);
            let snapshot = draft.borrow().current.clone();
            update_footer(&footer_label, &preparation_link, &snapshot);
        })
    };
    *refresh.borrow_mut() = Some(refresh_impl.clone());

    {
        let draft = draft.clone();
        let refresh = refresh.clone();
        filter.connect_search_changed(move |entry| {
            draft.borrow_mut().filter = entry.text().to_string();
            call_refresh(&refresh);
        });
    }
    {
        let draft = draft.clone();
        let refresh = refresh.clone();
        select_all.connect_clicked(move |_| {
            select_all_rows(&mut draft.borrow_mut().current);
            refresh_episode_selection(&mut draft.borrow_mut().current);
            call_refresh(&refresh);
        });
    }
    if let Some(control) = rule_control {
        connect_rule_control(kind, &control, &draft, &refresh);
    }
    {
        let dialog = dialog.downgrade();
        cancel.connect_clicked(move |_| {
            if let Some(dialog) = dialog.upgrade() {
                dialog.close();
            }
        });
    }
    {
        let dialog = dialog.downgrade();
        preparation_link.connect_clicked(move |_| {
            if let Some(dialog) = dialog.upgrade() {
                dialog.close();
            }
        });
    }
    {
        let runtime = runtime.clone();
        let device_id = device_id.to_string();
        let draft = draft.clone();
        let dialog = dialog.downgrade();
        let error_label = error_label.clone();
        save.connect_clicked(move |_| {
            let changes = picker_changes(&draft.borrow());
            match runtime.save_picker(&device_id, changes) {
                Ok(()) => {
                    if let Some(dialog) = dialog.upgrade() {
                        dialog.close();
                    }
                }
                Err(error) => {
                    error_label.set_label(&error);
                    error_label.set_visible(true);
                }
            }
        });
    }

    refresh_impl();
    dialog.present(Some(parent));
}

fn build_rule_control(
    kind: SyncTargetKind,
    draft: &Rc<RefCell<PickerDraft>>,
) -> Option<gtk4::Widget> {
    match kind {
        SyncTargetKind::Playlists => {
            let toggle = gtk4::Switch::new();
            let active = match &draft.borrow().current {
                PickerSnapshot::Playlists {
                    keep_smart_updated, ..
                } => *keep_smart_updated,
                PickerSnapshot::Episodes { .. } => true,
            };
            toggle.set_active(active);
            let label = picker_label(&device_sync_strings::text(
                device_sync_strings::KEEP_SMART_PLAYLISTS_UPDATED,
            ));
            label.set_hexpand(true);
            let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
            row.append(&label);
            row.append(&toggle);
            Some(row.upcast())
        }
        SyncTargetKind::YoutubeAudio => {
            let latest = match &draft.borrow().current {
                PickerSnapshot::Episodes {
                    latest_per_group, ..
                } => *latest_per_group,
                PickerSnapshot::Playlists { .. } => 5,
            };
            let spin = gtk4::SpinButton::with_range(0.0, 100.0, 1.0);
            spin.set_value(latest as f64);
            spin.update_property(&[gtk4::accessible::Property::Label(
                &device_sync_strings::text(device_sync_strings::LATEST_EPISODES_PER_CHANNEL),
            )]);
            Some(spin.upcast())
        }
        SyncTargetKind::PodcastEpisodes => None,
    }
}

fn connect_rule_control(
    kind: SyncTargetKind,
    control: &gtk4::Widget,
    draft: &Rc<RefCell<PickerDraft>>,
    refresh: &RefreshFn,
) {
    match kind {
        SyncTargetKind::Playlists => {
            let Some(toggle) = control
                .last_child()
                .and_then(|child| child.downcast::<gtk4::Switch>().ok())
            else {
                return;
            };
            let draft = draft.clone();
            toggle.connect_state_set(move |_, active| {
                if let PickerSnapshot::Playlists {
                    keep_smart_updated, ..
                } = &mut draft.borrow_mut().current
                {
                    *keep_smart_updated = active;
                }
                gtk4::glib::Propagation::Proceed
            });
        }
        SyncTargetKind::YoutubeAudio => {
            let Some(spin) = control.downcast_ref::<gtk4::SpinButton>() else {
                return;
            };
            let draft = draft.clone();
            let refresh = refresh.clone();
            spin.connect_value_changed(move |spin| {
                if let PickerSnapshot::Episodes {
                    latest_per_group, ..
                } = &mut draft.borrow_mut().current
                {
                    *latest_per_group = spin.value() as usize;
                }
                refresh_episode_selection(&mut draft.borrow_mut().current);
                call_refresh(&refresh);
            });
        }
        SyncTargetKind::PodcastEpisodes => {}
    }
}

fn rebuild_list(list: &gtk4::ListBox, draft: &Rc<RefCell<PickerDraft>>, refresh: &RefreshFn) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    let (snapshot, filter) = {
        let draft_value = draft.borrow();
        (
            draft_value.current.clone(),
            draft_value.filter.to_lowercase(),
        )
    };
    match &snapshot {
        PickerSnapshot::Playlists { rows, .. } => {
            for (index, row) in rows
                .iter()
                .enumerate()
                .filter(|(_, row)| filter.is_empty() || row.name.to_lowercase().contains(&filter))
            {
                list.append(&playlist_row(index, row, draft, refresh));
            }
        }
        PickerSnapshot::Episodes { kind, groups, .. } => {
            for (group_index, group) in groups.iter().enumerate().filter(|(_, group)| {
                filter.is_empty()
                    || group.name.to_lowercase().contains(&filter)
                    || group
                        .episodes
                        .iter()
                        .any(|episode| episode.title.to_lowercase().contains(&filter))
            }) {
                list.append(&episode_group_row(
                    group_index,
                    group,
                    *kind,
                    &filter,
                    draft,
                    refresh,
                ));
            }
        }
    }
}

fn playlist_row(
    index: usize,
    row: &super::device_sync_runtime::PickerPlaylistRow,
    draft: &Rc<RefCell<PickerDraft>>,
    refresh: &RefreshFn,
) -> gtk4::Box {
    let name = if row.source == EVERYTHING_SOURCE {
        device_sync_strings::text(device_sync_strings::EVERYTHING)
    } else {
        row.name.clone()
    };
    let check = gtk4::CheckButton::with_label(&name);
    check.set_active(row.selected);
    let subtitle = if row.smart {
        format!(
            "{} · {} · {}",
            device_sync_strings::text(device_sync_strings::SMART_PLAYLIST),
            device_sync_strings::picker_content(row.track_count, true),
            device_sync_strings::file_size(row.size_bytes)
        )
    } else {
        format!(
            "{} · {}",
            device_sync_strings::picker_content(row.track_count, true),
            device_sync_strings::file_size(row.size_bytes)
        )
    };
    let copy = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    copy.set_hexpand(true);
    copy.append(&check);
    copy.append(&picker_label(&subtitle));
    let draft = draft.clone();
    let refresh = refresh.clone();
    check.connect_toggled(move |check| {
        set_playlist_row_selected(&mut draft.borrow_mut().current, index, check.is_active());
        call_refresh(&refresh);
    });
    copy
}

fn episode_group_row(
    group_index: usize,
    group: &PickerEpisodeGroup,
    kind: SyncTargetKind,
    filter: &str,
    draft: &Rc<RefCell<PickerDraft>>,
    refresh: &RefreshFn,
) -> gtk4::Box {
    let selected = group
        .episodes
        .iter()
        .filter(|episode| episode.selected)
        .count();
    let group_check = gtk4::CheckButton::with_label(&group.name);
    group_check.set_active(group.enabled);
    let counter = picker_label(&device_sync_strings::group_counter(
        selected,
        group.episodes.len(),
    ));
    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    group_check.set_hexpand(true);
    header.append(&group_check);
    header.append(&counter);
    let rows = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    rows.append(&header);
    for (episode_index, episode) in group.episodes.iter().enumerate().filter(|(_, episode)| {
        filter.is_empty()
            || group.name.to_lowercase().contains(filter)
            || episode.title.to_lowercase().contains(filter)
    }) {
        rows.append(&episode_row(
            group_index,
            episode_index,
            episode,
            kind,
            draft,
            refresh,
        ));
    }
    let draft = draft.clone();
    let refresh = refresh.clone();
    group_check.connect_toggled(move |check| {
        if let PickerSnapshot::Episodes { groups, .. } = &mut draft.borrow_mut().current {
            groups[group_index].enabled = check.is_active();
        }
        refresh_episode_selection(&mut draft.borrow_mut().current);
        call_refresh(&refresh);
    });
    rows
}

fn episode_row(
    group_index: usize,
    episode_index: usize,
    episode: &super::device_sync_runtime::PickerEpisodeRow,
    kind: SyncTargetKind,
    draft: &Rc<RefCell<PickerDraft>>,
    refresh: &RefreshFn,
) -> gtk4::Box {
    let check = gtk4::CheckButton::with_label(&episode.title);
    check.set_active(episode.selected);
    let played_podcast = kind == SyncTargetKind::PodcastEpisodes && episode.played;
    check.set_inconsistent(episode.selected && !episode.pinned);
    check.set_sensitive(!played_podcast);
    if played_podcast {
        check.set_tooltip_text(Some(&device_sync_strings::text(
            device_sync_strings::PODCAST_REMOVAL_NOTE,
        )));
    } else if episode.selected && !episode.pinned {
        check.set_tooltip_text(Some(&device_sync_strings::text(
            device_sync_strings::SELECTED_BY_RULE,
        )));
    }
    let mut parts = Vec::new();
    if let Some(timestamp) = episode.published_at {
        if let Some(date) = chrono::DateTime::from_timestamp(timestamp, 0) {
            parts.push(date.format("%Y-%m-%d").to_string());
        }
    }
    if let Some(duration) = episode.duration_secs.filter(|duration| *duration > 0) {
        parts.push(device_sync_strings::duration_minutes(duration));
    }
    if episode.position_ms > 0 {
        parts.push(device_sync_strings::resume_minutes(episode.position_ms));
    }
    parts.push(device_sync_strings::text(if episode.downloaded {
        device_sync_strings::ON_DISK
    } else {
        device_sync_strings::NEEDS_DOWNLOAD
    }));
    if let Some(size) = episode.size_bytes {
        parts.push(device_sync_strings::file_size(size));
    }
    let copy = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    copy.set_margin_start(24);
    copy.append(&check);
    let subtitle = picker_label(&parts.join(" · "));
    if !episode.downloaded {
        subtitle.add_css_class("warning");
    }
    copy.append(&subtitle);
    let draft = draft.clone();
    let refresh = refresh.clone();
    check.connect_toggled(move |check| {
        if let PickerSnapshot::Episodes { groups, .. } = &mut draft.borrow_mut().current {
            let group = &mut groups[group_index];
            group.enabled |= check.is_active();
            let episode = &mut group.episodes[episode_index];
            episode.pinned =
                explicit_pin_after_toggle(episode.selected, episode.pinned, check.is_active());
        }
        refresh_episode_selection(&mut draft.borrow_mut().current);
        call_refresh(&refresh);
    });
    copy
}

fn explicit_pin_after_toggle(selected: bool, pinned: bool, toggle_active: bool) -> bool {
    (selected && !pinned) || toggle_active
}

fn set_playlist_row_selected(snapshot: &mut PickerSnapshot, index: usize, selected: bool) {
    let PickerSnapshot::Playlists { rows, .. } = snapshot else {
        return;
    };
    let selecting_everything = rows
        .get(index)
        .is_some_and(|row| row.source == EVERYTHING_SOURCE && selected);
    if selecting_everything {
        for row in rows.iter_mut() {
            row.selected = false;
        }
    } else if selected {
        if let Some(everything) = rows.iter_mut().find(|row| row.source == EVERYTHING_SOURCE) {
            everything.selected = false;
        }
    }
    if let Some(row) = rows.get_mut(index) {
        row.selected = selected;
    }
}

fn refresh_episode_selection(snapshot: &mut PickerSnapshot) {
    let PickerSnapshot::Episodes {
        kind,
        latest_per_group,
        groups,
    } = snapshot
    else {
        return;
    };
    let candidates = groups
        .iter()
        .flat_map(|group| {
            group
                .episodes
                .iter()
                .filter(|episode| episode.downloaded || episode.pinned)
                .map(|episode| EpisodeSelectionCandidate {
                    episode_id: episode.id,
                    group_id: group.id,
                    published_at: episode.published_at.unwrap_or_default(),
                    played: episode.played,
                    local: if episode.downloaded {
                        LocalAvailability::Available
                    } else {
                        LocalAvailability::Missing
                    },
                    pinned: episode.pinned,
                })
        })
        .collect::<Vec<_>>();
    let enabled = groups
        .iter()
        .filter(|group| group.enabled)
        .map(|group| group.id)
        .collect::<HashSet<_>>();
    let rule = match kind {
        SyncTargetKind::YoutubeAudio => EpisodeSelectionRule::LatestPerChannel {
            channel_latest: groups
                .iter()
                .filter(|group| enabled.contains(&group.id))
                .map(|group| (group.id, group.latest_override.unwrap_or(*latest_per_group)))
                .collect::<HashMap<_, _>>(),
        },
        SyncTargetKind::PodcastEpisodes => EpisodeSelectionRule::UnplayedDownloadsOnly {
            enabled_shows: enabled,
        },
        SyncTargetKind::Playlists => return,
    };
    let result = select_episodes(&candidates, &rule);
    let selected = result
        .ready
        .into_iter()
        .chain(result.waiting)
        .collect::<HashSet<_>>();
    for episode in groups
        .iter_mut()
        .flat_map(|group| group.episodes.iter_mut())
    {
        episode.selected = selected.contains(&episode.id);
    }
}

fn select_all_rows(snapshot: &mut PickerSnapshot) {
    match snapshot {
        PickerSnapshot::Playlists { rows, .. } => {
            if let Some(everything) = rows.iter_mut().find(|row| row.source == EVERYTHING_SOURCE) {
                everything.selected = true;
            }
            for row in rows
                .iter_mut()
                .filter(|row| row.source != EVERYTHING_SOURCE)
            {
                row.selected = false;
            }
        }
        PickerSnapshot::Episodes { kind, groups, .. } => {
            for group in groups {
                group.enabled = true;
                for episode in &mut group.episodes {
                    if *kind == SyncTargetKind::YoutubeAudio || !episode.played {
                        episode.pinned = true;
                    }
                }
            }
        }
    }
}

fn update_footer(label: &gtk4::Label, link: &gtk4::Button, snapshot: &PickerSnapshot) {
    let (items, tracks) = match snapshot {
        PickerSnapshot::Playlists { rows, .. } => (
            rows.iter()
                .map(|row| PickerSelectionItem {
                    selected: row.selected,
                    content_count: row.track_count,
                    size_bytes: Some(row.size_bytes),
                    needs_download: false,
                })
                .collect::<Vec<_>>(),
            true,
        ),
        PickerSnapshot::Episodes { groups, .. } => (
            groups
                .iter()
                .flat_map(|group| group.episodes.iter())
                .map(|episode| PickerSelectionItem {
                    selected: episode.selected,
                    content_count: 1,
                    size_bytes: episode.size_bytes,
                    needs_download: episode.selected && !episode.downloaded,
                })
                .collect::<Vec<_>>(),
            false,
        ),
    };
    let summary = summarize_picker_selection(&items);
    let content = device_sync_strings::picker_content(summary.content_count, tracks);
    let mut size = device_sync_strings::file_size(summary.known_size_bytes);
    if summary.unknown_size_items > 0 {
        size.push(' ');
        size.push_str(&device_sync_strings::unknown_sizes(
            summary.unknown_size_items,
        ));
    }
    label.set_label(&device_sync_strings::picker_footer(
        summary.selected_items,
        &content,
        &size,
    ));
    link.set_label(&device_sync_strings::picker_needs_download(
        summary.needs_download,
    ));
    link.set_visible(summary.needs_download > 0);
}

fn picker_changes(draft: &PickerDraft) -> PickerSave {
    let mut changes = PickerSave::default();
    match (&draft.original, &draft.current) {
        (
            PickerSnapshot::Playlists {
                rows: original,
                keep_smart_updated: original_keep,
            },
            PickerSnapshot::Playlists {
                rows,
                keep_smart_updated,
            },
        ) => {
            for row in rows {
                let was_selected = original
                    .iter()
                    .find(|candidate| candidate.source == row.source)
                    .is_some_and(|candidate| candidate.selected);
                if was_selected != row.selected {
                    changes
                        .playlist_changes
                        .push((row.source.clone(), row.selected));
                }
            }
            if original_keep != keep_smart_updated {
                changes.keep_smart_updated = Some(*keep_smart_updated);
            }
        }
        (
            PickerSnapshot::Episodes {
                latest_per_group: original_latest,
                groups: original,
                ..
            },
            PickerSnapshot::Episodes {
                kind,
                latest_per_group,
                groups,
            },
        ) => {
            for group in groups {
                if let Some(previous) = original.iter().find(|candidate| candidate.id == group.id) {
                    if previous.enabled != group.enabled {
                        changes.group_changes.push((group.id, group.enabled));
                    }
                    for episode in &group.episodes {
                        if previous
                            .episodes
                            .iter()
                            .find(|candidate| candidate.id == episode.id)
                            .is_some_and(|candidate| candidate.pinned != episode.pinned)
                        {
                            changes
                                .episode_pin_changes
                                .push((episode.id, episode.pinned));
                        }
                    }
                }
            }
            if kind == &SyncTargetKind::YoutubeAudio && original_latest != latest_per_group {
                changes.latest_per_channel = Some(*latest_per_group);
            }
        }
        _ => {}
    }
    changes
}

fn rule_text(kind: SyncTargetKind) -> String {
    device_sync_strings::text(match kind {
        SyncTargetKind::Playlists => device_sync_strings::KEEP_SMART_PLAYLISTS_UPDATED,
        SyncTargetKind::YoutubeAudio => device_sync_strings::YOUTUBE_PICKER_RULE,
        SyncTargetKind::PodcastEpisodes => device_sync_strings::PODCAST_PICKER_RULE,
    })
}

fn picker_label(text: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(text));
    label.set_xalign(0.0);
    label.set_wrap(true);
    label
}

fn call_refresh(refresh: &RefreshFn) {
    let callback = refresh.borrow().as_ref().cloned();
    if let Some(callback) = callback {
        callback();
    }
}

#[cfg(test)]
#[path = "device_sync_picker_unit_tests.rs"]
mod tests;
