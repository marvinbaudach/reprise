use super::*;

#[test]
fn style_14_numeric_columns_align_right() {
    assert_eq!(CellAlignment::Text.xalign(), 0.0);
    assert!(!CellAlignment::Text.uses_tabular_figures());
    assert_eq!(CellAlignment::Numeric.xalign(), 1.0);
    assert!(CellAlignment::Numeric.uses_tabular_figures());
}
