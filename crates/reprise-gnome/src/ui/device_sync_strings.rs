//! Translatable copy and compact formatting for Android synchronization.

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

pub fn text(message: &str) -> String {
    crate::i18n::gettext(message)
}

fn formatted(message: &str, values: &[(&str, &str)]) -> String {
    crate::i18n::format_message(&text(message), values)
}

fn plural(singular: &str, plural: &str, count: usize, values: &[(&str, &str)]) -> String {
    let count = u32::try_from(count).unwrap_or(u32::MAX);
    crate::i18n::format_message(&crate::i18n::ngettext(singular, plural, count), values)
}

pub const SYNCHRONIZATION: &str = N_!("Synchronization");
pub const CONNECTED_DEVICES: &str = N_!("Connected Android Devices");
pub const NO_DEVICE: &str = N_!("No Android Device Connected");
pub const NO_DEVICE_DESCRIPTION: &str =
    N_!("Connect your unlocked Android device by USB and select File transfer in its USB options.");
pub const CONNECTED: &str = N_!("Connected");
pub const DISCONNECTED: &str = N_!("Disconnected");
pub const SCANNING_DEVICE: &str = N_!("Reading music on device…");
pub const SCAN_FAILED: &str = N_!("Could not read music on this device");
pub const SPACE_UNKNOWN: &str = N_!("Available space unknown");
pub const DEVICE_MUSIC: &str = N_!("Music on Device");
pub const PHONE_PLAYLISTS: &str = N_!("Phone Playlists");
pub const NO_DEVICE_MUSIC: &str = N_!("No music was found in the device Music folder.");
pub const NO_PHONE_PLAYLISTS: &str = N_!("Create a phone playlist, then drag tracks onto it.");
pub const PLAYLIST_DRAFT: &str = N_!("Waiting for tracks");
pub const NEW_PHONE_PLAYLIST: &str = N_!("New Phone Playlist");
pub const PLAYLIST_NAME: &str = N_!("Playlist name");
pub const CREATE: &str = N_!("Create");
pub const REFRESH_DEVICE: &str = N_!("Refresh Device Music");
pub const SYNC_PROGRESS: &str = N_!("Synchronization Progress");
pub const CANCEL_CURRENT: &str = N_!("Cancel Current Copy");
pub const PREPARING: &str = N_!("Preparing copy…");
pub const PAUSED_DISCONNECTED: &str = N_!("Paused — reconnect the device to continue");
pub const CANCELLING: &str = N_!("Cancelling current copy…");
pub const COMPLETE: &str = N_!("Synchronization complete");
pub const FAILED: &str = N_!("Synchronization failed");
pub const IDLE: &str = N_!("Ready to synchronize");
pub const FILE_PROGRESS: &str = N_!("Current file");
pub const TOTAL_PROGRESS: &str = N_!("Overall progress");

pub fn available_space(bytes: Option<u64>) -> String {
    bytes.map_or_else(
        || text(SPACE_UNKNOWN),
        |bytes| {
            let size = format_bytes(bytes);
            formatted(N_!("{size} available"), &[("size", &size)])
        },
    )
}

pub fn device_subtitle(connected: bool, available: Option<u64>) -> String {
    if connected {
        formatted(
            N_!("{state} · {space}"),
            &[
                ("state", &text(CONNECTED)),
                ("space", &available_space(available)),
            ],
        )
    } else {
        text(DISCONNECTED)
    }
}

pub fn track_progress(completed: usize, total: usize) -> String {
    let completed = completed.to_string();
    let total = total.to_string();
    formatted(
        N_!("{completed} of {total} tracks"),
        &[("completed", &completed), ("total", &total)],
    )
}

pub fn queued_jobs(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} job waiting",
        "{count} jobs waiting",
        count,
        &[("count", &count_text)],
    )
}

pub fn outcome_counts(copied: usize, skipped: usize, failed: usize) -> String {
    formatted(
        N_!("{copied} copied · {skipped} skipped · {failed} failed"),
        &[
            ("copied", &copied.to_string()),
            ("skipped", &skipped.to_string()),
            ("failed", &failed.to_string()),
        ],
    )
}

pub fn tracks_queued(count: usize, position: usize) -> String {
    let count_text = count.to_string();
    let position_text = position.to_string();
    plural(
        "{count} track queued · job {position}",
        "{count} tracks queued · job {position}",
        count,
        &[("count", &count_text), ("position", &position_text)],
    )
}

pub fn playlist_entries(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} track",
        "{count} tracks",
        count,
        &[("count", &count_text)],
    )
}

pub fn file_size(bytes: u64) -> String {
    format_bytes(bytes)
}

fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1_024.0;
    const MIB: f64 = KIB * 1_024.0;
    const GIB: f64 = MIB * 1_024.0;
    let bytes = bytes as f64;
    if bytes >= GIB {
        format!("{:.1} GiB", bytes / GIB)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes / KIB)
    } else {
        format!("{} B", bytes as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_formatting_uses_compact_binary_units() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1_024), "1.0 KiB");
        assert_eq!(format_bytes(2 * 1_024 * 1_024), "2.0 MiB");
    }

    #[test]
    fn track_copy_and_queue_status_keep_all_counts_visible() {
        assert_eq!(track_progress(2, 5), "2 of 5 tracks");
        assert_eq!(queued_jobs(1), "1 job waiting");
        assert_eq!(queued_jobs(2), "2 jobs waiting");
        assert_eq!(outcome_counts(3, 2, 1), "3 copied · 2 skipped · 1 failed");
    }

    #[test]
    fn disconnected_device_does_not_claim_stale_available_space() {
        assert_eq!(device_subtitle(false, Some(1_024)), "Disconnected");
    }
}
