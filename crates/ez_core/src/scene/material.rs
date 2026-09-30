//! Surface materials of shapes: shading, translucency, PBR, relief and glitch.

use super::*;

labeled_enum! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
    pub enum EmissiveMode {
        /// Whole surface glows.
        #[default]
        Full => "Full",
        /// Glowing outlines along UV borders (Tron look).
        Edges => "Edges",
        /// Glowing stripes across the surface.
        Stripes => "Stripes",
        /// Glow where the texture is bright.
        Texture => "Texture",
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Material {
    pub base_color: Rgb,
    pub metallic: Param,
    pub roughness: Param,
    pub emissive_color: Rgb,
    /// Glow strength (animatable: pulse it on the beat!).
    pub emissive: Param,
    pub emissive_mode: EmissiveMode,
    /// Built-in texture name or a user texture name.
    pub texture: Option<String>,
    pub texture_scale: Param,
    /// Texture tiles scrolled per loop (U, V).
    pub scroll: [i32; 2],
    /// Nearest-neighbour texture sampling for chunky pixels (the same as
    /// the Nearest filter, kept for older projects).
    pub pixelated: bool,
    /// How the texture is smoothed between its pixels (retro filters).
    #[serde(skip_serializing_if = "is_default")]
    pub filter: crate::retro::TexFilter,
    /// See-through the Saturn way, 0..1 (animatable): a share of the
    /// pixels is left out in a fixed screen pattern instead of blending;
    /// 0.5 is the Saturn's checkerboard "mesh".
    #[serde(skip_serializing_if = "is_off")]
    pub mesh: Param,
    /// The glow flickers by a Quake light style.
    #[serde(skip_serializing_if = "is_default")]
    pub glow_style: crate::retro::LightStyle,
    /// Quake's wobbling liquid textures.
    #[serde(skip_serializing_if = "is_default")]
    pub turbulence: crate::retro::Turbulence,
    /// Faceted look (normals from the triangle faces).
    pub flat_shading: bool,
    /// Rim / fresnel light strength.
    pub rim: Param,
    /// Hue rotation over the loop (animatable, in turns).
    pub hue_shift: Param,
    /// Geometry corruption (off when the amount is 0).
    #[serde(skip_serializing_if = "is_default")]
    pub glitch: Glitch,
    /// Surface relief: bump / normal map / displacement.
    #[serde(skip_serializing_if = "is_default")]
    pub relief: Relief,
    /// Physically based shading and its extra layers and maps.
    #[serde(skip_serializing_if = "is_default")]
    pub pbr: Pbr,
    /// Light shining through, and seeing through.
    #[serde(skip_serializing_if = "is_default")]
    pub translucency: Translucency,
}

/// How much light and view pass through a material (both shadings).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Translucency {
    /// Light shining through thin or soft stuff from behind (leaves,
    /// paper, wax, skin, a lampshade), 0..1 (animatable). The surface
    /// stays solid.
    pub amount: Param,
    /// The colour the light takes on inside (multiplies the surface's).
    pub color: Rgb,
    /// See-through: what is behind shows, blended (0 = solid,
    /// 1 = invisible; animatable).
    pub transparency: Param,
}

impl Default for Translucency {
    fn default() -> Self {
        Translucency {
            amount: Param::new(0.0),
            color: [1.0, 1.0, 1.0],
            transparency: Param::new(0.0),
        }
    }
}

impl Translucency {
    /// Whether the material is blended over what is behind it.
    pub fn see_through(&self) -> bool {
        self.transparency.base > 0.0 || self.transparency.is_animated()
    }
}

labeled_enum! {
    /// How a surface reacts to light.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
    pub enum Shading {
        /// The original look: a Blinn highlight and a simple reflection.
        #[default]
        Classic => "Classic",
        /// Physically based (GGX highlights, energy-conserving reflections,
        /// clearcoat, sheen and transmission).
        Physical => "Physical",
    }
}

/// Physically based settings of a material (used with
/// [`Shading::Physical`]; the maps work in both).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pbr {
    pub shading: Shading,
    /// Occlusion (red), roughness (green) and metalness (blue) in one
    /// picture, as glTF packs them; multiplies the material's values.
    pub orm_map: Option<String>,
    /// Glow picture: multiplies the glow colour.
    pub emissive_map: Option<String>,
    /// A clear lacquer layer on top (car paint), 0..1.
    pub clearcoat: Param,
    pub clearcoat_roughness: Param,
    /// Soft velvet glow at grazing angles (cloth), 0..1.
    pub sheen: Param,
    pub sheen_color: Rgb,
    /// Light passing through (glass, liquids), 0..1.
    pub transmission: Param,
    /// Index of refraction: how much light bends going through.
    pub ior: f32,
}

impl Default for Pbr {
    fn default() -> Self {
        Pbr {
            shading: Shading::Classic,
            orm_map: None,
            emissive_map: None,
            clearcoat: Param::new(0.0),
            clearcoat_roughness: Param::new(0.05),
            sheen: Param::new(0.0),
            sheen_color: [1.0, 1.0, 1.0],
            transmission: Param::new(0.0),
            ior: 1.5,
        }
    }
}

labeled_enum! {
    /// Ready-made physical materials.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum MaterialPreset {
        Gold => "Gold",
        Copper => "Copper",
        Chrome => "Chrome",
        BrushedSteel => "Brushed steel",
        Rubber => "Rubber",
        CarPaint => "Car paint",
        Glass => "Glass",
        Velvet => "Velvet",
        Ceramic => "Ceramic",
    }
}

