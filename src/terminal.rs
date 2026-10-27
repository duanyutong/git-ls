use std::borrow::Cow;
use std::io::Write;

use console::truncate_str;

use crate::cli::ColourMode;
use crate::error::Result;
use crate::render::RenderLine;

const TRUNCATION_TAIL: &str = "...";

mod detect;

const fn truncation_tail(width: usize) -> &'static str {
    match width {
        0 => "",
        1 => ".",
        2 => "..",
        _ => TRUNCATION_TAIL,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RenderEnvironment {
    now_timestamp: i64,
    terminal_width: Option<usize>,
    stdout_is_terminal: bool,
}

impl RenderEnvironment {
    pub(crate) const fn new(
        now_timestamp: i64,
        terminal_width: Option<usize>,
        stdout_is_terminal: bool,
    ) -> Self {
        Self {
            now_timestamp,
            terminal_width,
            stdout_is_terminal,
        }
    }

    pub(crate) const fn now_timestamp(self) -> i64 {
        self.now_timestamp
    }

    pub(crate) const fn terminal_width(self) -> Option<usize> {
        self.terminal_width
    }

    pub(crate) const fn colour_enabled(self, mode: ColourMode) -> bool {
        match mode {
            ColourMode::Auto => self.stdout_is_terminal,
            ColourMode::Always => true,
            ColourMode::Never => false,
        }
    }
}

fn fit_line_to_terminal_width(line: &str, terminal_width: Option<usize>) -> Cow<'_, str> {
    let Some(width) = terminal_width else {
        return Cow::Borrowed(line);
    };
    if width == 0 {
        return Cow::Borrowed("");
    }
    let fitted = truncate_str(line, width, truncation_tail(width));
    match compact_styles(&fitted) {
        Cow::Borrowed(_) => fitted,
        Cow::Owned(value) => Cow::Owned(value),
    }
}

/// Drop overwritten style changes which have no visible characters between them.
fn compact_styles(text: &str) -> Cow<'_, str> {
    if !text.contains('\u{1b}') {
        return Cow::Borrowed(text);
    }
    let mut output = String::with_capacity(text.len().min(4096));
    let mut pending = String::new();
    for (part, ansi) in console::AnsiCodeIterator::new(text) {
        if ansi && part.starts_with("\u{1b}[") && part.ends_with('m') {
            if matches!(part, "\u{1b}[0m" | "\u{1b}[m") {
                pending.clear();
            }
            pending.push_str(part);
        } else {
            output.push_str(&pending);
            pending.clear();
            output.push_str(part);
        }
    }
    output.push_str(&pending);
    if output == text {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(output)
    }
}

fn fit_line_with_fixed_suffix(line: &RenderLine, terminal_width: Option<usize>) -> Cow<'_, str> {
    let Some(width) = terminal_width else {
        return Cow::Borrowed(line.as_str());
    };
    let Some((prefix, suffix)) = line.fixed_suffix() else {
        return fit_line_to_terminal_width(line.as_str(), Some(width));
    };

    if width == 0 {
        return Cow::Borrowed("");
    }

    let suffix_width = console::measure_text_width(suffix);
    if width <= suffix_width {
        return fit_line_to_terminal_width(suffix, Some(width));
    }

    let prefix_width = width - suffix_width - 1;
    let prefix = fit_line_to_terminal_width(prefix, Some(prefix_width));
    let padding_width = width
        .saturating_sub(console::measure_text_width(prefix.as_ref()) + suffix_width)
        .max(1);

    Cow::Owned(format!("{}{}{}", prefix, " ".repeat(padding_width), suffix))
}

pub(crate) fn write_rendered_line(
    stdout: &mut dyn Write,
    line: &RenderLine,
    environment: RenderEnvironment,
) -> Result<()> {
    let fitted = fit_line_with_fixed_suffix(line, environment.terminal_width());
    crate::diagnostics::count("output_bytes", fitted.len() + 1);
    writeln!(stdout, "{fitted}")?;
    Ok(())
}

#[cfg(test)]
#[path = "tests/terminal.rs"]
mod tests;
