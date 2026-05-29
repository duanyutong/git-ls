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

fn git_config_value(git: &dyn GitCommand, key: ConfigKey) -> Result<Option<String>> {
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

pub(crate) fn read_git_ls_config(git: &dyn GitCommand) -> Result<GitLsConfig> {
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
    use std::collections::HashMap;

    use super::*;
    use crate::test_support::MockGit;

    fn config_call(key: ConfigKey) -> Vec<String> {
        ["config", "--get", key.name()]
            .iter()
            .map(|value| (*value).to_string())
            .collect()
    }

    fn invalid_config_fields(error: GitLsError) -> Option<(&'static str, String, &'static str)> {
        match error {
            GitLsError::InvalidGitConfig {
                key,
                value,
                expected,
            } => Some((key, value, expected)),
            _ => None,
        }
    }

    fn assert_invalid_config(error: GitLsError, key: ConfigKey, value: &str) {
        let (actual_key, actual_value, expected) =
            invalid_config_fields(error).expect("expected invalid git config error");
        assert_eq!(actual_key, key.name());
        assert_eq!(actual_value, value);
        assert_eq!(expected, key.expected());
    }

    #[derive(Default)]
    struct ConfigGit {
        responses: HashMap<Vec<String>, String>,
        fail: bool,
    }

    impl ConfigGit {
        fn failing() -> Self {
            Self {
                responses: HashMap::new(),
                fail: true,
            }
        }

        fn with(mut self, args: &[&str], output: &str) -> Self {
            self.responses.insert(
                args.iter().map(|arg| (*arg).to_string()).collect(),
                output.to_string(),
            );
            self
        }
    }

    impl crate::backend::GitCommand for ConfigGit {
        fn run(&self, args: &[&str], _allow_failure: bool) -> Result<String> {
            if self.fail {
                Err(GitLsError::TestFixture(format!(
                    "forced git config failure: {}",
                    args.join(" ")
                )))
            } else {
                let key: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
                Ok(self.responses.get(&key).cloned().unwrap_or_default())
            }
        }
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
        assert_invalid_config(error, ConfigKey::Verbosity, "full");
    }

    #[test]
    fn reads_empty_git_config_values_as_absent() {
        let git = ConfigGit::default().with(&["config", "--get", ConfigKey::Backend.name()], " \n");

        assert_eq!(git_config_value(&git, ConfigKey::Backend).unwrap(), None);
        assert_eq!(
            git_config_value(&ConfigGit::default(), ConfigKey::Palette).unwrap(),
            None
        );
    }

    #[test]
    fn invalid_config_field_extraction_ignores_other_errors() {
        assert_eq!(
            invalid_config_fields(GitLsError::TestFixture("wrong variant".to_string())),
            None
        );
    }

    #[test]
    fn read_git_ls_config_propagates_git_config_read_errors() {
        let error = read_git_ls_config(&ConfigGit::failing()).unwrap_err();

        assert_eq!(
            error.to_string(),
            "forced git config failure: config --get git-ls.verbosity"
        );
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
        let git = ConfigGit::default()
            .with(&["config", "--get", ConfigKey::Verbosity.name()], "2")
            .with(&["config", "--get", ConfigKey::Backend.name()], "shell")
            .with(&["config", "--get", ConfigKey::Palette.name()], "okabe");

        let config = read_git_ls_config(&git).unwrap();

        assert_eq!(config.verbosity, Some(Verbosity::High));
        assert_eq!(config.backend, Some(Backend::Shell));
        assert_eq!(config.palette, Some(Palette::Okabe));
    }

    #[test]
    fn rejects_invalid_backend_and_palette_config_values() {
        let backend_error = parse_backend_config(ConfigKey::Backend, "native").unwrap_err();
        assert_invalid_config(backend_error, ConfigKey::Backend, "native");

        let palette_error = parse_palette_config(ConfigKey::Palette, "safe").unwrap_err();
        assert_invalid_config(palette_error, ConfigKey::Palette, "safe");
    }

    #[test]
    fn rejects_invalid_git_ls_config_values_by_key() {
        let cases = [
            (ConfigKey::Verbosity, "3"),
            (ConfigKey::Backend, "native"),
            (ConfigKey::Palette, "safe"),
        ];

        for (key, value) in cases {
            let git = MockGit::default().with(&["config", "--get", key.name()], value);

            let error = read_git_ls_config(&git).unwrap_err();

            assert_invalid_config(error, key, value);
        }
    }
}
