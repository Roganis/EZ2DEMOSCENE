//! Sprite layers.

use super::*;

labeled_enum! {
    /// How sprites turn.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
    pub enum SpriteFacing {
        /// Always square on to the camera.
        #[default]
        Camera => "Face the camera",
        /// Stand upright and turn around the vertical axis only (trees, people).
        Upright => "Upright",
        /// A plane in the scene facing +z, turned with the layer and the copies.
        Fixed => "Fixed plane",
    }
}

labeled_enum! {
    /// How sprites mix with what is behind them.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
    pub enum SpriteBlend {
        /// Soft edges from the image's alpha (copies drawn back to front).
        #[default]
        Alpha => "Alpha",
        /// Light adds up: glows, flares, fire.
        Additive => "Additive (glow)",
        /// Hard edges at half alpha; solid, so no sorting is needed.
        Cutout => "Cutout",
        /// Cutout with every other pixel left out in a checkerboard: the
        /// Saturn's see-through "mesh".
        Mesh => "Mesh (Saturn)",
    }
}

/// Images in the scene: billboards or planes, one per copy, optionally
/// playing a sprite sheet in step with the loop.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct SpriteLayer {
    /// A texture (built-in or added to the project); none draws a soft
    /// glowing dot.
    pub image: Option<String>,
    /// Sprite sheet: frames in a grid, read left to right, top to bottom.
    pub columns: u32,
    pub rows: u32,
    /// Frames used (0 = the whole grid).
    pub frames: u32,
    /// Whole passes through the frames per loop (0 = first frame only).
    pub cycles: i32,
    /// Each copy starts at a different frame.
    pub random_start: bool,
    pub facing: SpriteFacing,
    pub blend: SpriteBlend,
    /// Height of a sprite; the width follows the frame's shape.
    pub size: Param,
    pub opacity: Param,
    /// Multiplies the image colour.
    pub tint: [f32; 3],
    /// Brightness (above 1 blooms).
    pub glow: Param,
    /// Nearest-neighbour sampling for pixel art.
    pub pixelated: bool,
    pub instancer: Instancer,
    pub variation: Variation,
    /// The glow flickers by a Quake light style.
    #[serde(skip_serializing_if = "is_default")]
    pub glow_style: crate::retro::LightStyle,
}

impl Default for SpriteLayer {
    fn default() -> Self {
        SpriteLayer {
            image: None,
            columns: 1,
            rows: 1,
            frames: 0,
            cycles: 1,
            random_start: false,
            facing: SpriteFacing::Camera,
            blend: SpriteBlend::Alpha,
            size: Param::new(1.0),
            opacity: Param::new(1.0),
            tint: [1.0, 1.0, 1.0],
            glow: Param::new(1.0),
            pixelated: false,
            instancer: Instancer::Single,
            variation: Variation::default(),
            glow_style: Default::default(),
        }
    }
}

impl SpriteLayer {
    /// Frames played (at least one).
    pub fn frame_count(&self) -> u32 {
        let grid = self.columns.max(1) * self.rows.max(1);
        if self.frames == 0 {
            grid
        } else {
            self.frames.min(grid)
        }
    }
}
