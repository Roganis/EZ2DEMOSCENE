//! Web version of the user library: presets (with thumbnails), layer
//! templates, autosave and imported assets live in the browser's IndexedDB.
//!
//! IndexedDB is asynchronous: [`Library::open`] starts loading and
//! [`Library::poll`] installs the data once it has arrived. Writes are
//! fire-and-forget.

use crate::platform::slug;
use ez_core::{Layer, Project};
use idb::DatabaseEvent;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsValue;

const DB_NAME: &str = "ez2demoscene";
const STORE: &str = "kv";

pub struct UserPreset {
    pub name: String,
    key: String,
    json: String,
    pub thumb: Option<egui::TextureHandle>,
}

pub struct Template {
    pub layer: Layer,
    key: String,
}

#[derive(Default)]
struct Loaded {
    entries: Vec<(String, Vec<u8>)>,
    error: Option<String>,
}

pub struct Library {
    pub presets: Vec<UserPreset>,
    pub templates: Vec<Template>,
    pub recovery: Option<Project>,
    incoming: Rc<RefCell<Option<Loaded>>>,
    loaded: bool,
    pub load_error: Option<String>,
}

thread_local! {
    static DB: RefCell<Option<Rc<idb::Database>>> = const { RefCell::new(None) };
}

async fn db() -> Result<Rc<idb::Database>, String> {
    if let Some(d) = DB.with(|d| d.borrow().clone()) {
        return Ok(d);
    }
    let factory = idb::Factory::new().map_err(|e| e.to_string())?;
    let mut req = factory.open(DB_NAME, Some(1)).map_err(|e| e.to_string())?;
    req.on_upgrade_needed(|event| {
        if let Ok(db) = event.database() {
            let _ = db.create_object_store(STORE, idb::ObjectStoreParams::new());
        }
    });
    let d = Rc::new(req.await.map_err(|e| e.to_string())?);
    DB.with(|slot| *slot.borrow_mut() = Some(d.clone()));
    Ok(d)
}

async fn put(key: String, bytes: Vec<u8>) -> Result<(), String> {
    let d = db().await?;
    let tx = d
        .transaction(&[STORE], idb::TransactionMode::ReadWrite)
        .map_err(|e| e.to_string())?;
    let store = tx.object_store(STORE).map_err(|e| e.to_string())?;
    let value: JsValue = js_sys::Uint8Array::from(bytes.as_slice()).into();
    store
        .put(&value, Some(&JsValue::from_str(&key)))
        .map_err(|e| e.to_string())?
        .await
        .map_err(|e| e.to_string())?;
    tx.commit()
        .map_err(|e| e.to_string())?
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

async fn delete(key: String) -> Result<(), String> {
    let d = db().await?;
    let tx = d
        .transaction(&[STORE], idb::TransactionMode::ReadWrite)
        .map_err(|e| e.to_string())?;
    let store = tx.object_store(STORE).map_err(|e| e.to_string())?;
    store
        .delete(JsValue::from_str(&key))
        .map_err(|e| e.to_string())?
        .await
        .map_err(|e| e.to_string())?;
    tx.commit()
        .map_err(|e| e.to_string())?
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

async fn get_all() -> Result<Vec<(String, Vec<u8>)>, String> {
    let d = db().await?;
    let tx = d
        .transaction(&[STORE], idb::TransactionMode::ReadOnly)
        .map_err(|e| e.to_string())?;
    let store = tx.object_store(STORE).map_err(|e| e.to_string())?;
    let keys = store
        .get_all_keys(None, None)
        .map_err(|e| e.to_string())?
        .await
        .map_err(|e| e.to_string())?;
    let values = store
        .get_all(None, None)
        .map_err(|e| e.to_string())?
        .await
        .map_err(|e| e.to_string())?;
    Ok(keys
        .into_iter()
        .zip(values)
        .filter_map(|(k, v)| {
            let k = k.as_string()?;
            let bytes = js_sys::Uint8Array::new(&v).to_vec();
            Some((k, bytes))
        })
        .collect())
}

fn spawn_write(fut: impl std::future::Future<Output = Result<(), String>> + 'static) {
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(e) = fut.await {
            log::warn!("library write failed: {e}");
        }
    });
}

/// Keep an imported in-memory asset across page reloads.
pub fn persist_asset(path: &str) {
    if let Ok(bytes) = ez_core::store::read(path) {
        spawn_write(put(format!("asset:{path}"), bytes.to_vec()));
    }
}

fn thumb_texture(ctx: &egui::Context, key: &str, png: &[u8]) -> Option<egui::TextureHandle> {
    let img = image::load_from_memory(png).ok()?.to_rgba8();
    let ci = egui::ColorImage::from_rgba_unmultiplied(
        [img.width() as usize, img.height() as usize],
        img.as_raw(),
    );
    Some(ctx.load_texture(key, ci, Default::default()))
}

