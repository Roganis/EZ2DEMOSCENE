//! Asset bytes by path.
//!
//! Desktop builds read asset files from disk. In the browser there is no
//! filesystem: imported files and unpacked packs are kept in memory under
//! `mem://…` paths. Everything that loads an asset (textures, models, music)
//! goes through [`read`], so both work the same way.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// Prefix of in-memory asset paths.
pub const MEM_PREFIX: &str = "mem://";

fn mem() -> &'static Mutex<HashMap<String, Arc<[u8]>>> {
    static MEM: OnceLock<Mutex<HashMap<String, Arc<[u8]>>>> = OnceLock::new();
    MEM.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Store bytes under `path` (usually a `mem://` path).
pub fn insert(path: &str, bytes: impl Into<Arc<[u8]>>) {
    mem().lock().unwrap().insert(path.to_string(), bytes.into());
}

/// Store bytes under a fresh `mem://` path derived from `name` and return it.
pub fn insert_new(name: &str, bytes: impl Into<Arc<[u8]>>) -> String {
    let clean: String = name
        .chars()
        .map(|c| if c == '/' || c == '\\' { '_' } else { c })
        .collect();
    let mut map = mem().lock().unwrap();
    let mut path = format!("{MEM_PREFIX}{clean}");
    let mut k = 2;
    while map.contains_key(&path) {
        path = format!("{MEM_PREFIX}{k}_{clean}");
        k += 1;
    }
    map.insert(path.clone(), bytes.into());
    path
}

pub fn remove(path: &str) {
    mem().lock().unwrap().remove(path);
}

pub fn is_mem(path: &str) -> bool {
    path.starts_with(MEM_PREFIX)
}

/// All in-memory paths (for persisting them in the browser).
pub fn mem_paths() -> Vec<String> {
    mem().lock().unwrap().keys().cloned().collect()
}

/// Bytes of an asset: in-memory first, then (on desktop) the filesystem.
pub fn read(path: &str) -> std::io::Result<Arc<[u8]>> {
    if let Some(b) = mem().lock().unwrap().get(path) {
        return Ok(b.clone());
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Ok(std::fs::read(path)?.into())
    }
    #[cfg(target_arch = "wasm32")]
    {
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{path} is not loaded (re-import it or open the pack again)"),
        ))
    }
}

/// True if [`read`] would succeed.
pub fn exists(path: &str) -> bool {
    if mem().lock().unwrap().contains_key(path) {
        return true;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::path::Path::new(path).is_file()
    }
    #[cfg(target_arch = "wasm32")]
    {
        false
    }
}

/// File name part of a path (works for `mem://` paths too).
pub fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// Lower-case extension of a path.
pub fn extension(path: &str) -> String {
    let name = file_name(path);
    name.rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_assets() {
        let p = insert_new("my logo.png", vec![1u8, 2, 3]);
        assert!(is_mem(&p));
        assert_eq!(&*read(&p).unwrap(), &[1, 2, 3]);
        let q = insert_new("my logo.png", vec![4u8]);
        assert_ne!(p, q, "names are made unique");
        assert_eq!(extension(&q), "png");
        assert_eq!(file_name("mem://a/b.obj"), "b.obj");
        remove(&p);
        assert!(!exists(&p));
    }
}
