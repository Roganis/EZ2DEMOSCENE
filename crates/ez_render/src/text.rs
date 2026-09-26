//! Text: glyph outlines (via skrifa), a signed-distance atlas per font and
//! the per-frame layout of scrollers, typewriters and greetings.
//!
//! The atlas is a grid of `CELL`² cells, one per character. Each cell holds
//! a signed distance field: 0.5 on the outline, more inside, less outside,
//! `SPREAD` pixels to go from edge to 0 or 1. Letters stay sharp at any size
//! and outlines, glows and shadows come for free in the shader.

use ez_core::scene::{TextFont, TextLayer, TextStyle};
use ez_core::EvalCtx;
use glam::Vec2;
use image::RgbaImage;
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{FontRef, MetadataProvider};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// Atlas cell size in pixels.
pub const CELL: u32 = 64;
/// Pixels per em inside a cell.
pub const EM_PX: f32 = 38.0;
/// Pen position inside a cell (pixels from the left, baseline from the top).
const PAD_X: f32 = 12.0;
const BASELINE: f32 = 46.0;
/// Distance (pixels) from the outline to the ends of the field.
pub const SPREAD: f32 = 8.0;
const COLS: u32 = 16;
/// Pixel font resolution.
const BLOCKS_PER_EM: f32 = 11.0;

/// Characters in every atlas: printable ASCII and Latin-1.
fn charset() -> impl Iterator<Item = char> {
    (0x20u32..0x7f)
        .chain(0xa1..0x100)
        .filter_map(char::from_u32)
}

#[derive(Clone, Copy, Debug)]
pub struct Glyph {
    pub cell: u32,
    /// Advance in ems.
    pub advance: f32,
}

pub struct FontAtlas {
    /// Distance field in every channel (linear, not sRGB).
    pub image: RgbaImage,
    pub glyphs: HashMap<char, Glyph>,
    pub cols: u32,
    pub rows: u32,
}

impl FontAtlas {
    /// A cell's size in ems (quads cover whole cells).
    pub fn cell_em() -> f32 {
        CELL as f32 / EM_PX
    }

    /// Where the pen sits inside a cell, in ems from the cell's bottom-left.
    pub fn pen_in_cell() -> Vec2 {
        Vec2::new(PAD_X, CELL as f32 - BASELINE) / EM_PX
    }

    fn glyph(&self, c: char) -> Option<Glyph> {
        self.glyphs.get(&c).copied()
    }

    fn advance(&self, c: char) -> f32 {
        self.glyph(c)
            .or_else(|| self.glyph(' '))
            .map_or(0.5, |g| g.advance)
    }
}

/// Font outline flattened to line segments, in cell pixels (y down).
#[derive(Default)]
struct Flatten {
    segs: Vec<(Vec2, Vec2)>,
    start: Vec2,
    cur: Vec2,
}

impl Flatten {
    fn map(x: f32, y: f32) -> Vec2 {
        Vec2::new(PAD_X + x, BASELINE - y)
    }
    fn line(&mut self, p: Vec2) {
        if p != self.cur {
            self.segs.push((self.cur, p));
        }
        self.cur = p;
    }
}

impl OutlinePen for Flatten {
    fn move_to(&mut self, x: f32, y: f32) {
        self.close();
        self.cur = Self::map(x, y);
        self.start = self.cur;
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.line(Self::map(x, y));
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let (p0, c, p1) = (self.cur, Self::map(cx0, cy0), Self::map(x, y));
        for k in 1..=6 {
            let t = k as f32 / 6.0;
            let u = 1.0 - t;
            self.line(p0 * u * u + c * 2.0 * u * t + p1 * t * t);
        }
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let (p0, c0, c1, p1) = (
            self.cur,
            Self::map(cx0, cy0),
            Self::map(cx1, cy1),
            Self::map(x, y),
        );
        for k in 1..=8 {
            let t = k as f32 / 8.0;
            let u = 1.0 - t;
            self.line(
                p0 * u * u * u + c0 * 3.0 * u * u * t + c1 * 3.0 * u * t * t + p1 * t * t * t,
            );
        }
    }
    fn close(&mut self) {
        let s = self.start;
        self.line(s);
    }
}

/// Non-zero winding of the closed segments around `p`.
fn winding(segs: &[(Vec2, Vec2)], p: Vec2) -> i32 {
    let mut w = 0;
    for (a, b) in segs {
        if a.y <= p.y {
            if b.y > p.y && (*b - *a).perp_dot(p - *a) > 0.0 {
                w += 1;
            }
        } else if b.y <= p.y && (*b - *a).perp_dot(p - *a) < 0.0 {
            w -= 1;
        }
    }
    w
}

