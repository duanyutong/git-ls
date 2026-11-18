//! Disposable, bounded storage. Fixed slots impose a hard admission limit even
//! when maintenance never runs; readers never wait for a writer or worker.
use std::collections::hash_map::DefaultHasher;
use std::fs::{self, File, OpenOptions};
use std::hash::{Hash, Hasher};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize, de::DeserializeOwned};

mod compatibility;
pub(crate) mod facts;
pub(crate) mod maintenance;
mod policy;
pub(crate) mod topology;

use compatibility::ANALYSIS_SEMANTICS as SEMANTICS;

pub(crate) use compatibility::{BackendSemantics, FactsKey};
pub(crate) use facts::Facts;

pub(crate) const SLOTS: u64 = 32;
pub(crate) const ENTRY_LIMIT: u64 = 1024 * 1024;

#[derive(Clone, Debug)]
pub(crate) struct Store {
    pub(crate) root: PathBuf,
}

pub(crate) struct CacheLock(File);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LockFailure {
    Busy,
    Unavailable,
}

impl LockFailure {
    const fn reason(self) -> &'static str {
        match self {
            Self::Busy => "busy",
            Self::Unavailable => "unavailable",
        }
    }
}

impl Drop for CacheLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

#[derive(Serialize, Deserialize)]
struct Envelope {
    semantics: String,
    key: String,
    checksum: u64,
    payload: String,
}

