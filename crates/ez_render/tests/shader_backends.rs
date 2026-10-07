//! Translates every WGSL module through naga's HLSL, MSL, GLSL ES and
//! desktop GLSL backends, so shader problems specific to Windows, macOS,
//! WebGL or a desktop OpenGL context show up on any machine. The HLSL of
//! each module is written to `target/shader_dump/<name>.hlsl` for
//! inspection.
//!
//! naga translating is not the driver accepting: naga has written GLSL no
//! compiler takes (a depth texture sampled without a compare became
//! `textureLod(sampler2DShadow, vec2, ..)`). So when `glslangValidator` is
//! on the PATH the GLSL is also compiled, and a rejection fails the test.

use naga::back::{glsl, hlsl, msl};
use naga::valid::{Capabilities, ValidationFlags, Validator};

const COMMON: &str = include_str!("../src/shaders/common.wgsl");

fn modules() -> Vec<(&'static str, String)> {
    let with_common = |src: &str| format!("{COMMON}\n{src}");
    let backdrop = with_common(include_str!("../src/shaders/backdrop.wgsl"));
    // The renderer specialises the background per kind (override
    // constant); check each specialisation as the GPU would see it.
    let mut kinds: Vec<(&'static str, String)> = (0..ez_core::scene::BackdropKind::ALL.len())
        .map(|k| {
            let name: &'static str = format!("backdrop kind {k}").leak();
            let src = backdrop.replace(
                "override BG_KIND: i32 = -1;",
                &format!("const BG_KIND: i32 = {k};"),
            );
            assert_ne!(src, backdrop, "BG_KIND declaration not found");
            (name, src)
        })
        .collect();
    let mut all = vec![
        (
            "mesh",
            with_common(include_str!("../src/shaders/mesh.wgsl")),
        ),
        (
            "particles",
            with_common(include_str!("../src/shaders/particles.wgsl")),
        ),
        (
            "floor",
            with_common(include_str!("../src/shaders/floor.wgsl")),
        ),
        (
            "terrain",
            with_common(include_str!("../src/shaders/terrain.wgsl")),
        ),
        (
            "lasers",
            with_common(&format!(
                "{}\n{}",
                include_str!("../src/shaders/beams.wgsl"),
                include_str!("../src/shaders/lasers.wgsl")
            )),
        ),
        (
            "spots",
            with_common(&format!(
                "{}\n{}",
                include_str!("../src/shaders/beams.wgsl"),
                include_str!("../src/shaders/spots.wgsl")
            )),
        ),
        (
            "sky fx",
            with_common(include_str!("../src/shaders/skyfx.wgsl")),
        ),
        (
            "falls",
            with_common(include_str!("../src/shaders/falls.wgsl")),
        ),
        (
            "contact",
            with_common(include_str!("../src/shaders/contact.wgsl")),
        ),
        (
            "text",
            with_common(include_str!("../src/shaders/text.wgsl")),
        ),
        ("sdf", with_common(include_str!("../src/shaders/sdf.wgsl"))),
        (
            "logo",
            with_common(include_str!("../src/shaders/logo.wgsl")),
        ),
        (
            "copies",
            include_str!("../src/shaders/copies.wgsl").to_string(),
        ),
        (
            "arcs",
            with_common(include_str!("../src/shaders/arcs.wgsl")),
        ),
        (
            "sprite",
            with_common(include_str!("../src/shaders/sprite.wgsl")),
        ),
        (
            "splats",
            with_common(include_str!("../src/shaders/splat.wgsl")),
        ),
        (
            "weather",
            with_common(include_str!("../src/shaders/weather.wgsl")),
        ),
        ("post", include_str!("../src/shaders/post.wgsl").to_string()),
        (
            "env filter",
            include_str!("../src/shaders/env_filter.wgsl").to_string(),
        ),
        ("ssr", with_common(include_str!("../src/shaders/ssr.wgsl"))),
        (
            "shafts",
            with_common(include_str!("../src/shaders/shafts.wgsl")),
        ),
        (
            "liquid",
            with_common(include_str!("../src/shaders/liquid.wgsl")),
        ),
        (
            "mode 7",
            with_common(include_str!("../src/shaders/mode7.wgsl")),
        ),
        (
            "retro upscale",
            include_str!("../src/shaders/retro_up.wgsl").to_string(),
        ),
    ];
    all.append(&mut kinds);
    all
}

/// Local arrays indexed with a runtime value become "not natively
/// addressable" l-values in HLSL, which D3D's FXC compiler rejects
/// (error X3500). Flag any dynamically indexed local array declaration.
fn fxc_unfriendly_arrays(hlsl: &str) -> Vec<String> {
    hlsl.lines()
        .filter(|l| {
            let t = l.trim_start();
            // e.g. `float m[16] = Constructarray16_float_(...);` inside a function
            l.starts_with("    ") && t.contains("[") && t.contains("= Constructarray")
        })
        .map(|l| l.trim().to_string())
        .collect()
}

/// A store through a runtime index into a function-local value (e.g.
/// `v[i] = x` on a local vector) is also an l-value FXC can't address
/// (error X3500), so flag those from the IR.
fn fxc_unfriendly_stores(module: &naga::Module) -> Vec<String> {
    use naga::{Expression as E, Statement as S};

    fn dynamic_local_store(f: &naga::Function, mut e: naga::Handle<E>) -> bool {
        let mut dynamic = false;
        loop {
            match f.expressions[e] {
                E::Access { base, .. } => {
                    dynamic = true;
                    e = base;
                }
                E::AccessIndex { base, .. } => e = base,
                E::LocalVariable(_) => return dynamic,
                _ => return false,
            }
        }
    }

    fn walk(f: &naga::Function, block: &naga::Block, hits: &mut usize) {
        for st in block.iter() {
            match st {
                S::Store { pointer, .. } if dynamic_local_store(f, *pointer) => *hits += 1,
                S::Block(b) => walk(f, b, hits),
                S::If { accept, reject, .. } => {
                    walk(f, accept, hits);
                    walk(f, reject, hits);
                }
                S::Loop {
                    body, continuing, ..
                } => {
                    walk(f, body, hits);
                    walk(f, continuing, hits);
                }
                S::Switch { cases, .. } => cases.iter().for_each(|c| walk(f, &c.body, hits)),
                _ => {}
            }
        }
    }

    let functions = module
        .functions
        .iter()
        .map(|(_, f)| f)
        .chain(module.entry_points.iter().map(|ep| &ep.function));
    let mut out = Vec::new();
    for f in functions {
        let mut hits = 0;
        walk(f, &f.body, &mut hits);
        if hits > 0 {
            let name = f.name.as_deref().unwrap_or("?");
            out.push(format!(
                "fn {name}: {hits} runtime-indexed store(s) into a local"
            ));
        }
    }
    out
}

#[test]
fn shaders_translate_for_every_backend() {
    let dump = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/shader_dump");
    let _ = std::fs::create_dir_all(&dump);
    let mut problems = Vec::new();
    for (name, src) in modules() {
        let module = naga::front::wgsl::parse_str(&src)
            .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(&src)));
        let info = Validator::new(ValidationFlags::all(), Capabilities::empty())
            .validate(&module)
            .unwrap_or_else(|e| panic!("{name}: {e:?}"));

        // D3D12 (FXC compiles shader model 5.1).
        let opts = hlsl::Options::default();
        let pipe = hlsl::PipelineOptions { entry_point: None };
        let mut out = String::new();
        hlsl::Writer::new(&mut out, &opts, &pipe)
            .write(&module, &info, None)
            .unwrap_or_else(|e| panic!("{name} → HLSL: {e}"));
        let _ = std::fs::write(dump.join(format!("{name}.hlsl")), &out);
        for l in fxc_unfriendly_arrays(&out) {
            problems.push(format!("{name}.hlsl: local array FXC may reject: {l}"));
        }
        for p in fxc_unfriendly_stores(&module) {
            problems.push(format!("{name}: FXC rejects (X3500) {p}"));
        }

        // Metal.
        msl::write_string(
            &module,
            &info,
            &msl::Options {
                lang_version: (2, 4),
                ..Default::default()
            },
            &msl::PipelineOptions::default(),
        )
        .unwrap_or_else(|e| panic!("{name} → MSL: {e}"));

        // WebGL2 (GLSL ES 3.00) and a desktop GL 3.3 core context (what
        // wgpu's GL backend writes for one, e.g. a host's context lent to
        // ez_embed), one entry point at a time. Compute shaders are left
        // out: WebGL2 has none (the renderer falls back to the CPU there).
        let targets = [
            (
                "GLSL ES",
                "es300",
                glsl::Version::Embedded {
                    version: 300,
                    is_webgl: true,
                },
            ),
            ("GLSL 330", "gl330", glsl::Version::Desktop(330)),
        ];
        for ep in &module.entry_points {
            if ep.stage == naga::ShaderStage::Compute {
                continue;
            }
            for (what, tag, version) in targets {
                let options = glsl::Options {
                    version,
                    ..Default::default()
                };
                let pipeline = glsl::PipelineOptions {
                    shader_stage: ep.stage,
                    entry_point: ep.name.clone(),
                    multiview: None,
                };
                let mut s = String::new();
                glsl::Writer::new(
                    &mut s,
                    &module,
                    &info,
                    &options,
                    &pipeline,
                    naga::proc::BoundsCheckPolicies::default(),
                )
                .and_then(|mut w| w.write())
                .unwrap_or_else(|e| panic!("{name}::{} → {what}: {e}", ep.name));
                if let Some(err) = glslang_rejects(&dump, name, &ep.name, tag, ep.stage, &s) {
                    problems.push(format!("{name}::{} → {what}: {err}", ep.name));
                }
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// Compiles GLSL with glslangValidator when it is installed; `Some(why)`
/// when it rejects the shader. Without the tool this checks nothing.
fn glslang_rejects(
    dump: &std::path::Path,
    module: &str,
    entry: &str,
    tag: &str,
    stage: naga::ShaderStage,
    glsl: &str,
) -> Option<String> {
    let ext = match stage {
        naga::ShaderStage::Vertex => "vert",
        naga::ShaderStage::Fragment => "frag",
        _ => return None,
    };
    let path = dump.join(format!("{module}.{entry}.{tag}.{ext}"));
    std::fs::write(&path, glsl).ok()?;
    let out = std::process::Command::new("glslangValidator")
        .arg(&path)
        .output()
        .ok()?; // not installed
    if out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let first = text
        .lines()
        .find(|l| l.contains("ERROR"))
        .unwrap_or("rejected");
    Some(format!("glslangValidator: {first} ({})", path.display()))
}
