//! Per-category sync diff and the overall balance (`MTP-22`) — turn E3.
//!
//! Design 7c is explicit about why this needs its own type rather than a
//! display-time computation: the current sidebar card
//! (`sidebar_gnome::sidebar_device_card`) sums additions, replacements,
//! removals and playlist removals into one "N changes" count, then prints
//! `transfer_bytes` — which only ever counts bytes moving *onto* the
//! device — next to it. A deletions-only sync (three files removed, zero
//! copied) reads "3 changes · 0 B", and "0 B sounds like nothing to do even
//! though three files are being deleted" (design 7c, verbatim). That bug
//! lives in the *type*, not the label: as long as "changes" and "bytes"
//! are each a single blended number, a deletions-only sync is
//! indistinguishable from "nothing to do" at zero bytes. [`CategoryDiff`]
//! and [`SyncBalance`] fix this by never blending copy and remove into one
//! count or one byte figure — each direction keeps its own file count and
//! its own byte figure, so "0 to copy · 3 to remove · frees 148 MiB" and
//! "frees 0 B" are both representable, and both distinguishable from
//! nothing pending at all (`has_work() == false`).
//!
//! ## Reused, not reinvented
//!
//! Two category engines already exist and stay exactly as they are:
//! `mirror::plan_mirror` for playlists, and `podcasts::build_plan` for RSS
//! (and, modelled but inert, YouTube) podcast episodes. `delta.rs`'s
//! `compute_delta`/`SyncDelta` is an earlier, superseded shape — nothing in
//! the running app calls it any more (only its own test module does); it is
//! not extended here because it is not the "general shape" in production,
//! `mirror.rs` is. What is missing is the piece that spans all three
//! categories at once — a per-category *reading* (including the two special
//! states design 7c/E3 name) and the cross-category balance. That is what
//! this module adds; [`CategoryDiff::from_mirror_plan`] and
//! [`CategoryDiff::from_podcast_plan`] are thin translations from the two
//! existing engines, not a parallel diff calculator.
//!
//! ## Cap enforcement (`MTP-39`)
//!
//! [`apply_cap`] folds a target's optional cap on top of an already-computed
//! diff using `cap::items_to_evict` directly — eviction is additional
//! removal beyond "no longer selected", so it can only ever add to
//! `files_to_remove`/`bytes_freed`, never touch `files_to_copy`.
//!
//! ## Offline (`NET-3a`)
//!
//! [`candidate_source`] derives whether a category's diff can be trusted
//! from `connectivity::Connectivity` rather than inventing a second offline
//! notion. Offline does not, by itself, block a diff — most of what a diff
//! needs (the desired set from the DB, the device inventory) is already
//! local. Only a category with nothing local to compare against at all
//! falls back to [`CategoryReading::UnavailableKeptOnPhone`].

use super::cap::{items_to_evict, CapItem};
use super::mirror::MirrorPlan;
use super::podcasts::PodcastSyncPlan;
use crate::connectivity::Connectivity;

/// One category's diff, counted in files and bytes on both directions —
/// see the module docs for why copy and remove are never blended into one
/// count or one byte figure.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CategoryDiff {
    pub files_to_copy: usize,
    pub bytes_to_copy: u64,
    pub files_to_remove: usize,
    pub bytes_freed: u64,
    /// Wanted but not yet downloaded (`MTP-40`) — informative only, never
    /// folded into `files_to_copy`: this sync will not move these bytes
    /// until a download satisfies them.
    pub files_waiting_for_download: usize,
    /// Playlists only: playlist files rewritten this sync. Reprise
    /// rewrites every selected, available playlist's `.m3u8` on each
    /// successful sync — there is no stored per-playlist content hash to
    /// diff against — so this is the count of currently selected and
    /// available playlists, not a "content changed since last time" count.
    pub playlists_rewritten: usize,
}

impl CategoryDiff {
    /// Whether this category has anything a sync would actually do. Reads
    /// file counts, **not** bytes — "3 to remove · frees 0 B" must still
    /// report `true` here; that is the whole point of this type (see the
    /// module docs on design 7c).
    #[must_use]
    pub fn has_work(&self) -> bool {
        self.files_to_copy > 0
            || self.files_to_remove > 0
            || self.files_waiting_for_download > 0
            || self.playlists_rewritten > 0
    }

    /// Translates the existing playlist mirror engine's output
    /// (`mirror::plan_mirror`) into a category diff. `copy` and `replace`
    /// both move a file onto the device, so both count toward
    /// `files_to_copy`.
    #[must_use]
    pub fn from_mirror_plan(plan: &MirrorPlan) -> Self {
        Self {
            files_to_copy: plan.copy.len() + plan.replace.len(),
            bytes_to_copy: plan.transfer_bytes,
            files_to_remove: plan.remove.len(),
            bytes_freed: plan.bytes_freed,
            files_waiting_for_download: 0,
            playlists_rewritten: plan.playlist_writes.len(),
        }
    }

