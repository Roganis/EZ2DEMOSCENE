//! The user's data folder: own presets (with thumbnails), layer templates,
//! autosave and crash recovery.

use crate::export_ui::slug;
use ez_core::{Layer, Project};
use std::path::{Path, PathBuf};

pub struct UserPreset {
    pub name: String,
    pub path: PathBuf,
    pub thumb: Option<egui::TextureHandle>,
}

pub struct Template {
    pub layer: Layer,
    pub path: PathBuf,
}

pub struct Library {
    root: PathBuf,
    pub presets: Vec<UserPreset>,
    pub templates: Vec<Template>,
    /// An autosave left behind by a session that did not exit cleanly.
    pub recovery: Option<Project>,
}

/// Per-user data folder (e.g. `~/.local/share/ez2demoscene` or `%APPDATA%`).
pub fn data_dir() -> PathBuf {
    if let Ok(d) = std::env::var("EZ2_DATA_DIR") {
        return PathBuf::from(d);
    }
    directories::ProjectDirs::from("org", "ez2demoscene", "EZ2DEMOSCENE")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("ez2-data"))
}

impl Library {
    pub fn open(ctx: &egui::Context) -> Library {
        let root = data_dir();
        for sub in ["presets", "templates"] {
            let _ = std::fs::create_dir_all(root.join(sub));
        }
        let mut lib = Library {
            root,
            presets: Vec::new(),
            templates: Vec::new(),
            recovery: None,
        };
        lib.check_recovery();
        lib.reload(ctx);
        lib.write_lock();
        lib
    }

    fn lock_path(&self) -> PathBuf {
        self.root.join("session.lock")
    }

    fn autosave_path(&self) -> PathBuf {
        self.root.join("autosave.ez2.json")
    }

    fn check_recovery(&mut self) {
        // The lock is removed on a clean exit; if it's still there, the last
        // session crashed or was killed.
        if self.lock_path().exists() && self.autosave_path().exists() {
            self.recovery = Project::load(&self.autosave_path()).ok();
        }
    }

    fn write_lock(&self) {
        let _ = std::fs::write(self.lock_path(), std::process::id().to_string());
    }

    /// Called on a clean exit.
    pub fn clean_exit(&self) {
        let _ = std::fs::remove_file(self.lock_path());
        let _ = std::fs::remove_file(self.autosave_path());
    }

    pub fn autosave(&self, project: &Project) -> Result<(), String> {
        project
            .save(&self.autosave_path())
            .map_err(|e| e.to_string())
    }

    pub fn discard_recovery(&mut self) {
        self.recovery = None;
        let _ = std::fs::remove_file(self.autosave_path());
    }

    pub fn reload(&mut self, ctx: &egui::Context) {
        self.presets.clear();
        let mut files = list(&self.root.join("presets"), ".ez2.json");
        files.sort();
        for path in files {
            let Ok(p) = Project::load(&path) else {
                continue;
            };
            let thumb_path = path.with_file_name(format!(
                "{}.png",
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .trim_end_matches(".ez2.json")
            ));
            let thumb = image::open(&thumb_path).ok().map(|img| {
                let img = img.to_rgba8();
                let ci = egui::ColorImage::from_rgba_unmultiplied(
                    [img.width() as usize, img.height() as usize],
                    img.as_raw(),
                );
                ctx.load_texture(thumb_path.to_string_lossy(), ci, Default::default())
            });
            self.presets.push(UserPreset {
                name: p.name,
                path,
                thumb,
            });
        }
        self.templates.clear();
        let mut files = list(&self.root.join("templates"), ".json");
        files.sort();
        for path in files {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if let Ok(layer) = serde_json::from_str::<Layer>(&text) {
                self.templates.push(Template { layer, path });
            }
        }
    }

    fn unique(&self, dir: &str, name: &str, ext: &str) -> PathBuf {
        let base = slug(name);
        let mut p = self.root.join(dir).join(format!("{base}{ext}"));
        let mut k = 2;
        while p.exists() {
            p = self.root.join(dir).join(format!("{base}_{k}{ext}"));
            k += 1;
        }
        p
    }

    pub fn save_preset(
        &mut self,
        ctx: &egui::Context,
        project: &Project,
        thumbnail: &image::RgbaImage,
    ) -> Result<(), String> {
        let path = self.unique("presets", &project.name, ".ez2.json");
        project.save(&path).map_err(|e| e.to_string())?;
        let thumb = path.with_file_name(format!(
            "{}.png",
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .trim_end_matches(".ez2.json")
        ));
        thumbnail.save(&thumb).map_err(|e| e.to_string())?;
        self.reload(ctx);
        Ok(())
    }

    pub fn delete_preset(&mut self, ctx: &egui::Context, i: usize) {
        if let Some(p) = self.presets.get(i) {
            let _ = std::fs::remove_file(&p.path);
            let _ = std::fs::remove_file(p.path.with_file_name(format!(
                "{}.png",
                p.path.file_name().unwrap().to_string_lossy().trim_end_matches(".ez2.json")
            )));
        }
        self.reload(ctx);
    }

    pub fn save_template(&mut self, ctx: &egui::Context, layer: &Layer) -> Result<(), String> {
        let path = self.unique("templates", &layer.name, ".json");
        let json = serde_json::to_string_pretty(layer).map_err(|e| e.to_string())?;
        ez_core::assets::write_atomic(&path, json.as_bytes()).map_err(|e| e.to_string())?;
        self.reload(ctx);
        Ok(())
    }

    pub fn delete_template(&mut self, ctx: &egui::Context, i: usize) {
        if let Some(t) = self.templates.get(i) {
            let _ = std::fs::remove_file(&t.path);
        }
        self.reload(ctx);
    }

    pub fn template_layers(&self) -> Vec<Layer> {
        self.templates.iter().map(|t| t.layer.clone()).collect()
    }

    /// Folder where opened packs are extracted.
    pub fn unpack_dir(&self, pack: &Path) -> PathBuf {
        let stem = pack
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "pack".into());
        let mut dir = self.root.join("packs").join(slug(&stem));
        let mut k = 2;
        while dir.exists() {
            dir = self.root.join("packs").join(format!("{}_{k}", slug(&stem)));
            k += 1;
        }
        dir
    }
}

fn list(dir: &Path, suffix: &str) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| {
                    p.file_name()
                        .is_some_and(|n| n.to_string_lossy().ends_with(suffix))
                })
                .collect()
        })
        .unwrap_or_default()
}
