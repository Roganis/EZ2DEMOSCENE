//! Asset paths, portable project files and `.ez2pack` bundles.
//!
//! In memory, asset paths (models, images, music) are absolute. On disk,
//! paths inside the project's folder are stored relative to the project file
//! so a project folder can be moved or shared. A `.ez2pack` is a zip holding
//! the project plus copies of every asset it uses.

use crate::graph::NodeKind;
use crate::scene::*;
use std::io::{Read, Write};
use std::path::{Component, Path};

pub const PACK_EXTENSION: &str = "ez2pack";
const PACK_PROJECT: &str = "project.ez2.json";

#[derive(Debug)]
pub enum AssetError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Zip(zip::result::ZipError),
    Invalid(String),
}

impl std::fmt::Display for AssetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AssetError::Io(e) => write!(f, "{e}"),
            AssetError::Json(e) => write!(f, "invalid project file: {e}"),
            AssetError::Zip(e) => write!(f, "invalid pack: {e}"),
            AssetError::Invalid(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for AssetError {}

impl From<std::io::Error> for AssetError {
    fn from(e: std::io::Error) -> Self {
        AssetError::Io(e)
    }
}
impl From<serde_json::Error> for AssetError {
    fn from(e: serde_json::Error) -> Self {
        AssetError::Json(e)
    }
}
impl From<zip::result::ZipError> for AssetError {
    fn from(e: zip::result::ZipError) -> Self {
        AssetError::Zip(e)
    }
}

fn layer_path(l: &mut Layer) -> Option<&mut String> {
    match &mut l.kind {
        LayerKind::Mesh(MeshLayer {
            source: MeshSource::File { path },
            ..
        }) => Some(path),
        _ => None,
    }
}

impl Project {
    /// Calls `f` on every asset path in the project (layers, graph sources,
    /// images, music).
    pub fn for_each_asset_path(&mut self, mut f: impl FnMut(&mut String)) {
        for l in &mut self.layers {
            if let Some(p) = layer_path(l) {
                f(p);
            }
        }
        if let Some(g) = &mut self.graph {
            for n in &mut g.nodes {
                if let NodeKind::Source { layer } = &mut n.kind {
                    if let Some(p) = layer_path(layer) {
                        f(p);
                    }
                }
            }
        }
        for t in &mut self.textures {
            f(&mut t.path);
        }
        if let Some(a) = &mut self.audio {
            f(a);
        }
    }

    /// All distinct asset paths.
    pub fn asset_paths(&self) -> Vec<String> {
        let mut c = self.clone();
        let mut out = Vec::new();
        c.for_each_asset_path(|p| {
            if !p.is_empty() && !out.contains(p) {
                out.push(p.clone());
            }
        });
        out
    }

    /// Make paths inside `base` relative to it (for saving). Paths outside
    /// `base` stay absolute.
    pub fn make_paths_relative(&mut self, base: &Path) {
        self.for_each_asset_path(|p| {
            let path = Path::new(p.as_str());
            if let Ok(rel) = path.strip_prefix(base) {
                *p = to_slash(rel);
            }
        });
    }

    /// Turn relative paths into absolute ones (after loading).
    pub fn resolve_paths(&mut self, base: &Path) {
        self.for_each_asset_path(|p| {
            if !p.is_empty() && Path::new(p.as_str()).is_relative() {
                *p = base.join(p.as_str()).to_string_lossy().to_string();
            }
        });
    }

    /// Upgrade older project files. Returns true if something changed.
    pub fn migrate(&mut self) -> bool {
        let old = self.version;
        // v1 -> v2: only absolute paths existed, which remain valid.
        self.version = PROJECT_VERSION;
        old != self.version
    }

    /// Load a project file and resolve its asset paths.
    pub fn load(path: &Path) -> Result<Project, AssetError> {
        if is_pack(path) {
            return Err(AssetError::Invalid(
                "this is a pack: open it with unpack()".into(),
            ));
        }
        let text = std::fs::read_to_string(path)?;
        let mut p = Project::from_json(&text)?;
        p.migrate();
        if let Some(dir) = path.parent() {
            p.resolve_paths(dir);
        }
        Ok(p)
    }

    /// Save as JSON with paths relative to the file's folder.
    pub fn save(&self, path: &Path) -> Result<(), AssetError> {
        let mut p = self.clone();
        p.version = PROJECT_VERSION;
        if let Some(dir) = path.parent() {
            let dir = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
            p.make_paths_relative(&dir);
        }
        write_atomic(path, p.to_json().as_bytes())?;
        Ok(())
    }
}

/// Writes via a temporary file + rename so a crash never leaves half a file.
pub fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp~");
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)
}

