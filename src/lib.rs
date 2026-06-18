//! Public crate boundary for the `git-ls` command.
//!
//! This crate is intentionally CLI-first. The supported execution surface is
//! [`run_from_env`], which reads process arguments, Git configuration, terminal
//! capabilities, and writes the rendered command output. Internal modules remain
//! private implementation details and should not be treated as a library API.

mod app;
mod backend;
mod cli;
mod error;
mod lanes;
mod model;
mod render;
mod terminal;
#[cfg(test)]
mod test_support;

pub use error::{GitLsError, Result};

/// Executes `git-ls` with process arguments and detected terminal capabilities.
///
/// This is the only supported public execution entry point. It is deliberately
/// narrow because command parsing, repository access, lane construction, and
/// rendering remain implementation details of the CLI.
pub use app::run_from_env;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_execution_boundary_has_stable_signature() {
        let entry_point: fn() -> Result<()> = run_from_env;
        let _ = entry_point;
    }
}
