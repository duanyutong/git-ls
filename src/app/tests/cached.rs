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
    assert_eq!(after.0, before.0);
    assert_eq!(after.1, before.1);
    assert_eq!(after.2, "trunk");
    assert_ne!(before, after);
}
