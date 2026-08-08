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
pub const CHANGE: &str = N_!("Change…");
pub const CHANGE_CONTENT: &str = N_!("Change content…");
pub const CHANGE_FOLDER: &str = N_!("Change folder…");
pub const TARGET_FOLDER: &str = N_!("Target folder: {path}");
pub const CAP_IN_GIB: &str = N_!("Size limit in GiB (0 means no size limit)");
pub const CHOOSE_CATEGORY: &str = N_!("Choose {category}");
pub const FILTER_SYNC_CONTENT: &str = N_!("Filter sync content");
pub const SELECT_ALL: &str = N_!("Select all");
pub const CANCEL: &str = N_!("Cancel");
pub const SAVE: &str = N_!("Save");
pub const EVERYTHING: &str = N_!("Everything");
pub const SMART_PLAYLIST: &str = N_!("Smart playlist");
pub const KEEP_SMART_PLAYLISTS_UPDATED: &str = N_!("Keep smart playlists up to date on each sync");
pub const YOUTUBE_PICKER_RULE: &str = N_!("Per channel, sync the latest N episodes");
pub const PODCAST_PICKER_RULE: &str = N_!("Per show, sync unplayed only");
pub const LATEST_EPISODES_PER_CHANNEL: &str = N_!("Latest episodes per channel, 0 for unlimited");
pub const PODCAST_REMOVAL_NOTE: &str = N_!(
    "Once played on the phone, an episode is removed on the next sync — this is a standing rule."
);
pub const ON_DISK: &str = N_!("On disk");
pub const NEEDS_DOWNLOAD: &str = N_!("Needs download");
pub const PREPARATION_LINK: &str = N_!("Downloaded in the preparation phase");
pub const SHOW_PREPARATION_PHASE: &str = N_!("Show the preparation phase on the device page");
pub const SELECTED_BY_RULE: &str = N_!("Selected by the rule");
pub const UNAVAILABLE_PLAYLIST: &str = N_!("Unavailable playlist");
pub const DURATION_MINUTES: &str = N_!("{minutes} min");
pub const RESUME_MINUTES: &str = N_!("{minutes} min in");
pub const GROUP_COUNTER: &str = N_!("{selected} of {total}");
pub const PICKER_FOOTER: &str = N_!("{selected} selected · {content} · {size}");
pub const PICKER_NEEDS_DOWNLOAD: &str = N_!("{count} still need downloading · preparation phase");
pub const TRACKS: &str = N_!("{count} tracks");
pub const EPISODES: &str = N_!("{count} episodes");
pub const UNKNOWN_SIZES: &str = N_!("+ {count} unknown sizes");
pub const SMART_LISTS_UPDATED: &str = N_!("smart lists kept up to date");
pub const SMART_LISTS_FROZEN: &str = N_!("smart lists keep their current contents");
pub const ALL_EPISODES: &str = N_!("all episodes");
pub const UNPLAYED_ONLY: &str = N_!("unplayed only");
pub const PLAYED_ARE_REMOVED: &str = N_!("played are removed");
pub const NOT_SYNCHRONIZED_WITH_PHONE: &str = N_!("Not synchronized with this phone");
pub const NO_SIZE_LIMIT: &str = N_!("no size limit");
const LEGEND_MUSIC: &str = N_!("Music");
const LEGEND_YOUTUBE: &str = N_!("YouTube");
const LEGEND_PODCASTS: &str = N_!("Podcasts");

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

pub fn group_counter(selected: usize, total: usize) -> String {
    formatted(
        GROUP_COUNTER,
        &[
            ("selected", &selected.to_string()),
            ("total", &total.to_string()),
        ],
    )
}

pub fn resume_minutes(position_ms: i64) -> String {
    formatted(
        RESUME_MINUTES,
        &[("minutes", &(position_ms.max(0) / 60_000).to_string())],
    )
}

pub fn duration_minutes(duration_secs: i64) -> String {
    formatted(
        DURATION_MINUTES,
        &[(
            "minutes",
            &duration_secs
                .max(0)
                .saturating_add(59)
                .div_euclid(60)
                .to_string(),
        )],
    )
}

pub fn picker_footer(selected: usize, content: &str, size: &str) -> String {
    formatted(
        PICKER_FOOTER,
        &[
            ("selected", &selected.to_string()),
            ("content", content),
            ("size", size),
        ],
    )
}

