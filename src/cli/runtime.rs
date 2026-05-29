use super::args::Args;
use super::config::GitLsConfig;
use super::defaults::DEFAULT_RUNTIME_OPTIONS;
use super::values::{Backend, ColourMode, Order, Palette, Verbosity};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RuntimeOptions {
    pub(crate) revset: String,
    pub(crate) hidden: bool,
    pub(crate) verbosity: Verbosity,
    pub(crate) backend: Backend,
    pub(crate) order: Order,
    pub(crate) colour_mode: ColourMode,
    pub(crate) palette: Palette,
}

impl RuntimeOptions {
    fn resolve(args: &Args, config: &GitLsConfig) -> Self {
        let defaults = DEFAULT_RUNTIME_OPTIONS;

        Self {
            revset: args.revset.clone(),
            hidden: args.hidden,
            verbosity: args
                .explicit_verbosity()
                .or(config.verbosity)
                .unwrap_or(defaults.verbosity),
            backend: args.backend.or(config.backend).unwrap_or(defaults.backend),
            order: args.order.unwrap_or(defaults.order),
            colour_mode: args.colour_mode.unwrap_or(defaults.colour_mode),
            palette: args.palette.or(config.palette).unwrap_or(defaults.palette),
        }
    }
}

impl Args {
    pub(crate) fn resolve(&self, config: &GitLsConfig) -> RuntimeOptions {
        RuntimeOptions::resolve(self, config)
    }

    fn explicit_verbosity(&self) -> Option<Verbosity> {
        (self.verbose > 0).then(|| Verbosity::from_count(self.verbose))
    }
}

#[cfg(test)]
mod tests {
    use super::super::defaults::DEFAULT_REVSET;
    use super::*;

    fn args_with(
        verbose: u8,
        backend: Option<Backend>,
        order: Option<Order>,
        colour_mode: Option<ColourMode>,
        palette: Option<Palette>,
    ) -> Args {
        Args {
            revset: DEFAULT_REVSET.to_string(),
            hidden: false,
            verbose,
            backend,
            order,
            colour_mode,
            palette,
        }
    }

    fn default_args() -> Args {
        args_with(0, None, None, None, None)
    }

    #[test]
    fn runtime_options_use_central_defaults_without_cli_or_config() {
        let args = default_args().resolve(&GitLsConfig::default());

        assert_eq!(
            args,
            RuntimeOptions {
                revset: DEFAULT_REVSET.to_string(),
                hidden: false,
                verbosity: DEFAULT_RUNTIME_OPTIONS.verbosity,
                backend: DEFAULT_RUNTIME_OPTIONS.backend,
                order: DEFAULT_RUNTIME_OPTIONS.order,
                colour_mode: DEFAULT_RUNTIME_OPTIONS.colour_mode,
                palette: DEFAULT_RUNTIME_OPTIONS.palette,
            }
        );
    }

    #[test]
    fn runtime_options_use_git_config_before_defaults() {
        let config = GitLsConfig {
            verbosity: Some(Verbosity::High),
            backend: Some(Backend::Shell),
            palette: Some(Palette::Okabe),
        };

        let args = default_args().resolve(&config);

        assert_eq!(args.verbosity, Verbosity::High);
        assert_eq!(args.backend, Backend::Shell);
        assert_eq!(args.palette, Palette::Okabe);
        assert_eq!(args.order, DEFAULT_RUNTIME_OPTIONS.order);
        assert_eq!(args.colour_mode, DEFAULT_RUNTIME_OPTIONS.colour_mode);
    }

    #[test]
    fn runtime_options_prefer_cli_over_git_config() {
        let config = GitLsConfig {
            verbosity: Some(Verbosity::High),
            backend: Some(Backend::Shell),
            palette: Some(Palette::Okabe),
        };
        let args = args_with(
            1,
            Some(Backend::Gix),
            Some(Order::Oldest),
            Some(ColourMode::Never),
            Some(Palette::Classic),
        )
        .resolve(&config);

        assert_eq!(args.verbosity, Verbosity::Medium);
        assert_eq!(args.backend, Backend::Gix);
        assert_eq!(args.order, Order::Oldest);
        assert_eq!(args.colour_mode, ColourMode::Never);
        assert_eq!(args.palette, Palette::Classic);
    }
}
