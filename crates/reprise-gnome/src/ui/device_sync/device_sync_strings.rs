//! Translatable copy and compact formatting for Android synchronization.

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

pub fn text(message: &str) -> String {
    crate::i18n::gettext(message)
}

pub const EJECT_DEVICE: &str = N_!("Eject device");
pub const OPEN_DEVICE: &str = N_!("Open {name}");
pub const EJECT_BLOCKED_SYNCING: &str = N_!("Eject device — Sync in progress");
pub const INERT_DEVICE_STATUS: &str = N_!("Plugged in · disconnect {other} to use it");
pub const UNREMEMBERABLE_DEVICE_STATUS: &str =
    N_!("This device can be used now but cannot be remembered");
pub const RENAME_DEVICE: &str = N_!("Rename device");
const RENAME_REQUIRES_DURABLE_IDENTITY: &str = N_!(
    "Renaming is unavailable because this phone has no durable identity, so its per-device settings cannot be kept between connections."
);
pub const LOCAL_DEVICE_NAME: &str = N_!("Local device name");
pub const RENAME: &str = N_!("Rename");
pub const FORGET_DEVICE: &str = N_!("Forget device");
pub const MUSIC_TRANSFER_PROFILE_HEADING: &str = N_!("Music · Opus 160 kbit/s");
pub const UP_NEXT: &str = N_!("Up next");
pub const RESCAN: &str = N_!("Rescan");
pub(super) const VERIFIED_AGO: &str = N_!("verified {time}");
pub(super) const JUST_NOW: &str = N_!("just now");
pub(super) const MINUTES_AGO: &str = N_!("{minutes} min ago");
pub(super) const HOURS_AGO: &str = N_!("{hours} h ago");
pub(super) const DAYS_AGO: &str = N_!("{days} d ago");

/// Spinner tooltip while syncing, e.g. "Syncing Pixel 8 · 42%".
pub fn syncing_spinner_tooltip(name: &str, percent: u64) -> String {
    let percent = percent.to_string();
    formatted(
        N_!("Syncing {name} · {percent}%"),
        &[("name", name), ("percent", &percent)],
    )
}

pub fn open_device_label(name: &str) -> String {
    formatted(OPEN_DEVICE, &[("name", name)])
}

pub fn inert_device_status(other: &str) -> String {
    formatted(INERT_DEVICE_STATUS, &[("other", other)])
}

pub fn unrememberable_device_status() -> String {
    text(UNREMEMBERABLE_DEVICE_STATUS)
}

pub fn rename_requires_durable_identity() -> String {
    text(RENAME_REQUIRES_DURABLE_IDENTITY)
}

/// TIP-2a: a disabled eject keeps its tooltip and appends the reason.
pub fn eject_tooltip(syncing: bool) -> String {
    text(if syncing {
        EJECT_BLOCKED_SYNCING
    } else {
        EJECT_DEVICE
    })
}

pub(super) fn formatted(message: &str, values: &[(&str, &str)]) -> String {
    crate::i18n::format_message(&text(message), values)
}

pub const SPACE_UNKNOWN: &str = N_!("Available space unknown");
pub const SYNC_PROGRESS: &str = N_!("Synchronization Progress");
pub const CHOOSE_CONTENT: &str = N_!("Choose…");
pub const CHOOSE_PLAYLISTS: &str = N_!("Choose playlists");
pub const CHOOSE_PLAYLIST_FOLDER: &str = N_!("Choose folder for Playlists");
pub const CHANGE_FOLDER: &str = N_!("Change folder…");
pub const PLAYLISTS: &str = N_!("Playlists");
pub const REMOVE_FROM_PHONE: &str = N_!("Remove from phone when removed from a playlist");
pub const SYNC_AUTOMATICALLY: &str = N_!("Sync automatically when this phone connects");
pub const TARGET_FOLDER: &str = N_!("Target folder: {path}");
pub const FILTER_SYNC_CONTENT: &str = N_!("Filter sync content");
pub const SELECT_ALL: &str = N_!("Select all");
pub const CANCEL: &str = N_!("Cancel");
pub const SAVE: &str = N_!("Save");
pub const EVERYTHING: &str = N_!("Everything");
pub const SMART_PLAYLIST: &str = N_!("Smart playlist");
pub const KEEP_SMART_PLAYLISTS_UPDATED: &str = N_!("Keep smart playlists up to date on each sync");
pub const UNAVAILABLE_PLAYLIST: &str = N_!("Unavailable playlist");
pub const PICKER_FOOTER: &str = N_!("{selected} selected · {content} · {size}");
pub const TRACKS: &str = N_!("{count} tracks");
pub const SMART_LISTS_UPDATED: &str = N_!("smart lists kept up to date");
pub const SMART_LISTS_FROZEN: &str = N_!("smart lists keep their current contents");

