use super::args::Args;
use super::config::GitLsConfig;
use super::defaults::DEFAULT_RUNTIME_OPTIONS;
use super::values::{Backend, ColourMode, Layout, Order, Palette, Verbosity};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RuntimeOptions {
    pub(crate) revset: String,
    pub(crate) hidden: bool,
    pub(crate) debug: bool,
    pub(crate) verbosity: Verbosity,
    pub(crate) backend: Backend,
    pub(crate) order: Order,
    pub(crate) colour_mode: ColourMode,
    pub(crate) palette: Palette,
    pub(crate) layout: Layout,
}

impl RuntimeOptions {
    fn resolve(args: &Args, config: &GitLsConfig) -> Self {
        let defaults = DEFAULT_RUNTIME_OPTIONS;

        Self {
            revset: args.revset.clone(),
            hidden: args.hidden,
            debug: args.debug,
            verbosity: args
                .explicit_verbosity()
                .or(config.verbosity)
                .unwrap_or(defaults.verbosity),
            backend: args.backend.or(config.backend).unwrap_or(defaults.backend),
            order: args.order.unwrap_or(defaults.order),
            colour_mode: args.colour_mode.unwrap_or(defaults.colour_mode),
            palette: args.palette.or(config.palette).unwrap_or(defaults.palette),
            layout: args.layout.or(config.layout).unwrap_or(defaults.layout),
        }
    }
}

impl Args {
    pub(crate) fn resolve(&self, config: &GitLsConfig) -> RuntimeOptions {
        RuntimeOptions::resolve(self, config)
    }

    fn explicit_verbosity(&self) -> Option<Verbosity> {
        self.verbosity
            .map(Verbosity::from_count)
            .or_else(|| (self.verbose > 0).then(|| Verbosity::from_count(self.verbose)))
    }
}

#[cfg(test)]
#[path = "tests/runtime.rs"]
mod tests;
