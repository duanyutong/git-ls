use clap::ValueEnum;

use crate::backend::GitCommand;
use crate::error::{GitLsError, Result};

use super::values::{Backend, Layout, Palette, Verbosity};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct GitLsConfig {
    pub(crate) verbosity: Option<Verbosity>,
    pub(crate) backend: Option<Backend>,
    pub(crate) palette: Option<Palette>,
    pub(crate) layout: Option<Layout>,
    pub(crate) cache: Option<bool>,
    pub(crate) cache_maintenance: Option<bool>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConfigKey {
    Verbosity,
    Backend,
    Palette,
    Layout,
    Cache,
    CacheMaintenance,
}

impl ConfigKey {
    const LOOKUP_ORDER: [Self; 6] = [
        Self::Verbosity,
        Self::Backend,
        Self::Palette,
        Self::Layout,
        Self::Cache,
        Self::CacheMaintenance,
    ];

    const fn name(self) -> &'static str {
        match self {
            Self::Verbosity => "git-ls.verbosity",
            Self::Backend => "git-ls.backend",
            Self::Palette => "git-ls.palette",
            Self::Layout => "git-ls.layout",
            Self::Cache => "git-ls.cache",
            Self::CacheMaintenance => "git-ls.cachemaintenance",
        }
    }

    const fn expected(self) -> &'static str {
        match self {
            Self::Verbosity => "0, 1, or 2",
            Self::Backend => "gix or shell",
            Self::Palette => {
                "okabe, tableau, dark2, set1, set2, paired, bold, vivid, tol, or classic"
            }
            Self::Layout => "columns or inline",
            Self::Cache | Self::CacheMaintenance => "true or false",
        }
    }
}

fn parse_boolean(key: ConfigKey, value: &str) -> Result<bool> {
    value.parse().map_err(|_| invalid_git_config(key, value))
}

fn invalid_git_config(key: ConfigKey, value: &str) -> GitLsError {
    GitLsError::invalid_git_config(key.name(), value, key.expected())
}

fn parse_backend_config(key: ConfigKey, value: &str) -> Result<Backend> {
    Backend::from_str(value, true).map_err(|_| invalid_git_config(key, value))
}

fn parse_palette_config(key: ConfigKey, value: &str) -> Result<Palette> {
    Palette::from_str(value, true).map_err(|_| invalid_git_config(key, value))
}

fn parse_layout_config(key: ConfigKey, value: &str) -> Result<Layout> {
    Layout::from_str(value, true).map_err(|_| invalid_git_config(key, value))
}

fn parse_verbosity_config(key: ConfigKey, value: &str) -> Result<Verbosity> {
    value
        .trim()
        .parse::<u8>()
        .ok()
        .and_then(Verbosity::try_from_config)
        .ok_or_else(|| invalid_git_config(key, value))
}

impl GitLsConfig {
    fn set_from_git_config(&mut self, key: ConfigKey, value: &str) -> Result<()> {
        match key {
            ConfigKey::Verbosity => self.verbosity = Some(parse_verbosity_config(key, value)?),
            ConfigKey::Backend => self.backend = Some(parse_backend_config(key, value)?),
            ConfigKey::Palette => self.palette = Some(parse_palette_config(key, value)?),
            ConfigKey::Layout => self.layout = Some(parse_layout_config(key, value)?),
            ConfigKey::Cache => self.cache = Some(parse_boolean(key, value)?),
            ConfigKey::CacheMaintenance => {
                self.cache_maintenance = Some(parse_boolean(key, value)?);
            }
        }
        Ok(())
    }
}

pub(crate) fn read_git_ls_config(git: &dyn GitCommand) -> Result<GitLsConfig> {
    let mut config = GitLsConfig::default();
    let keys = ConfigKey::LOOKUP_ORDER.map(ConfigKey::name);
    let values = git.config_values(&keys)?;
    for key in ConfigKey::LOOKUP_ORDER {
        if let Some(value) = values.get(key.name())
            && (matches!(key, ConfigKey::Cache | ConfigKey::CacheMaintenance)
                || !value.trim().is_empty())
        {
            if matches!(key, ConfigKey::Cache | ConfigKey::CacheMaintenance) {
                // Git converts the effective value, including valueless keys and integer forms.
                let canonical = git.run(&["config", "--type=bool", "--get", key.name()], false)?;
                config.set_from_git_config(key, &canonical)?;
            } else {
                config.set_from_git_config(key, value)?;
            }
        }
    }
    Ok(config)
}

#[cfg(test)]
#[path = "tests/config.rs"]
mod tests;