impl MaterialPreset {
    /// Set the look of `m` (colour, metal, roughness and the physical
    /// layers), leaving textures, glow and geometry settings alone.
    pub fn apply(self, m: &mut Material) {
        // (colour, metallic, roughness)
        let (color, metallic, rough) = match self {
            MaterialPreset::Gold => (0xffc35a, 1.0, 0.22),
            MaterialPreset::Copper => (0xf2946a, 1.0, 0.3),
            MaterialPreset::Chrome => (0xf4f4f6, 1.0, 0.04),
            MaterialPreset::BrushedSteel => (0xc4c6ca, 1.0, 0.42),
            MaterialPreset::Rubber => (0x26262a, 0.0, 0.9),
            MaterialPreset::CarPaint => (0xb0101a, 0.3, 0.45),
            MaterialPreset::Glass => (0xf2f8fa, 0.0, 0.03),
            MaterialPreset::Velvet => (0x6a1238, 0.0, 0.85),
            MaterialPreset::Ceramic => (0xf0ece2, 0.0, 0.25),
        };
        m.base_color = hex(color);
        m.metallic = Param::new(metallic);
        m.roughness = Param::new(rough);
        m.rim = Param::new(0.0);
        let keep = (m.pbr.orm_map.take(), m.pbr.emissive_map.take());
        m.pbr = Pbr {
            shading: Shading::Physical,
            orm_map: keep.0,
            emissive_map: keep.1,
            ..Default::default()
        };
        match self {
            MaterialPreset::CarPaint => {
                m.pbr.clearcoat = Param::new(1.0);
                m.pbr.clearcoat_roughness = Param::new(0.03);
            }
            MaterialPreset::Glass => {
                m.pbr.transmission = Param::new(1.0);
                m.pbr.ior = 1.5;
            }
            MaterialPreset::Velvet => {
                m.pbr.sheen = Param::new(1.0);
                m.pbr.sheen_color = hex(0xff8ab8);
            }
            MaterialPreset::Ceramic => {
                m.pbr.clearcoat = Param::new(0.6);
                m.pbr.clearcoat_roughness = Param::new(0.08);
            }
            _ => {}
        }
    }
}

labeled_enum! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
    pub enum ReliefMode {
        /// Brightness of the texture is height.
        #[default]
        Bump => "Bump (brightness = height)",
        /// The texture is a (tangent-space) normal map.
        NormalMap => "Normal map",
    }
}

/// Makes a surface look (bump / normal map) or be (displacement) uneven.
/// Uses the material's tiling and scrolling.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Relief {
    /// Relief texture (built-in or user image); `None` uses the colour texture.
    pub texture: Option<String>,
    pub mode: ReliefMode,
    /// Strength of the lighting relief (animatable). 0 = off.
    pub bump: Param,
    /// Moves the surface outwards by the texture brightness (animatable).
    /// Needs detailed geometry (Subdivide).
    pub displace: Param,
}

impl Default for Relief {
    fn default() -> Self {
        Relief {
            texture: None,
            mode: ReliefMode::Bump,
            bump: Param::new(0.0),
            displace: Param::new(0.0),
        }
    }
}

labeled_enum! {
    /// How a glitched mesh is corrupted.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
    pub enum GlitchStyle {
        /// Vertices shake to random positions.
        #[default]
        Jitter => "Jitter",
        /// Horizontal bands shift sideways (VHS tearing).
        Slices => "Slices (VHS)",
        /// Faces fly apart along their normals.
        Shatter => "Shatter",
    }
}

impl GlitchStyle {
    pub fn index(self) -> u32 {
        GlitchStyle::ALL
            .iter()
            .position(|g| *g == self)
            .unwrap_or(0) as u32
    }
}

/// Loop-safe geometry corruption: the pattern changes `rate` times per
/// loop, and only `chance` of those steps are glitched.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Glitch {
    /// Strength (animatable). 0 = off.
    pub amount: Param,
    pub style: GlitchStyle,
    /// New random pattern this many times per loop.
    pub rate: u32,
    /// Fraction of the steps that glitch (1 = always).
    pub chance: Param,
    pub seed: u32,
}

impl Default for Glitch {
    fn default() -> Self {
        Glitch {
            amount: Param::new(0.0),
            style: GlitchStyle::Jitter,
            rate: 16,
            chance: Param::new(0.5),
            seed: 1,
        }
    }
}

impl Default for Material {
    fn default() -> Self {
        Material {
            base_color: hex(0xb0b0b8),
            metallic: Param::new(0.2),
            roughness: Param::new(0.4),
            emissive_color: hex(0xff3020),
            emissive: Param::new(0.0),
            emissive_mode: EmissiveMode::Full,
            texture: None,
            texture_scale: Param::new(1.0),
            scroll: [0, 0],
            pixelated: false,
            filter: Default::default(),
            mesh: Param::new(0.0),
            glow_style: Default::default(),
            turbulence: Default::default(),
            flat_shading: false,
            rim: Param::new(0.3),
            hue_shift: Param::new(0.0),
            glitch: Glitch::default(),
            relief: Relief::default(),
            pbr: Pbr::default(),
            translucency: Translucency::default(),
        }
    }
}

impl Material {
    /// The texture filter in effect (`pixelated` is the older switch for
    /// Nearest).
    pub fn tex_filter(&self) -> crate::retro::TexFilter {
        if self.pixelated {
            crate::retro::TexFilter::Nearest
        } else {
            self.filter
        }
    }
}