impl Library {
    pub fn open(_ctx: &egui::Context) -> Library {
        let incoming: Rc<RefCell<Option<Loaded>>> = Rc::default();
        let slot = incoming.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let loaded = match get_all().await {
                Ok(entries) => Loaded {
                    entries,
                    error: None,
                },
                Err(e) => Loaded {
                    entries: Vec::new(),
                    error: Some(e),
                },
            };
            *slot.borrow_mut() = Some(loaded);
        });
        Library {
            presets: Vec::new(),
            templates: Vec::new(),
            recovery: None,
            incoming,
            loaded: false,
            load_error: None,
        }
    }

    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// Install the library once IndexedDB has answered.
    pub fn poll(&mut self, ctx: &egui::Context) {
        if self.loaded {
            return;
        }
        let Some(data) = self.incoming.borrow_mut().take() else {
            return;
        };
        self.loaded = true;
        self.load_error = data.error;
        let mut thumbs = std::collections::HashMap::new();
        let mut assets = Vec::new();
        for (key, bytes) in data.entries {
            if let Some(path) = key.strip_prefix("asset:") {
                assets.push((path.to_string(), bytes));
            } else if let Some(id) = key.strip_prefix("thumb:") {
                thumbs.insert(id.to_string(), bytes);
            } else if let Some(id) = key.strip_prefix("preset:") {
                let json = String::from_utf8_lossy(&bytes).to_string();
                if let Ok(p) = Project::from_json(&json) {
                    self.presets.push(UserPreset {
                        name: p.name,
                        key: id.to_string(),
                        json,
                        thumb: None,
                    });
                }
            } else if let Some(id) = key.strip_prefix("template:") {
                if let Ok(layer) = serde_json::from_slice::<Layer>(&bytes) {
                    self.templates.push(Template {
                        layer,
                        key: id.to_string(),
                    });
                }
            } else if key == "autosave" {
                self.recovery = Project::from_json(&String::from_utf8_lossy(&bytes)).ok();
            }
        }
        for p in &mut self.presets {
            if let Some(png) = thumbs.get(&p.key) {
                p.thumb = thumb_texture(ctx, &p.key, png);
            }
        }
        // Keep only assets still used by something stored; forget the rest.
        let mut used: Vec<String> = Vec::new();
        let mut collect = |p: &Project| used.extend(p.asset_paths());
        for pr in &self.presets {
            if let Ok(p) = Project::from_json(&pr.json) {
                collect(&p);
            }
        }
        if let Some(r) = &self.recovery {
            collect(r);
        }
        for t in &self.templates {
            let mut p = Project::default();
            p.layers.push(t.layer.clone());
            collect(&p);
        }
        for (path, bytes) in assets {
            if used.contains(&path) {
                ez_core::store::insert(&path, bytes);
            } else {
                spawn_write(delete(format!("asset:{path}")));
            }
        }
    }

    pub fn clean_exit(&self) {}

    pub fn autosave(&self, project: &Project) -> Result<(), String> {
        spawn_write(put("autosave".into(), project.to_json().into_bytes()));
        Ok(())
    }

    pub fn discard_recovery(&mut self) {
        self.recovery = None;
        spawn_write(delete("autosave".into()));
    }

    fn unique_key(&self, name: &str, taken: impl Fn(&str) -> bool) -> String {
        let base = slug(name);
        let mut k = base.clone();
        let mut n = 2;
        while taken(&k) {
            k = format!("{base}_{n}");
            n += 1;
        }
        k
    }

    pub fn save_preset(
        &mut self,
        ctx: &egui::Context,
        project: &Project,
        thumbnail: &image::RgbaImage,
    ) -> Result<(), String> {
        let key = self.unique_key(&project.name, |k| self.presets.iter().any(|p| p.key == k));
        let json = project.to_json();
        let mut png = Vec::new();
        thumbnail
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        spawn_write(put(format!("preset:{key}"), json.clone().into_bytes()));
        spawn_write(put(format!("thumb:{key}"), png.clone()));
        let thumb = thumb_texture(ctx, &key, &png);
        self.presets.push(UserPreset {
            name: project.name.clone(),
            key,
            json,
            thumb,
        });
        Ok(())
    }

    pub fn delete_preset(&mut self, _ctx: &egui::Context, i: usize) {
        if i < self.presets.len() {
            let p = self.presets.remove(i);
            spawn_write(delete(format!("preset:{}", p.key)));
            spawn_write(delete(format!("thumb:{}", p.key)));
        }
    }

    pub fn load_preset(&self, i: usize) -> Result<Project, String> {
        let p = self.presets.get(i).ok_or("no such preset")?;
        Project::from_json(&p.json).map_err(|e| e.to_string())
    }

    pub fn save_template(&mut self, _ctx: &egui::Context, layer: &Layer) -> Result<(), String> {
        let key = self.unique_key(&layer.name, |k| self.templates.iter().any(|t| t.key == k));
        let json = serde_json::to_vec_pretty(layer).map_err(|e| e.to_string())?;
        spawn_write(put(format!("template:{key}"), json));
        self.templates.push(Template {
            layer: layer.clone(),
            key,
        });
        Ok(())
    }

    pub fn delete_template(&mut self, _ctx: &egui::Context, i: usize) {
        if i < self.templates.len() {
            let t = self.templates.remove(i);
            spawn_write(delete(format!("template:{}", t.key)));
        }
    }

    pub fn template_layers(&self) -> Vec<Layer> {
        self.templates.iter().map(|t| t.layer.clone()).collect()
    }
}
