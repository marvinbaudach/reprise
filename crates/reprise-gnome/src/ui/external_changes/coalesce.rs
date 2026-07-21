//! Pure change-log → coarse-refresh algebra for the external-changes runtime.
//!
//! No GTK here: the runtime consumes [`reprise_core::events::Change`] rows and
//! folds them into a [`RefreshPlan`] that names which coarse, navigation-neutral
//! refresh paths must run. Keeping filter + coalescing a pure function makes it
//! unit-testable headlessly; the async plumbing that drives it lives in the
//! parent module and is exercised only by the single display test.

use reprise_core::events::{Change, WriterToken};

/// The coarse refreshes a batch of foreign changes asks for. Both fields map to
/// an existing, navigation-neutral refresh path: `sidebar` → `Sidebar::refresh`
/// (rebuilds counts/rows and re-selects the current source), `track_list` →
/// `TrackList::reload` (TAG-1: preserves selection and scroll, skips untouched
/// lists). A plan is a *set* of views to refresh, never a list of operations to
/// replay — which is what makes at-least-once delivery plus coalescing
/// idempotent.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::ui) struct RefreshPlan {
    pub(in crate::ui) sidebar: bool,
    pub(in crate::ui) track_list: bool,
}

impl RefreshPlan {
    /// Nothing to refresh — the runtime sends no command for an empty plan.
    pub(in crate::ui) const fn is_empty(self) -> bool {
        !self.sidebar && !self.track_list
    }

    /// Union of two plans. Coalescing is repeated union, so it is idempotent
    /// (`plan.union(plan) == plan`) and order-independent by construction.
    const fn union(self, other: Self) -> Self {
        Self {
            sidebar: self.sidebar || other.sidebar,
            track_list: self.track_list || other.track_list,
        }
    }
}

/// Which coarse views a single entity's change touches.
///
/// The entities are exactly those the P0 facades record: `playlist`,
/// `smart_playlist`, `settings`, `library`. `settings` maps to nothing — a
/// coarse view reload cannot *apply* a foreign setting change, and external
/// setting writes are out of scope for v1 (multi-frontend-core plan, 3.3). Any
/// unknown entity — notably Track 2's future `ai_jobs`/`provenance` rows — also
/// maps to nothing: this Track 1 runtime deliberately ignores entities it does
/// not own, leaving them to their own package's runtime.
fn impact(entity: &str) -> RefreshPlan {
    match entity {
        // Playlists and smart playlists live in the sidebar (list + counts) and,
        // when the affected one is open, in the track list — contents, rename,
        // or a delete that takes the existing empty-state path. A library scan
        // adds/removes tracks, changing both the sidebar counts and the current
        // view's rows.
        "playlist" | "smart_playlist" | "library" => RefreshPlan {
            sidebar: true,
            track_list: true,
        },
        _ => RefreshPlan::default(),
    }
}

/// Filters the app's own writes and coalesces the rest into one [`RefreshPlan`].
///
/// `excluded` is the app process's own writer token: the app already refreshes
/// itself after its own mutations, so replaying them here would be redundant
/// (and, without the filter, every self-write would trigger a second refresh).
/// Everything from another writer folds — per entity and across entities — into
/// a single coarse plan.
pub(in crate::ui) fn plan_for(changes: &[Change], excluded: Option<WriterToken>) -> RefreshPlan {
    changes
        .iter()
        .filter(|change| excluded != Some(change.writer))
        .fold(RefreshPlan::default(), |plan, change| {
            plan.union(impact(&change.entity))
        })
}

#[cfg(test)]
mod tests {
    use super::{impact, plan_for, RefreshPlan};
    use reprise_core::events::{writer_token, Change};

    /// A change carrying this process's writer token (the only token a
    /// single-process test can mint — see `writer_token`'s `OnceLock`).
    fn change(entity: &str, operation: &str) -> Change {
        Change {
            id: 1,
            entity: entity.to_string(),
            entity_id: "1".to_string(),
            operation: operation.to_string(),
            writer: writer_token(),
            at: 0,
        }
    }

    #[test]
    fn refresh_plan_default_is_empty() {
        assert!(RefreshPlan::default().is_empty());
    }

    #[test]
    fn impact_playlist_refreshes_sidebar_and_track_list() {
        assert_eq!(
            impact("playlist"),
            RefreshPlan {
                sidebar: true,
                track_list: true
            }
        );
    }

    #[test]
    fn impact_smart_playlist_refreshes_sidebar_and_track_list() {
        assert_eq!(
            impact("smart_playlist"),
            RefreshPlan {
                sidebar: true,
                track_list: true
            }
        );
    }

    #[test]
    fn impact_library_scan_refreshes_sidebar_and_track_list() {
        assert_eq!(
            impact("library"),
            RefreshPlan {
                sidebar: true,
                track_list: true
            }
        );
    }

    #[test]
    fn impact_settings_requests_no_refresh() {
        assert!(impact("settings").is_empty());
    }

    #[test]
    fn impact_unknown_track_two_entities_request_no_refresh() {
        // Track 2's future entities must not trip this Track 1 runtime.
        assert!(impact("ai_jobs").is_empty());
        assert!(impact("provenance").is_empty());
    }

    #[test]
    fn plan_for_empty_batch_is_empty() {
        assert!(plan_for(&[], Some(writer_token())).is_empty());
        assert!(plan_for(&[], None).is_empty());
    }

    #[test]
    fn plan_for_keeps_a_change_when_nothing_is_excluded() {
        assert_eq!(
            plan_for(&[change("playlist", "create")], None),
            RefreshPlan {
                sidebar: true,
                track_list: true
            }
        );
    }

    #[test]
    fn plan_for_drops_a_change_from_the_excluded_writer() {
        // The change carries the process token; excluding it silences the plan
        // (the app already refreshed itself for its own write).
        assert!(plan_for(&[change("playlist", "create")], Some(writer_token())).is_empty());
    }

    #[test]
    fn plan_for_coalesces_many_playlist_ops_into_one_plan() {
        let batch: Vec<Change> = ["create", "rename", "add", "remove", "move", "delete"]
            .iter()
            .map(|operation| change("playlist", operation))
            .collect();
        // Six operations on the same entity, still exactly one coarse refresh.
        assert_eq!(
            plan_for(&batch, None),
            RefreshPlan {
                sidebar: true,
                track_list: true
            }
        );
    }

    #[test]
    fn plan_for_unions_mixed_entities_and_ignores_inert_ones() {
        let batch = vec![change("settings", "set"), change("playlist", "create")];
        // settings contributes nothing; the playlist drives the union.
        assert_eq!(
            plan_for(&batch, None),
            RefreshPlan {
                sidebar: true,
                track_list: true
            }
        );
    }

    #[test]
    fn plan_for_is_order_independent() {
        let settings = change("settings", "set");
        let library = change("library", "scan");
        assert_eq!(
            plan_for(&[settings.clone(), library.clone()], None),
            plan_for(&[library, settings], None)
        );
    }
}
