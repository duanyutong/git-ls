use super::super::defaults::DEFAULT_REVSET;
use super::*;

fn args_with(
    verbose: u8,
    verbosity: Option<u8>,
    backend: Option<Backend>,
    order: Option<Order>,
    colour_mode: Option<ColourMode>,
    palette: Option<Palette>,
    layout: Option<Layout>,
) -> Args {
    Args {
        revset: DEFAULT_REVSET.to_string(),
        hidden: false,
        debug: false,
        no_cache: false,
        refresh_cache: false,
        clear_cache: false,
        no_cache_maintenance: false,
        verbose,
        verbosity,
        backend,
        order,
        colour_mode,
        palette,
        layout,
    }
}

fn default_args() -> Args {
    args_with(0, None, None, None, None, None, None)
}

#[test]
fn runtime_options_use_central_defaults_without_cli_or_config() {
    let args = default_args().resolve(&GitLsConfig::default());

    assert_eq!(
        args,
        RuntimeOptions {
            revset: DEFAULT_REVSET.to_string(),
            hidden: false,
            debug: false,
            verbosity: DEFAULT_RUNTIME_OPTIONS.verbosity,
            backend: DEFAULT_RUNTIME_OPTIONS.backend,
            order: DEFAULT_RUNTIME_OPTIONS.order,
            colour_mode: DEFAULT_RUNTIME_OPTIONS.colour_mode,
            palette: DEFAULT_RUNTIME_OPTIONS.palette,
            layout: DEFAULT_RUNTIME_OPTIONS.layout,
        }
    );
}

#[test]
fn runtime_options_use_git_config_before_defaults() {
    let config = GitLsConfig {
        verbosity: Some(Verbosity::High),
        backend: Some(Backend::Shell),
        palette: Some(Palette::Okabe),
        layout: Some(Layout::Columns),
        cache: None,
        cache_maintenance: None,
    };

    let args = default_args().resolve(&config);

    assert_eq!(args.verbosity, Verbosity::High);
    assert_eq!(args.backend, Backend::Shell);
    assert_eq!(args.palette, Palette::Okabe);
    assert_eq!(args.layout, Layout::Columns);
    assert_eq!(args.order, DEFAULT_RUNTIME_OPTIONS.order);
    assert_eq!(args.colour_mode, DEFAULT_RUNTIME_OPTIONS.colour_mode);
}

#[test]
fn runtime_options_prefer_cli_over_git_config() {
    let config = GitLsConfig {
        verbosity: Some(Verbosity::High),
        backend: Some(Backend::Shell),
        palette: Some(Palette::Okabe),
        layout: Some(Layout::Columns),
        cache: None,
        cache_maintenance: None,
    };
    let args = args_with(
        1,
        Some(0),
        Some(Backend::Gix),
        Some(Order::Oldest),
        Some(ColourMode::Never),
        Some(Palette::Classic),
        Some(Layout::Inline),
    )
    .resolve(&config);

    assert_eq!(args.verbosity, Verbosity::Low);
    assert_eq!(args.backend, Backend::Gix);
    assert_eq!(args.order, Order::Oldest);
    assert_eq!(args.colour_mode, ColourMode::Never);
    assert_eq!(args.palette, Palette::Classic);
    assert_eq!(args.layout, Layout::Inline);
}
