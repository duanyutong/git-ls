use semver::Version;
use toml_edit::DocumentMut;

use crate::Result;

pub(crate) const CARGO_TOML: &str = "Cargo.toml";
pub(crate) const CARGO_LOCK: &str = "Cargo.lock";

pub(crate) fn cargo_version_from_toml(input: &str) -> Result<Version> {
    let document = input.parse::<DocumentMut>()?;
    let version = document["package"]["version"]
        .as_str()
        .ok_or("Cargo.toml is missing package.version")?;
    Ok(Version::parse(version)?)
}

pub(crate) fn cargo_lock_version(input: &str) -> Result<Version> {
    let document = input.parse::<DocumentMut>()?;
    let packages = document["package"]
        .as_array_of_tables()
        .ok_or("Cargo.lock is missing package table")?;

    for package in packages {
        if package["name"].as_str() == Some("git-ls") {
            let version = package["version"]
                .as_str()
                .ok_or("Cargo.lock git-ls package is missing version")?;
            return Ok(Version::parse(version)?);
        }
    }

    Err("Cargo.lock does not contain git-ls package".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cargo_toml_package_version() {
        let version = cargo_version_from_toml(
            r#"
                [package]
                name = "git-ls"
                version = "1.2.3"
            "#,
        )
        .expect("version parses");

        assert_eq!(version.to_string(), "1.2.3");
    }

    #[test]
    fn parses_git_ls_version_from_cargo_lock() {
        let version = cargo_lock_version(
            r#"
                [[package]]
                name = "dependency"
                version = "0.1.0"

                [[package]]
                name = "git-ls"
                version = "1.2.3"
            "#,
        )
        .expect("lock version parses");

        assert_eq!(version.to_string(), "1.2.3");
    }

    #[test]
    fn rejects_missing_git_ls_package_in_cargo_lock() {
        let error = cargo_lock_version(
            r#"
                [[package]]
                name = "dependency"
                version = "0.1.0"
            "#,
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "Cargo.lock does not contain git-ls package"
        );
    }
}
