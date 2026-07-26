use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::device_sync::DeviceSelection;

use super::super::device_sync_runtime::{
    DeviceSyncRuntime, DeviceView, PlannedSyncPhase, SyncStep,
};
use super::copy;
use crate::ui::device_sync_strings;

pub(super) fn device_header_group(
    device: &DeviceView,
    runtime: &Rc<DeviceSyncRuntime>,
) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    let last_sync = device.last_sync.map_or_else(
        || "Never synchronized".to_string(),
        |timestamp| format!("Last sync {}", timestamp.format("%Y-%m-%d %H:%M")),
    );
    let row = adw::ActionRow::builder()
        .title(&device.name)
        .subtitle(format!(
            "MTP · connected · {} · {}",
            last_sync,
            copy::available_space(device.storage.free_bytes)
        ))
        .build();
    let icon = gtk4::Image::from_gicon(&device.icon);
    icon.set_pixel_size(36);
    row.add_prefix(&icon);
    let ejecting_blocked = matches!(
        device.sync_phase,
        PlannedSyncPhase::Syncing { .. } | PlannedSyncPhase::Finishing
    );
    let eject = gtk4::Button::builder()
        .icon_name("media-eject-symbolic")
        .tooltip_text(device_sync_strings::eject_tooltip(ejecting_blocked))
        .valign(gtk4::Align::Center)
        .build();
    eject.set_sensitive(!ejecting_blocked);
    let id = device.id.clone();
    let runtime = runtime.clone();
    eject.connect_clicked(move |_| runtime.eject(&id));
    row.add_suffix(&eject);
    group.add(&row);
    group
}

pub(super) fn selection_group(
    device: &DeviceView,
    runtime: &Rc<DeviceSyncRuntime>,
) -> adw::PreferencesGroup {
    let selected_count = selected_track_count(device, runtime);
    let group = adw::PreferencesGroup::builder()
        .title("Selection")
        .description(format!("{selected_count} tracks selected"))
        .build();
    let entire = matches!(device.settings.selection, DeviceSelection::EntireLibrary);
    let entire_row = adw::SwitchRow::builder()
        .title("Entire library")
        .subtitle("Synchronize every available track")
        .active(entire)
        .build();
    let runtime_for_entire = runtime.clone();
    let settings_for_entire = device.settings.clone();
    entire_row.connect_active_notify(move |row| {
        let mut settings = settings_for_entire.clone();
        settings.selection = if row.is_active() {
            DeviceSelection::EntireLibrary
        } else {
            DeviceSelection::Sources(Vec::new())
        };
        if let Err(error) = runtime_for_entire.update_settings(settings) {
            tracing::warn!(%error, "could not update entire-library selection");
        }
    });
    group.add(&entire_row);

    let selected = match &device.settings.selection {
        DeviceSelection::Sources(sources) => sources.clone(),
        DeviceSelection::EntireLibrary => Vec::new(),
    };
    match runtime.selection_options() {
        Ok(options) => {
            for option in options {
                let subtitle = if option.smart {
                    format!("{} tracks · smart playlist snapshot", option.track_count)
                } else {
                    format!("{} tracks", option.track_count)
                };
                let row = selection_option_row(
                    &option.name,
                    &subtitle,
                    selected.contains(&option.source),
                    !entire,
                );
                let runtime = runtime.clone();
                let settings_template = device.settings.clone();
                let source = option.source;
                row.connect_active_notify(move |row| {
                    let mut settings = settings_template.clone();
                    let DeviceSelection::Sources(sources) = &mut settings.selection else {
                        return;
                    };
                    if row.is_active() {
                        if !sources.contains(&source) {
                            sources.push(source.clone());
                        }
                    } else {
                        sources.retain(|candidate| candidate != &source);
                    }
                    if let Err(error) = runtime.update_settings(settings.clone()) {
                        tracing::warn!(%error, "could not update device playlist selection");
                    }
                });
                group.add(&row);
            }
        }
        Err(error) => group.set_description(Some(&format!("Could not load playlists: {error}"))),
    }
    group
}

