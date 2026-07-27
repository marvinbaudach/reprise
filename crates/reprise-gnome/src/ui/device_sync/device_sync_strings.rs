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

/// TIP-2a: a disabled eject keeps its tooltip and appends the reason.
pub fn eject_tooltip(syncing: bool) -> String {
    text(if syncing {
        EJECT_BLOCKED_SYNCING
    } else {
        EJECT_DEVICE
    })
}

fn formatted(message: &str, values: &[(&str, &str)]) -> String {
    crate::i18n::format_message(&text(message), values)
}

pub const SPACE_UNKNOWN: &str = N_!("Available space unknown");
pub const SYNC_PROGRESS: &str = N_!("Synchronization Progress");

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
        assert_eq!(free_space(Some(2 * 1_024 * 1_024)), "2.0 MiB free");
    }

    #[test]
    fn tip_2a_eject_tooltip_names_reason_while_syncing() {
        assert_eq!(eject_tooltip(true), "Eject device — Sync in progress");
        assert_eq!(eject_tooltip(false), "Eject device");
    }

    #[test]
    fn device_open_action_names_its_target() {
        assert_eq!(open_device_label("Pixel 8"), "Open Pixel 8");
    }

    #[test]
    fn track_progress_keeps_both_counts_visible() {
        assert_eq!(track_progress(2, 5), "2 of 5 tracks");
    }
}
