use super::*;

#[test]
fn text_cells_stay_left_aligned_while_numeric_cells_are_centered() {
    assert_eq!(CellAlignment::Text.xalign(), 0.0);
    assert!(!CellAlignment::Text.uses_tabular_figures());
    assert_eq!(CellAlignment::Numeric.xalign(), 0.5);
    assert!(CellAlignment::Numeric.uses_tabular_figures());
}