pub fn picker_needs_download(count: usize) -> String {
    formatted(PICKER_NEEDS_DOWNLOAD, &[("count", &count.to_string())])
}

pub fn choose_category(category: &str) -> String {
    formatted(CHOOSE_CATEGORY, &[("category", category)])
}

pub fn picker_content(count: usize, tracks: bool) -> String {
    formatted(
        if tracks { TRACKS } else { EPISODES },
        &[("count", &count.to_string())],
    )
}

pub fn unknown_sizes(count: usize) -> String {
    formatted(UNKNOWN_SIZES, &[("count", &count.to_string())])
}

/// Design 7a: "Playlists" / "YouTube audio" / "Podcast episodes" — the
/// Content section and Next synchronization panel's category labels.
pub fn category_name(kind: reprise_core::device_sync::SyncTargetKind) -> &'static str {
    use reprise_core::device_sync::SyncTargetKind;
    match kind {
        SyncTargetKind::Playlists => "Playlists",
        SyncTargetKind::YoutubeAudio => "YouTube audio",
        SyncTargetKind::PodcastEpisodes => "Podcast episodes",
    }
}

/// Design 2c's compact storage legend uses source identities rather than the
/// longer content-row headings. These messages already exist elsewhere in the
/// catalog and remain independently translated in required-complete locales.
pub fn category_legend_text(kind: reprise_core::device_sync::SyncTargetKind, bytes: u64) -> String {
    use reprise_core::device_sync::SyncTargetKind;
    let name = text(match kind {
        SyncTargetKind::Playlists => LEGEND_MUSIC,
        SyncTargetKind::YoutubeAudio => LEGEND_YOUTUBE,
        SyncTargetKind::PodcastEpisodes => LEGEND_PODCASTS,
    });
    format!("{name} {}", file_size(bytes))
}

