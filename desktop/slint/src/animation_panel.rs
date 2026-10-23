//! Timeline presentation and transport; edits are validated document transactions.
use crate::{
    AnimationRow, Desktop, FrameDot, FramePose, StudioWindow, refresh, report, view_models,
};
use overlay_studio::model::{Animation, Easing, Keyframe};
use slint::{ComponentHandle, Model};
use std::{cell::RefCell, collections::BTreeSet, rc::Rc, time::Instant};

pub struct Session {
    pub time: f32,
    pub preview: bool,
    pub playing: bool,
    anchor: Instant,
    anchor_time: f32,
    media_end: f32,
    pub selected: BTreeSet<(u64, usize)>,
    clipboard: Vec<(u64, Keyframe)>,
    clipboard_generation: u64,
    clipboard_design: usize,
}
impl Default for Session {
    fn default() -> Self {
        Self {
            time: 0.,
            preview: false,
            playing: false,
            anchor: Instant::now(),
            anchor_time: 0.,
            media_end: 0.,
            selected: BTreeSet::new(),
            clipboard: vec![],
            clipboard_generation: 0,
            clipboard_design: 0,
        }
    }
}
impl Session {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    fn seek(&mut self, time: f32) -> Result<(), String> {
        if !time.is_finite() || !(0. ..=300.).contains(&time) {
            return Err("Time must be from 0 to 300 seconds".into());
        }
        self.time = time;
        self.anchor_time = time;
        self.anchor = Instant::now();
        self.preview = true;
        self.playing = false;
        Ok(())
    }
}
pub fn tick(ui: &StudioWindow, s: &mut Desktop) -> bool {
    if !s.animation.playing {
        return false;
    }
    if ui.get_workspace() != 3 || ui.get_event_editor() {
        s.animation.playing = false;
        ui.set_animation_playing(false);
        return false;
    }
    let end = s
        .document
        .active()
        .elements
        .iter()
        .map(|e| {
            e.keyframes
                .iter()
                .map(|f| f.time)
                .fold(e.duration, f32::max)
        })
        .fold(s.animation.media_end.max(0.01), f32::max);
    let looping = s
        .document
        .active()
        .elements
        .iter()
        .any(|e| e.loop_animation);
    let elapsed = s.animation.anchor_time + s.animation.anchor.elapsed().as_secs_f32();
    s.animation.time = if looping {
        elapsed % end
    } else {
        elapsed.min(end)
    };
    if !looping && elapsed >= end {
        s.animation.playing = false;
        ui.set_animation_playing(false);
    }
    ui.set_animation_time(s.animation.time);
    true
}
pub fn refresh_panel(ui: &StudioWindow, s: &Desktop) {
    ui.set_animation_time(s.animation.time);
    ui.set_animation_preview(s.animation.preview);
    ui.set_animation_playing(s.animation.playing);
    let old = ui.get_animation_layers();
    let rows = s
        .document
        .active()
        .elements
        .iter()
        .map(|e| {
            let existing = old
                .iter()
                .find(|row| row.id.as_str() == e.id.to_string())
                .map(|row| row.frames)
                .unwrap_or_default();
            let frames = view_models::sync(
                existing,
                e.keyframes
                    .iter()
                    .enumerate()
                    .map(|(index, f)| FrameDot {
                        index: index as i32,
                        time: f.time,
                        selected: s.animation.selected.contains(&(e.id, index)),
                    })
                    .collect(),
            );
            AnimationRow {
                id: e.id.to_string().into(),
                name: e.name.clone().into(),
                selected: s.document.selected.contains(&e.id),
                frames,
            }
        })
        .collect();
    ui.set_animation_layers(view_models::sync(old, rows));
    let mut pose = FramePose::default();
    if let Some(e) = s.document.selected_layer() {
        pose.layer = e.name.clone().into();
        pose.duration = e.duration.to_string().into();
        pose.looping = e.loop_animation;
        pose.preset = match e.animation {
            Animation::None => 0,
            Animation::Fade => 1,
            Animation::Slide => 2,
            Animation::Pop => 3,
        };
        if let Some((_, index)) = s.animation.selected.iter().find(|(id, _)| *id == e.id)
            && let Some(f) = e.keyframes.get(*index)
        {
            pose.has_frame = true;
            pose.time = f.time.to_string().into();
            pose.x = f.x.to_string().into();
            pose.y = f.y.to_string().into();
            pose.w = f.w.to_string().into();
            pose.h = f.h.to_string().into();
            pose.rotation = f.rotation.to_string().into();
            pose.opacity = f.opacity.to_string().into();
            pose.easing = match f.easing {
                Easing::Linear => 0,
                Easing::EaseOut => 1,
                Easing::EaseInOut => 2,
                Easing::Hold => 3,
                Easing::EaseOutBack => 4,
                Easing::Spring => 5,
            };
        }
    }
    ui.set_animation_pose(pose);
}
fn edit(ui: &StudioWindow, s: &mut Desktop, key: &str, value: &str) -> Result<(), String> {
    let number = || {
        value
            .parse::<f32>()
            .map_err(|_| "Enter a number".to_string())
    };
    if key == "seek" {
        return s.animation.seek(number()?);
    }
    if key == "span" {
        let span = number()?;
        if !span.is_finite() || !(1. ..=300.).contains(&span) {
            return Err("View must be from 1 to 300 seconds".into());
        }
        ui.set_animation_span(span);
        return Ok(());
    }
    let e = s
        .document
        .selected_layer()
        .cloned()
        .ok_or("Select a layer")?;
    if matches!(key, "preset" | "duration" | "loop") {
        let preset = if key == "preset" {
            match value {
                "0" => Animation::None,
                "1" => Animation::Fade,
                "2" => Animation::Slide,
                "3" => Animation::Pop,
                _ => return Err("Choose an entrance preset".into()),
            }
        } else {
            e.animation
        };
        let duration = if key == "duration" {
            number()?
        } else {
            e.duration
        };
        let looping = if key == "loop" {
            value == "true"
        } else {
            e.loop_animation
        };
        return s.document.animation_setup(e.id, preset, duration, looping);
    }
    let index = s
        .animation
        .selected
        .iter()
        .find(|(id, _)| *id == e.id)
        .map(|(_, i)| *i)
        .ok_or("Select a keyframe diamond")?;
    let mut f = e
        .keyframes
        .get(index)
        .cloned()
        .ok_or("Keyframe no longer exists")?;
    match key {
        "time" => f.time = number()?,
        "x" => f.x = number()?,
        "y" => f.y = number()?,
        "w" => f.w = number()?,
        "h" => f.h = number()?,
        "rotation" => f.rotation = number()?,
        "opacity" => f.opacity = number()?,
        "easing" => {
            f.easing = match value {
                "0" => Easing::Linear,
                "1" => Easing::EaseOut,
                "2" => Easing::EaseInOut,
                "3" => Easing::Hold,
                "4" => Easing::EaseOutBack,
                "5" => Easing::Spring,
                _ => return Err("Choose easing".into()),
            }
        }
        _ => return Err("Unknown animation property".into()),
    }
    let time = f.time;
    s.document.edit_keyframe(e.id, index, f)?;
    s.animation.seek(time)
}
pub fn connect(ui: &StudioWindow, state: Rc<RefCell<Desktop>>) {
    let shared = state.clone();
    let weak = ui.as_weak();
    ui.on_animation_seek(move |time| {
        if let Some(ui) = weak.upgrade() {
            let mut s = shared.borrow_mut();
            let result = s.animation.seek(time);
            report(&ui, result);
            ui.set_animation_time(s.animation.time);
            ui.set_animation_preview(s.animation.preview);
            ui.set_animation_playing(false);
            ui.window().request_redraw();
        }
    });
    let shared = state.clone();
    let weak = ui.as_weak();
    ui.on_animation_edit(move |key, value| {
        if let Some(ui) = weak.upgrade() {
            let mut s = shared.borrow_mut();
            let result = edit(&ui, &mut s, &key, &value);
            report(&ui, result);
            refresh(&ui, &s);
            ui.window().request_redraw();
        }
    });
    let shared = state.clone();
    let weak = ui.as_weak();
    ui.on_animation_select(move |id, index, extend| {
        if let Some(ui) = weak.upgrade() {
            let mut s = shared.borrow_mut();
            if let Ok(id) = id.parse::<u64>() {
                let frame = if index >= 0 {
                    s.document
                        .active()
                        .elements
                        .iter()
                        .find(|e| e.id == id)
                        .and_then(|e| e.keyframes.get(index as usize))
                        .cloned()
                } else {
                    None
                };
                if index < 0 {
                    s.animation.selected.clear();
                } else if let Some(f) = frame {
                    let key = (id, index as usize);
                    if !s.animation.selected.contains(&key) {
                        if !extend {
                            s.animation.selected.clear();
                        }
                        s.animation.selected.insert(key);
                    }
                    let _ = s.animation.seek(f.time);
                }
                s.document.select(Some(id), false);
                refresh(&ui, &s);
                ui.window().request_redraw();
            }
        }
    });
    let shared = state.clone();
    let weak = ui.as_weak();
    ui.on_animation_move(move |_id, _index, delta| {
        if let Some(ui) = weak.upgrade() {
            let mut s = shared.borrow_mut();
            let keys = s.animation.selected.iter().copied().collect::<Vec<_>>();
            let times = keys
                .iter()
                .filter_map(|(id, i)| {
                    s.document
                        .active()
                        .elements
                        .iter()
                        .find(|e| e.id == *id)
                        .and_then(|e| e.keyframes.get(*i))
                        .map(|f| f.time)
                })
                .collect::<Vec<_>>();
            let min = times.iter().copied().fold(300., f32::min);
            let max = times.iter().copied().fold(0., f32::max);
            let delta = (delta * 100.).round() / 100.;
            let delta = delta.clamp(-min, 300. - max);
            let result = s.document.move_keyframes(&keys, delta);
            report(&ui, result);
            refresh(&ui, &s);
            ui.window().request_redraw();
        }
    });
    let weak = ui.as_weak();
    ui.on_animation_command(move |command| {
        if let Some(ui) = weak.upgrade() {
            let mut s = state.borrow_mut();
            let result = (|| {
                match command.as_str() {
                    "play" => {
                        s.animation.media_end = s
                            .document
                            .active()
                            .elements
                            .iter()
                            .filter_map(|e| e.motion)
                            .filter_map(|id| s.document.project.motions.iter().find(|m| m.id == id))
                            .filter_map(|motion| {
                                overlay_studio::scene::decode_motion(&motion.gif)
                                    .ok()
                                    .map(|frames| frames.len() as f32 / motion.fps)
                            })
                            .fold(0., f32::max);
                        let end = s
                            .document
                            .active()
                            .elements
                            .iter()
                            .map(|e| {
                                e.keyframes
                                    .iter()
                                    .map(|f| f.time)
                                    .fold(e.duration, f32::max)
                            })
                            .fold(s.animation.media_end.max(0.01), f32::max);
                        if !s.animation.playing && s.animation.time >= end {
                            s.animation.time = 0.;
                        }
                        s.animation.playing = !s.animation.playing;
                        s.animation.preview = true;
                        s.animation.anchor_time = s.animation.time;
                        s.animation.anchor = Instant::now();
                    }
                    "restart" => {
                        s.animation.seek(0.)?;
                    }
                    "preview" => {
                        s.animation.preview = !s.animation.preview;
                        s.animation.playing = false;
                    }
                    "add" => {
                        let e = s
                            .document
                            .selected_layer()
                            .cloned()
                            .ok_or("Select a layer")?;
                        let sampled = overlay_studio::scene::sample_keyframes(&e, s.animation.time);
                        let f = Keyframe::capture(&sampled, s.animation.time);
                        let keys = s.document.insert_keyframes(vec![(e.id, f)])?;
                        s.animation.selected = keys.into_iter().collect();
                        s.animation.playing = false;
                    }
                    "copy" => {
                        let keys = s.animation.selected.clone();
                        let copied = keys
                            .iter()
                            .filter_map(|(id, i)| {
                                s.document
                                    .active()
                                    .elements
                                    .iter()
                                    .find(|e| e.id == *id)
                                    .and_then(|e| e.keyframes.get(*i))
                                    .map(|f| (*id, f.clone()))
                            })
                            .collect::<Vec<_>>();
                        if copied.is_empty() {
                            return Err("Select keyframes to copy".into());
                        }
                        s.animation.clipboard = copied;
                        s.animation.clipboard_generation = s.document.generation;
                        s.animation.clipboard_design = s.document.design;
                    }
                    "paste" => {
                        if s.animation.clipboard.is_empty()
                            || s.animation.clipboard_generation != s.document.generation
                            || s.animation.clipboard_design != s.document.design
                        {
                            return Err("Copy keyframes in this project first".into());
                        }
                        let start = s
                            .animation
                            .clipboard
                            .iter()
                            .map(|(_, f)| f.time)
                            .fold(300., f32::min);
                        let frames = s
                            .animation
                            .clipboard
                            .iter()
                            .map(|(id, f)| {
                                let mut f = f.clone();
                                f.time += s.animation.time - start;
                                (*id, f)
                            })
                            .collect();
                        let keys = s.document.insert_keyframes(frames)?;
                        s.animation.selected = keys.into_iter().collect();
                    }
                    "delete" => {
                        let keys = s.animation.selected.iter().copied().collect::<Vec<_>>();
                        s.document.remove_keyframes(&keys)?;
                        s.animation.selected.clear();
                    }
                    _ => return Err("Unknown animation action".into()),
                }
                Ok(())
            })();
            report(&ui, result);
            refresh(&ui, &s);
            ui.window().request_redraw();
        }
    });
}
