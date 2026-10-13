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

#[test]
fn palette_names_and_display_text_cover_every_variant() {
    let names = [
        (Palette::Okabe, "okabe"),
        (Palette::Tableau, "tableau"),
        (Palette::Dark2, "dark2"),
        (Palette::Set1, "set1"),
        (Palette::Set2, "set2"),
        (Palette::Paired, "paired"),
        (Palette::Bold, "bold"),
        (Palette::Vivid, "vivid"),
        (Palette::Tol, "tol"),
        (Palette::Classic, "classic"),
    ];

    for (palette, name) in names {
        assert_eq!(palette.name(), name);
        assert_eq!(palette.to_string(), name);
    }
    assert_eq!(Palette::default(), Palette::Classic);
}

#[test]
fn palette_ansi_tables_cover_every_variant() {
    assert_eq!(Palette::Okabe.ansi_colours(), OKABE_PALETTE);
    assert_eq!(Palette::Tableau.ansi_colours(), TABLEAU_PALETTE);
    assert_eq!(Palette::Dark2.ansi_colours(), DARK2_PALETTE);
    assert_eq!(Palette::Set1.ansi_colours(), SET1_PALETTE);
    assert_eq!(Palette::Set2.ansi_colours(), SET2_PALETTE);
    assert_eq!(Palette::Paired.ansi_colours(), PAIRED_PALETTE);
    assert_eq!(Palette::Bold.ansi_colours(), BOLD_PALETTE);
    assert_eq!(Palette::Vivid.ansi_colours(), VIVID_PALETTE);
    assert_eq!(Palette::Tol.ansi_colours(), TOL_PALETTE);
    assert_eq!(Palette::Classic.ansi_colours(), CLASSIC_PALETTE);
}