    /// Translates the existing podcast/YouTube plan engine's output
    /// (`podcasts::build_plan`) into a category diff. `waiting` comes from
    /// `selection::select_episodes` — the two pipelines are joined here by
    /// the caller supplying both, not by merging them into one engine.
    #[must_use]
    pub fn from_podcast_plan(plan: &PodcastSyncPlan, files_waiting_for_download: usize) -> Self {
        Self {
            files_to_copy: plan.to_copy.len(),
            bytes_to_copy: plan.bytes,
            files_to_remove: plan.to_remove.len(),
            bytes_freed: plan.bytes_freed,
            files_waiting_for_download,
            playlists_rewritten: 0,
        }
    }
}

/// `MTP-39`: folds a target's optional cap on top of an already-computed
/// diff. `items_after_sync` is every item that would remain on the device
/// once `diff`'s copies land — the caller assembles that list; this
/// function only decides who leaves once it is oversized, exactly like
/// `cap::items_to_evict` already does for a bare item list.
#[must_use]
pub fn apply_cap<Id: Copy + Ord>(
    diff: CategoryDiff,
    items_after_sync: &[CapItem<Id>],
    cap_bytes: Option<u64>,
) -> CategoryDiff {
    let Some(cap_bytes) = cap_bytes else {
        return diff;
    };
    let mut evicted = items_to_evict(items_after_sync, cap_bytes);
    if evicted.is_empty() {
        return diff;
    }
    evicted.sort_unstable();
    let bytes_evicted = items_after_sync
        .iter()
        .filter(|item| evicted.binary_search(&item.id).is_ok())
        .map(|item| item.size_bytes)
        .fold(0_u64, u64::saturating_add);
    CategoryDiff {
        files_to_remove: diff.files_to_remove + evicted.len(),
        bytes_freed: diff.bytes_freed.saturating_add(bytes_evicted),
        ..diff
    }
}

/// Whether a category's diff can currently be trusted. Derived by the
/// caller via [`candidate_source`] from `connectivity::Connectivity`
/// (`NET-3a`) rather than a second offline notion invented here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandidateSource {
    /// Enough is known locally to trust a computed diff.
    Computed(CategoryDiff),
    /// Nothing local to compare against right now — see
    /// [`CategoryReading::UnavailableKeptOnPhone`].
    Unavailable,
}

/// `NET-3a`-derived: turns connectivity plus "is there anything local to
/// compare against" into a [`CandidateSource`]. Offline alone does not make
/// a category `Unavailable` — a device with at least one already-known
/// local item can still trust removals computed among the items it does
/// know about; only *nothing* local at all makes the diff unknowable.
#[must_use]
pub fn candidate_source(
    connectivity: Connectivity,
    has_any_local_reference: bool,
    diff: CategoryDiff,
) -> CandidateSource {
    if connectivity.is_offline() && !has_any_local_reference {
        CandidateSource::Unavailable
    } else {
        CandidateSource::Computed(diff)
    }
}

/// `MTP-22`: what a category's "Next synchronization" row reads (design
/// 7c/E3). The two special readings are distinct states, not degenerate
/// zero-diffs — "nothing to report" must never look like "nothing
/// configured" or "can't tell right now".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CategoryReading {
    /// The normal numeric reading for an active, enabled category —
    /// design's "0 new · 3 removed" / "2 new · 0 removed".
    Diff(CategoryDiff),
    /// The category's per-device target is off — nothing was evaluated, a
    /// different fact from "evaluated to zero changes". Design's "source
    /// off".
    SourceOff,
    /// The source has nothing to compare against right now; existing
    /// device files are left untouched rather than guessed at. Design's
    /// "Unavailable, kept on phone".
    UnavailableKeptOnPhone,
}

/// `MTP-22`: projects a category's reading from `rule_enabled` — a second,
/// generic AND-gate every production call site passes as `true` since
/// `E-6` withdrew the once-planned global rule this parameter modeled —
/// combined with whether the category is enabled for this device
/// (`target_enabled`, `SyncTarget::enabled`, `MTP-38`), and what could be
/// computed ([`CandidateSource`]).
#[must_use]
pub fn project_category_reading(
    target_enabled: bool,
    rule_enabled: bool,
    source: CandidateSource,
) -> CategoryReading {
    if !target_enabled || !rule_enabled {
        return CategoryReading::SourceOff;
    }
    match source {
        CandidateSource::Computed(diff) => CategoryReading::Diff(diff),
        CandidateSource::Unavailable => CategoryReading::UnavailableKeptOnPhone,
    }
}

