//! Text layers, and numbers shown in text.

use super::*;
use crate::clock::EvalCtx;
use std::borrow::Cow;

/// A number shown in a text through a `{0}`, `{1}`… placeholder (the
/// first value, the second…). Animate it like any setting: count down with
/// a ramp, count up to a score, follow the music.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct TextValue {
    pub value: Param,
    /// Fewest digits before the point, padded with zeros (0 = as needed).
    pub digits: u32,
    /// Digits after the point.
    pub decimals: u32,
    /// Thousands separated by commas (987,650).
    pub group: bool,
}

impl Default for TextValue {
    fn default() -> Self {
        TextValue {
            value: Param::new(0.0),
            digits: 0,
            decimals: 0,
            group: false,
        }
    }
}

impl TextValue {
    /// The number at `ctx`, rounded down to its decimals (so a count
    /// only shows its final number once it gets there).
    pub fn format(&self, ctx: &EvalCtx) -> String {
        let d = self.decimals.min(6);
        let scale = 10f64.powi(d as i32);
        let v = (self.value.eval(ctx) as f64 * scale + 1e-4).floor() / scale;
        let neg = v < 0.0;
        let s = format!("{:.*}", d as usize, v.abs());
        let (int, frac) = s
            .split_once('.')
            .map_or((s.as_str(), None), |(a, b)| (a, Some(b)));
        let mut int = format!("{int:0>width$}", width = self.digits.min(16) as usize);
        if self.group {
            let digits: Vec<char> = int.chars().collect();
            int = digits
                .iter()
                .enumerate()
                .flat_map(|(i, c)| {
                    let sep = i > 0 && (digits.len() - i).is_multiple_of(3);
                    sep.then_some(',').into_iter().chain(std::iter::once(*c))
                })
                .collect();
        }
        let mut out = String::new();
        if neg && v != 0.0 {
            out.push('-');
        }
        out.push_str(&int);
        if let Some(f) = frac {
            out.push('.');
            out.push_str(f);
        }
        out
    }
}

