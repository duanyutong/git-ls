//! Explicit compatibility and canonical keys for disposable analysis results.
use std::path::Path;

use serde::Serialize;

use crate::backend::Inputs;

// Increment when graph interpretation, backend behaviour, or stored answers
// change incompatibly, including relevant dependency upgrades. Storage schema
// versioning is separate; dependency version strings are not a validity proof.
pub(super) const ANALYSIS_SEMANTICS: &str = "analysis-2";

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
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn canonical_keys_include_compatibility_and_every_selection_dependency() {
        let inputs = Inputs {
            branches: BTreeMap::from([("oid".into(), vec!["topic".into()])]),
            configured_main: "main".into(),
        };
        let native = FactsKey::new(BackendSemantics::Gix, Path::new("/repo"), "sha1");
        let key = native.snapshot(&inputs).unwrap();
        let json: serde_json::Value = serde_json::from_str(&key).unwrap();
        assert_eq!(json["compatibility"]["analysis"], ANALYSIS_SEMANTICS);
        assert_eq!(json["compatibility"]["backend"], "gix");
        assert_eq!(json["inputs"]["configured_main"], "main");
        for alternative in [
            FactsKey::new(BackendSemantics::Shell, Path::new("/repo"), "sha1"),
            FactsKey::new(BackendSemantics::Gix, Path::new("/other"), "sha1"),
            FactsKey::new(BackendSemantics::Gix, Path::new("/repo"), "sha256"),
        ] {
            assert_ne!(alternative.encode(), native.encode());
            assert_ne!(alternative.snapshot(&inputs), Some(key.clone()));
        }
        let mut changed = inputs.clone();
        changed.configured_main = "trunk".into();
        assert_ne!(native.snapshot(&changed), Some(key.clone()));
        changed = inputs.clone();
        changed.branches.insert("new".into(), vec!["topic".into()]);
        assert_ne!(native.snapshot(&changed), Some(key.clone()));
        changed = inputs;
        changed.branches.get_mut("oid").unwrap()[0] = "renamed".into();
        assert_ne!(native.snapshot(&changed), Some(key));
    }

    #[cfg(unix)]
    #[test]
    fn unrepresentable_paths_bypass_keys_without_lossy_aliasing() {
        use std::os::unix::ffi::OsStringExt;
        let path = std::path::PathBuf::from(std::ffi::OsString::from_vec(vec![255]));
        let key = FactsKey::new(BackendSemantics::Gix, &path, "sha1");
        assert_eq!(key.encode(), None);
        assert_eq!(
            key.snapshot(&Inputs {
                branches: BTreeMap::new(),
                configured_main: "main".into()
            }),
            None
        );
    }
}
