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

/// Small plot of the value over one loop, with the current moment marked.
fn curve_preview(ui: &mut Ui, p: &Param, clock: Clock, range: &RangeInclusive<f32>) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().min(240.0), 34.0),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, ui.visuals().extreme_bg_color);
    let ctx_at = |phase: f32| {
        let mut c = EvalCtx::at(phase);
        c.loop_beats = clock.loop_beats;
        c
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
                    .height(420.0)
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
                changed |= ui
                    .add(egui::DragValue::new(&mut p.audio).speed(0.01).prefix("♪ "))
                    .on_hover_text("Add the music loudness (needs a music file)")
                    .changed();
            });
            if wake && p.amp == 0.0 {
                p.amp = span * 0.25;
            }
            if p.is_animated() {
                curve_preview(ui, p, clock, &range);
            }
        });
    }
    ui.data_mut(|d| d.insert_temp(id, open));
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
