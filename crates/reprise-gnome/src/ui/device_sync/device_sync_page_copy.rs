//! Pure presentation copy for the device page — split out of
//! `device_sync_page.rs` to keep that file under the project's 800-line
//! limit as the preparation surface grew its own progress and
//! button-label projections.
//!
//! The toolkit-free projections live in `reprise-view`. This adapter narrows
//! the GTK-owned `DeviceView`, formats local time, and renders
//! [`reprise_view::strings::Message`] values through gettext.

use chrono::{Datelike, TimeZone, Timelike};
use reprise_core::device_sync::{
    MirrorBlocker, SyncChangeSummary, SyncPageWarning, SyncPlaylistRow, TransferProfile,
};
use reprise_view::device_sync as projection;
use reprise_view::strings::Message;

use super::device_sync_runtime::{DeviceView, PlannedSyncPhase};

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

pub(super) fn eject_sensitive(device: &DeviceView) -> bool {
    projection::eject_sensitive(device.page.controls, device.connected, &device.sync_phase)
}

/// The selection summary counts tracks with the very plural forms the
/// per-row subtitle already uses. Passing bare `"unique tracks"` here would
/// hand gettext a template without a `{count}` placeholder, and the number
/// would be dropped on the floor.
pub(super) fn unique_tracks(count: usize) -> String {
    render(&projection::counted(
        count,
        projection::UNIQUE_TRACKS.0,
        projection::UNIQUE_TRACKS.1,
    ))
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
