//! Small reusable editor widgets. Every helper returns `true` when the value
//! changed.

use egui::{Color32, RichText, Ui};
use ez_core::{Param, Wave};
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

/// An animatable parameter: a slider for the base value plus a "~" toggle
/// revealing the loop-locked oscillator and audio modulation.
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
            .on_hover_text("Animate this value in sync with the loop / music")
            .clicked()
        {
            open = !open;
        }
    });
    if open {
        ui.indent(id, |ui| {
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt(id.with("wave"))
                    .selected_text(p.wave.label())
                    .width(80.0)
                    .show_ui(ui, |ui| {
                        for w in Wave::ALL {
                            changed |= ui.selectable_value(&mut p.wave, w, w.label()).changed();
                        }
                    });
                let span = (range.end() - range.start()).abs().max(0.01);
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.amp)
                            .speed(span * 0.005)
                            .prefix("amount "),
                    )
                    .on_hover_text("How far the value swings (0 = not animated)")
                    .changed();
            });
            ui.horizontal(|ui| {
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.cycles)
                            .range(-64..=64)
                            .speed(0.1)
                            .suffix(" ×/loop"),
                    )
                    .on_hover_text(
                        "Whole cycles per loop — use the number of beats to hit every beat",
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.offset)
                            .range(0.0..=1.0)
                            .speed(0.01)
                            .prefix("offset "),
                    )
                    .changed();
                changed |= ui
                    .add(egui::DragValue::new(&mut p.audio).speed(0.01).prefix("♪ "))
                    .on_hover_text("Add the music loudness (needs a music file)")
                    .changed();
            });
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