fn seg_dist(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
}

/// Fill one cell of the atlas with the distance field of `segs`.
fn write_cell(
    img: &mut RgbaImage,
    cell: u32,
    segs: &[(Vec2, Vec2)],
    inside: &dyn Fn(Vec2) -> bool,
) {
    let (cx, cy) = ((cell % COLS) * CELL, (cell / COLS) * CELL);
    let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
    for (a, b) in segs {
        lo = lo.min(a.min(*b));
        hi = hi.max(a.max(*b));
    }
    for y in 0..CELL {
        for x in 0..CELL {
            let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            let v = if segs.is_empty()
                || p.x < lo.x - SPREAD
                || p.y < lo.y - SPREAD
                || p.x > hi.x + SPREAD
                || p.y > hi.y + SPREAD
            {
                0.0
            } else {
                let d = segs
                    .iter()
                    .map(|(a, b)| seg_dist(p, *a, *b))
                    .fold(f32::MAX, f32::min);
                let d = if inside(p) { d } else { -d };
                (0.5 + d / (2.0 * SPREAD)).clamp(0.0, 1.0)
            };
            let b = (v * 255.0).round() as u8;
            img.put_pixel(cx + x, cy + y, image::Rgba([b, b, b, 255]));
        }
    }
}

/// Pixel-font version of an outline: whole blocks lit where the letter
/// covers their centre, and the blocks' outer edges as segments.
fn pixelate(segs: &[(Vec2, Vec2)]) -> (Vec<(Vec2, Vec2)>, Vec<Vec<bool>>, f32) {
    let block = EM_PX / BLOCKS_PER_EM;
    let n = (CELL as f32 / block).ceil() as usize;
    // Blocks line up with the pen position so letters share a grid.
    let origin = Vec2::new(PAD_X, BASELINE) - Vec2::new(block * 3.0, block * 12.0);
    let lit: Vec<Vec<bool>> = (0..n)
        .map(|j| {
            (0..n)
                .map(|i| {
                    let c = origin + Vec2::new((i as f32 + 0.5) * block, (j as f32 + 0.5) * block);
                    // Inside, or close enough that thin strokes still get
                    // a full block.
                    winding(segs, c) != 0
                        || segs.iter().any(|(a, b)| seg_dist(c, *a, *b) < block * 0.3)
                })
                .collect()
        })
        .collect();
    let on = |i: isize, j: isize| {
        i >= 0 && j >= 0 && (j as usize) < n && (i as usize) < n && lit[j as usize][i as usize]
    };
    let mut out = Vec::new();
    for j in 0..n as isize {
        for i in 0..n as isize {
            if !on(i, j) {
                continue;
            }
            let p = |a: isize, b: isize| origin + Vec2::new(a as f32, b as f32) * block;
            if !on(i, j - 1) {
                out.push((p(i, j), p(i + 1, j)));
            }
            if !on(i, j + 1) {
                out.push((p(i + 1, j + 1), p(i, j + 1)));
            }
            if !on(i - 1, j) {
                out.push((p(i, j + 1), p(i, j)));
            }
            if !on(i + 1, j) {
                out.push((p(i + 1, j), p(i + 1, j + 1)));
            }
        }
    }
    let _ = origin;
    (out, lit, block)
}

/// Glyph outline of `c` in cell pixels.
fn outline(font: &FontRef, c: char) -> Option<(Vec<(Vec2, Vec2)>, f32)> {
    let gid = font.charmap().map(c)?;
    let size = Size::new(EM_PX);
    let advance = font
        .glyph_metrics(size, LocationRef::default())
        .advance_width(gid)
        .unwrap_or(EM_PX * 0.5)
        / EM_PX;
    let mut pen = Flatten::default();
    if let Some(g) = font.outline_glyphs().get(gid) {
        let _ = g.draw(
            DrawSettings::unhinted(size, LocationRef::default()),
            &mut pen,
        );
        pen.close();
    }
    Some((pen.segs, advance))
}

