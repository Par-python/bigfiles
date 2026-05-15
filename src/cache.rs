use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

const HASH_ALGORITHM: &str = "blake3";
const CACHE_FILENAME: &str = "hashes.json";

#[derive(Serialize, Deserialize, Clone)]
struct CacheEntry {
    mtime_ns: u128,
    size: u64,
    hash_hex: String,
}

#[derive(Serialize, Deserialize, Default)]
struct CacheFile {
    algorithm: String,
    entries: HashMap<String, CacheEntry>,
}

pub struct HashCache {
    inner: Mutex<HashMap<String, CacheEntry>>,
    dirty: Mutex<bool>,
}

impl HashCache {
    pub fn load() -> Self {
        let path = match cache_path() {
            Some(p) => p,
            None => return Self::empty(),
        };
        let bytes = match fs::read(&path) {
            Ok(b) => b,
            Err(_) => return Self::empty(),
        };
        let parsed: CacheFile = match serde_json::from_slice(&bytes) {
            Ok(p) => p,
            Err(_) => return Self::empty(),
        };
        if parsed.algorithm != HASH_ALGORITHM {
            return Self::empty();
        }
        HashCache {
            inner: Mutex::new(parsed.entries),
            dirty: Mutex::new(false),
        }
    }

    pub fn empty() -> Self {
        HashCache {
            inner: Mutex::new(HashMap::new()),
            dirty: Mutex::new(false),
        }
    }

    pub fn get(&self, path: &Path, mtime: SystemTime, size: u64) -> Option<[u8; 32]> {
        let key = path.to_string_lossy().into_owned();
        let mtime_ns = systime_to_ns(mtime)?;
        let guard = self.inner.lock().ok()?;
        let entry = guard.get(&key)?;
        if entry.mtime_ns != mtime_ns || entry.size != size {
            return None;
        }
        decode_hash(&entry.hash_hex)
    }

    pub fn insert(&self, path: &Path, mtime: SystemTime, size: u64, hash: [u8; 32]) {
        let Some(mtime_ns) = systime_to_ns(mtime) else {
            return;
        };
        let key = path.to_string_lossy().into_owned();
        let entry = CacheEntry {
            mtime_ns,
            size,
            hash_hex: encode_hash(&hash),
        };
        if let Ok(mut guard) = self.inner.lock() {
            guard.insert(key, entry);
        }
        if let Ok(mut d) = self.dirty.lock() {
            *d = true;
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let dirty = self.dirty.lock().map(|g| *g).unwrap_or(false);
        if !dirty {
            return Ok(());
        }
        let Some(path) = cache_path() else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let entries = self.inner.lock().map(|g| g.clone()).unwrap_or_default();
        let pruned: HashMap<String, CacheEntry> = entries
            .into_iter()
            .filter(|(k, _)| Path::new(k).exists())
            .collect();
        let file = CacheFile {
            algorithm: HASH_ALGORITHM.to_string(),
            entries: pruned,
        };
        let bytes = serde_json::to_vec(&file).map_err(std::io::Error::other)?;
        fs::write(&path, bytes)?;
        Ok(())
    }
}

pub fn cache_path() -> Option<PathBuf> {
    let base = dirs::cache_dir()?;
    Some(base.join("bigfiles").join(CACHE_FILENAME))
}

pub fn clear() -> std::io::Result<()> {
    let Some(path) = cache_path() else {
        return Ok(());
    };
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

fn systime_to_ns(t: SystemTime) -> Option<u128> {
    t.duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_nanos())
}

fn encode_hash(h: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for b in h {
        use std::fmt::Write as _;
        let _ = write!(s, "{:02x}", b);
    }
    s
}

fn decode_hash(s: &str) -> Option<[u8; 32]> {
    if s.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        let hi = u8::from_str_radix(&s[i * 2..i * 2 + 1], 16).ok()?;
        let lo = u8::from_str_radix(&s[i * 2 + 1..i * 2 + 2], 16).ok()?;
        *byte = (hi << 4) | lo;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn hash_roundtrip() {
        let h = [0xABu8; 32];
        let encoded = encode_hash(&h);
        assert_eq!(encoded.len(), 64);
        let decoded = decode_hash(&encoded).unwrap();
        assert_eq!(decoded, h);
    }

    #[test]
    fn miss_on_empty_cache() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("x");
        File::create(&p).unwrap().write_all(b"hi").unwrap();
        let meta = fs::metadata(&p).unwrap();
        let c = HashCache::empty();
        assert!(c.get(&p, meta.modified().unwrap(), meta.len()).is_none());
    }

    #[test]
    fn hit_after_insert() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("x");
        File::create(&p).unwrap().write_all(b"hi").unwrap();
        let meta = fs::metadata(&p).unwrap();
        let mtime = meta.modified().unwrap();
        let h = [0x42u8; 32];
        let c = HashCache::empty();
        c.insert(&p, mtime, meta.len(), h);
        assert_eq!(c.get(&p, mtime, meta.len()), Some(h));
    }

    #[test]
    fn miss_when_size_changes() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("x");
        File::create(&p).unwrap().write_all(b"hi").unwrap();
        let meta = fs::metadata(&p).unwrap();
        let mtime = meta.modified().unwrap();
        let h = [0x42u8; 32];
        let c = HashCache::empty();
        c.insert(&p, mtime, meta.len(), h);
        assert!(c.get(&p, mtime, meta.len() + 1).is_none());
    }

    #[test]
    fn miss_when_mtime_changes() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("x");
        File::create(&p).unwrap().write_all(b"hi").unwrap();
        let meta = fs::metadata(&p).unwrap();
        let h = [0x42u8; 32];
        let c = HashCache::empty();
        c.insert(&p, meta.modified().unwrap(), meta.len(), h);
        let later = meta.modified().unwrap() + std::time::Duration::from_secs(1);
        assert!(c.get(&p, later, meta.len()).is_none());
    }
}
