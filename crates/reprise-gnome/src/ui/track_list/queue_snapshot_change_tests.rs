use reprise_core::up_next::QueueItem;
use reprise_view::queue::{compose_virtual, QueueViewModel, TailChange, VirtualContext};

fn tracks(ids: &[i64]) -> Vec<QueueItem> {
    ids.iter().copied().map(QueueItem::Track).collect()
}

fn queue_pair(change: Option<TailChange>) -> (QueueViewModel, QueueViewModel) {
    let prefix = tracks(&[90, 91]);
    let old = compose_virtual(
        None,
        &prefix,
        Some(VirtualContext::identified(5, (7, 1), 4)),
        Some("Music"),
        "Music",
    );
    let new = compose_virtual(
        None,
        &prefix,
        Some(VirtualContext::identified_with_change(3, (7, 2), 4, change)),
        Some("Music"),
        "Music",
    );
    (old, new)
}

#[test]
fn queue_snapshot_change_uses_the_valid_tail_hint_triple() {
    let (old, new) = queue_pair(Some(TailChange {
        base: (7, 1),
        base_start: 4,
        position: 1,
        removed: 2,
        added: 0,
    }));

    assert_eq!(super::queue_snapshot_change(&old, &new), (3, 2, 0));
}

#[test]
fn queue_snapshot_change_without_a_hint_replaces_the_full_range() {
    let (old, new) = queue_pair(None);

    assert_eq!(super::queue_snapshot_change(&old, &new), (0, 7, 5));
}

/// The advance shape from the live bug: one leading row removed, every
/// section boundary behind it shifted. `items-changed` covers no surviving
/// row, so GTK would keep its stale header tiles — the swap MUST also emit
/// `sections-changed` over the whole model.
#[test]
fn leading_removal_with_shifted_sections_also_emits_sections_changed() {
    assert_eq!(
        super::queue_snapshot_emissions((0, 1, 0), true, 5),
        (Some((0, 1, 0)), Some((0, 5)))
    );
}

#[test]
fn removal_at_a_section_start_resections_from_there() {
    assert_eq!(
        super::queue_snapshot_emissions((3, 2, 0), true, 7),
        (Some((3, 2, 0)), Some((3, 4)))
    );
}

#[test]
fn removal_in_the_last_section_resections_its_tail_only() {
    assert_eq!(
        super::queue_snapshot_emissions((8, 1, 0), true, 10),
        (Some((8, 1, 0)), Some((8, 2)))
    );
}

#[test]
fn queue_snapshot_emissions_skips_redundant_and_illegal_signals() {
    // A full-range items-changed re-matches every header by itself.
    assert_eq!(
        super::queue_snapshot_emissions((0, 6, 5), true, 5),
        (Some((0, 6, 5)), None)
    );
    // Unchanged section ranges (plain context advance): items-changed only.
    assert_eq!(
        super::queue_snapshot_emissions((3, 1, 0), false, 5),
        (Some((3, 1, 0)), None)
    );
    // Sections moved without any row delta: sections-changed alone, no
    // fake full replace that would rebuild every row widget.
    assert_eq!(
        super::queue_snapshot_emissions((0, 0, 0), true, 5),
        (None, Some((0, 5)))
    );
    // Emptied queue: `gtk_section_model_sections_changed` requires
    // `n_items > 0`, so nothing may be emitted for a zero-row model.
    assert_eq!(
        super::queue_snapshot_emissions((0, 4, 0), true, 0),
        (Some((0, 4, 0)), None)
    );
}
