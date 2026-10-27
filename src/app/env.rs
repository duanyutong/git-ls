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
///
/// # Errors
/// Returns an error if arguments or configuration are invalid, repository analysis fails, or output cannot be written.
pub fn run_from_env() -> Result<()> {
    if env::args_os().nth(1).as_deref()
        == Some(std::ffi::OsStr::new(
            crate::cache::maintenance::PRIVATE_MODE,
        ))
    {
        if let (Some(path), Some(retain)) = (env::args_os().nth(2), env::args_os().nth(3))
            && let Some(retain) = retain.to_str()
        {
            crate::cache::maintenance::run(std::path::Path::new(&path), retain);
        }
        return Ok(());
    }
    let mut stdout = io::BufWriter::with_capacity(64 * 1024, io::stdout().lock());
    let environment = RenderEnvironment::detect();
    let args = parse_args_from(env::args_os().skip(1))?;
    crate::diagnostics::observe(args.debug, || {
        crate::diagnostics::event(format_args!(
            "clock=monotonic interval=after_arguments_before_summary"
        ));
        let config_git = ProcessGit::default();
        let config = {
            let _span = crate::diagnostics::stage("config");
            read_git_ls_config(&config_git)?
        };
        let mut raw = args;
        raw.no_cache |= config.cache == Some(false) && !raw.refresh_cache;
        raw.no_cache_maintenance |= config.cache_maintenance == Some(false);
        let args = raw.resolve(&config);
        match args.backend {
            Backend::Gix => {
                let git = {
                    let _span = crate::diagnostics::stage("backend_open");
                    GixBackend::discover(config_git)?
                };
                if git.requires_shell_graph()? {
                    crate::diagnostics::event(format_args!(
                        "backend_fallback requested=gix effective=shell reason=graph_interpretation"
                    ));
                    execute_cached(
                        &raw,
                        &args,
                        &ProcessGit::default(),
                        &mut stdout,
                        environment,
                    )
                } else {
                    execute_cached(&raw, &args, &git, &mut stdout, environment)
                }
            }
            Backend::Shell => {
                let git = config_git;
                execute_cached(&raw, &args, &git, &mut stdout, environment)
            }
        }
    })
}
