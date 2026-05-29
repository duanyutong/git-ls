use std::borrow::Cow;
use std::io::Write;

use console::{Term, truncate_str};

use crate::error::Result;

pub(crate) const TRUNCATION_TAIL: &str = "...";

fn truncation_tail(width: usize) -> &'static str {
    match width {
        0 => "",
        1 => ".",
        2 => "..",
        _ => TRUNCATION_TAIL,
    }
}

pub(crate) fn fit_line_to_terminal_width(
    line: &str,
    terminal_width: Option<usize>,
) -> Cow<'_, str> {
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