/// The category rule's size phrase: "max 8.0 GiB" or "no size limit".
pub fn cap_text(cap_bytes: Option<u64>) -> String {
    cap_bytes.map_or_else(
        || text(NO_SIZE_LIMIT),
        |bytes| {
            let size = file_size(bytes);
            formatted(N_!("max {size}"), &[("size", &size)])
        },
    )
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

pub fn selected_channels(selected: usize) -> String {
    formatted(
        N_!("{selected} channels"),
        &[("selected", &selected.to_string())],
    )
}

pub fn selected_shows(selected: usize, total: usize) -> String {
    formatted(
        N_!("{selected} of {total} shows"),
        &[
            ("selected", &selected.to_string()),
            ("total", &total.to_string()),
        ],
    )
}

pub fn latest_each(count: usize) -> String {
    formatted(N_!("latest {count} each"), &[("count", &count.to_string())])
}

pub fn category_item_count(
    kind: reprise_core::device_sync::SyncTargetKind,
    count: usize,
) -> String {
    use reprise_core::device_sync::SyncTargetKind;
    let message = if kind == SyncTargetKind::Playlists {
        N_!("{count} tracks")
    } else {
        N_!("{count} episodes")
    };
    formatted(message, &[("count", &count.to_string())])
}

pub fn to_download(count: usize) -> String {
    formatted(N_!("{count} to download"), &[("count", &count.to_string())])
}

/// `MTP-22`'s exact vocabulary for one category's `CategoryReading` — used by
/// the "Up next" card's per-category row and, through the shared
/// [`detailed_balance_parts`], by the sidebar card's full-balance tooltip, so
/// the two surfaces never drift into different wording for the same numbers.
/// (The device page's own one-line summary is the shorter [`balance_text`],
/// built from `plan_balance_parts`; the split is deliberate.)
pub fn category_reading_text(reading: &reprise_core::device_sync::CategoryReading) -> String {
    use reprise_core::device_sync::CategoryReading;
    match reading {
        CategoryReading::SourceOff => text(N_!("Source off")),
        CategoryReading::UnavailableKeptOnPhone => "Unavailable, kept on phone".to_string(),
        CategoryReading::Diff(diff) => detailed_balance_parts(
            diff.files_to_copy,
            diff.bytes_to_copy,
            diff.files_to_remove,
            diff.bytes_freed,
            diff.playlists_rewritten,
        ),
    }
}

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

/// `MTP-43`'s preparation overview: "2 files to download · 312 MiB" for
/// `Offered`/`Planned`, "2 episodes skipped · not downloaded" for
/// `SkippedOffline`. `None` for every other phase — including `Absent` and
/// `NothingMissing` — so the caller knows the surface must not exist at all,
/// not render an empty box (`MTP-42`).
pub fn preparation_overview(phase: &reprise_core::device_sync::PreparationPhase) -> Option<String> {
    use reprise_core::device_sync::PreparationPhase;
    match phase {
        PreparationPhase::Absent | PreparationPhase::NothingMissing => None,
        PreparationPhase::SkippedOffline { files } => Some(preparation_skipped_offline(*files)),
        PreparationPhase::Offered { files, bytes } | PreparationPhase::Planned { files, bytes } => {
            Some(preparation_files_to_download(*files, *bytes))
        }
    }
}

/// "2 files to download · 312 MiB". `bytes == 0` means no source in this
/// codebase yet persists an expected size for that episode (see
/// `device_sync_compact::gather_missing_files`'s doc comment) — the count
/// still shows, the byte figure is simply omitted rather than claiming
/// "0 B".
fn preparation_files_to_download(files: usize, bytes: u64) -> String {
    let noun = if files == 1 { "file" } else { "files" };
    if bytes == 0 {
        format!("{files} {noun} to download")
    } else {
        format!("{files} {noun} to download · {}", file_size(bytes))
    }
}

/// "2 episodes skipped · not downloaded" (`NET-3`/`MTP-42`'s
/// `SkippedOffline`) — every one of these episodes stays `wanted_on_device`
/// for the next attempt.
fn preparation_skipped_offline(files: usize) -> String {
    let noun = if files == 1 { "episode" } else { "episodes" };
    format!("{files} {noun} skipped · not downloaded")
}

/// How many episode titles the preparation overview names before it starts
/// counting. `MTP-43` wants the user to know *what* is about to download,
/// not to read the whole queue: pasting all of them into one label grew the
/// overview card several screens tall and pulled the page's layout with it.
const PREPARATION_TITLE_PREVIEW: usize = 3;

/// "Nachtwind, Abendrot … and 57 more" — the titles line below
/// `preparation_files_to_download`. `None` when nothing is named, so the
/// caller appends no empty line.
pub fn preparation_titles(titles: &[&str]) -> Option<String> {
    let shown = titles.len().min(PREPARATION_TITLE_PREVIEW);
    if shown == 0 {
        return None;
    }
    let named = titles[..shown].join(", ");
    Some(match titles.len() - shown {
        0 => named,
        rest => format!("{named} \u{2026} and {rest} more"),
    })
}

/// "Step 1 of 2 · Downloading 1 of 2 · 62%" — the preparation download's own
/// progress line. Always step 1: a preparation phase only ever precedes the
/// transfer, never follows it.
#[cfg(test)]
fn preparation_step_progress(current_index: usize, total: usize, percent: u64) -> String {
    format!(
        "Step 1 of 2 · Downloading {} of {total} · {percent}%",
        current_index + 1
    )
}

/// Prefixes an existing transfer-progress title with "Step 2 of 2" — used
/// only when this run's transfer phase was actually preceded by a
/// preparation download, never for a plain single-phase sync.
#[cfg(test)]
fn two_phase_title(title: &str) -> String {
    format!("Step 2 of 2 · {title}")
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

pub const CHANGE_CATEGORY: &str = N_!("Change {category}");
pub const CHANGE_CATEGORY_CAP: &str = N_!("{category} size limit");

/// Accessible name for a category row's "Change…" menu, which is visually
/// tied to its row but reads as one of three identical buttons in the tree.
pub fn change_category_label(kind: reprise_core::device_sync::SyncTargetKind) -> String {
    formatted(CHANGE_CATEGORY, &[("category", category_name(kind))])
}

/// Accessible name for a category row's size-limit menu, whose visible label
/// is the value rather than what it controls.
pub fn change_cap_label(kind: reprise_core::device_sync::SyncTargetKind) -> String {
    formatted(CHANGE_CATEGORY_CAP, &[("category", category_name(kind))])
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
