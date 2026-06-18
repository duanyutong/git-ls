use std::env;
use std::io;

use crate::backend::{GixBackend, ProcessGit};
use crate::cli::{Backend, read_git_ls_config};
use crate::error::Result;
use crate::terminal::RenderEnvironment;

use super::{execute, parse_args_from};

/// Executes the command-line entry point with process arguments and detected
/// terminal capabilities.
pub fn run_from_env() -> Result<()> {
    let mut stdout = io::stdout().lock();
    let environment = RenderEnvironment::detect();
    let args = parse_args_from(env::args().skip(1))?;
    let config_git = ProcessGit;
    let config = read_git_ls_config(&config_git)?;
    let args = args.resolve(&config);
    match args.backend {
        Backend::Gix => {
            let git = GixBackend::discover()?;
            execute(&args, &git, &mut stdout, environment)
        }
        Backend::Shell => {
            let git = ProcessGit;
            execute(&args, &git, &mut stdout, environment)
        }
    }
}