fn selection_option_row(
    title: &str,
    subtitle: &str,
    active: bool,
    sensitive: bool,
) -> adw::SwitchRow {
    let text = selection_option_text(title, subtitle);
    adw::SwitchRow::builder()
        .title(text.title)
        .subtitle(text.subtitle)
        .use_markup(text.uses_markup)
        .active(active)
        .sensitive(sensitive)
        .build()
}

#[derive(Debug, PartialEq, Eq)]
struct SelectionOptionText<'a> {
    title: &'a str,
    subtitle: &'a str,
    uses_markup: bool,
}

fn selection_option_text<'a>(title: &'a str, subtitle: &'a str) -> SelectionOptionText<'a> {
    SelectionOptionText {
        title,
        subtitle,
        uses_markup: false,
    }
}

pub(super) fn delta_group(device: &DeviceView) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder().title("Next Sync").build();
    let (title, subtitle, fraction) = delta_copy(device);
    let row = adw::ActionRow::builder()
        .title(title)
        .subtitle(subtitle)
        .build();
    let button = gtk4::Button::with_label(
        if matches!(
            device.sync_phase,
            PlannedSyncPhase::Syncing { .. } | PlannedSyncPhase::Finishing
        ) {
            "Cancel"
        } else {
            "Sync now"
        },
    );
    button.add_css_class("suggested-action");
    button.set_valign(gtk4::Align::Center);
    let has_delta = device
        .delta
        .as_ref()
        .is_some_and(|delta| !delta.to_copy.is_empty() || !delta.to_remove.is_empty());
    button
        .set_sensitive(has_delta || matches!(device.sync_phase, PlannedSyncPhase::Syncing { .. }));
    button.set_action_name(Some("app.sync-device"));
    button.set_action_target_value(Some(&device.id.to_variant()));
    row.add_suffix(&button);
    group.add(&row);
    if fraction > 0.0 {
        let bar = gtk4::ProgressBar::new();
        bar.set_fraction(fraction);
        bar.set_show_text(false);
        group.add(&bar);
    }
    if let Some(error) = &device.sync_error {
        let title = if error.message.starts_with("sync needs ") {
            "Device full"
        } else {
            "Synchronization finished with errors"
        };
        let error_row = adw::ActionRow::builder()
            .title(title)
            .subtitle(&error.message)
            .build();
        error_row.add_css_class("error");
        group.add(&error_row);
    }
    group
}

pub(super) fn settings_group(
    device: &DeviceView,
    runtime: &Rc<DeviceSyncRuntime>,
) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder()
        .title("Sync Settings")
        .build();
    let ratings = adw::SwitchRow::builder()
        .title("Sync ratings and play counts back")
        .subtitle("Requires the Reprise Android companion app")
        .active(false)
        .sensitive(false)
        .build();
    group.add(&ratings);

    let bitrates = reprise_core::device_sync::settings::SUPPORTED_OPUS_BITRATES;
    let labels = gtk4::StringList::new(&[
        "Do not convert",
        "64 kbit/s",
        "96 kbit/s",
        "128 kbit/s",
        "160 kbit/s",
        "192 kbit/s",
        "256 kbit/s",
    ]);
    let bitrate = adw::ComboRow::builder()
        .title("Convert lossless tracks to Opus")
        .model(&labels)
        .selected(
            bitrates
                .iter()
                .position(|value| *value == device.settings.opus_bitrate)
                .unwrap_or(0) as u32,
        )
        .build();
    let runtime_for_bitrate = runtime.clone();
    let settings_for_bitrate = device.settings.clone();
    bitrate.connect_selected_notify(move |row| {
        let mut settings = settings_for_bitrate.clone();
        settings.opus_bitrate = bitrates.get(row.selected() as usize).copied().unwrap_or(0);
        if let Err(error) = runtime_for_bitrate.update_settings(settings) {
            tracing::warn!(%error, "could not update Opus bitrate");
        }
    });
    group.add(&bitrate);

    let remove = adw::SwitchRow::builder()
        .title("Remove unselected tracks from device")
        .subtitle("Pinned tracks are always kept")
        .active(device.settings.remove_deleted)
        .build();
    let runtime_for_remove = runtime.clone();
    let settings_for_remove = device.settings.clone();
    remove.connect_active_notify(move |row| {
        let mut settings = settings_for_remove.clone();
        settings.remove_deleted = row.is_active();
        if let Err(error) = runtime_for_remove.update_settings(settings) {
            tracing::warn!(%error, "could not update removal preference");
        }
    });
    group.add(&remove);
    group
}