pub fn available_space(bytes: Option<u64>) -> String {
    bytes.map_or_else(
        || text(SPACE_UNKNOWN),
        |bytes| {
            let size = format_bytes(bytes);
            formatted(N_!("{size} available"), &[("size", &size)])
        },
    )
}

pub fn free_space(bytes: Option<u64>) -> String {
    bytes.map_or_else(
        || text(SPACE_UNKNOWN),
        |bytes| {
            let size = format_bytes(bytes);
            formatted(N_!("{size} free"), &[("size", &size)])
        },
    )
}

pub fn track_progress(completed: usize, total: usize) -> String {
    let completed = completed.to_string();
    let total = total.to_string();
    formatted(
        N_!("{completed} of {total} tracks"),
        &[("completed", &completed), ("total", &total)],
    )
}

/// Rough USB throughput used for the remaining-time hint — the same
/// assumption the delta card's estimate is built on, so both agree.
const ESTIMATED_BYTES_PER_SECOND: u64 = 5 * 1_024 * 1_024;

/// `98 %` — kept in its own label (never folded into the subtitle) so the
/// track text beside it cannot make the number jump around.
pub fn sync_percent(bytes_done: u64, bytes_total: u64) -> String {
    let percent = bytes_done
        .saturating_mul(100)
        .checked_div(bytes_total)
        .unwrap_or(0)
        .min(100);
    formatted(N_!("{percent} %"), &[("percent", &percent.to_string())])
}

/// `↑ Immortal — Lorna Shore` — the prefix says what is happening to the
/// named track and the runtime supplies both title and artist.
pub fn sync_activity(step: &str, current_track: &str) -> String {
    if current_track.is_empty() {
        return step.to_string();
    }
    format!("{step} {current_track}")
}

/// `28 of 82 · 340.0 MiB of 1.2 GiB · ~2 min left · Immortal`
pub fn sync_tooltip(
    done: u32,
    total: u32,
    bytes_done: u64,
    bytes_total: u64,
    current_track: &str,
) -> String {
    let mut parts = vec![
        track_progress(done as usize, total as usize),
        formatted(
            N_!("{done} of {total}"),
            &[
                ("done", &format_bytes(bytes_done)),
                ("total", &format_bytes(bytes_total)),
            ],
        ),
    ];
    if let Some(remaining) = remaining_hint(bytes_done, bytes_total) {
        parts.push(remaining);
    }
    if !current_track.is_empty() {
        parts.push(current_track.to_string());
    }
    parts.join(" · ")
}

fn remaining_hint(bytes_done: u64, bytes_total: u64) -> Option<String> {
    let remaining = bytes_total
        .checked_sub(bytes_done)
        .filter(|left| *left > 0)?;
    let seconds = remaining.div_ceil(ESTIMATED_BYTES_PER_SECOND);
    let text = if seconds >= 60 {
        let minutes = seconds.div_ceil(60);
        formatted(
            N_!("~{minutes} min left"),
            &[("minutes", &minutes.to_string())],
        )
    } else {
        formatted(
            N_!("~{seconds} s left"),
            &[("seconds", &seconds.max(1).to_string())],
        )
    };
    Some(text)
}

pub fn file_size(bytes: u64) -> String {
    format_bytes(bytes)
}

pub fn picker_content(count: usize) -> String {
    formatted(TRACKS, &[("count", &count.to_string())])
}

pub fn selected_playlists(selected: usize, total: usize) -> String {
    formatted(
        N_!("{selected} of {total} playlists"),
        &[
            ("selected", &selected.to_string()),
            ("total", &total.to_string()),
        ],
    )
}

pub fn target_folder(path: &str) -> String {
    formatted(TARGET_FOLDER, &[("path", path)])
}

pub fn music_track_count(count: usize) -> String {
    formatted(N_!("{count} tracks"), &[("count", &count.to_string())])
}

/// `MTP-22`'s exact vocabulary for one category's `CategoryReading` — used by
/// the "Up next" card's per-category row and, through the shared
/// [`detailed_balance_parts`], by the sidebar card's full-balance tooltip, so
/// the two surfaces never drift into different wording for the same numbers.
/// (The device page's own one-line summary is the shorter [`balance_text`],
/// built from `plan_balance_parts`; the split is deliberate.)
/// The compact aggregate result: "14 files to copy · 2.6 GiB · 3 to
/// remove". Copy and removal counts stay separate.
pub fn balance_text(balance: &reprise_core::device_sync::SyncBalance) -> String {
    plan_balance_parts(
        balance.files_to_copy,
        balance.bytes_to_copy,
        balance.files_to_remove,
        balance.bytes_freed,
        balance.playlists_rewritten,
    )
}

