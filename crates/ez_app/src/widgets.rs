//! Small reusable editor widgets. Every helper returns `true` when the value
//! changed.

use egui::{Color32, RichText, Ui};
use ez_core::{EvalCtx, Param, WAVE_GROUPS};
use std::ops::RangeInclusive;

pub const ACCENT: Color32 = Color32::from_rgb(255, 70, 140);

/// Two-column row: label on the left, widget on the right.
pub fn row<R>(ui: &mut Ui, label: &str, tip: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.horizontal(|ui| {
        let r = ui.add_sized([110.0, 18.0], egui::Label::new(label).truncate());
        if !tip.is_empty() {
            r.on_hover_text(tip);
        }
        add(ui)
    })
    .inner
}

pub fn slider(
    ui: &mut Ui,
    label: &str,
    tip: &str,
    v: &mut f32,
    range: RangeInclusive<f32>,
) -> bool {
    row(ui, label, tip, |ui| {
        ui.add(egui::Slider::new(v, range).clamping(egui::SliderClamping::Never))
            .changed()
    })
}

pub fn drag_u(
    ui: &mut Ui,
    label: &str,
    tip: &str,
    v: &mut u32,
    range: RangeInclusive<u32>,
) -> bool {
    row(ui, label, tip, |ui| {
        ui.add(egui::DragValue::new(v).range(range).speed(0.2))
            .changed()
    })
}

pub fn drag_i(
    ui: &mut Ui,
    label: &str,
    tip: &str,
    v: &mut i32,
    range: RangeInclusive<i32>,
) -> bool {
    row(ui, label, tip, |ui| {
        ui.add(egui::DragValue::new(v).range(range).speed(0.1))
            .changed()
    })
}

pub fn check(ui: &mut Ui, label: &str, tip: &str, v: &mut bool) -> bool {
    row(ui, label, tip, |ui| ui.checkbox(v, "").changed())
}

pub fn color(ui: &mut Ui, label: &str, tip: &str, c: &mut [f32; 3]) -> bool {
    row(ui, label, tip, |ui| ui.color_edit_button_rgb(c).changed())
}

pub fn vec3(ui: &mut Ui, label: &str, tip: &str, v: &mut [f32; 3], speed: f32) -> bool {
    row(ui, label, tip, |ui| {
        let mut changed = false;
        for (i, axis) in ["x", "y", "z"].iter().enumerate() {
            changed |= ui
                .add(
                    egui::DragValue::new(&mut v[i])
                        .speed(speed)
                        .prefix(format!("{axis} ")),
                )
                .changed();
        }
        changed
    })
}

pub fn ivec3(
    ui: &mut Ui,
    label: &str,
    tip: &str,
    v: &mut [i32; 3],
    range: RangeInclusive<i32>,
) -> bool {
    row(ui, label, tip, |ui| {
        let mut changed = false;
        for (i, axis) in ["x", "y", "z"].iter().enumerate() {
            changed |= ui
                .add(
                    egui::DragValue::new(&mut v[i])
                        .range(range.clone())
                        .speed(0.1)
                        .prefix(format!("{axis} ")),
                )
                .changed();
        }
        changed
    })
}

pub fn combo<T: PartialEq + Copy>(
    ui: &mut Ui,
    label: &str,
    tip: &str,
    v: &mut T,
    options: &[T],
    name: impl Fn(T) -> &'static str,
) -> bool {
    row(ui, label, tip, |ui| {
        let mut changed = false;
        egui::ComboBox::from_id_salt(ui.id().with(label))
            .selected_text(name(*v))
            .width(150.0)
            .show_ui(ui, |ui| {
                for o in options {
                    changed |= ui.selectable_value(v, *o, name(*o)).changed();
                }
            });
        changed
    })
}

/// Loop clock shared with the animation panels (set once per frame).
#[derive(Clone, Copy)]
struct Clock {
    phase: f32,
    loop_beats: u32,
}

/// Tell the `~` panels where the loop is (for the preview's playhead and
/// the beat-sync choices).
pub fn set_clock(ctx: &egui::Context, phase: f32, loop_beats: u32) {
    ctx.data_mut(|d| {
        d.insert_temp(
            egui::Id::new("ez2_clock"),
            Clock {
                phase,
                loop_beats: loop_beats.max(1),
            },
        )
    });
}

