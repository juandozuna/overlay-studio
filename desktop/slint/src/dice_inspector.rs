//! Dice layer inspector wiring for the Design workspace (El Dado).
use crate::{Desktop, DiceFace, StudioWindow, refresh, report};
use overlay_studio::{
    application::document_session::LayerEdit,
    domain::{
        controls::Action,
        dice::{self, DiceConfig, Face},
        wheel,
    },
    model::Kind,
};
use slint::{ComponentHandle, Model, ModelRc, VecModel};

const SIZE: f32 = 520.;
const MARGIN: f32 = 40.;
const PRESETS: [&str; 5] = ["centre", "top-left", "top-right", "bottom-left", "bottom-right"];

/// Default footprint of a freshly created layer of this kind, when it differs from the 320x140 box.
pub fn default_size(kind: Kind) -> Option<(f32, f32)> {
    (kind == Kind::Dice).then_some((SIZE, SIZE))
}
/// A dragged Dice layer stays square; other kinds keep the dragged box.
pub fn square(kind: Kind, w: f32, h: f32) -> (f32, f32) {
    if kind == Kind::Dice {
        let side = w.max(h);
        (side, side)
    } else {
        (w, h)
    }
}
/// Top-left corner for a layer of `w`x`h` on a `canvas_w`x`canvas_h` canvas, 40px from the chosen edges.
pub fn placement(preset: &str, canvas_w: f32, canvas_h: f32, w: f32, h: f32) -> Option<(f32, f32)> {
    let (right, bottom) = (canvas_w - w - MARGIN, canvas_h - h - MARGIN);
    Some(match preset {
        "centre" => ((canvas_w - w) / 2., (canvas_h - h) / 2.),
        "top-left" => (MARGIN, MARGIN),
        "top-right" => (right, MARGIN),
        "bottom-left" => (MARGIN, bottom),
        "bottom-right" => (right, bottom),
        _ => return None,
    })
}
pub fn parse_roll(text: &str) -> Result<String, String> {
    text.trim()
        .parse::<f32>()
        .ok()
        .filter(|s| (0.5..=10.).contains(s))
        .map(|s| s.to_string())
        .ok_or("Roll time must be 0.5 to 10 seconds".into())
}
/// Comma-separated list for `dice.colors` with face `index` replaced by `text` (#RRGGBB).
pub fn colors_with(current: [[u8; 3]; 6], index: usize, text: &str) -> Result<String, String> {
    if index >= current.len() {
        return Err("Unknown face".into());
    }
    let color = wheel::parse_hex(text).ok_or(format!("\"{}\" is not a #RRGGBB color", text.trim()))?;
    let mut colors = current;
    colors[index] = color;
    Ok(colors.map(wheel::hex).join(","))
}
/// Rules are a single Spanish line; empty restores the default.
pub fn clean_rule(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(80).collect()
}

fn rule_key(index: usize) -> String {
    format!("{}{index}", dice::RULES_PREFIX)
}
fn ensure_defaults(s: &mut Desktop) {
    let values = &mut s.document.project.values;
    let before = values.len();
    values.entry(dice::ROLL_SECONDS.into()).or_insert_with(|| "2".into());
    values.entry(dice::COLORS.into()).or_default();
    for i in 0..6 {
        values.entry(rule_key(i)).or_default();
    }
    if values.len() != before {
        s.document.mark_external_edit();
    }
}
fn set(s: &mut Desktop, key: &str, value: &str) -> Result<(), String> {
    ensure_defaults(s);
    s.execute(Action::SetValue {
        field: key.into(),
        value: value.into(),
    })
}
fn index(key: &str, prefix: &str) -> Option<usize> {
    key.strip_prefix(prefix)?.parse().ok().filter(|i| *i < 6)
}
fn edit(s: &mut Desktop, key: &str, value: &str) -> Result<(), String> {
    let cfg = DiceConfig::read(&s.document.project.values);
    if key == "roll" {
        return set(s, dice::ROLL_SECONDS, &parse_roll(value)?);
    }
    if let Some(i) = index(key, "color.") {
        return set(s, dice::COLORS, &colors_with(cfg.colors, i, value)?);
    }
    if let Some(i) = index(key, "rule.") {
        return set(s, &rule_key(i), &clean_rule(value));
    }
    if key == "reset" {
        set(s, dice::COLORS, "")?;
        return (0..6).try_for_each(|i| set(s, &rule_key(i), ""));
    }
    if PRESETS.contains(&key) {
        let id = s
            .document
            .selected_layer()
            .filter(|e| e.kind == Kind::Dice)
            .map(|e| (e.id, e.w, e.h, e.rotation))
            .ok_or("Select a Dice layer")?;
        let d = s.document.active();
        let (x, y) = placement(key, d.width as f32, d.height as f32, id.1, id.2).ok_or("Unknown placement")?;
        return s.document.apply(
            id.0,
            LayerEdit::Geometry {
                x,
                y,
                width: id.1,
                height: id.2,
                rotation: id.3,
            },
        );
    }
    Err("Unknown dice property".into())
}

