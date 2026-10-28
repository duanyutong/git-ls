use super::*;
use crate::repo_support::TestRepo;

#[test]
fn revalidation_observes_configuration_changes_after_backend_creation() {
    let repo = TestRepo::new();
    repo.commit("root");
    repo.git(&["config", "branchless.core.mainBranch", "main"]);
    let backend = gix::discover(repo.path()).unwrap();
    let before = location(repo.path()).unwrap();
    repo.git(&["config", "branchless.core.mainBranch", "trunk"]);
    assert_eq!(
        backend
            .config_snapshot()
            .string("branchless.core.mainBranch")
            .unwrap()
            .to_str_lossy(),
        "main"
    );
    let after = location(repo.path()).unwrap();
    assert_eq!(after.common, before.common);
    assert_eq!(after.object_format, before.object_format);
    assert_eq!(after.configured_main, "trunk");
    assert_ne!(before, after);
}

#[test]
fn native_boundary_capture_must_match_the_cache_context() {
    let repo = TestRepo::new();
    let root = repo.commit("root");
    let complete = crate::backend::GixBackend::discover_from(repo.path()).unwrap();
    let before = location(repo.path()).unwrap();
    assert!(before.matches_backend(&complete));

    std::fs::write(repo.path().join(".git/shallow"), format!("{root}\n")).unwrap();
    let shallow = location(repo.path()).unwrap();
    assert_eq!(shallow.shallow_boundary, vec![root]);
    assert!(!shallow.matches_backend(&complete));
    let captured = crate::backend::GixBackend::discover_from(repo.path()).unwrap();
    assert!(shallow.matches_backend(&captured));

    std::fs::remove_file(repo.path().join(".git/shallow")).unwrap();
    let unshallowed = location(repo.path()).unwrap();
    assert_eq!(before, unshallowed);
    assert!(!unshallowed.matches_backend(&captured));
}

#[test]
fn shallow_context_identity_uses_normalised_content_instead_of_file_metadata() {
    let repo = TestRepo::new();
    let root = repo.commit("root");
    let head = repo.commit("head");
    let path = repo.path().join(".git/shallow");
    std::fs::write(&path, format!("{root}\n{head}\n")).unwrap();
    let before = location(repo.path()).unwrap();
    std::fs::write(&path, format!("{head}\n{root}\n{head}\n")).unwrap();
    assert_eq!(location(repo.path()), Some(before));
    std::fs::write(&path, "not an object ID\n").unwrap();
    assert!(location(repo.path()).is_none());
}
