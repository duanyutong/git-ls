use super::values::{Backend, ColourMode, Layout, Order, Palette, Verbosity};

pub(crate) const DEFAULT_REVSET: &str = "draft()";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RuntimeDefaults {
    pub(super) verbosity: Verbosity,
    pub(super) backend: Backend,
    pub(super) order: Order,
    pub(super) colour_mode: ColourMode,
    pub(super) palette: Palette,
    pub(super) layout: Layout,
}

pub(super) const DEFAULT_RUNTIME_OPTIONS: RuntimeDefaults = RuntimeDefaults {
    verbosity: Verbosity::Medium,
    backend: Backend::Gix,
    order: Order::Newest,
    colour_mode: ColourMode::Auto,
    palette: Palette::Classic,
    layout: Layout::Inline,
};
