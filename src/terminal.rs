use std::borrow::Cow;
use std::io::Write;

use console::{Term, truncate_str};

use crate::error::Result;

const TRUNCATION_TAIL: &str = "...";

fn truncation_tail(width: usize) -> &'static str {
    match width {
        0 => "",
        1 => ".",
        2 => "..",
        _ => TRUNCATION_TAIL,
    }
}

fn fit_line_to_terminal_width(line: &str, terminal_width: Option<usize>) -> Cow<'_, str> {
    let Some(width) = terminal_width else {
        return Cow::Borrowed(line);
    };
    if width == 0 {
        return Cow::Borrowed("");
    }
    truncate_str(line, width, truncation_tail(width))
}

pub(crate) fn write_rendered_line<W: Write>(
    stdout: &mut W,
    line: &str,
    terminal_width: Option<usize>,
) -> Result<()> {
    writeln!(
        stdout,
        "{}",
        fit_line_to_terminal_width(line, terminal_width)
    )?;
    Ok(())
}

pub(crate) fn terminal_output_width() -> Option<usize> {
    let terminal = Term::stdout();
    if !terminal.is_term() {
        return None;
    }
    terminal
        .size_checked()
        .map(|(_, columns)| usize::from(columns))
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(fit_line_to_terminal_width("abcdef", Some(0)).as_ref(), "");
        assert_eq!(fit_line_to_terminal_width("abcdef", Some(1)).as_ref(), ".");
        assert_eq!(fit_line_to_terminal_width("abcdef", Some(2)).as_ref(), "..");
        assert_eq!(
            fit_line_to_terminal_width("abcdef", None).as_ref(),
            "abcdef"
        );
    }
}