/// `MTP-22`: the overall balance across all active categories — design
/// 7c's "To copy 14 files · 2.6 GiB", "To remove 3 files · 148 MiB",
/// "Playlists rewritten 2". See the module docs for why copy and remove
/// each keep their own count and their own byte figure.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SyncBalance {
    pub files_to_copy: usize,
    pub bytes_to_copy: u64,
    pub files_to_remove: usize,
    pub bytes_freed: u64,
    pub files_waiting_for_download: usize,
    pub playlists_rewritten: usize,
}

impl SyncBalance {
    /// Same rule as [`CategoryDiff::has_work`]: file counts decide, bytes
    /// never do.
    #[must_use]
    pub fn has_work(&self) -> bool {
        self.files_to_copy > 0
            || self.files_to_remove > 0
            || self.files_waiting_for_download > 0
            || self.playlists_rewritten > 0
    }
}

/// `MTP-22`: sums every category currently reading a computed
/// [`CategoryDiff`]. `SourceOff` and `UnavailableKeptOnPhone` categories
/// contribute nothing to the totals — on purpose, not silently: a caller
/// that needs to say *why* a category is missing from the balance reads the
/// per-category [`CategoryReading`]s directly, this function does not lose
/// that information, it simply does not carry it.
#[must_use]
pub fn aggregate_balance(readings: &[CategoryReading]) -> SyncBalance {
    let mut balance = SyncBalance::default();
    for reading in readings {
        let CategoryReading::Diff(diff) = reading else {
            continue;
        };
        balance.files_to_copy += diff.files_to_copy;
        balance.bytes_to_copy = balance.bytes_to_copy.saturating_add(diff.bytes_to_copy);
        balance.files_to_remove += diff.files_to_remove;
        balance.bytes_freed = balance.bytes_freed.saturating_add(diff.bytes_freed);
        balance.files_waiting_for_download += diff.files_waiting_for_download;
        balance.playlists_rewritten += diff.playlists_rewritten;
    }
    balance
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diff(
        files_to_copy: usize,
        bytes_to_copy: u64,
        files_to_remove: usize,
        bytes_freed: u64,
    ) -> CategoryDiff {
        CategoryDiff {
            files_to_copy,
            bytes_to_copy,
            files_to_remove,
            bytes_freed,
            files_waiting_for_download: 0,
            playlists_rewritten: 0,
        }
    }

    #[test]
    fn mtp_22_a_deletions_only_diff_reports_removed_files_with_correct_bytes_even_when_zero() {
        let removed_but_zero_bytes = diff(0, 0, 3, 0);
        let nothing_pending = CategoryDiff::default();

        assert!(
            removed_but_zero_bytes.has_work(),
            "3 files removed is work even when their total size is 0 B"
        );
        assert!(
            !nothing_pending.has_work(),
            "genuinely nothing pending must stay distinguishable from the case above"
        );
        assert_ne!(removed_but_zero_bytes, nothing_pending);
    }

    #[test]
    fn mtp_22_category_reads_source_off_when_disabled_regardless_of_computed_diff() {
        let would_have_had_changes = CandidateSource::Computed(diff(2, 1_000, 0, 0));

        assert_eq!(
            project_category_reading(true, false, would_have_had_changes),
            CategoryReading::SourceOff,
            "global rule off wins even though a diff was computable"
        );
        assert_eq!(
            project_category_reading(
                false,
                true,
                CandidateSource::Computed(CategoryDiff::default())
            ),
            CategoryReading::SourceOff,
            "per-device target disabled also reads source off"
        );
    }

    #[test]
    fn mtp_22_category_reads_unavailable_kept_on_phone_when_nothing_local_to_compare() {
        let source = candidate_source(Connectivity::Offline, false, diff(0, 0, 0, 0));

        assert_eq!(source, CandidateSource::Unavailable);
        assert_eq!(
            project_category_reading(true, true, source),
            CategoryReading::UnavailableKeptOnPhone
        );
    }

    #[test]
    fn mtp_22_an_offline_category_with_a_local_reference_still_trusts_its_diff() {
        let expected = diff(0, 0, 1, 500);

        let source = candidate_source(Connectivity::Offline, true, expected);

        assert_eq!(
            source,
            CandidateSource::Computed(expected),
            "offline alone does not block a diff computed from local data"
        );
    }

    #[test]
    fn mtp_22_balance_keeps_copy_and_remove_bytes_separate_so_zero_copy_never_hides_removals() {
        let readings = [
            CategoryReading::Diff(diff(0, 0, 3, 148 * 1024 * 1024)),
            CategoryReading::SourceOff,
        ];

        let balance = aggregate_balance(&readings);

        assert_eq!(balance.files_to_copy, 0);
        assert_eq!(balance.bytes_to_copy, 0);
        assert_eq!(balance.files_to_remove, 3);
        assert_eq!(balance.bytes_freed, 148 * 1024 * 1024);
        assert!(
            balance.has_work(),
            "0 B to copy must never read as nothing to do while 3 files are removed"
        );
    }

    #[test]
    fn mtp_22_balance_excludes_source_off_and_unavailable_categories_from_the_totals() {
        let readings = [
            CategoryReading::Diff(diff(14, 2 * 1024 * 1024 * 1024, 3, 100)),
            CategoryReading::SourceOff,
            CategoryReading::UnavailableKeptOnPhone,
        ];

        let balance = aggregate_balance(&readings);

        assert_eq!(balance.files_to_copy, 14);
        assert_eq!(balance.files_to_remove, 3);
    }

    #[test]
    fn mtp_22_cap_eviction_adds_to_removals_beyond_the_selection_diff() {
        let base = diff(2, 1_000, 0, 0);
        let items = [
            CapItem {
                id: 1_i64,
                size_bytes: 30,
                age: 1,
            },
            CapItem {
                id: 2,
                size_bytes: 30,
                age: 2,
            },
        ];

        let capped = apply_cap(base, &items, Some(40));

        assert_eq!(
            capped.files_to_remove, 1,
            "only the oldest item needs to leave to reach the cap"
        );
        assert_eq!(capped.bytes_freed, 30);
        assert_eq!(
            capped.files_to_copy, base.files_to_copy,
            "cap never touches copies"
        );
    }

    #[test]
    fn mtp_22_cap_eviction_is_a_no_op_without_a_cap_or_already_under_it() {
        let base = diff(0, 0, 0, 0);
        let items = [CapItem {
            id: 1_i64,
            size_bytes: 10,
            age: 1,
        }];

        assert_eq!(apply_cap(base, &items, None), base);
        assert_eq!(apply_cap(base, &items, Some(100)), base);
    }

    #[test]
    fn mtp_22_from_mirror_plan_and_from_podcast_plan_translate_existing_plans() {
        use super::super::mirror::{plan_mirror, MirrorInput, MirrorPlaylistSnapshot, MirrorTrack};
        use super::super::podcasts::{
            build_plan, PodcastDeviceFile, PodcastSyncCandidate, PodcastSyncSource,
        };
        use super::super::{SelectionSource, SyncTrack, TransferProfile};

        let track = SyncTrack {
            id: 1,
            source_path: "/music/one.mp3".into(),
            original_name: "one.mp3".into(),
            title: "One".into(),
            artist: "Artist".into(),
            album: "Album".into(),
            album_artist: "Artist".into(),
            track_number: Some(1),
            duration_ms: 1_000,
            bitrate_kbps: Some(192),
            size_bytes: 500,
            source_mtime: 1,
        };
        let mirror_plan = plan_mirror(MirrorInput {
            selected: vec![SelectionSource::Playlist(1)],
            playlists: vec![MirrorPlaylistSnapshot {
                source: SelectionSource::Playlist(1),
                name: "Road Trip".into(),
                entries: vec![MirrorTrack::Available(track)],
            }],
            profile: TransferProfile::default(),
            inventory: Vec::new(),
            playlist_inventory: Vec::new(),
            managed_files: Vec::new(),
        });

        let category = CategoryDiff::from_mirror_plan(&mirror_plan);
        assert_eq!(category.files_to_copy, 1);
        assert_eq!(category.playlists_rewritten, 1);

        let podcast_candidate = PodcastSyncCandidate {
            episode_id: 1,
            source: PodcastSyncSource::Rss,
            source_path: "/downloads/one.mp3".into(),
            device_path: "Show/1-One.mp3".into(),
            title: "One".into(),
            show: "Show".into(),
            size_bytes: 100,
            source_mtime: 1,
        };
        let podcast_plan = build_plan(
            vec![podcast_candidate],
            &Vec::<PodcastDeviceFile>::new(),
            true,
            PodcastSyncSource::Rss,
            None,
            crate::device_sync::podcasts::EnabledSyncSources {
                rss: true,
                youtube: true,
            },
        );

        let podcast_category = CategoryDiff::from_podcast_plan(&podcast_plan, 2);
        assert_eq!(podcast_category.files_to_copy, 1);
        assert_eq!(podcast_category.bytes_to_copy, 100);
        assert_eq!(podcast_category.files_waiting_for_download, 2);
    }
}
