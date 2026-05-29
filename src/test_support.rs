use std::cell::RefCell;
use std::collections::HashMap;

use crate::backend::GitCommand;
use crate::error::{GitLsError, Result};

pub(crate) const TEST_NOW: i64 = 1_700_000_120;
pub(crate) const TEST_COMMIT_TIME: i64 = 1_700_000_000;

#[derive(Default)]
pub(crate) struct MockGit {
    responses: HashMap<Vec<String>, String>,
    calls: RefCell<Vec<Vec<String>>>,
}

impl MockGit {
    pub(crate) fn with(mut self, args: &[&str], output: &str) -> Self {
        self.responses.insert(
            args.iter().map(|arg| (*arg).to_string()).collect(),
            output.to_string(),
        );
        self
    }

    pub(crate) fn calls(&self) -> Vec<Vec<String>> {
        self.calls.borrow().clone()
    }
}

impl GitCommand for MockGit {
    fn run(&self, args: &[&str], allow_failure: bool) -> Result<String> {
        let key: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
        self.calls.borrow_mut().push(key.clone());
        if let Some(output) = self.responses.get(&key) {
            return Ok(output.clone());
        }
        if allow_failure {
            return Ok(String::new());
        }
        Err(GitLsError::TestFixture(format!(
            "missing mock git response: {}",
            args.join(" ")
        )))
    }
}