/// Build the atlas of a TTF/OTF font (`pixel`: the blocky version).
pub fn build_atlas(bytes: &[u8], pixel: bool) -> anyhow::Result<FontAtlas> {
    let font = FontRef::new(bytes).map_err(|e| anyhow::anyhow!("not a usable font: {e}"))?;
    let chars: Vec<char> = charset().collect();
    let rows = (chars.len() as u32).div_ceil(COLS);
    let mut image = RgbaImage::new(COLS * CELL, rows * CELL);
    let mut glyphs = HashMap::new();
    for (k, c) in chars.into_iter().enumerate() {
        let Some((segs, advance)) = outline(&font, c) else {
            continue;
        };
        let cell = k as u32;
        if pixel {
            let (edges, lit, block) = pixelate(&segs);
            let origin = Vec2::new(PAD_X, BASELINE) - Vec2::new(block * 3.0, block * 12.0);
            let inside = |p: Vec2| {
                let q = (p - origin) / block;
                let (i, j) = (q.x.floor(), q.y.floor());
                i >= 0.0
                    && j >= 0.0
                    && lit
                        .get(j as usize)
                        .and_then(|r| r.get(i as usize))
                        .copied()
                        .unwrap_or(false)
            };
            write_cell(&mut image, cell, &edges, &inside);
            // Advances snap to whole blocks.
            let adv = ((advance * EM_PX / block).round() * block) / EM_PX;
            glyphs.insert(c, Glyph { cell, advance: adv });
        } else {
            let inside = |p: Vec2| winding(&segs, p) != 0;
            write_cell(&mut image, cell, &segs, &inside);
            glyphs.insert(c, Glyph { cell, advance });
        }
    }
    if glyphs.is_empty() {
        anyhow::bail!("the font has no letters we can use");
    }
    Ok(FontAtlas {
        image,
        glyphs,
        cols: COLS,
        rows,
    })
}

/// A built-in font's atlas (built once per process).
pub fn builtin_atlas(font: TextFont) -> Arc<FontAtlas> {
    static CACHE: OnceLock<Mutex<HashMap<TextFont, Arc<FontAtlas>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(a) = cache.lock().unwrap().get(&font) {
        return a.clone();
    }
    let (bytes, pixel) = match font {
        TextFont::Pixel => (epaint_default_fonts::HACK_REGULAR, true),
        TextFont::Mono => (epaint_default_fonts::HACK_REGULAR, false),
        TextFont::Sans => (epaint_default_fonts::UBUNTU_LIGHT, false),
    };
    let atlas = Arc::new(build_atlas(bytes, pixel).expect("built-in font"));
    cache.lock().unwrap().insert(font, atlas.clone());
    atlas
}

// ---------------------------------------------------------------------------
// Layout

/// One letter to draw, in ems of the text size, around the layer origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedGlyph {
    pub cell: u32,
    /// Pen position (baseline, left) in ems.
    pub pos: Vec2,
    pub alpha: f32,
    /// Position along the text (0..1), for gradients across it.
    pub along: f32,
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Letters of one line starting at x = 0, with their advances.
fn run(atlas: &FontAtlas, line: &str, spacing: f32) -> (Vec<(char, f32)>, f32) {
    let mut x = 0.0;
    let mut out = Vec::new();
    for c in line.chars() {
        out.push((c, x));
        x += atlas.advance(c) + spacing;
    }
    (out, (x - spacing).max(0.0))
}

