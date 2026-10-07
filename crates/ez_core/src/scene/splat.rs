//! Gaussian splat layers: a captured object or place, made of millions of
//! soft coloured blobs (3D Gaussian splatting), loaded from a file.

use super::*;

labeled_enum! {
    /// Which way is up in a splat file.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
    pub enum SplatUp {
        /// What the format usually means: PLY and .splat files keep the
        /// training camera's frame (Y down), SPZ is Y up.
        #[default]
        Auto => "As the format says",
        YUp => "Y up",
        YDown => "Y down (upside down)",
        ZUp => "Z up",
        ZDown => "Z down",
    }
}

impl SplatUp {
    /// The turn (rows) that brings the file's up to +Y, for a file with
    /// extension `ext`.
    pub fn matrix(self, ext: &str) -> [[f32; 3]; 3] {
        let up = match self {
            SplatUp::Auto if ext.eq_ignore_ascii_case("spz") => SplatUp::YUp,
            SplatUp::Auto => SplatUp::YDown,
            other => other,
        };
        match up {
            SplatUp::YDown => [[1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, -1.0]],
            SplatUp::ZUp => [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]],
            SplatUp::ZDown => [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]],
            _ => [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        }
    }
}

/// Gaussian splats from a `.ply`, `.spz` or `.splat` file (none: a small
/// built-in cloud). Like a model, they are centred and fitted into the
/// unit sphere unless `fit` is off; the layer's transform places, turns
/// and spins them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct SplatLayer {
    /// The splat file (asset path); none shows the built-in cloud.
    pub file: Option<String>,
    pub up: SplatUp,
    /// Centre the splats and fit them into the unit sphere (the stray
    /// splats far out ignored). Off: the file's own units and origin.
    pub fit: bool,
    /// Multiplies the colours.
    pub tint: Rgb,
    /// Brightness (animatable; above 1 blooms).
    pub brightness: Param,
    /// Opacity of every splat (animatable).
    pub opacity: Param,
    /// Size of every splat (animatable): 1 as captured, smaller turns the
    /// scene into dots.
    pub splat_size: Param,
    /// Moves every splat out from the centre (animatable, in layer units):
    /// the scene bursts apart and, as it goes back to 0, comes together.
    pub scatter: Param,
    /// The most splats drawn; above it the faintest and smallest are left
    /// out.
    pub max_splats: u32,
}

impl Default for SplatLayer {
    fn default() -> Self {
        SplatLayer {
            file: None,
            up: SplatUp::Auto,
            fit: true,
            tint: [1.0, 1.0, 1.0],
            brightness: Param::new(1.0),
            opacity: Param::new(1.0),
            splat_size: Param::new(1.0),
            scatter: Param::new(0.0),
            max_splats: 2_000_000,
        }
    }
}

/// File extensions of splat files: read (`ply`, `spz`, `splat`).
pub const SPLAT_EXTENSIONS: &[&str] = &["ply", "spz", "splat"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn up_turns_bring_the_named_axis_to_y() {
        let apply = |m: [[f32; 3]; 3], v: [f32; 3]| {
            [0, 1, 2].map(|r| m[r][0] * v[0] + m[r][1] * v[1] + m[r][2] * v[2])
        };
        for (up, ext, axis) in [
            (SplatUp::Auto, "ply", [0.0, -1.0, 0.0]),
            (SplatUp::Auto, "SPZ", [0.0, 1.0, 0.0]),
            (SplatUp::ZUp, "ply", [0.0, 0.0, 1.0]),
            (SplatUp::ZDown, "spz", [0.0, 0.0, -1.0]),
            (SplatUp::YDown, "spz", [0.0, -1.0, 0.0]),
        ] {
            assert_eq!(apply(up.matrix(ext), axis), [0.0, 1.0, 0.0], "{up:?} {ext}");
        }
    }

    #[test]
    fn short_in_project_files() {
        let json = serde_json::to_string(&SplatLayer::default()).unwrap();
        let back: SplatLayer = serde_json::from_str("{}").unwrap();
        assert_eq!(back, SplatLayer::default());
        assert!(json.contains("\"max_splats\":2000000"));
    }
}
