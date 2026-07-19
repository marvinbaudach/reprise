#[test]
fn doc_3b_source_column_keeps_its_caption_and_value_in_one_parented_section() {
    let source = include_str!("review_row.rs");

    assert!(source.contains("source_box.append(&source.section);"));
    assert!(!source.contains("source_box.append(&source.value);"));
}
