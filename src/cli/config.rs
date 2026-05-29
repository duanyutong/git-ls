use clap::ValueEnum;

use crate::backend::{GitCommand, non_empty};
use crate::error::{GitLsError, Result};

use super::values::{Backend, Palette, Verbosity};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct GitLsConfig {
    pub(crate) verbosity: Option<Verbosity>,
    pub(crate) backend: Option<Backend>,
    pub(crate) palette: Option<Palette>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConfigKey {
    Verbosity,
    Backend,
    Palette,
}

impl ConfigKey {
    const LOOKUP_ORDER: [Self; 3] = [Self::Verbosity, Self::Backend, Self::Palette];

    fn name(self) -> &'static str {
        match self {
            Self::Verbosity => "git-ls.verbosity",
            Self::Backend => "git-ls.backend",
            Self::Palette => "git-ls.palette",
        }
    }

    fn expected(self) -> &'static str {
        match self {
            Self::Verbosity => "0, 1, or 2",
            Self::Backend => "gix or shell",
            Self::Palette => {
                "okabe, tableau, dark2, set1, set2, paired, bold, vivid, tol, or classic"
            }
        }
    }
}

fn git_config_value<G: GitCommand + ?Sized>(git: &G, key: ConfigKey) -> Result<Option<String>> {
    Ok(non_empty(&git.run(&["config", "--get", key.name()], true)?))
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
        }
        Ok(())
    }
}

pub(crate) fn read_git_ls_config<G: GitCommand + ?Sized>(git: &G) -> Result<GitLsConfig> {
    let mut config = GitLsConfig::default();
    for key in ConfigKey::LOOKUP_ORDER {
        if let Some(value) = git_config_value(git, key)? {
            config.set_from_git_config(key, &value)?;
        }
    }
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::MockGit;

    fn config_call(key: ConfigKey) -> Vec<String> {
        ["config", "--get", key.name()]
            .iter()
            .map(|value| (*value).to_string())
            .collect()
    }

    #[test]
    fn parses_integer_verbosity_config_only() {
        assert_eq!(
            parse_verbosity_config(ConfigKey::Verbosity, "0").unwrap(),
            Verbosity::Low
        );
        assert_eq!(
            parse_verbosity_config(ConfigKey::Verbosity, "1").unwrap(),
            Verbosity::Medium
        );
        assert_eq!(
            parse_verbosity_config(ConfigKey::Verbosity, "2").unwrap(),
            Verbosity::High
        );

        let error = parse_verbosity_config(ConfigKey::Verbosity, "full").unwrap_err();
        assert!(matches!(
            error,
            GitLsError::InvalidGitConfig { key, expected, .. }
                if key == ConfigKey::Verbosity.name()
                    && expected == ConfigKey::Verbosity.expected()
        ));
    }

    #[test]
    fn reads_git_ls_config_in_declared_lookup_order() {
        let git = MockGit::default();

        let config = read_git_ls_config(&git).unwrap();

        assert_eq!(config, GitLsConfig::default());
        assert_eq!(
            git.calls(),
            ConfigKey::LOOKUP_ORDER
                .into_iter()
                .map(config_call)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn reads_git_ls_config_values() {
        let git = MockGit::default()
            .with(&["config", "--get", ConfigKey::Verbosity.name()], "2")
            .with(&["config", "--get", ConfigKey::Backend.name()], "shell")
            .with(&["config", "--get", ConfigKey::Palette.name()], "okabe");

        let config = read_git_ls_config(&git).unwrap();

        assert_eq!(config.verbosity, Some(Verbosity::High));
        assert_eq!(config.backend, Some(Backend::Shell));
        assert_eq!(config.palette, Some(Palette::Okabe));
    }
}