pub fn is_pack(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(PACK_EXTENSION))
}

fn to_slash(p: &Path) -> String {
    p.components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Result of packing: which assets could not be found (they are left out).
#[derive(Debug, Default)]
pub struct PackReport {
    pub packed: usize,
    pub missing: Vec<String>,
}

/// Build a `.ez2pack` (zip) in memory with `project` and copies of all its
/// assets (read through [`crate::store`], so in-memory assets work too).
pub fn pack_to_bytes(project: &Project) -> Result<(Vec<u8>, PackReport), AssetError> {
    let mut p = project.clone();
    p.version = PROJECT_VERSION;
    let mut report = PackReport::default();
    let mut entries: Vec<(String, String)> = Vec::new(); // (zip name, source path)
    p.for_each_asset_path(|path| {
        if path.is_empty() {
            return;
        }
        if let Some((name, _)) = entries.iter().find(|(_, src)| src == path) {
            *path = name.clone();
            return;
        }
        if !crate::store::exists(path) {
            report.missing.push(path.clone());
            return;
        }
        let file = crate::store::file_name(path).to_string();
        let mut name = format!("assets/{file}");
        let mut k = 2;
        while entries.iter().any(|(n, _)| *n == name) {
            name = format!("assets/{k}_{file}");
            k += 1;
        }
        entries.push((name.clone(), path.clone()));
        *path = name;
    });
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file(PACK_PROJECT, opts)?;
    zip.write_all(p.to_json().as_bytes())?;
    for (name, src) in &entries {
        zip.start_file(name.as_str(), opts)?;
        zip.write_all(&crate::store::read(src)?)?;
        report.packed += 1;
    }
    let bytes = zip.finish()?.into_inner();
    Ok((bytes, report))
}

/// Write `project` and copies of all its assets into a `.ez2pack` file.
pub fn pack(project: &Project, out: &Path) -> Result<PackReport, AssetError> {
    let (bytes, report) = pack_to_bytes(project)?;
    write_atomic(out, &bytes)?;
    Ok(report)
}

/// Open a `.ez2pack` held in memory: its assets are put in the in-memory
/// [`crate::store`] under `mem://<prefix>/…` and the project points at them.
pub fn unpack_bytes(bytes: &[u8], prefix: &str) -> Result<Project, AssetError> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    let mut json = None;
    let mut mapping: Vec<(String, String)> = Vec::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let Some(rel) = entry.enclosed_name() else {
            continue;
        };
        if entry.is_dir() {
            continue;
        }
        let rel = to_slash(&rel);
        let mut data = Vec::new();
        entry.read_to_end(&mut data)?;
        if rel == PACK_PROJECT {
            json = Some(String::from_utf8_lossy(&data).to_string());
        } else {
            let path = format!("{}{prefix}/{rel}", crate::store::MEM_PREFIX);
            crate::store::insert(&path, data);
            mapping.push((rel, path));
        }
    }
    let json =
        json.ok_or_else(|| AssetError::Invalid(format!("{PACK_PROJECT} missing in pack")))?;
    let mut p = Project::from_json(&json)?;
    p.migrate();
    p.for_each_asset_path(|path| {
        if let Some((_, m)) = mapping.iter().find(|(rel, _)| rel == path) {
            *path = m.clone();
        }
    });
    Ok(p)
}

