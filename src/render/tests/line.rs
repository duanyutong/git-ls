use crate::render::RenderLine;

#[test]
fn render_line_preserves_fixed_suffix_metadata() {
    let without_prefix = RenderLine::with_fixed_suffix("", "tail");
    assert_eq!(without_prefix.as_str(), "tail");
    assert_eq!(without_prefix.fixed_suffix(), Some(("", "tail")));

    let with_prefix = RenderLine::with_fixed_suffix("prefix", "tail");
    assert_eq!(with_prefix.as_str(), "prefix tail");
    assert_eq!(with_prefix.fixed_suffix(), Some(("prefix", "tail")));
}

#[test]
fn render_line_detects_only_trailing_fixed_suffixes() {
    let with_suffix = RenderLine::with_trailing_fixed_suffix(
        "rendered body abc1234".to_string(),
        "abc1234".to_string(),
    );
    assert_eq!(with_suffix.as_str(), "rendered body abc1234");
    assert_eq!(
        with_suffix.fixed_suffix(),
        Some(("rendered body", "abc1234"))
    );

    let plain = RenderLine::with_trailing_fixed_suffix(
        "rendered body abc1234".to_string(),
        "def5678".to_string(),
    );
    assert_eq!(plain.as_str(), "rendered body abc1234");
    assert_eq!(plain.fixed_suffix(), None);
}

#[test]
fn render_line_supports_string_conversions_and_comparisons() {
    let from_string = RenderLine::from("branch row".to_string());
    let from_str = RenderLine::from("branch row");
    let owned = "branch row".to_string();

    assert_eq!(from_string, owned);
    assert_eq!(owned, from_string);
    assert_eq!(from_str, "branch row");
    assert_eq!("branch row", from_str);
    assert_eq!(&*from_str, "branch row");
}
