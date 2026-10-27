use super::*;

#[test]
fn verbosity_rejects_out_of_range_config_values() {
    assert_eq!(Verbosity::try_from_config(3), None);
    assert_eq!(Verbosity::from_count(7), Verbosity::High);
    assert!(Verbosity::Medium.includes_metadata());
    assert!(!Verbosity::Medium.includes_oid());
    assert!(!Verbosity::Medium.includes_title());
    assert!(Verbosity::High.includes_oid());
}

#[test]
fn layout_default_is_inline_and_columns_is_detected() {
    assert_eq!(Layout::default(), Layout::Inline);
    assert!(!Layout::Inline.is_columns());
    assert!(Layout::Columns.is_columns());
}