fn plan_balance_parts(
    files_to_copy: usize,
    bytes_to_copy: u64,
    files_to_remove: usize,
    _bytes_freed: u64,
    playlists_rewritten: usize,
) -> String {
    let mut parts = Vec::new();
    if files_to_copy > 0 {
        let count = counted_files(files_to_copy);
        parts.push(formatted(N_!("{count} to copy"), &[("count", &count)]));
        parts.push(file_size(bytes_to_copy));
    }
    if files_to_remove > 0 {
        parts.push(formatted(
            N_!("{count} to remove"),
            &[("count", &files_to_remove.to_string())],
        ));
    }
    if playlists_rewritten > 0 {
        parts.push(formatted(
            N_!("{count} playlists to update"),
            &[("count", &playlists_rewritten.to_string())],
        ));
    }
    if parts.is_empty() {
        return text(N_!("Nothing to transfer"));
    }
    parts.join(" · ")
}

/// The sidebar tooltip keeps the complete directional byte accounting from
/// MTP-29; the device page's summary intentionally uses [`balance_text`]'s
/// shorter result-oriented sentence.
pub fn detailed_balance_text(balance: &reprise_core::device_sync::SyncBalance) -> String {
    detailed_balance_parts(
        balance.files_to_copy,
        balance.bytes_to_copy,
        balance.files_to_remove,
        balance.bytes_freed,
        balance.playlists_rewritten,
    )
}

fn detailed_balance_parts(
    files_to_copy: usize,
    bytes_to_copy: u64,
    files_to_remove: usize,
    bytes_freed: u64,
    playlists_rewritten: usize,
) -> String {
    let mut parts = Vec::new();
    if files_to_copy > 0 {
        parts.push(format!(
            "To copy {} · {}",
            counted_files(files_to_copy),
            file_size(bytes_to_copy)
        ));
    }
    if files_to_remove > 0 {
        parts.push(format!(
            "To remove {} · {}",
            counted_files(files_to_remove),
            file_size(bytes_freed)
        ));
    }
    if playlists_rewritten > 0 {
        parts.push(format!("Playlists rewritten {playlists_rewritten}"));
    }
    if parts.is_empty() {
        return "Nothing pending".to_string();
    }
    parts.join(" · ")
}

fn counted_files(count: usize) -> String {
    if count == 1 {
        "1 file".to_string()
    } else {
        format!("{count} files")
    }
}

/// Design 2c: "175.0 → 172.4 GiB free". Collapses to a
/// single figure when this sync would not move the free-space needle at
/// all, so a device with nothing pending never shows a pointless arrow to
/// the same number.
pub fn free_space_line(free_before_bytes: u64, free_after_bytes: u64) -> String {
    if free_before_bytes == free_after_bytes {
        let size = file_size(free_before_bytes);
        return formatted(N_!("{size} free"), &[("size", &size)]);
    }
    let before = file_size(free_before_bytes);
    let after = file_size(free_after_bytes);
    let before = before
        .strip_suffix(after.split_once(' ').map_or("", |(_, unit)| unit))
        .map_or(before.as_str(), str::trim_end);
    formatted(
        N_!("{before} → {after} free"),
        &[("before", before), ("after", &after)],
    )
}

/// Design 7c: "synced 12 min ago". Coarse buckets are deliberate — the
/// sidebar card is a glance surface, not a log.
pub use super::device_sync_time_copy::{relative_time, verified_ago};

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

/// `MTP-26`: the "Up next" heading row's status, one line per contents state.
/// Rendered by `device_sync_verification_copy`, which stays display-free.
pub const CONTENTS_NEVER_VERIFIED: &str = N_!("Device contents never verified");
pub const CONTENTS_SCAN_INVITATION: &str =
    N_!("Scan the device to see what's already there before syncing.");
pub const CONTENTS_VERIFYING: &str = N_!("Verifying device contents…");
pub const CONTENTS_VERIFYING_DETAIL: &str =
    N_!("Reading storage over MTP — this can take a moment.");
pub const CONTENTS_VERIFIED: &str = N_!("Device contents verified");
pub const CONTENTS_NOT_VERIFIABLE: &str = N_!("Could not verify device contents");

#[cfg(test)]
#[path = "device_sync_strings_tests.rs"]
mod tests;
