use std::time::{SystemTime, UNIX_EPOCH};

use console::Term;

use super::RenderEnvironment;

impl RenderEnvironment {
    pub(crate) fn detect() -> Self {
        let terminal = Term::stdout();
        let stdout_is_terminal = terminal.is_term();
        let terminal_width = if stdout_is_terminal {
            terminal
                .size_checked()
                .map(|(_, columns)| usize::from(columns))
        } else {
            None
        };
        Self::new(current_unix_timestamp(), terminal_width, stdout_is_terminal)
    }
}

fn current_unix_timestamp() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => i64::try_from(duration.as_secs()).unwrap_or(i64::MAX),
        Err(_) => 0,
    }
}
