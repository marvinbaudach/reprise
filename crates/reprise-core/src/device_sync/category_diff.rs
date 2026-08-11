//! Playlist synchronization diff and overall balance (`MTP-22`).

use super::mirror::MirrorPlan;

/// The single target's diff, counted independently in both directions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CategoryDiff {
    pub files_to_copy: usize,
    pub bytes_to_copy: u64,
    pub files_to_remove: usize,
    pub bytes_freed: u64,
    /// Playlist files rewritten during this synchronization.
    pub playlists_rewritten: usize,
}

impl CategoryDiff {
    #[must_use]
    pub fn has_work(&self) -> bool {
        self.files_to_copy > 0 || self.files_to_remove > 0 || self.playlists_rewritten > 0
    }

    #[must_use]
    pub fn from_mirror_plan(plan: &MirrorPlan) -> Self {
        Self {
            files_to_copy: plan.copy.len() + plan.replace.len() + plan.analysis_writes.len(),
            bytes_to_copy: plan.transfer_bytes,
            files_to_remove: plan.remove.len(),
            bytes_freed: plan.bytes_freed,
            playlists_rewritten: plan.playlist_writes.len(),
        }
    }
}

/// `MTP-22`: the reading for the device's single playlists target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CategoryReading {
    Diff(CategoryDiff),
}

impl Default for CategoryReading {
    fn default() -> Self {
        Self::Diff(CategoryDiff::default())
    }
}

/// The aggregate remains a distinct outward type even though it now contains
/// exactly one target's reading.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SyncBalance {
    pub files_to_copy: usize,
    pub bytes_to_copy: u64,
    pub files_to_remove: usize,
    pub bytes_freed: u64,
    pub playlists_rewritten: usize,
}

impl SyncBalance {
    #[must_use]
    pub fn has_work(&self) -> bool {
        self.files_to_copy > 0 || self.files_to_remove > 0 || self.playlists_rewritten > 0
    }
}

#[must_use]
pub fn aggregate_balance(readings: &[CategoryReading]) -> SyncBalance {
    let mut balance = SyncBalance::default();
    for reading in readings {
        let CategoryReading::Diff(diff) = reading;
        balance.files_to_copy += diff.files_to_copy;
        balance.bytes_to_copy = balance.bytes_to_copy.saturating_add(diff.bytes_to_copy);
        balance.files_to_remove += diff.files_to_remove;
        balance.bytes_freed = balance.bytes_freed.saturating_add(diff.bytes_freed);
        balance.playlists_rewritten += diff.playlists_rewritten;
    }
    balance
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diff(copy: usize, copy_bytes: u64, remove: usize, freed: u64) -> CategoryDiff {
        CategoryDiff {
            files_to_copy: copy,
            bytes_to_copy: copy_bytes,
            files_to_remove: remove,
            bytes_freed: freed,
            playlists_rewritten: 0,
        }
    }

    #[test]
    fn mtp_22_a_deletions_only_diff_remains_visible_even_when_it_frees_zero_bytes() {
        assert!(diff(0, 0, 3, 0).has_work());
        assert!(!CategoryDiff::default().has_work());
    }

    #[test]
    fn mtp_22_the_single_target_reading_projects_an_exact_balance() {
        let reading = CategoryReading::Diff(diff(14, 2_600, 3, 148));
        assert_eq!(
            aggregate_balance(&[reading]),
            SyncBalance {
                files_to_copy: 14,
                bytes_to_copy: 2_600,
                files_to_remove: 3,
                bytes_freed: 148,
                playlists_rewritten: 0,
            }
        );
    }
}