/// `template` with each `{n}` replaced by value `n` at `ctx`; anything
/// else (including `{n}` past the last value) stays as written.
pub fn format_text<'a>(template: &'a str, values: &[TextValue], ctx: &EvalCtx) -> Cow<'a, str> {
    if values.is_empty() || !template.contains('{') {
        return Cow::Borrowed(template);
    }
    let mut out = String::with_capacity(template.len() + 8);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let slot = after.find('}').and_then(|close| {
            let n: usize = after[..close].parse().ok()?;
            Some((values.get(n)?, close))
        });
        match slot {
            Some((v, close)) => {
                out.push_str(&v.format(ctx));
                rest = &after[close + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    Cow::Owned(out)
}

labeled_enum! {
    /// Built-in fonts.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize, JsonSchema)]
    pub enum TextFont {
        /// Chunky pixel letters, demoscene style.
        #[default]
        Pixel => "Pixel",
        /// Clean monospaced letters (Hack).
        Mono => "Mono",
        /// Light rounded letters (Ubuntu Light).
        Sans => "Sans",
    }
}

labeled_enum! {
    /// How text is shown and moves.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
    pub enum TextStyle {
        /// Still text, centred.
        #[default]
        Static => "Still",
        /// Runs across a window a whole number of times per loop.
        Scroller => "Scroller",
        /// A scroller whose letters ride a sine wave.
        SineScroller => "Sine scroller",
        /// Letters appear one by one on the beat.
        Typewriter => "Typewriter",
        /// One line at a time, a new line every few beats.
        Greetings => "Greetings list",
    }
}

/// Glowing letters in the scene (a flat sign facing +z; place and turn it
/// like any layer).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct TextLayer {
    /// One line per line for Greetings; Scrollers run it as one line.
    /// `{0}`, `{1}`… show the numbers in `values`.
    pub text: String,
    /// Numbers shown by the `{0}`, `{1}`… placeholders of the text.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<TextValue>,
    pub font: TextFont,
    /// A TTF/OTF file used instead of `font`.
    pub font_file: Option<String>,
    pub style: TextStyle,
    /// Letter height in world units.
    pub size: f32,
    /// Extra space between letters (fraction of the size).
    pub spacing: f32,
    /// Width of the scroller window in world units.
    pub width: f32,
    /// Times the scroller runs past per loop (negative = the other way).
    pub speed: i32,
    /// Sine scroller: wave height (fraction of the size).
    pub wave: Param,
    /// Letters per wave.
    pub wavelength: f32,
    /// Times the wave rolls per loop.
    pub wave_cycles: i32,
    /// Typewriter: letters per beat.
    pub letters_per_beat: u32,
    /// Greetings: beats per line.
    pub beats_per_line: u32,
    /// Colour at the top and bottom of the letters.
    pub color_top: Rgb,
    pub color_bottom: Rgb,
    pub glow: Param,
    /// Outline width (0 = none, 1 = thick).
    pub outline: f32,
    pub outline_color: Rgb,
    /// Drop shadow strength.
    pub shadow: f32,
    /// Chrome: shiny bevelled letters reflecting the sky.
    pub chrome: f32,
    /// Always turn the text towards the camera.
    pub face_camera: bool,
}

impl Default for TextLayer {
    fn default() -> Self {
        TextLayer {
            text: "EZ2DEMOSCENE".into(),
            values: Vec::new(),
            font: TextFont::Pixel,
            font_file: None,
            style: TextStyle::Static,
            size: 1.0,
            spacing: 0.0,
            width: 12.0,
            speed: 1,
            wave: Param::new(0.4),
            wavelength: 8.0,
            wave_cycles: 2,
            letters_per_beat: 2,
            beats_per_line: 4,
            color_top: hex(0xffffff),
            color_bottom: hex(0xff2bd6),
            glow: Param::new(1.0),
            outline: 0.0,
            outline_color: hex(0x000000),
            shadow: 0.0,
            chrome: 0.0,
            face_camera: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn num(v: f32, digits: u32, decimals: u32, group: bool) -> String {
        TextValue {
            value: Param::new(v),
            digits,
            decimals,
            group,
        }
        .format(&EvalCtx::at(0.0))
    }

    #[test]
    fn numbers_are_padded_grouped_and_rounded_down() {
        assert_eq!(num(7.0, 0, 0, false), "7");
        assert_eq!(num(7.0, 3, 0, false), "007");
        assert_eq!(num(987650.0, 0, 0, true), "987,650");
        assert_eq!(num(1234567.0, 0, 0, true), "1,234,567");
        assert_eq!(num(999.0, 0, 0, true), "999");
        assert_eq!(num(42.0, 7, 0, true), "0,000,042");
        // Rounded down, so a count reaches a number only when it gets there.
        assert_eq!(num(17.99, 0, 0, false), "17");
        assert_eq!(num(4.56789, 0, 2, false), "4.56");
        assert_eq!(num(2.5, 2, 1, false), "02.5");
        assert_eq!(num(-3.2, 0, 0, false), "-4");
        assert_eq!(num(-0.0, 0, 0, false), "0");
    }

    #[test]
    fn placeholders_are_filled_and_the_rest_left_alone() {
        let ctx = EvalCtx::at(0.0);
        let vals = [
            TextValue {
                value: Param::new(18.0),
                ..Default::default()
            },
            TextValue {
                value: Param::new(454.0),
                ..Default::default()
            },
        ];
        assert_eq!(format_text("TIME {0}", &vals, &ctx), "TIME 18");
        assert_eq!(
            format_text("{1} COMBO {0}{0}", &vals, &ctx),
            "454 COMBO 1818"
        );
        assert_eq!(format_text("{2} {x} {} {", &vals, &ctx), "{2} {x} {} {");
        assert_eq!(format_text("NO VALUES {0}", &[], &ctx), "NO VALUES {0}");
    }

    #[test]
    fn a_ramped_value_counts() {
        let mut v = TextValue {
            value: Param::new(0.0),
            group: true,
            ..Default::default()
        };
        v.value.ramp = crate::param::Ramp {
            start: 0.0,
            length: 8.0,
            by: 987650.0,
            ease: crate::param::Ease::Linear,
        };
        // Beats count from the loop's start outside a timeline (16 beats).
        assert_eq!(v.format(&EvalCtx::at(0.0)), "0");
        assert_eq!(v.format(&EvalCtx::at(0.25)), "493,825");
        assert_eq!(v.format(&EvalCtx::at(0.75)), "987,650");
    }
}