/// The project's music, so the `~` previews can show music-driven values.
#[derive(Clone)]
pub struct MusicPreview {
    pub env: Option<std::sync::Arc<ez_core::AudioEnvelope>>,
    pub settings: ez_core::MusicSettings,
    pub timing: ez_core::Timing,
    pub live: Option<ez_core::MusicFrame>,
}

pub fn set_music(ctx: &egui::Context, m: MusicPreview) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("ez2_music"), m));
}

fn music(ui: &Ui) -> Option<MusicPreview> {
    ui.ctx().data(|d| d.get_temp(egui::Id::new("ez2_music")))
}

/// Beats per loop of the current project (for beat-synced defaults).
pub fn loop_beats(ui: &Ui) -> u32 {
    clock(ui).loop_beats
}

fn clock(ui: &Ui) -> Clock {
    ui.ctx()
        .data(|d| d.get_temp(egui::Id::new("ez2_clock")))
        .unwrap_or(Clock {
            phase: 0.0,
            loop_beats: 16,
        })
}

/// Rhythm choices offered by the ♩ menu: (label, cycles per loop).
fn sync_choices(beats: u32) -> Vec<(String, i32)> {
    let mut v: Vec<(String, i32)> = Vec::new();
    let mut push = |label: String, cycles: u32| {
        if cycles >= 1 && !v.iter().any(|(_, c)| *c == cycles as i32) {
            v.push((label, cycles as i32));
        }
    };
    push("Twice per beat".into(), beats * 2);
    push("Every beat".into(), beats);
    if beats.is_multiple_of(2) {
        push("Every 2 beats".into(), beats / 2);
    }
    if beats.is_multiple_of(4) {
        push("Every bar (4 beats)".into(), beats / 4);
    }
    if beats.is_multiple_of(8) {
        push("Every 2 bars".into(), beats / 8);
    }
    if beats.is_multiple_of(16) {
        push("Every 4 bars".into(), beats / 16);
    }
    push("Once per loop".into(), 1);
    v
}

