//! Disposable, bounded storage. Fixed slots impose a hard admission limit even
//! when maintenance never runs; readers never wait for a writer or worker.
use std::collections::hash_map::DefaultHasher;
use std::fs::{self, File, OpenOptions};
use std::hash::{Hash, Hasher};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize, de::DeserializeOwned};

pub(crate) mod maintenance;

pub(crate) const SLOTS: u64 = 32;
pub(crate) const ENTRY_LIMIT: u64 = 1024 * 1024;
const SEMANTICS: &str = "analysis-1";

#[derive(Clone, Debug)]
pub(crate) struct Store {
    pub(crate) root: PathBuf,
}

pub(crate) struct CacheLock(File);

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

fn encode(value: &impl Serialize) -> Option<Vec<u8>> {
    let mut buffer = BoundedBuffer(Vec::new());
    serde_json::to_writer(&mut buffer, value).ok()?;
    Some(buffer.0)
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
        let path = self.root.join(name);
        if path.exists() && !regular(&path) {
            return None;
        }
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)
            .ok()?;
        file.try_lock().ok()?;
        Some(CacheLock(file))
    }

    pub(crate) fn publish<T: Serialize>(&self, name: &str, key: &str, value: &T) -> bool {
        let _perf = crate::diagnostics::stage("cache_write");
        let Some(lock) = self.lock("publication.lock") else {
            crate::diagnostics::event(format_args!(
                "cache_publication=skipped reason=busy_or_unavailable"
            ));
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
        let Some(payload) = encode(value).and_then(|bytes| String::from_utf8(bytes).ok()) else {
            return false;
        };
        let entry = Envelope {
            semantics: SEMANTICS.to_string(),
            key: key.to_string(),
            checksum: hash(&payload),
            payload,
        };
        let Some(bytes) = encode(&entry) else {
            crate::diagnostics::event(format_args!("cache_publication=skipped reason=entry_limit"));
            return false;
        };
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
            file.write_all(&bytes)?;
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
mod tests;
