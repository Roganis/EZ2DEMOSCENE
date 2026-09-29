//! The bundled texture library: seamless CC0 textures (see
//! `tools/texture_library.py`), one zip holding `index.json` and the
//! pictures. Two kinds: PBR materials (colour, normal map and occlusion /
//! roughness / metalness) and low-resolution textures.
//!
//! A library picture is used by name like any other texture:
//! `lib:<id>` is the colour, `lib:<id>.normal` and `lib:<id>.orm` a PBR
//! material's maps (see [`color_name`], [`normal_name`], [`orm_name`]).
//! [`crate::store::read`] reads these names, so everything that loads a
//! texture loads them too.
//!
//! Like the model library, desktop builds carry the zip inside the
//! executable and the web build fetches `texture_library.zip` next to the
//! page the first time a library texture is wanted ([`wanted`] /
//! [`install`]).

use serde::Deserialize;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

/// Prefix of library texture names.
pub const PREFIX: &str = "lib:";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A physically based material: colour, normal map and ORM map.
    Pbr,
    /// A small texture (at most 64 × 64).
    Tile,
}

/// One texture of the library.
#[derive(Clone, Debug, Deserialize)]
pub struct Entry {
    /// Stable id saved in projects, e.g. `polyhaven/brick_wall_001`.
    pub id: String,
    pub name: String,
    pub category: String,
    pub kind: Kind,
    /// Width (and height) in pixels.
    pub size: u32,
}

/// The texture name of an entry's colour picture.
pub fn color_name(id: &str) -> String {
    format!("{PREFIX}{id}")
}

/// The texture name of a PBR material's normal map.
pub fn normal_name(id: &str) -> String {
    format!("{PREFIX}{id}.normal")
}

/// The texture name of a PBR material's occlusion / roughness / metalness.
pub fn orm_name(id: &str) -> String {
    format!("{PREFIX}{id}.orm")
}

/// True for a library texture name.
pub fn is_lib(name: &str) -> bool {
    name.starts_with(PREFIX)
}

/// The entry id of a library texture name (any of its maps).
pub fn id_of(name: &str) -> Option<&str> {
    let rest = name.strip_prefix(PREFIX)?;
    Some(
        rest.strip_suffix(".normal")
            .or_else(|| rest.strip_suffix(".orm"))
            .unwrap_or(rest),
    )
}

/// What the texture picker shows for a library texture name.
pub fn display_name(name: &str) -> String {
    let Some(id) = id_of(name) else {
        return name.to_string();
    };
    let base = loaded()
        .and_then(|l| l.entry(id).map(|e| e.name.clone()))
        .unwrap_or_else(|| id.rsplit('/').next().unwrap_or(id).to_string());
    if name.ends_with(".normal") {
        format!("{base} (normal)")
    } else if name.ends_with(".orm") {
        format!("{base} (ORM)")
    } else {
        base
    }
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

    pub fn entry(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// The file bytes (PNG or JPEG) of a library texture name.
    pub fn read(&self, name: &str) -> Result<Vec<u8>, String> {
        let missing = || format!("'{name}' is not in the texture library");
        let id = id_of(name).ok_or_else(missing)?;
        let entry = self.entry(id).ok_or_else(missing)?;
        let file = match (entry.kind, &name[PREFIX.len() + id.len()..]) {
            (Kind::Tile, "") => format!("{id}.png"),
            (Kind::Pbr, "") => format!("{id}.jpg"),
            (Kind::Pbr, map) => format!("{id}{map}.jpg"),
            _ => return Err(missing()),
        };
        let mut archive =
            zip::ZipArchive::new(std::io::Cursor::new(&self.zip[..])).map_err(|e| e.to_string())?;
        let mut f = archive.by_name(&file).map_err(|_| missing())?;
        let mut out = Vec::new();
        f.read_to_end(&mut out).map_err(|e| e.to_string())?;
        Ok(out)
    }

    /// Categories of one kind, in library order.
    pub fn categories(&self, kind: Kind) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for e in self.entries.iter().filter(|e| e.kind == kind) {
            if !out.contains(&e.category.as_str()) {
                out.push(&e.category);
            }
        }
        out
    }
}

static LIBRARY: RwLock<Option<Arc<Library>>> = RwLock::new(None);
static WANTED: AtomicBool = AtomicBool::new(false);

#[cfg(not(target_arch = "wasm32"))]
static BUNDLED: &[u8] = include_bytes!("../../../assets/textures/texture_library.zip");

/// The library, if it is loaded. On desktop it loads on first use; on the
/// web this asks the app to fetch it (see [`wanted`]).
pub fn library() -> Option<Arc<Library>> {
    if let Some(lib) = LIBRARY.read().unwrap().as_ref() {
        return Some(lib.clone());
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Err(e) = install(BUNDLED.to_vec()) {
            eprintln!("texture library: {e}");
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

/// Install the library from the bytes of `texture_library.zip`.
pub fn install(zip: Vec<u8>) -> Result<(), String> {
    let lib = Library::parse(zip)?;
    *LIBRARY.write().unwrap() = Some(Arc::new(lib));
    Ok(())
}

/// Bytes of a library texture name, or `None` while the library is still
/// being fetched (web).
pub fn read(name: &str) -> Option<Result<Vec<u8>, String>> {
    library().map(|l| l.read(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(id_of("lib:a/b"), Some("a/b"));
        assert_eq!(id_of("lib:a/b.normal"), Some("a/b"));
        assert_eq!(id_of("lib:a/b.orm"), Some("a/b"));
        assert_eq!(id_of("brick"), None);
        assert_eq!(normal_name("x/y"), "lib:x/y.normal");
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn bundled_library_loads() {
        let lib = library().expect("library");
        let pbr = lib.entries.iter().filter(|e| e.kind == Kind::Pbr).count();
        let tiles = lib.entries.iter().filter(|e| e.kind == Kind::Tile).count();
        assert!(pbr >= 100, "{pbr} materials");
        assert!(tiles >= 500, "{tiles} low-res textures");
        for e in &lib.entries {
            let names = match e.kind {
                Kind::Pbr => vec![color_name(&e.id), normal_name(&e.id), orm_name(&e.id)],
                Kind::Tile => vec![color_name(&e.id)],
            };
            for n in names {
                let b = lib.read(&n).unwrap_or_else(|err| panic!("{n}: {err}"));
                assert!(b.len() > 64, "{n}");
            }
            assert!(e.size <= 256);
        }
        assert!(lib.read("lib:nope/nothing").is_err());
        assert!(!lib.categories(Kind::Pbr).is_empty());
        assert!(!lib.categories(Kind::Tile).is_empty());
    }
}