/// The editor of an `Envelope` wave: one cycle of it, with its points.
/// Click to add a point, drag to move one (snapped to quarter beats; hold
/// Shift to move freely), right-click (long press) for its curve or to
/// delete it, double-click to delete it.
fn envelope_editor(ui: &mut Ui, id: egui::Id, p: &mut Param, clock: Clock) -> bool {
    use ez_core::{Curve, EnvPoint, EnvRef, Envelope};
    let mut changed = false;
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().min(320.0), 110.0),
        egui::Sense::click_and_drag(),
    );
    let inner = rect.shrink2(egui::vec2(6.0, 8.0));
    let to_screen = |t: f32, v: f32| {
        egui::pos2(
            inner.left() + inner.width() * t,
            inner.bottom() - inner.height() * v,
        )
    };
    let from_screen = |pos: egui::Pos2| {
        (
            ((pos.x - inner.left()) / inner.width()).clamp(0.0, 1.0),
            ((inner.bottom() - pos.y) / inner.height()).clamp(0.0, 1.0),
        )
    };
    // Beats in one cycle of the envelope: the snapping grid.
    let cycles = p.cycles.unsigned_abs().max(1);
    let beats = clock.loop_beats as f32 / cycles as f32;
    let step = if beats >= 1.0 && (beats - beats.round()).abs() < 1e-4 {
        1.0 / (beats * 4.0)
    } else {
        1.0 / 16.0
    };
    let free = ui.input(|i| i.modifiers.shift);
    let snap = |t: f32| {
        if free {
            t
        } else {
            ((t / step).round() * step).clamp(0.0, 1.0)
        }
    };
    let env = p.env.get();
    let mut pts: Vec<EnvPoint> = env.points().to_vec();
    let near = |pos: egui::Pos2, pts: &[EnvPoint]| {
        pts.iter()
            .enumerate()
            .map(|(i, q)| (i, to_screen(q.t, q.v).distance(pos)))
            .filter(|(_, d)| *d < 12.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    };
    let drag_id = id.with("env drag");
    let menu_id = id.with("env menu");
    if resp.drag_started() {
        // Where the button went down: a quick flick may already be away.
        let origin = ui
            .input(|i| i.pointer.press_origin())
            .or(resp.interact_pointer_pos());
        let hit = origin.and_then(|pos| near(pos, &pts));
        ui.data_mut(|d| d.insert_temp(drag_id, hit));
    }
    let dragging: Option<usize> = ui.data(|d| d.get_temp(drag_id)).flatten();
    if resp.dragged() {
        if let (Some(i), Some(pos)) = (dragging, resp.interact_pointer_pos()) {
            if i < pts.len() {
                let (t, v) = from_screen(pos);
                // Stay between the neighbours.
                let lo = if i > 0 { pts[i - 1].t } else { 0.0 };
                let hi = if i + 1 < pts.len() { pts[i + 1].t } else { 1.0 };
                pts[i].t = snap(t).clamp(lo, hi);
                pts[i].v = v;
                changed = true;
            }
        }
    }
    if resp.drag_stopped() {
        ui.data_mut(|d| d.remove::<Option<usize>>(drag_id));
    }
    if let Some(pos) = resp.interact_pointer_pos() {
        if resp.double_clicked() {
            if let Some(i) = near(pos, &pts) {
                pts.remove(i);
                changed = true;
            }
        } else if resp.clicked() && near(pos, &pts).is_none() && pts.len() < Envelope::MAX {
            let (t, v) = from_screen(pos);
            let t = snap(t);
            // New points keep the curve of the segment they split.
            let curve = pts
                .iter()
                .rev()
                .find(|q| q.t <= t)
                .or(pts.last())
                .map_or(Curve::Linear, |q| q.curve);
            pts.push(EnvPoint::new(t, v, curve));
            changed = true;
        }
        if resp.secondary_clicked() {
            let hit = near(pos, &pts);
            ui.data_mut(|d| d.insert_temp(menu_id, hit));
        }
    }
    let menu_point: Option<usize> = ui.data(|d| d.get_temp(menu_id)).flatten();
    resp.context_menu(|ui| match menu_point {
        Some(i) if i < pts.len() => {
            ui.label(RichText::new("Curve to the next point").small().weak());
            for c in Curve::ALL {
                if ui.selectable_label(pts[i].curve == c, c.label()).clicked() {
                    pts[i].curve = c;
                    changed = true;
                    ui.close();
                }
            }
            ui.separator();
            if ui.button("Delete point").clicked() {
                pts.remove(i);
                changed = true;
                ui.close();
            }
        }
        _ => {
            ui.label("Right-click a point to change its curve or delete it.");
            if ui.button("Reset to a rise and fall").clicked() {
                pts = Envelope::DEFAULT.points().to_vec();
                changed = true;
                ui.close();
            }
        }
    });
    let env = if changed {
        p.env = EnvRef::from_points(&pts);
        p.env.get()
    } else {
        env
    };

    // Drawing.
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, ui.visuals().extreme_bg_color);
    let grid = |t: f32, strong: bool| {
        let x = to_screen(t, 0.0).x;
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(1.0, Color32::from_gray(if strong { 62 } else { 40 })),
        );
    };
    if (1.0..=64.0).contains(&beats) {
        let n = beats.round() as u32;
        for b in 1..n {
            grid(b as f32 / beats, b % 4 == 0);
        }
    }
    for v in [0.0, 1.0] {
        let y = to_screen(0.0, v).y;
        painter.line_segment(
            [egui::pos2(inner.left(), y), egui::pos2(inner.right(), y)],
            egui::Stroke::new(1.0, Color32::from_gray(48)),
        );
    }
    let curve: Vec<egui::Pos2> = (0..=160)
        .map(|i| {
            let t = i as f32 / 160.0;
            to_screen(t, env.eval(t))
        })
        .collect();
    painter.add(egui::Shape::line(curve, egui::Stroke::new(1.5, ACCENT)));
    let hover = resp.hover_pos().and_then(|pos| near(pos, env.points()));
    for (i, q) in env.points().iter().enumerate() {
        let c = to_screen(q.t, q.v);
        let hot = hover == Some(i) || dragging == Some(i);
        painter.circle_filled(c, if hot { 6.0 } else { 4.5 }, Color32::WHITE);
        painter.circle_stroke(
            c,
            if hot { 6.0 } else { 4.5 },
            egui::Stroke::new(1.5, ACCENT),
        );
    }
    // Playhead: where the cycle is now.
    let now = (clock.phase * p.cycles as f32 + p.offset).rem_euclid(1.0);
    let x = to_screen(now, 0.0).x;
    painter.line_segment(
        [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
        egui::Stroke::new(1.0, Color32::from_gray(200)),
    );
    painter.circle_filled(to_screen(now, env.eval(now)), 3.0, Color32::from_gray(230));
    // The real values at the top and bottom.
    let font = egui::FontId::proportional(10.0);
    let weak = ui.visuals().weak_text_color();
    painter.text(
        rect.left_top() + egui::vec2(3.0, 1.0),
        egui::Align2::LEFT_TOP,
        format!("{:.2}", p.base + p.amp),
        font.clone(),
        weak,
    );
    painter.text(
        rect.left_bottom() + egui::vec2(3.0, -1.0),
        egui::Align2::LEFT_BOTTOM,
        format!("{:.2}", p.base),
        font,
        weak,
    );
    ui.label(
        RichText::new(
            "Click to add a point · drag to move (Shift: no snap) · right-click: curve / delete",
        )
        .small()
        .weak(),
    );
    changed
}

