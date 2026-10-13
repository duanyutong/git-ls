use super::*;

#[test]
fn resolves_colour_capability_without_reading_process_state() {
    let terminal = RenderEnvironment::new(1_700_000_000, Some(80), true);
    let pipe = RenderEnvironment::new(1_700_000_000, None, false);

    assert!(terminal.colour_enabled(ColourMode::Auto));
    assert!(!pipe.colour_enabled(ColourMode::Auto));
    assert!(pipe.colour_enabled(ColourMode::Always));
    assert!(!terminal.colour_enabled(ColourMode::Never));
}

#[test]
fn carries_clock_and_terminal_width_capabilities() {
    let environment = RenderEnvironment::new(1_700_000_123, Some(42), true);

    assert_eq!(environment.now_timestamp(), 1_700_000_123);
    assert_eq!(environment.terminal_width(), Some(42));
}

#[test]
fn fits_plain_rows_to_terminal_width() {
    let line = "  ◯ feature/very-long-branch-name ci(package): publish generated api package";

    let fitted = fit_line_to_terminal_width(line, Some(24));

    assert_ne!(fitted.as_ref(), line);
    assert!(fitted.ends_with(TRUNCATION_TAIL));
    assert!(console::measure_text_width(fitted.as_ref()) <= 24);
}

#[test]
fn fits_coloured_rows_without_counting_ansi_sequences() {
    let line = "  \x1b[38;5;41mfeature/very-long-branch-name\x1b[0m \x1b[38;5;251mci(package): publish generated api package\x1b[0m";

    let fitted = fit_line_to_terminal_width(line, Some(24));
    let visible = console::strip_ansi_codes(fitted.as_ref());

    assert_ne!(fitted.as_ref(), line);
    assert!(fitted.contains("\x1b["));
    assert!(visible.ends_with(TRUNCATION_TAIL));
    assert!(console::measure_text_width(fitted.as_ref()) <= 24);
}

#[test]
fn fits_rows_to_tiny_terminal_widths() {
    assert_eq!(truncation_tail(0), "");
    assert_eq!(fit_line_to_terminal_width("abcdef", Some(0)).as_ref(), "");
    assert_eq!(fit_line_to_terminal_width("abcdef", Some(1)).as_ref(), ".");
    assert_eq!(fit_line_to_terminal_width("abcdef", Some(2)).as_ref(), "..");
    assert_eq!(
        fit_line_to_terminal_width("abcdef", None).as_ref(),
        "abcdef"
    );
}

#[test]
fn fits_columns_rows_by_preserving_the_fixed_suffix() {
    let line = RenderLine::with_fixed_suffix(
        " 1h   ⁝ │ │ │ ◯ 1 dyt/topic ci(pkg-types): derive releases from package metadata",
        "6b49c9d",
    );

    let fitted = fit_line_with_fixed_suffix(&line, Some(72));

    assert_eq!(
        fitted.as_ref(),
        " 1h   ⁝ │ │ │ ◯ 1 dyt/topic ci(pkg-types): derive releases fr... 6b49c9d"
    );
    assert_eq!(console::measure_text_width(fitted.as_ref()), 72);
}

#[test]
fn fits_fixed_suffix_rows_to_tiny_terminal_widths() {
    let line = RenderLine::with_fixed_suffix("prefix", "abcdef");

    assert_eq!(fit_line_with_fixed_suffix(&line, Some(0)).as_ref(), "");
    assert_eq!(fit_line_with_fixed_suffix(&line, Some(3)).as_ref(), "...");
}

#[test]
fn pads_columns_suffix_to_the_right_edge_when_no_truncation_is_required() {
    let line = RenderLine::with_fixed_suffix(" 1h   ◇─┘ - main", "25353c1");

    let fitted = fit_line_with_fixed_suffix(&line, Some(30));

    assert_eq!(fitted.as_ref(), " 1h   ◇─┘ - main       25353c1");
    assert_eq!(console::measure_text_width(fitted.as_ref()), 30);
}

#[test]
fn writes_lines_with_environment_terminal_width() {
    let mut output = Vec::new();
    let environment = RenderEnvironment::new(1_700_000_000, Some(5), false);

    write_rendered_line(&mut output, &RenderLine::plain("abcdefgh"), environment).unwrap();

    assert_eq!(String::from_utf8(output).unwrap(), "ab...\n");
}

struct FailingWriter;

impl Write for FailingWriter {
    fn write(&mut self, _buffer: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("closed"))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn propagates_output_write_errors() {
    let environment = RenderEnvironment::new(1_700_000_000, None, false);
    let mut writer = FailingWriter;

    assert!(writer.flush().is_ok());
    let error =
        write_rendered_line(&mut writer, &RenderLine::plain("line"), environment).unwrap_err();

    assert_eq!(error.to_string(), "failed to write output: closed");
}