pub(crate) fn hash(value: &impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

struct BoundedBuffer(Vec<u8>);

impl Write for BoundedBuffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if (self.0.len() + bytes.len()) as u64 > ENTRY_LIMIT {
            return Err(std::io::Error::other("cache entry limit"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EncodingFailure {
    EntryBytes,
    Representation,
}

impl EncodingFailure {
    pub(crate) const fn reason(self) -> &'static str {
        match self {
            Self::EntryBytes => "entry_bytes",
            Self::Representation => "representation_bound",
        }
    }
}

fn encode(value: &impl Serialize) -> Result<Vec<u8>, EncodingFailure> {
    let mut buffer = BoundedBuffer(Vec::new());
    serde_json::to_writer(&mut buffer, value).map_err(|error| {
        if error.is_io() {
            EncodingFailure::EntryBytes
        } else {
            EncodingFailure::Representation
        }
    })?;
    Ok(buffer.0)
}

pub(crate) fn encoded_entry(key: &str, value: &impl Serialize) -> Option<Vec<u8>> {
    encoded_entry_checked(key, value).ok()
}

pub(crate) fn encoded_entry_checked(
    key: &str,
    value: &impl Serialize,
) -> Result<Vec<u8>, EncodingFailure> {
    let payload = String::from_utf8(encode(value)?).map_err(|_| EncodingFailure::Representation)?;
    encode(&Envelope {
        semantics: SEMANTICS.to_string(),
        key: key.to_string(),
        checksum: hash(&payload),
        payload,
    })
}

pub(crate) fn regular(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_file())
}

impl Store {
    pub(crate) fn open(common: &Path) -> Option<Self> {
        let mut root = common.canonicalize().ok()?;
        for part in ["git-ls", "cache", "v1"] {
            root.push(part);
            match fs::create_dir(&root) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(_) => return None,
            }
            if !fs::symlink_metadata(&root).ok()?.is_dir() {
                return None;
            }
        }
        Some(Self { root })
    }

    pub(crate) fn snapshot_name(key: &str) -> String {
        format!("snapshot-{:02}.json", hash(&key) % SLOTS)
    }

    pub(crate) fn read<T: DeserializeOwned>(&self, name: &str, key: &str) -> Option<T> {
        let _perf = crate::diagnostics::stage("cache_load");
        let path = self.root.join(name);
        if !regular(&path) {
            return None;
        }
        let file = File::open(path).ok()?;
        if file.metadata().ok()?.len() > ENTRY_LIMIT {
            return None;
        }
        let mut bytes = Vec::new();
        file.take(ENTRY_LIMIT + 1).read_to_end(&mut bytes).ok()?;
        if bytes.len() as u64 > ENTRY_LIMIT {
            return None;
        }
        crate::diagnostics::count("cache_bytes_read", bytes.len());
        crate::diagnostics::count("cache_entries_read", 1);
        let entry: Envelope = serde_json::from_slice(&bytes).ok()?;
        if entry.semantics != SEMANTICS
            || entry.key != key
            || entry.checksum != hash(&entry.payload)
        {
            return None;
        }
        serde_json::from_str(&entry.payload).ok()
    }

    pub(crate) fn lock(&self, name: &str) -> Option<CacheLock> {
        self.lock_checked(name).ok()
    }

    fn lock_checked(&self, name: &str) -> Result<CacheLock, LockFailure> {
        let path = self.root.join(name);
        if path.exists() && !regular(&path) {
            return Err(LockFailure::Unavailable);
        }
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)
            .map_err(|_| LockFailure::Unavailable)?;
        file.try_lock().map_err(|error| match error {
            std::fs::TryLockError::WouldBlock => LockFailure::Busy,
            std::fs::TryLockError::Error(_) => LockFailure::Unavailable,
        })?;
        Ok(CacheLock(file))
    }

    fn publication_lock(&self, name: &str) -> Option<CacheLock> {
        match self.lock_checked("publication.lock") {
            Ok(lock) => Some(lock),
            Err(reason) => {
                crate::diagnostics::event(format_args!(
                    "cache_publication=skipped reason={} entry={name}",
                    reason.reason()
                ));
                None
            }
        }
    }

    pub(crate) fn publish<T: Serialize>(&self, name: &str, key: &str, value: &T) -> bool {
        let _perf = crate::diagnostics::stage("cache_write");
        let Some(lock) = self.publication_lock(name) else {
            return false;
        };
        self.publish_locked(&lock, name, key, value)
    }

    fn publish_locked<T: Serialize>(
        &self,
        _lock: &CacheLock,
        name: &str,
        key: &str,
        value: &T,
    ) -> bool {
        let Some(bytes) = encoded_entry(key, value) else {
            crate::diagnostics::event(format_args!(
                "cache_publication=skipped reason=entry_limit_or_encoding entry={name} entry_limit_bytes={ENTRY_LIMIT}"
            ));
            return false;
        };
        self.commit_encoded(name, &bytes)
    }

    pub(crate) fn publish_encoded(&self, name: &str, bytes: &[u8]) -> bool {
        let _perf = crate::diagnostics::stage("cache_write");
        let Some(_lock) = self.publication_lock(name) else {
            return false;
        };
        self.commit_encoded(name, bytes)
    }

    pub(crate) fn update_encoded<T: DeserializeOwned>(
        &self,
        name: &str,
        key: &str,
        update: impl FnOnce(Option<T>) -> Option<Vec<u8>>,
    ) -> bool {
        let _perf = crate::diagnostics::stage("cache_write");
        let Some(_lock) = self.publication_lock(name) else {
            return false;
        };
        let Some(bytes) = update(self.read(name, key)) else {
            crate::diagnostics::event(format_args!(
                "cache_publication=skipped reason=admission_rejected entry={name}"
            ));
            return false;
        };
        self.commit_encoded(name, &bytes)
    }

    fn commit_encoded(&self, name: &str, bytes: &[u8]) -> bool {
        let written = self.write_locked(name, bytes);
        crate::diagnostics::event(format_args!(
            "cache_publication={} reason={} entry={name} bytes={}",
            if written { "published" } else { "failed" },
            if written { "complete" } else { "write_failed" },
            bytes.len()
        ));
        written
    }

    fn write_locked(&self, name: &str, bytes: &[u8]) -> bool {
        let temp = self.root.join("publication.tmp");
        // The lock permits safe reclamation of one interrupted publication.
        if temp.exists() && fs::remove_file(&temp).is_err() {
            return false;
        }
        let written = (|| -> std::io::Result<()> {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)?;
            file.write_all(bytes)?;
            file.flush()?;
            fs::rename(&temp, self.root.join(name))
        })()
        .is_ok();
        if !written {
            let _ = fs::remove_file(temp);
        }
        crate::diagnostics::count("cache_bytes_written", if written { bytes.len() } else { 0 });
        crate::diagnostics::count("cache_entries_written", usize::from(written));
        written
    }

    pub(crate) fn clear(&self) -> bool {
        let Some(_lock) = self.lock("publication.lock") else {
            return false;
        };
        let mut complete = true;
        let mut remove = |path| {
            if let Err(error) = fs::remove_file(path) {
                complete &= error.kind() == std::io::ErrorKind::NotFound;
            }
        };
        for slot in 0..SLOTS {
            remove(self.root.join(format!("snapshot-{slot:02}.json")));
        }
        for name in [
            "facts-gix.json",
            "facts-shell.json",
            "publication.tmp",
            "maintenance-state.json",
            "maintenance-report.json",
        ] {
            remove(self.root.join(name));
        }
        complete
    }
}

#[cfg(test)]
#[path = "cache/tests/repositories.rs"]
mod repositories;