/// Small plot of the value over one loop, with the current moment marked.
fn curve_preview(ui: &mut Ui, p: &Param, clock: Clock, range: &RangeInclusive<f32>) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().min(240.0), 34.0),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, ui.visuals().extreme_bg_color);
    let music = music(ui);
    let ctx_at = |phase: f32| match &music {
        Some(m) if m.env.is_some() => {
            EvalCtx::with_music(&m.timing, &m.settings, phase, m.env.as_deref())
        }
        Some(MusicPreview {
            live: Some(f),
            timing,
            ..
        }) => EvalCtx::new(timing, phase, None).with_frame(*f),
        _ => {
            let mut c = EvalCtx::at(phase);
            c.loop_beats = clock.loop_beats;
            c
        }
    };
    let n = 120;
    let vals: Vec<f32> = (0..=n)
        .map(|i| p.eval(&ctx_at(i as f32 / n as f32)))
        .collect();
    // Fit the curve (not the whole slider range) so small swings stay visible.
    let (mut lo, mut hi) = (p.base, p.base);
    for v in &vals {
        lo = lo.min(*v);
        hi = hi.max(*v);
    }
    let min_span = (range.end() - range.start()).abs().max(1e-3) * 0.02;
    if hi - lo < min_span {
        let mid = (hi + lo) * 0.5;
        lo = mid - min_span * 0.5;
        hi = mid + min_span * 0.5;
    }
    let pad = (hi - lo) * 0.08;
    let (lo, hi) = (lo - pad, hi + pad);
    let span = hi - lo;
    let to_y = |v: f32| rect.bottom() - 3.0 - (v - lo) / span * (rect.height() - 6.0);
    // Beat ticks.
    let beats = clock.loop_beats.min(64);
    for b in 1..beats {
        let x = rect.left() + rect.width() * b as f32 / beats as f32;
        let bar = b % 4 == 0;
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(1.0, Color32::from_gray(if bar { 55 } else { 38 })),
        );
    }
    let base_y = to_y(p.base);
    painter.line_segment(
        [
            egui::pos2(rect.left(), base_y),
            egui::pos2(rect.right(), base_y),
        ],
        egui::Stroke::new(1.0, Color32::from_gray(80)),
    );
    let pts: Vec<egui::Pos2> = vals
        .iter()
        .enumerate()
        .map(|(i, v)| egui::pos2(rect.left() + rect.width() * i as f32 / n as f32, to_y(*v)))
        .collect();
    painter.add(egui::Shape::line(pts, egui::Stroke::new(1.5, ACCENT)));
    let x = rect.left() + rect.width() * clock.phase.rem_euclid(1.0);
    let y = to_y(p.eval(&ctx_at(clock.phase)));
    painter.line_segment(
        [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
        egui::Stroke::new(1.0, Color32::from_gray(200)),
    );
    painter.circle_filled(egui::pos2(x, y), 3.0, Color32::WHITE);
}