pub fn refresh_panel(ui: &StudioWindow, s: &Desktop) {
    let is_dice = s.document.selected_layer().is_some_and(|e| e.kind == Kind::Dice);
    ui.set_dice_layer(is_dice);
    if !is_dice {
        return;
    }
    let values = &s.document.project.values;
    let cfg = DiceConfig::read(values);
    let roll = cfg.roll_seconds.to_string();
    if ui.get_dice_roll().as_str() != roll {
        ui.set_dice_roll(roll.into());
    }
    let faces: Vec<DiceFace> = Face::ALL
        .iter()
        .map(|f| {
            let [r, g, b] = cfg.colors[f.index()];
            DiceFace {
                name: f.name().into(),
                code: f.code().into(),
                swatch: slint::Color::from_rgb_u8(r, g, b),
                color_text: wheel::hex(cfg.colors[f.index()]).into(),
                rule: values.get(&rule_key(f.index())).map(String::as_str).unwrap_or("").into(),
                placeholder: f.default_rule().into(),
            }
        })
        .collect();
    let model = ui.get_dice_faces();
    let same = model.row_count() == faces.len() && faces.iter().enumerate().all(|(i, f)| model.row_data(i).as_ref() == Some(f));
    if !same {
        // A fresh model rebuilds the fields, so edited text never goes stale after undo.
        ui.set_dice_faces(ModelRc::new(VecModel::from(faces)));
    }
}

pub fn connect(ui: &StudioWindow, state: std::rc::Rc<std::cell::RefCell<Desktop>>) {
    let weak = ui.as_weak();
    ui.on_dice_edit(move |key, value| {
        if let Some(ui) = weak.upgrade() {
            let mut s = state.borrow_mut();
            let result = edit(&mut s, &key, &value);
            report(&ui, result);
            refresh(&ui, &s);
            ui.window().request_redraw();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placements_keep_size_and_margin() {
        assert_eq!(placement("centre", 1920., 1080., 520., 520.), Some((700., 280.)));
        assert_eq!(placement("top-left", 1920., 1080., 520., 520.), Some((40., 40.)));
        assert_eq!(placement("top-right", 1920., 1080., 520., 520.), Some((1360., 40.)));
        assert_eq!(placement("bottom-left", 1920., 1080., 520., 520.), Some((40., 520.)));
        assert_eq!(placement("bottom-right", 1920., 1080., 520., 520.), Some((1360., 520.)));
        assert_eq!(placement("nowhere", 1920., 1080., 520., 520.), None);
    }
    #[test]
    fn colors_are_validated_and_normalised() {
        let defaults = DiceConfig::default().colors;
        let list = colors_with(defaults, 2, "#00ff00").unwrap();
        assert_eq!(list.split(',').count(), 6);
        assert_eq!(list.split(',').nth(2), Some("#00FF00"));
        assert!(colors_with(defaults, 0, "red").is_err());
        assert!(colors_with(defaults, 6, "#000000").is_err());
    }
    #[test]
    fn roll_time_is_bounded() {
        assert_eq!(parse_roll(" 3 ").unwrap(), "3");
        assert!(parse_roll("0.4").is_err());
        assert!(parse_roll("10.5").is_err());
        assert!(parse_roll("fast").is_err());
    }
    #[test]
    fn rules_are_single_line() {
        assert_eq!(clean_rule("  ROJO \n – Directa "), "ROJO – Directa");
        assert_eq!(clean_rule("   "), "");
    }
    #[test]
    fn dice_is_square() {
        assert_eq!(default_size(Kind::Dice), Some((520., 520.)));
        assert_eq!(default_size(Kind::Text), None);
        assert_eq!(square(Kind::Dice, 300., 200.), (300., 300.));
        assert_eq!(square(Kind::Rectangle, 300., 200.), (300., 200.));
    }
}
