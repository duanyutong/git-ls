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
fn rejects_malformed_cargo_toml() {
    let error = cargo_version_from_toml(
        r#"
            [package
            name = "git-ls"
            version = "1.2.3"
        "#,
    )
    .unwrap_err();
    let message = error.to_string();

    assert!(message.contains("TOML parse error"), "{message}");
    assert!(message.contains("unclosed table"), "{message}");
}

#[test]
fn rejects_cargo_toml_missing_package_version() {
    let error = cargo_version_from_toml(
        r#"
            [package]
            name = "git-ls"
        "#,
    )
    .unwrap_err();

    assert_eq!(error.to_string(), "Cargo.toml is missing package.version");
}

#[test]
fn rejects_invalid_cargo_toml_package_version() {
    let error = cargo_version_from_toml(
        r#"
            [package]
            name = "git-ls"
            version = "invalid"
        "#,
    )
    .unwrap_err();
    let message = error.to_string();

    assert!(message.contains("unexpected character"), "{message}");
    assert!(message.contains("major version number"), "{message}");
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
fn rejects_malformed_cargo_lock() {
    let error = cargo_lock_version(
        r#"
            [[package]
            name = "git-ls"
            version = "1.2.3"
        "#,
    )
    .unwrap_err();
    let message = error.to_string();

    assert!(message.contains("TOML parse error"), "{message}");
    assert!(message.contains("unclosed array table"), "{message}");
}

#[test]
fn rejects_missing_cargo_lock_package_table() {
    let error = cargo_lock_version(
        r"
            version = 3
        ",
    )
    .unwrap_err();

    assert_eq!(error.to_string(), "Cargo.lock is missing package table");
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

#[test]
fn rejects_git_ls_package_missing_version() {
    let error = cargo_lock_version(
        r#"
            [[package]]
            name = "git-ls"
        "#,
    )
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "Cargo.lock git-ls package is missing version"
    );
}

#[test]
fn rejects_invalid_git_ls_package_version() {
    let error = cargo_lock_version(
        r#"
            [[package]]
            name = "git-ls"
            version = "invalid"
        "#,
    )
    .unwrap_err();
    let message = error.to_string();

    assert!(message.contains("unexpected character"), "{message}");
    assert!(message.contains("major version number"), "{message}");
}
