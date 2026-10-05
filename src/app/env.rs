use std::env;
use std::io;

use crate::backend::{GixBackend, ProcessGit};
use crate::cli::{Backend, read_git_ls_config};
use crate::error::Result;
use crate::terminal::RenderEnvironment;

use super::cached::execute_cached;
use super::parse_args_from;

/// Executes the command-line entry point with process arguments and detected
/// terminal capabilities.
pub fn run_from_env() -> Result<()> {
    let mut stdout = io::stdout().lock();
    let environment = RenderEnvironment::detect();
    let args = parse_args_from(env::args().skip(1))?;
    crate::diagnostics::observe(args.debug, || {
        crate::diagnostics::event(format_args!(
            "clock=monotonic interval=after_arguments_before_summary"
        ));
        let config_git = ProcessGit;
        let config = {
            let _span = crate::diagnostics::stage("config");
            read_git_ls_config(&config_git)?
        };
        let raw = args;
        let args = raw.resolve(&config);
        match args.backend {
            Backend::Gix => {
                let git = {
                    let _span = crate::diagnostics::stage("backend_open");
                    GixBackend::discover()?
                };
                execute_cached(&raw, &args, &git, &mut stdout, environment)
            }
            Backend::Shell => {
                let git = ProcessGit;
                execute_cached(&raw, &args, &git, &mut stdout, environment)
            }
        }
    })
}
