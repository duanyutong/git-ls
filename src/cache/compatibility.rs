//! Explicit compatibility and canonical keys for disposable analysis results.
use std::path::Path;

use serde::Serialize;

use crate::backend::Inputs;

// Increment when graph interpretation, backend behaviour, or stored answers
// change incompatibly, including relevant dependency upgrades. Storage schema
// versioning is separate; dependency version strings are not a validity proof.
pub(super) const ANALYSIS_SEMANTICS: &str = "analysis-4";

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BackendSemantics {
    Gix,
    Shell,
}

#[derive(Serialize)]
pub(crate) struct FactsKey<'a> {
    analysis: &'static str,
    backend: BackendSemantics,
    object_format: &'a str,
    repository: &'a Path,
}

impl<'a> FactsKey<'a> {
    pub(crate) const fn new(
        backend: BackendSemantics,
        repository: &'a Path,
        object_format: &'a str,
    ) -> Self {
        Self {
            analysis: ANALYSIS_SEMANTICS,
            backend,
            object_format,
            repository,
        }
    }

    pub(crate) fn encode(&self) -> Option<String> {
        serde_json::to_string(self).ok()
    }

    pub(crate) fn snapshot(&self, inputs: &Inputs) -> Option<String> {
        #[derive(Serialize)]
        struct SnapshotKey<'a, 'b> {
            compatibility: &'a FactsKey<'b>,
            inputs: &'a Inputs,
        }
        serde_json::to_string(&SnapshotKey {
            compatibility: self,
            inputs,
        })
        .ok()
    }
}

#[cfg(test)]
#[path = "tests/compatibility.rs"]
mod tests;