/// Where every visible letter of `t` is at `ctx`.
pub fn layout(t: &TextLayer, atlas: &FontAtlas, ctx: &EvalCtx) -> Vec<PlacedGlyph> {
    let spacing = t.spacing;
    // Centre lines on the letters' middle, not their baseline.
    let mid = 0.36;
    let mut out = Vec::new();
    let mut place = |c: char, pos: Vec2, alpha: f32, along: f32| {
        if let Some(g) = atlas.glyph(c) {
            if c != ' ' && alpha > 0.002 {
                out.push(PlacedGlyph {
                    cell: g.cell,
                    pos,
                    alpha,
                    along,
                });
            }
        }
    };
    let beat = ctx.beat_phase.rem_euclid(1.0) * ctx.loop_beats as f32;
    match t.style {
        TextStyle::Static | TextStyle::Typewriter => {
            let lines: Vec<&str> = t.text.lines().collect();
            let total: usize = t.text.chars().filter(|c| *c != '\n').count().max(1);
            let shown = match t.style {
                TextStyle::Typewriter => (beat * t.letters_per_beat.max(1) as f32).floor() as usize,
                _ => usize::MAX,
            };
            let line_h = 1.25;
            let top = (lines.len().max(1) as f32 - 1.0) * line_h * 0.5;
            let mut k = 0usize;
            for (li, line) in lines.iter().enumerate() {
                let (letters, width) = run(atlas, line, spacing);
                let y = top - li as f32 * line_h - mid;
                for (c, x) in letters {
                    if k < shown {
                        place(
                            c,
                            Vec2::new(x - width * 0.5, y),
                            1.0,
                            k as f32 / total as f32,
                        );
                    }
                    k += 1;
                }
            }
        }
        TextStyle::Greetings => {
            let lines: Vec<&str> = t.text.lines().filter(|l| !l.trim().is_empty()).collect();
            if lines.is_empty() {
                return out;
            }
            let per = t.beats_per_line.max(1) as f32;
            let slot = (beat / per).floor();
            let line = lines[slot as usize % lines.len()];
            // Fade in and out within the line's slot.
            let f = (beat / per).fract();
            let alpha = smoothstep(0.0, 0.12, f) * (1.0 - smoothstep(0.85, 1.0, f));
            let (letters, width) = run(atlas, line, spacing);
            let n = letters.len().max(1) as f32;
            for (i, (c, x)) in letters.into_iter().enumerate() {
                place(c, Vec2::new(x - width * 0.5, -mid), alpha, i as f32 / n);
            }
        }
        TextStyle::Scroller | TextStyle::SineScroller => {
            let line = t.text.replace('\n', "   ");
            let window = (t.width / t.size.max(1e-3)).max(1.0);
            let (letters, width) = run(atlas, &line, spacing);
            // Leave the window empty between passes.
            let period = width + window;
            let offset = (ctx.phase * t.speed as f32).rem_euclid(1.0) * period;
            let wave = t.wave.eval(ctx);
            let wl = t.wavelength.max(0.5);
            let roll = ctx.phase * t.wave_cycles as f32;
            let n = letters.len().max(1) as f32;
            for (i, (c, x0)) in letters.into_iter().enumerate() {
                let adv = atlas.advance(c);
                // Enter on the right, wrapping with the period.
                let x = (x0 - offset + window * 0.5).rem_euclid(period) - window * 0.5;
                if x + adv < -window * 0.5 || x > window * 0.5 {
                    continue;
                }
                let centre = x + adv * 0.5;
                let edge = window * 0.5 - centre.abs();
                let alpha = smoothstep(0.0, window * 0.08, edge);
                let y = if t.style == TextStyle::SineScroller {
                    wave * (std::f32::consts::TAU * (centre / wl + roll)).sin()
                } else {
                    0.0
                };
                place(c, Vec2::new(x, y - mid), alpha, i as f32 / n);
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// 3D letters

/// Glyph outline as separate closed contours, in ems (y up).
#[derive(Default)]
struct Contours {
    all: Vec<Vec<Vec2>>,
    cur: Vec<Vec2>,
    offset: Vec2,
}

impl Contours {
    fn map(&self, x: f32, y: f32) -> Vec2 {
        Vec2::new(x, y) / EM_PX + self.offset
    }
    fn push(&mut self, p: Vec2) {
        if self.cur.last() != Some(&p) {
            self.cur.push(p);
        }
    }
    fn finish(&mut self) {
        let mut c = std::mem::take(&mut self.cur);
        if c.len() > 1 && c.first() == c.last() {
            c.pop();
        }
        if c.len() >= 3 {
            self.all.push(c);
        }
    }
}

impl OutlinePen for Contours {
    fn move_to(&mut self, x: f32, y: f32) {
        self.finish();
        let p = self.map(x, y);
        self.push(p);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.map(x, y);
        self.push(p);
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let p0 = *self.cur.last().unwrap_or(&Vec2::ZERO);
        let (c, p1) = (self.map(cx0, cy0), self.map(x, y));
        for k in 1..=6 {
            let t = k as f32 / 6.0;
            let u = 1.0 - t;
            self.push(p0 * u * u + c * 2.0 * u * t + p1 * t * t);
        }
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let p0 = *self.cur.last().unwrap_or(&Vec2::ZERO);
        let (c0, c1, p1) = (self.map(cx0, cy0), self.map(cx1, cy1), self.map(x, y));
        for k in 1..=8 {
            let t = k as f32 / 8.0;
            let u = 1.0 - t;
            self.push(
                p0 * u * u * u + c0 * 3.0 * u * u * t + c1 * 3.0 * u * t * t + p1 * t * t * t,
            );
        }
    }
    fn close(&mut self) {
        self.finish();
    }
}

fn area(p: &[Vec2]) -> f32 {
    let n = p.len();
    (0..n).map(|i| p[i].perp_dot(p[(i + 1) % n])).sum::<f32>() * 0.5
}

fn inside_poly(pt: Vec2, poly: &[Vec2]) -> bool {
    let mut c = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a.y > pt.y) != (b.y > pt.y) && pt.x < a.x + (pt.y - a.y) * (b.x - a.x) / (b.y - a.y) {
            c = !c;
        }
    }
    c
}

/// Join the holes into the outer polygon (CCW outer, CW holes) with
/// bridges, then clip ears. Returns triangles (CCW).
fn triangulate(outer: &[Vec2], holes: &[Vec<Vec2>]) -> Vec<[Vec2; 3]> {
    let mut poly: Vec<Vec2> = outer.to_vec();
    let mut holes: Vec<&Vec<Vec2>> = holes.iter().collect();
    let max_x = |h: &Vec<Vec2>| h.iter().map(|p| p.x).fold(f32::MIN, f32::max);
    holes.sort_by(|a, b| max_x(b).total_cmp(&max_x(a)));
    for h in holes {
        let (mi, m) = h
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.x.total_cmp(&b.1.x))
            .map(|(i, p)| (i, *p))
            .unwrap();
        // Nearest edge to the right of the hole's rightmost point.
        let mut best: Option<(f32, usize)> = None;
        let n = poly.len();
        for i in 0..n {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            if (a.y > m.y) == (b.y > m.y) || (a.y - b.y).abs() < 1e-9 {
                continue;
            }
            let x = a.x + (m.y - a.y) * (b.x - a.x) / (b.y - a.y);
            if x >= m.x && best.is_none_or(|(bx, _)| x < bx) {
                let pick = if a.x > b.x { i } else { (i + 1) % n };
                best = Some((x, pick));
            }
        }
        let Some((_, pi)) = best else {
            continue;
        };
        let mut joined = Vec::with_capacity(poly.len() + h.len() + 2);
        joined.extend_from_slice(&poly[..=pi]);
        joined.extend(h[mi..].iter().chain(h[..=mi].iter()).copied());
        joined.extend_from_slice(&poly[pi..]);
        poly = joined;
    }
    // Ear clipping.
    let mut idx: Vec<usize> = (0..poly.len()).collect();
    let mut tris = Vec::new();
    let mut guard = 0;
    while idx.len() > 3 && guard < 100_000 {
        guard += 1;
        let n = idx.len();
        let mut clipped = false;
        for k in 0..n {
            let (ia, ib, ic) = (idx[(k + n - 1) % n], idx[k], idx[(k + 1) % n]);
            let (a, b, c) = (poly[ia], poly[ib], poly[ic]);
            if (b - a).perp_dot(c - b) <= 1e-12 {
                continue;
            }
            let blocked = idx.iter().any(|&j| {
                let p = poly[j];
                if j == ia || j == ib || j == ic || p == a || p == b || p == c {
                    return false;
                }
                (b - a).perp_dot(p - a) >= 0.0
                    && (c - b).perp_dot(p - b) >= 0.0
                    && (a - c).perp_dot(p - c) >= 0.0
            });
            if !blocked {
                tris.push([a, b, c]);
                idx.remove(k);
                clipped = true;
                break;
            }
        }
        if !clipped {
            // Degenerate remainder: drop the flattest vertex and go on.
            idx.remove(0);
        }
    }
    if idx.len() == 3 {
        let (a, b, c) = (poly[idx[0]], poly[idx[1]], poly[idx[2]]);
        if (b - a).perp_dot(c - b) > 0.0 {
            tris.push([a, b, c]);
        }
    }
    tris
}

/// Outer contours with their holes, oriented (outer CCW, holes CW).
fn shapes(contours: Vec<Vec<Vec2>>) -> Vec<(Vec<Vec2>, Vec<Vec<Vec2>>)> {
    let n = contours.len();
    let depth: Vec<usize> = (0..n)
        .map(|i| {
            (0..n)
                .filter(|&j| j != i && inside_poly(contours[i][0], &contours[j]))
                .count()
        })
        .collect();
    let mut out: Vec<(usize, Vec<Vec2>, Vec<Vec<Vec2>>)> = Vec::new();
    for i in 0..n {
        if depth[i].is_multiple_of(2) {
            let mut c = contours[i].clone();
            if area(&c) < 0.0 {
                c.reverse();
            }
            out.push((i, c, Vec::new()));
        }
    }
    for i in 0..n {
        if !depth[i].is_multiple_of(2) {
            let mut h = contours[i].clone();
            if area(&h) > 0.0 {
                h.reverse();
            }
            // The innermost outer contour around it.
            let parent = out.iter_mut().find(|(j, _, _)| {
                depth[*j] + 1 == depth[i] && inside_poly(contours[i][0], &contours[*j])
            });
            if let Some((_, _, holes)) = parent {
                holes.push(h);
            }
        }
    }
    out.into_iter().map(|(_, o, h)| (o, h)).collect()
}

fn font_bytes(font: TextFont) -> &'static [u8] {
    match font {
        TextFont::Pixel | TextFont::Mono => epaint_default_fonts::HACK_REGULAR,
        TextFont::Sans => epaint_default_fonts::UBUNTU_LIGHT,
    }
}

