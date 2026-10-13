use super::*;

#[test]
fn public_execution_boundary_has_stable_signature() {
    let entry_point: fn() -> Result<()> = run_from_env;
    let _ = entry_point;
}