fn selected_track_count(device: &DeviceView, runtime: &DeviceSyncRuntime) -> usize {
    match &device.settings.selection {
        DeviceSelection::EntireLibrary => device.selected_track_count,
        DeviceSelection::Sources(sources) => runtime
            .selection_options()
            .unwrap_or_default()
            .into_iter()
            .filter(|option| sources.contains(&option.source))
            .map(|option| option.track_count)
            .sum(),
    }
}

fn delta_copy(device: &DeviceView) -> (String, String, f64) {
    match &device.sync_phase {
        PlannedSyncPhase::ComputingDelta => (
            "Checking device…".into(),
            "Computing the next synchronization".into(),
            0.0,
        ),
        PlannedSyncPhase::Syncing {
            step,
            done,
            total,
            current_track,
            bytes_done,
            bytes_total,
        } => {
            let step = match step {
                SyncStep::Removing => "Removing",
                SyncStep::Transcoding => "Transcoding",
                SyncStep::Copying => "Copying",
                SyncStep::WritingPlaylists => "Writing playlists",
            };
            let fraction = if *bytes_total == 0 {
                0.0
            } else {
                (*bytes_done as f64 / *bytes_total as f64).clamp(0.0, 1.0)
            };
            (
                format!("{step} · {done} of {total}"),
                current_track.clone(),
                fraction,
            )
        }
        PlannedSyncPhase::Finishing => (
            "Finishing synchronization…".into(),
            "Updating the device inventory".into(),
            1.0,
        ),
        // Fresh device with nothing selected: prompt to choose rather than
        // claim it is already in sync (empty selection → empty delta).
        PlannedSyncPhase::Idle
            if matches!(
                &device.settings.selection,
                DeviceSelection::Sources(sources) if sources.is_empty()
            ) =>
        {
            (
                "Nothing selected to sync yet".into(),
                "Tick a playlist or Entire library above to get started.".into(),
                0.0,
            )
        }
        PlannedSyncPhase::Idle => device.delta.as_ref().map_or_else(
            || ("Ready to synchronize".into(), String::new(), 0.0),
            |delta| {
                if delta.to_copy.is_empty() && delta.to_remove.is_empty() {
                    ("Everything in sync ✓".into(), String::new(), 0.0)
                } else {
                    (
                        format!(
                            "Next sync: +{} tracks · −{} removed",
                            delta.to_copy.len(),
                            delta.to_remove.len()
                        ),
                        format!(
                            "{} will be copied · about {} s via USB",
                            copy::file_size(delta.bytes),
                            delta.est_secs
                        ),
                        0.0,
                    )
                }
            },
        ),
    }
}

#[cfg(test)]
mod tests {
    use libadwaita::prelude::*;

    use super::*;

    #[test]
    fn playlist_titles_are_not_interpreted_as_markup() {
        assert_eq!(
            selection_option_text("Lorna Shore & Similar…", "100 tracks"),
            SelectionOptionText {
                title: "Lorna Shore & Similar…",
                subtitle: "100 tracks",
                uses_markup: false,
            }
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn playlist_titles_with_markup_characters_render_as_plain_text() {
        gtk4::init().unwrap();

        let row = selection_option_row("Lorna Shore & Similar…", "100 tracks", false, true);

        assert_eq!(row.title(), "Lorna Shore & Similar…");
        assert!(
            !row.uses_markup(),
            "user-provided playlist names must not be parsed as Pango markup"
        );
    }
}
