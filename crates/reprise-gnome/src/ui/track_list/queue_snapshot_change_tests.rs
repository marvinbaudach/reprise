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
