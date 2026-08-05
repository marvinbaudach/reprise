#[test]
fn doc_9b_rows_carry_no_caption_labels() {
    let source = include_str!("review_row.rs");

    assert!(!source.contains("value_widgets("));
}