/// Contours of every letter of `text`, laid out (lines centred, in ems).
fn text_contours(font: &FontRef, text: &str) -> Vec<Vec<Vec2>> {
    let size = Size::new(EM_PX);
    let metrics = font.glyph_metrics(size, LocationRef::default());
    let lines: Vec<&str> = text.lines().collect();
    let mut all = Vec::new();
    for (li, line) in lines.iter().enumerate() {
        let y = -(li as f32) * 1.25;
        let mut pen = Contours::default();
        let mut x = 0.0;
        for c in line.chars() {
            let Some(gid) = font.charmap().map(c) else {
                x += 0.5;
                continue;
            };
            pen.offset = Vec2::new(x, y);
            if let Some(g) = font.outline_glyphs().get(gid) {
                let _ = g.draw(
                    DrawSettings::unhinted(size, LocationRef::default()),
                    &mut pen,
                );
                pen.finish();
            }
            x += metrics.advance_width(gid).unwrap_or(EM_PX * 0.5) / EM_PX;
        }
        // Centre the line.
        for c in &mut pen.all {
            for p in c.iter_mut() {
                p.x -= x * 0.5;
            }
        }
        all.extend(pen.all);
    }
    all
}

/// Extruded 3D text, centred and scaled to fit a unit sphere like the
/// other shapes. `bytes` overrides the built-in font.
pub fn text_mesh(
    text: &str,
    font: TextFont,
    bytes: Option<&[u8]>,
    depth: f32,
) -> crate::mesh::MeshData {
    use crate::mesh::{MeshData, Vertex};
    let text = if text.trim().is_empty() { "?" } else { text };
    let font_ref = bytes
        .and_then(|b| FontRef::new(b).ok())
        .unwrap_or_else(|| FontRef::new(font_bytes(font)).expect("built-in font"));
    let half = depth.clamp(0.01, 4.0) * 0.5;
    let mut m = MeshData::default();
    let vert = |m: &mut MeshData, p: glam::Vec3, n: glam::Vec3, uv: Vec2, edge: f32| {
        m.vertices.push(Vertex {
            pos: p.into(),
            normal: n.into(),
            uv: uv.into(),
            edge,
        });
        (m.vertices.len() - 1) as u32
    };
    let contours = text_contours(&font_ref, text);
    if font == TextFont::Pixel && bytes.is_none() {
        // Voxel letters: one block per lit pixel, only the outer faces.
        for block in pixel_blocks(&font_ref, text) {
            let (lo, hi) = block;
            let faces = [
                (
                    glam::Vec3::Z,
                    [(lo.x, lo.y), (hi.x, lo.y), (hi.x, hi.y), (lo.x, hi.y)],
                    half,
                ),
                (
                    glam::Vec3::NEG_Z,
                    [(lo.x, lo.y), (lo.x, hi.y), (hi.x, hi.y), (hi.x, lo.y)],
                    -half,
                ),
            ];
            for (n, q, z) in faces {
                let base = m.vertices.len() as u32;
                for (x, y) in q {
                    vert(&mut m, glam::Vec3::new(x, y, z), n, Vec2::new(x, y), 1.0);
                }
                m.indices
                    .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
            }
            let sides = [
                (glam::Vec3::X, Vec2::new(hi.x, lo.y), Vec2::new(hi.x, hi.y)),
                (
                    glam::Vec3::NEG_X,
                    Vec2::new(lo.x, hi.y),
                    Vec2::new(lo.x, lo.y),
                ),
                (glam::Vec3::Y, Vec2::new(hi.x, hi.y), Vec2::new(lo.x, hi.y)),
                (
                    glam::Vec3::NEG_Y,
                    Vec2::new(lo.x, lo.y),
                    Vec2::new(hi.x, lo.y),
                ),
            ];
            for (n, a, b) in sides {
                let base = m.vertices.len() as u32;
                vert(&mut m, a.extend(half), n, Vec2::new(0.0, 0.0), 0.0);
                vert(&mut m, b.extend(half), n, Vec2::new(1.0, 0.0), 0.0);
                vert(&mut m, b.extend(-half), n, Vec2::new(1.0, 1.0), 0.0);
                vert(&mut m, a.extend(-half), n, Vec2::new(0.0, 1.0), 0.0);
                m.indices
                    .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
            }
        }
    } else {
        for (outer, holes) in shapes(contours) {
            // Caps.
            for tri in triangulate(&outer, &holes) {
                for (z, n, order) in [
                    (half, glam::Vec3::Z, [0, 1, 2]),
                    (-half, glam::Vec3::NEG_Z, [0, 2, 1]),
                ] {
                    for k in order {
                        let p = tri[k];
                        vert(&mut m, p.extend(z), n, p, 1.0);
                        let i = m.vertices.len() as u32 - 1;
                        m.indices.push(i);
                    }
                }
            }
            // Walls, smooth across gentle corners.
            for c in std::iter::once(&outer).chain(holes.iter()) {
                let n = c.len();
                let edge_n = |i: usize| {
                    let d = c[(i + 1) % n] - c[i];
                    Vec2::new(d.y, -d.x).normalize_or_zero()
                };
                let corner = |i: usize, own: Vec2| {
                    let prev = edge_n((i + n - 1) % n);
                    let next = edge_n(i);
                    let other = if own == prev { next } else { prev };
                    if own.dot(other) > 0.7 {
                        (own + other).normalize_or(own)
                    } else {
                        own
                    }
                };
                let mut along = 0.0;
                for i in 0..n {
                    let (a, b) = (c[i], c[(i + 1) % n]);
                    let en = edge_n(i);
                    let (na, nb) = (corner(i, en), corner((i + 1) % n, en));
                    let len = (b - a).length();
                    let base = m.vertices.len() as u32;
                    vert(
                        &mut m,
                        a.extend(half),
                        na.extend(0.0),
                        Vec2::new(along, 0.0),
                        0.0,
                    );
                    vert(
                        &mut m,
                        b.extend(half),
                        nb.extend(0.0),
                        Vec2::new(along + len, 0.0),
                        0.0,
                    );
                    vert(
                        &mut m,
                        b.extend(-half),
                        nb.extend(0.0),
                        Vec2::new(along + len, half * 2.0),
                        0.0,
                    );
                    vert(
                        &mut m,
                        a.extend(-half),
                        na.extend(0.0),
                        Vec2::new(along, half * 2.0),
                        0.0,
                    );
                    m.indices
                        .extend([base, base + 2, base + 1, base, base + 3, base + 2]);
                    along += len;
                }
            }
        }
    }
    // Centre, and fit the widest side into -1..1.
    let (mut lo, mut hi) = (glam::Vec3::splat(f32::MAX), glam::Vec3::splat(f32::MIN));
    for v in &m.vertices {
        let p = glam::Vec3::from(v.pos);
        lo = lo.min(p);
        hi = hi.max(p);
    }
    if m.vertices.is_empty() {
        return crate::mesh::primitive(&ez_core::Primitive::Cube);
    }
    let centre = (lo + hi) * 0.5;
    let k = 2.0 / (hi - lo).truncate().max_element().max(1e-3);
    for v in &mut m.vertices {
        v.pos = ((glam::Vec3::from(v.pos) - centre) * k).into();
    }
    m
}

