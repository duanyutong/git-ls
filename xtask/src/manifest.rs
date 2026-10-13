use semver::Version;
use toml_edit::DocumentMut;

use crate::Result;

pub(crate) const CARGO_TOML: &str = "Cargo.toml";
pub(crate) const CARGO_LOCK: &str = "Cargo.lock";

pub(crate) fn cargo_version_from_toml(input: &str) -> Result<Version> {
    let document = input.parse::<DocumentMut>()?;
    let version = document
        .get("package")
        .and_then(|package| package.get("version"))
        .and_then(|version| version.as_str())
        .ok_or("Cargo.toml is missing package.version")?;
    Ok(Version::parse(version)?)
}

pub(crate) fn cargo_lock_version(input: &str) -> Result<Version> {
    let document = input.parse::<DocumentMut>()?;
    let packages = document
        .get("package")
        .and_then(|package| package.as_array_of_tables())
        .ok_or("Cargo.lock is missing package table")?;

    for package in packages {
        if package.get("name").and_then(|name| name.as_str()) == Some("git-ls") {
            let version = package
                .get("version")
                .and_then(|version| version.as_str())
                .ok_or("Cargo.lock git-ls package is missing version")?;
            return Ok(Version::parse(version)?);
        }
    }

    Err("Cargo.lock does not contain git-ls package".into())
}

#[cfg(test)]
#[path = "tests/manifest.rs"]
mod tests;