/// An animatable parameter: a slider for the base value plus a "~" toggle
/// revealing the loop-locked animation (wave, amount, rhythm, music) with a
/// live preview of the resulting curve.
pub fn param(
    ui: &mut Ui,
    label: &str,
    tip: &str,
    p: &mut Param,
    range: RangeInclusive<f32>,
) -> bool {
    let id = ui.id().with(("param", label));
    let mut open = ui
        .data(|d| d.get_temp::<bool>(id))
        .unwrap_or(p.is_animated());
    let mut changed = false;
    let span = (range.end() - range.start()).abs().max(0.01);
    ui.horizontal(|ui| {
        let r = ui.add_sized([110.0, 18.0], egui::Label::new(label).truncate());
        if !tip.is_empty() {
            r.on_hover_text(tip);
        }
        changed |= ui
            .add(
                egui::Slider::new(&mut p.base, range.clone()).clamping(egui::SliderClamping::Never),
            )
            .changed();
        let txt = if p.is_animated() {
            RichText::new("~").color(ACCENT).strong()
        } else {
            RichText::new("~")
        };
        if ui
            .selectable_label(open, txt)
            .on_hover_text("Animate this value in sync with the beat / music")
            .clicked()
        {
            open = !open;
        }
    });
    if open {
        let clock = clock(ui);
        // Picking a shape or a rhythm on a still value gives it a visible
        // amount straight away.
        let mut wake = false;
        ui.indent(id, |ui| {
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt(id.with("wave"))
                    .selected_text(p.wave.label())
                    .width(118.0)
                    .height(560.0)
                    .show_ui(ui, |ui| {
                        for (gi, (group, waves)) in WAVE_GROUPS.iter().enumerate() {
                            if gi > 0 {
                                ui.separator();
                            }
                            ui.label(RichText::new(*group).small().weak());
                            for w in waves.iter() {
                                if ui
                                    .selectable_value(&mut p.wave, *w, w.label())
                                    .on_hover_text(w.description())
                                    .changed()
                                {
                                    changed = true;
                                    wake = true;
                                }
                            }
                        }
                    })
                    .response
                    .on_hover_text(p.wave.description());
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.amp)
                            .speed(span * 0.005)
                            .prefix("amount "),
                    )
                    .on_hover_text("How far the value moves (0 = not animated)")
                    .changed();
            });
            ui.horizontal(|ui| {
                ui.menu_button("♩", |ui| {
                    ui.label(RichText::new("Rhythm").small().weak());
                    for (label, cycles) in sync_choices(clock.loop_beats) {
                        let r = ui
                            .selectable_label(p.cycles == cycles, format!("{label}  ({cycles}×)"));
                        if r.clicked() {
                            p.cycles = cycles;
                            changed = true;
                            wake = true;
                            ui.close();
                        }
                    }
                })
                .response
                .on_hover_text("Sync to the beat");
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.cycles)
                            .range(-128..=128)
                            .speed(0.1)
                            .suffix(" ×/loop"),
                    )
                    .on_hover_text(
                        "Whole cycles per loop: the number of beats in the loop hits every beat",
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.offset)
                            .range(0.0..=1.0)
                            .speed(0.01)
                            .prefix("offset "),
                    )
                    .on_hover_text("Shift the timing (fraction of one cycle)")
                    .changed();
                if p.audio != 0.0 {
                    changed |= ui
                        .add(egui::DragValue::new(&mut p.audio).speed(0.01).prefix("♪ "))
                        .on_hover_text("Adds the music loudness (older projects; use the 🎵 row)")
                        .changed();
                }
            });
            changed |= music_row(ui, id, p, span);
            if wake && p.amp == 0.0 {
                p.amp = span * 0.25;
            }
            if p.wave == ez_core::Wave::Envelope {
                changed |= envelope_editor(ui, id, p, clock);
            } else if p.is_animated() {
                curve_preview(ui, p, clock, &range);
            }
        });
    }
    ui.data_mut(|d| d.insert_temp(id, open));
    changed
}