/// Extract a `.ez2pack` into `dest` and load the project from it.
pub fn unpack(pack_path: &Path, dest: &Path) -> Result<Project, AssetError> {
    let f = std::fs::File::open(pack_path)?;
    let mut zip = zip::ZipArchive::new(f)?;
    std::fs::create_dir_all(dest)?;
    let mut json = None;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let Some(rel) = entry.enclosed_name() else {
            continue; // refuses "../" tricks
        };
        if entry.is_dir() {
            continue;
        }
        let target = dest.join(&rel);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if rel == Path::new(PACK_PROJECT) {
            let mut s = String::new();
            entry.read_to_string(&mut s)?;
            json = Some(s.clone());
            std::fs::write(&target, s)?;
        } else {
            let mut out = std::fs::File::create(&target)?;
            std::io::copy(&mut entry, &mut out)?;
        }
    }
    let json =
        json.ok_or_else(|| AssetError::Invalid(format!("{PACK_PROJECT} missing in pack")))?;
    let mut p = Project::from_json(&json)?;
    p.migrate();
    p.resolve_paths(dest);
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets;
    use std::path::PathBuf;

    fn tmpdir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join("ez2_assets_tests").join(name);
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::canonicalize(&d).unwrap()
    }

    fn project_with_assets(dir: &Path) -> Project {
        std::fs::create_dir_all(dir.join("models")).unwrap();
        std::fs::write(
            dir.join("models/ship.obj"),
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
        )
        .unwrap();
        std::fs::write(dir.join("logo.png"), b"not really a png").unwrap();
        std::fs::write(dir.join("song.wav"), b"RIFF").unwrap();
        let mut p = presets::empty();
        p.layers.push(Layer::new(
            "ship",
            LayerKind::Mesh(MeshLayer {
                source: MeshSource::File {
                    path: dir.join("models/ship.obj").to_string_lossy().to_string(),
                },
                ..Default::default()
            }),
        ));
        p.textures.push(UserTexture {
            name: "logo".into(),
            path: dir.join("logo.png").to_string_lossy().to_string(),
            retro: None,
        });
        p.audio = Some(dir.join("song.wav").to_string_lossy().to_string());
        p
    }

    #[test]
    fn save_uses_relative_paths_and_load_resolves() {
        let dir = tmpdir("rel");
        let p = project_with_assets(&dir);
        let file = dir.join("scene.ez2.json");
        p.save(&file).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("\"models/ship.obj\""), "{text}");
        assert!(text.contains("\"song.wav\""));
        // Move the whole folder: the project still finds its assets.
        let moved = tmpdir("rel_moved");
        for f in ["scene.ez2.json", "logo.png", "song.wav"] {
            std::fs::copy(dir.join(f), moved.join(f)).unwrap();
        }
        std::fs::create_dir_all(moved.join("models")).unwrap();
        std::fs::copy(dir.join("models/ship.obj"), moved.join("models/ship.obj")).unwrap();
        let back = Project::load(&moved.join("scene.ez2.json")).unwrap();
        for a in back.asset_paths() {
            assert!(Path::new(&a).is_file(), "{a}");
            assert!(a.starts_with(moved.to_str().unwrap()));
        }
    }

    #[test]
    fn pack_roundtrip() {
        let dir = tmpdir("pack_src");
        let mut p = project_with_assets(&dir);
        p.textures.push(UserTexture {
            name: "gone".into(),
            path: dir.join("missing.png").to_string_lossy().to_string(),
            retro: None,
        });
        let pack_file = dir.join("scene.ez2pack");
        let report = pack(&p, &pack_file).unwrap();
        assert_eq!(report.packed, 3);
        assert_eq!(report.missing.len(), 1);
        let dest = tmpdir("pack_dest");
        let back = unpack(&pack_file, &dest).unwrap();
        assert_eq!(back.layers.len(), p.layers.len());
        let obj = back
            .asset_paths()
            .into_iter()
            .find(|a| a.ends_with("ship.obj"))
            .unwrap();
        assert!(obj.starts_with(dest.to_str().unwrap()));
        assert_eq!(
            std::fs::read(&obj).unwrap(),
            std::fs::read(dir.join("models/ship.obj")).unwrap()
        );
    }

    #[test]
    fn pack_bytes_roundtrip_in_memory() {
        let logo = crate::store::insert_new("logo.png", b"PNGDATA".to_vec());
        let mut p = presets::empty();
        p.textures.push(UserTexture {
            name: "logo".into(),
            path: logo.clone(),
            retro: None,
        });
        let (bytes, report) = pack_to_bytes(&p).unwrap();
        assert_eq!(report.packed, 1);
        let back = unpack_bytes(&bytes, "test-pack").unwrap();
        let path = &back.textures[0].path;
        assert!(crate::store::is_mem(path), "{path}");
        assert_eq!(&*crate::store::read(path).unwrap(), b"PNGDATA");
    }

    #[test]
    fn v1_projects_migrate() {
        let mut p = Project::from_json(r#"{"version":1,"name":"old"}"#).unwrap();
        assert!(p.migrate());
        assert_eq!(p.version, PROJECT_VERSION);
    }
}
