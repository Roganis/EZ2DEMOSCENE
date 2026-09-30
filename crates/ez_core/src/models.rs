//! The bundled model library: a curated set of CC0 low-poly models (see
//! `tools/model_library.py`), one zip holding `index.json` and a `.glb` per
//! model.
//!
//! Desktop builds carry the zip inside the executable. The web build (and
//! the Android app, which runs it) loads it from `library.zip` next to the
//! page the first time a model is wanted: [`wanted`] tells the app to fetch
//! it, and [`install`] hands it over.

use schemars::JsonSchema;
use serde::Deserialize;
use std::io::Read;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, RwLock};

/// One model of the library.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct Entry {
    /// Stable id saved in projects, e.g. `kenney/space-kit/craft_speederA`.
    pub id: String,
    pub name: String,
    pub category: String,
    pub tris: u32,
}

pub struct Library {
    pub entries: Vec<Entry>,
    zip: Vec<u8>,
}

impl Library {
    fn parse(zip: Vec<u8>) -> Result<Library, String> {
        let mut archive =
            zip::ZipArchive::new(std::io::Cursor::new(&zip[..])).map_err(|e| e.to_string())?;
        let mut json = String::new();
        archive
            .by_name("index.json")
            .map_err(|e| e.to_string())?
            .read_to_string(&mut json)
            .map_err(|e| e.to_string())?;
        let entries = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        Ok(Library { entries, zip })
    }

    /// The `.glb` bytes of a model.
    pub fn glb(&self, id: &str) -> Result<Vec<u8>, String> {
        let mut archive =
            zip::ZipArchive::new(std::io::Cursor::new(&self.zip[..])).map_err(|e| e.to_string())?;
        let mut file = archive
            .by_name(&format!("{id}.glb"))
            .map_err(|_| format!("'{id}' is not in the model library"))?;
        let mut out = Vec::new();
        file.read_to_end(&mut out).map_err(|e| e.to_string())?;
        Ok(out)
    }

    pub fn entry(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// Categories in library order.
    pub fn categories(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for e in &self.entries {
            if !out.contains(&e.category.as_str()) {
                out.push(&e.category);
            }
        }
        out
    }
}

static LIBRARY: RwLock<Option<Arc<Library>>> = RwLock::new(None);
static WANTED: AtomicBool = AtomicBool::new(false);
static GENERATION: AtomicU32 = AtomicU32::new(0);

#[cfg(not(target_arch = "wasm32"))]
static BUNDLED: &[u8] = include_bytes!("../../../assets/models/library.zip");

/// The library, if it is loaded. On desktop it loads on first use; on the
/// web this asks the app to fetch it (see [`wanted`]).
pub fn library() -> Option<Arc<Library>> {
    if let Some(lib) = LIBRARY.read().unwrap().as_ref() {
        return Some(lib.clone());
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Err(e) = install(BUNDLED.to_vec()) {
            eprintln!("model library: {e}");
            return None;
        }
        LIBRARY.read().unwrap().clone()
    }
    #[cfg(target_arch = "wasm32")]
    {
        WANTED.store(true, Ordering::Relaxed);
        None
    }
}

/// The library if it is already loaded, without loading or fetching it.
pub fn loaded() -> Option<Arc<Library>> {
    LIBRARY.read().unwrap().clone()
}

/// Whether something asked for the library before it was loaded (the web
/// app then fetches it). Clears the request.
pub fn wanted() -> bool {
    WANTED.swap(false, Ordering::Relaxed)
}

/// Install the library from the bytes of `library.zip`.
pub fn install(zip: Vec<u8>) -> Result<(), String> {
    let lib = Library::parse(zip)?;
    *LIBRARY.write().unwrap() = Some(Arc::new(lib));
    GENERATION.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

/// Changes when the library is (re)installed, so meshes wanted before it
/// arrived are loaded again.
pub fn generation() -> u32 {
    GENERATION.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn bundled_library_loads() {
        let lib = super::library().expect("library");
        assert!(lib.entries.len() > 500, "{} models", lib.entries.len());
        let first = &lib.entries[0];
        let glb = lib.glb(&first.id).expect("glb");
        assert_eq!(&glb[..4], b"glTF");
        assert!(lib.glb("nope/nothing").is_err());
        assert!(!lib.categories().is_empty());
    }
}