/// Lit blocks of the pixel font for `text` as (min, max) corners in ems.
fn pixel_blocks(font: &FontRef, text: &str) -> Vec<(Vec2, Vec2)> {
    let block = EM_PX / BLOCKS_PER_EM;
    let size = Size::new(EM_PX);
    let metrics = font.glyph_metrics(size, LocationRef::default());
    let mut out = Vec::new();
    for (li, line) in text.lines().enumerate() {
        let y0 = -(li as f32) * 1.25;
        let mut x = 0.0f32;
        let mut line_blocks = Vec::new();
        for c in line.chars() {
            let adv = font
                .charmap()
                .map(c)
                .and_then(|g| metrics.advance_width(g))
                .unwrap_or(EM_PX * 0.5)
                / EM_PX;
            if let Some((segs, _)) = outline(font, c) {
                let (_, lit, _) = pixelate(&segs);
                let origin = Vec2::new(PAD_X, BASELINE) - Vec2::new(block * 3.0, block * 12.0);
                for (j, row) in lit.iter().enumerate() {
                    for (i, on) in row.iter().enumerate() {
                        if !on {
                            continue;
                        }
                        // Cell pixels (y down) to ems (y up) around the pen.
                        let px = origin + Vec2::new(i as f32, j as f32) * block;
                        let lo = Vec2::new(
                            (px.x - PAD_X) / EM_PX + x,
                            (BASELINE - px.y - block) / EM_PX + y0,
                        );
                        let hi = lo + Vec2::splat(block / EM_PX);
                        line_blocks.push((lo, hi));
                    }
                }
            }
            x += ((adv * EM_PX / block).round() * block) / EM_PX;
        }
        for (lo, hi) in &mut line_blocks {
            lo.x -= x * 0.5;
            hi.x -= x * 0.5;
        }
        out.extend(line_blocks);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atlases_build_with_letters() {
        for font in TextFont::ALL {
            let a = builtin_atlas(font);
            assert!(a.glyphs.len() > 150, "{font:?}: {}", a.glyphs.len());
            // 'O' has an inside (1.0 in the middle of its stroke) and an
            // outside; its hole is outside.
            let g = a.glyphs[&'I'];
            let (cx, cy) = ((g.cell % COLS) * CELL, (g.cell / COLS) * CELL);
            let px = |x: u32, y: u32| a.image.get_pixel(cx + x, cy + y)[0];
            assert_eq!(px(1, 1), 0, "{font:?}: corner should be far outside");
            let max = (0..CELL)
                .flat_map(|y| (0..CELL).map(move |x| (x, y)))
                .map(|(x, y)| px(x, y))
                .max()
                .unwrap();
            assert!(max > 135, "{font:?}: no inside ({max})");
            if let Ok(dir) = std::env::var("EZ2_DUMP_ATLAS") {
                a.image.save(format!("{dir}/atlas_{font:?}.png")).unwrap();
            }
            assert!(a.glyphs[&'W'].advance > a.glyphs[&'i'].advance || font != TextFont::Sans);
        }
    }

    fn layer(style: TextStyle) -> TextLayer {
        TextLayer {
            text: "HELLO SCENE\nGREETINGS TO ALL".into(),
            style,
            ..Default::default()
        }
    }

    #[test]
    fn every_style_loops() {
        let atlas = builtin_atlas(TextFont::Mono);
        for style in TextStyle::ALL {
            let t = layer(style);
            let a = layout(&t, &atlas, &EvalCtx::at(0.0));
            let b = layout(&t, &atlas, &EvalCtx::at(1.0));
            assert_eq!(a.len(), b.len(), "{style:?}");
            for (x, y) in a.iter().zip(&b) {
                assert!((x.pos - y.pos).length() < 1e-3, "{style:?}");
                assert!((x.alpha - y.alpha).abs() < 1e-3, "{style:?}");
            }
            let mid = layout(&t, &atlas, &EvalCtx::at(0.4));
            assert!(!mid.is_empty(), "{style:?} shows nothing mid-loop");
        }
    }

    #[test]
    fn scroller_moves_and_typewriter_types() {
        let atlas = builtin_atlas(TextFont::Mono);
        let t = layer(TextStyle::Scroller);
        let a = layout(&t, &atlas, &EvalCtx::at(0.3));
        let b = layout(&t, &atlas, &EvalCtx::at(0.31));
        assert!(a[0].pos.x > b[0].pos.x - 1e-4 || a[0].cell != b[0].cell);
        let t = layer(TextStyle::Typewriter);
        let early = layout(&t, &atlas, &EvalCtx::at(0.05)).len();
        let later = layout(&t, &atlas, &EvalCtx::at(0.5)).len();
        assert!(early < later, "{early} {later}");
    }

    #[test]
    fn letters_extrude_into_closed_solids() {
        for font in TextFont::ALL {
            let m = text_mesh("A8o", font, None, 0.3);
            assert!(m.indices.len() > 300, "{font:?}: {}", m.indices.len());
            // Fits the unit box, centred.
            for v in &m.vertices {
                assert!(v.pos.iter().all(|c| c.abs() <= 1.0 + 1e-4), "{font:?}");
            }
            // Front caps cover about the letters' ink: area > 0.
            let front: f32 = m
                .indices
                .chunks(3)
                .filter(|t| t.iter().all(|&i| m.vertices[i as usize].normal[2] > 0.9))
                .map(|t| {
                    let p = |k: usize| Vec2::from_slice(&m.vertices[t[k] as usize].pos[..2]);
                    (p(1) - p(0)).perp_dot(p(2) - p(0)).abs() * 0.5
                })
                .sum();
            assert!(front > 0.1, "{font:?}: front area {front}");
        }
    }

    #[test]
    fn holes_stay_open() {
        // The centre of an 'O' must not be covered by the front cap.
        let m = text_mesh("O", TextFont::Sans, None, 0.2);
        let centre = Vec2::ZERO;
        let covered = m.indices.chunks(3).any(|t| {
            let p = |k: usize| Vec2::from_slice(&m.vertices[t[k] as usize].pos[..2]);
            if m.vertices[t[0] as usize].normal[2] < 0.9 {
                return false;
            }
            let (a, b, c) = (p(0), p(1), p(2));
            let s1 = (b - a).perp_dot(centre - a);
            let s2 = (c - b).perp_dot(centre - b);
            let s3 = (a - c).perp_dot(centre - c);
            (s1 >= 0.0 && s2 >= 0.0 && s3 >= 0.0) || (s1 <= 0.0 && s2 <= 0.0 && s3 <= 0.0)
        });
        assert!(!covered, "the O's hole was filled");
    }
}
