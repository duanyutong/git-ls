use clap::ValueEnum;

use crate::backend::GitCommand;
use crate::error::{GitLsError, Result};

use super::values::{Backend, Layout, Verbosity};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct GitLsConfig {
    pub(crate) verbosity: Option<Verbosity>,
    pub(crate) diff_stat: Option<bool>,
    pub(crate) backend: Option<Backend>,
    pub(crate) layout: Option<Layout>,
    pub(crate) cache: Option<bool>,
    pub(crate) cache_maintenance: Option<bool>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConfigKey {
    Verbosity,
    DiffStat,
    Backend,
    Layout,
    Cache,
    CacheMaintenance,
}

impl ConfigKey {
    const LOOKUP_ORDER: [Self; 6] = [
        Self::Verbosity,
        Self::DiffStat,
        Self::Backend,
        Self::Layout,
        Self::Cache,
        Self::CacheMaintenance,
    ];

    const fn name(self) -> &'static str {
        match self {
            Self::Verbosity => "git-ls.verbosity",
            Self::DiffStat => "git-ls.diffstat",
            Self::Backend => "git-ls.backend",
            Self::Layout => "git-ls.layout",
            Self::Cache => "git-ls.cache",
            Self::CacheMaintenance => "git-ls.cachemaintenance",
        }
    }

    const fn expected(self) -> &'static str {
        match self {
            Self::Verbosity => "0, 1, or 2",
            Self::Backend => "gix or shell",
            Self::Layout => "columns or inline",
            Self::DiffStat | Self::Cache | Self::CacheMaintenance => "true or false",
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
            ConfigKey::DiffStat => self.diff_stat = Some(parse_boolean(key, value)?),
            ConfigKey::Backend => self.backend = Some(parse_backend_config(key, value)?),
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
    let booleans = git.boolean_values(&values)?;
    for key in ConfigKey::LOOKUP_ORDER {
        if let Some(value) = values.get(key.name())
            && (matches!(
                key,
                ConfigKey::DiffStat | ConfigKey::Cache | ConfigKey::CacheMaintenance
            ) || !value.trim().is_empty())
        {
            if matches!(
                key,
                ConfigKey::DiffStat | ConfigKey::Cache | ConfigKey::CacheMaintenance
            ) {
                // Git converts the effective value, including valueless keys and integer forms.
                let canonical = booleans.get(key.name()).map_or("", String::as_str);
                config.set_from_git_config(key, canonical)?;
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
