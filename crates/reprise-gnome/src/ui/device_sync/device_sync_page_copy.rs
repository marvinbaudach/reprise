//! Pure presentation copy for the device page — split out of
//! `device_sync_page.rs` to keep that file under the project's 800-line
//! limit as the preparation surface (`MTP-43`) grew its own progress and
//! button-label projections.
//!
//! The toolkit-free projections live in `reprise-view`. This adapter narrows
//! the GTK-owned `DeviceView`, formats local time, and renders
//! [`reprise_view::strings::Message`] values through gettext.

use chrono::{Datelike, TimeZone, Timelike};
use reprise_core::device_sync::{
    MirrorBlocker, PrimaryAction, SyncChangeSummary, SyncPageControls, SyncPageWarning,
    SyncPlaylistRow, TransferProfile,
};
use reprise_view::device_sync as projection;
use reprise_view::strings::Message;

use super::device_sync_runtime::{DeviceView, PlannedSyncPhase, PreparationRunState};
use super::device_sync_strings;

pub(super) use projection::PageActionCopy;

fn borrowed<'a>(args: &'a [(&'static str, String)]) -> Vec<(&'static str, &'a str)> {
    args.iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect()
}

fn render(message: &Message) -> String {
    let template = match &message.plural {
        Some(plural) => crate::i18n::ngettext(
            message.id,
            plural.id,
            u32::try_from(plural.count).unwrap_or(u32::MAX),
        ),
        None => crate::i18n::gettext(message.id),
    };
    crate::i18n::format_message(&template, &borrowed(&message.args))
}

fn render_joined(messages: &[Message]) -> String {
    messages.iter().map(render).collect::<Vec<_>>().join(" · ")
}

pub(super) fn profile_label(profile: TransferProfile) -> &'static str {
    projection::profile_label(profile)
}

pub(super) fn playlist_subtitle(row: &SyncPlaylistRow) -> String {
    let last_sync = match row.last_synced_at {
        None => projection::VerifiedSyncTime::Never,
        Some(last_synced_at) => chrono::Local
            .timestamp_opt(last_synced_at, 0)
            .single()
            .map_or(projection::VerifiedSyncTime::Unavailable, |timestamp| {
                projection::VerifiedSyncTime::Formatted(format_local_date_time(&timestamp))
            }),
    };
    render_joined(&projection::playlist_subtitle(row, last_sync))
}

pub(super) fn device_last_sync_copy(device: &DeviceView) -> String {
    if device.sync_phase == PlannedSyncPhase::Finishing {
        return verification_summary(device);
    }
    let last_sync = device
        .last_sync
        .map(|timestamp| format_local_date_time(&timestamp.with_timezone(&chrono::Local)));
    render_joined(&projection::device_last_sync_copy(
        &device.sync_phase,
        last_sync,
        &device.session_state,
        device.size_on_device_bytes,
        device.verified_managed_track_count,
    ))
}

pub(super) fn format_local_date_time(timestamp: &chrono::DateTime<chrono::Local>) -> String {
    let format = crate::ui::date_format::current();
    let date = format.date.render(
        Some(timestamp.year()),
        Some(timestamp.month()),
        Some(timestamp.day()),
    );
    let time = format
        .clock
        .render(i64::from(timestamp.hour()), i64::from(timestamp.minute()));
    format!("{date} at {time}")
}

pub(super) fn change_summary(changes: &SyncChangeSummary) -> String {
    render_joined(&projection::change_summary(changes))
}

pub(super) fn verification_summary(device: &DeviceView) -> String {
    render(&projection::verification_summary(
        &device.sync_phase,
        device.last_sync.is_some(),
        device.verified_managed_track_count,
    ))
}

pub(super) fn blocker_summary(blockers: &[MirrorBlocker]) -> Option<String> {
    match projection::blocker_summary(blockers)? {
        projection::BlockerCopy::Standalone(message) => Some(render(&message)),
        projection::BlockerCopy::Reasons(reasons) => {
            Some(format!("Cannot synchronize: {}.", render_joined(&reasons)))
        }
    }
}

pub(super) fn warning_summary(warnings: &[SyncPageWarning]) -> Vec<String> {
    projection::warning_summary(warnings)
        .iter()
        .map(render)
        .collect()
}

pub(super) fn action_copy(controls: SyncPageControls, action: PrimaryAction) -> PageActionCopy {
    projection::action_copy(controls, action)
}

pub(super) fn eject_sensitive(device: &DeviceView) -> bool {
    projection::eject_sensitive(device.page.controls, device.connected, &device.sync_phase)
}

pub(super) fn counted(count: usize, singular: &'static str, plural: &'static str) -> String {
    render(&projection::counted(count, singular, plural))
}

pub(super) fn progress_copy(device: &DeviceView) -> Option<(String, String, String, f64)> {
    let preparation = match &device.preparation_run {
        PreparationRunState::Idle => None,
        PreparationRunState::Downloading {
            done,
            total,
            title,
            received_bytes,
            total_bytes,
        } => Some(projection::PreparationProgress {
            done: *done,
            total: *total,
            title: title.clone(),
            received_bytes: *received_bytes,
            total_bytes: *total_bytes,
        }),
    };
    if preparation.is_none() && !device.prepared_this_run {
        return transfer_progress_copy(&device.sync_phase, device.bytes_per_second);
    }
    projection::progress_copy(
        preparation.as_ref(),
        &device.sync_phase,
        device.bytes_per_second,
        device.prepared_this_run,
    )
    .map(render_progress)
}

pub(super) fn transfer_progress_copy(
    phase: &PlannedSyncPhase,
    bytes_per_second: u64,
) -> Option<(String, String, String, f64)> {
    projection::transfer_progress_copy(phase, bytes_per_second).map(render_progress)
}

fn render_progress(copy: projection::ProgressCopy) -> (String, String, String, f64) {
    let subtitle = match copy.subtitle {
        projection::ProgressSubtitle::Message(message) => render(&message),
        projection::ProgressSubtitle::CurrentTrack(title) => title,
    };
    let speed = match copy.speed {
        projection::ProgressSpeed::Unavailable => "—".into(),
        projection::ProgressSpeed::BytesPerSecond(bytes) => {
            format!("{}/s", device_sync_strings::file_size(bytes))
        }
    };
    (render(&copy.title), subtitle, speed, copy.fraction)
}

#[cfg(test)]
mod projection_tests {
    use reprise_view::strings::{Message, Plural};

    use super::projection;

    #[test]
    fn counted_copy_crosses_the_projection_seam_as_a_plural_message() {
        assert_eq!(
            projection::counted(2, "{count} entry", "{count} entries"),
            Message {
                id: "{count} entry",
                plural: Some(Plural {
                    id: "{count} entries",
                    count: 2,
                }),
                args: vec![("count", "2".into())],
            }
        );
    }
}
