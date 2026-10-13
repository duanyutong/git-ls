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
fn parses_cache_boolean_configuration_and_rejects_invalid_values() {
    for (value, expected) in [("true", true), ("false", false)] {
        assert_eq!(parse_boolean(ConfigKey::Cache, value).unwrap(), expected);
    }
    assert!(parse_boolean(ConfigKey::Cache, "unknown").is_err());
    let git = ConfigGit::default()
        .with(&["config", "--get", "git-ls.cache"], "false")
        .with(&["config", "--get", "git-ls.cachemaintenance"], "true")
        .with(&["config", "--type=bool", "--get", "git-ls.cache"], "false")
        .with(
            &["config", "--type=bool", "--get", "git-ls.cachemaintenance"],
            "true",
        );
    let config = read_git_ls_config(&git).unwrap();
    assert_eq!(config.cache, Some(false));
    assert_eq!(config.cache_maintenance, Some(true));
    for key in [ConfigKey::Cache, ConfigKey::CacheMaintenance] {
        assert!(
            read_git_ls_config(
                &ConfigGit::default().with(&["config", "--get", key.name()], "invalid")
            )
            .is_err()
        );
    }
}

#[test]
fn boolean_conversion_errors_are_preserved() {
    struct UnavailableBooleans;
    impl GitCommand for UnavailableBooleans {
        fn config_values(&self, _keys: &[&str]) -> Result<HashMap<String, String>> {
            Ok(HashMap::from([("git-ls.cache".into(), "invalid".into())]))
        }

        fn run(&self, _args: &[&str], _allow_failure: bool) -> Result<String> {
            Err(GitLsError::TestFixture(
                "Git rejected the Boolean value".into(),
            ))
        }
    }

    let error = read_git_ls_config(&UnavailableBooleans).unwrap_err();
    assert_eq!(error.to_string(), "Git rejected the Boolean value");
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
fn parses_layout_config() {
    assert_eq!(
        parse_layout_config(ConfigKey::Layout, "columns").unwrap(),
        Layout::Columns
    );
    assert_eq!(
        parse_layout_config(ConfigKey::Layout, "inline").unwrap(),
        Layout::Inline
    );

    let error = parse_layout_config(ConfigKey::Layout, "wide").unwrap_err();
    assert_invalid_config(error, ConfigKey::Layout, "wide");
}

#[test]
fn reads_empty_git_config_values_as_absent() {
    let git = ConfigGit::default().with(&["config", "--get", ConfigKey::Backend.name()], " \n");

    assert_eq!(read_git_ls_config(&git).unwrap(), GitLsConfig::default());
    assert_eq!(
        read_git_ls_config(&ConfigGit::default()).unwrap(),
        GitLsConfig::default()
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
        .with(&["config", "--get", ConfigKey::Palette.name()], "okabe")
        .with(&["config", "--get", ConfigKey::Layout.name()], "columns");

    let config = read_git_ls_config(&git).unwrap();

    assert_eq!(config.verbosity, Some(Verbosity::High));
    assert_eq!(config.backend, Some(Backend::Shell));
    assert_eq!(config.palette, Some(Palette::Okabe));
    assert_eq!(config.layout, Some(Layout::Columns));
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
        (ConfigKey::Layout, "wide"),
    ];

    for (key, value) in cases {
        let git = MockGit::default().with(&["config", "--get", key.name()], value);

        let error = read_git_ls_config(&git).unwrap_err();

        assert_invalid_config(error, key, value);
    }
}
