use std::process::ExitCode;

fn main() -> ExitCode {
    match git_ls::run_from_env() {
        Ok(()) => ExitCode::SUCCESS,
        Err(git_ls::GitLsError::Cli(error)) => error.exit(),
        Err(error) => {
            eprintln!("git ls: {error}");
            ExitCode::FAILURE
        }
    }
}