/// The 🎵 row of a `~` panel: which part of the music drives the value.
fn music_row(ui: &mut Ui, id: egui::Id, p: &mut Param, span: f32) -> bool {
    use ez_core::AudioSource;
    let mut changed = false;
    let m = &mut p.music;
    ui.horizontal(|ui| {
        let text = if m.amount == 0.0 {
            "🎵 music: off".to_string()
        } else {
            format!("🎵 {}", m.source.label())
        };
        egui::ComboBox::from_id_salt(id.with("music"))
            .selected_text(text)
            .width(150.0)
            .height(420.0)
            .show_ui(ui, |ui| {
                if ui.selectable_label(m.amount == 0.0, "Off").clicked() {
                    m.amount = 0.0;
                    changed = true;
                }
                for (title, list) in [
                    ("Follow", &AudioSource::FOLLOW[..]),
                    ("On each hit", &AudioSource::HITS[..]),
                ] {
                    ui.separator();
                    ui.label(RichText::new(title).small().weak());
                    for src in list {
                        let r = ui
                            .selectable_label(m.amount != 0.0 && m.source == *src, src.label())
                            .on_hover_text(src.description());
                        if r.clicked() {
                            m.source = *src;
                            if m.amount == 0.0 {
                                m.amount = if *src == AudioSource::Pitch {
                                    1.0
                                } else {
                                    span * 0.3
                                };
                            }
                            changed = true;
                        }
                    }
                }
            })
            .response
            .on_hover_text("React to the music (needs a music file, MIDI notes or live input)");
        if m.amount != 0.0 {
            changed |= ui
                .add(
                    egui::DragValue::new(&mut m.amount)
                        .speed(span * 0.005)
                        .prefix("× "),
                )
                .on_hover_text("How much the music moves the value (negative pulls it down)")
                .changed();
        }
    });
    if m.amount == 0.0 {
        return changed;
    }
    ui.horizontal(|ui| {
        if m.source.is_hit() {
            egui::ComboBox::from_id_salt(id.with("music shape"))
                .selected_text(m.shape.label())
                .width(118.0)
                .show_ui(ui, |ui| {
                    for w in ez_core::Wave::ALL.iter().filter(|w| w.is_unipolar()) {
                        changed |= ui
                            .selectable_value(&mut m.shape, *w, w.label())
                            .on_hover_text(w.description())
                            .changed();
                    }
                })
                .response
                .on_hover_text("Shape played on every hit");
            changed |= ui
                .add(
                    egui::DragValue::new(&mut m.length)
                        .range(0.02..=16.0)
                        .speed(0.01)
                        .suffix(" beats"),
                )
                .on_hover_text("How long each hit lasts")
                .changed();
        } else {
            changed |= ui
                .checkbox(&mut m.smooth, "smooth")
                .on_hover_text("Follow a smoothed curve instead of every twitch")
                .changed();
            if m.source != AudioSource::Pitch {
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut m.threshold)
                            .range(0.0..=0.95)
                            .speed(0.005)
                            .prefix("ignore below "),
                    )
                    .on_hover_text("Only react to the loud parts")
                    .changed();
            }
        }
    });
    changed
}

/// Section header with an "enabled" checkbox.
pub fn toggle_section(
    ui: &mut Ui,
    title: &str,
    enabled: &mut bool,
    body: impl FnOnce(&mut Ui),
) -> bool {
    let mut changed = false;
    let id = ui.make_persistent_id(("section", title));
    egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false)
        .show_header(ui, |ui| {
            changed |= ui
                .checkbox(enabled, RichText::new(title).strong())
                .changed();
        })
        .body(|ui| {
            ui.add_enabled_ui(*enabled, body);
        });
    changed
}

pub fn section(ui: &mut Ui, title: &str, open: bool, body: impl FnOnce(&mut Ui)) {
    egui::CollapsingHeader::new(RichText::new(title).strong())
        .default_open(open)
        .show(ui, body);
}
