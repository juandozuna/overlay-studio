//! Effects and sound panel wiring (FX-4).
use crate::{Desktop, FxDesign, FxTeam, FxView, StudioWindow, report};
use overlay_studio::{
    domain::{
        controls::Action,
        fx::{self, Dir, FxConfig, FxStyle},
        sound::{self, BUILTINS, SoundRef, SoundRequest},
        wheel,
    },
    infrastructure::audio_import,
    model::{Design, SoundAsset},
};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::{cell::RefCell, path::Path, rc::Rc, time::Duration};

#[derive(Default)]
pub struct Session {
    snap: RefCell<Option<Snap>>,
    redraw: Rc<slint::Timer>,
}

#[derive(Clone, PartialEq)]
struct Snap {
    colors: (String, String),
    seconds: String,
    volume: f32,
    global: Vec<String>,
    team: Vec<String>,
    imported: Vec<String>,
    teams: Vec<(String, String, i32, i32)>,
    designs: Vec<(String, i32, i32)>,
    picks: [i32; 5],
    has_team: bool,
}

const DEFAULTS: [(&str, &str); 9] = [
    (fx::ADD_COLOR, "#22C55E"),
    (fx::SUB_COLOR, "#000000"),
    (fx::SECONDS, "1.2"),
    (sound::VOLUME, "0.6"),
    (sound::ADD, ""),
    (sound::SUB, ""),
    (sound::WHEEL_SPIN, ""),
    (sound::WHEEL_LAND, ""),
    (sound::WHEEL_REMOVE, ""),
];
const PICKS: [&str; 5] = [sound::ADD, sound::SUB, sound::WHEEL_SPIN, sound::WHEEL_LAND, sound::WHEEL_REMOVE];

pub fn normalize_color(text: &str) -> Result<String, String> {
    wheel::parse_hex(text).map(wheel::hex).ok_or(format!("\"{}\" is not a #RRGGBB color", text.trim()))
}
pub fn parse_seconds(text: &str) -> Result<String, String> {
    text.trim()
        .parse::<f32>()
        .ok()
        .filter(|s| (0.3..=4.).contains(s))
        .map(|s| s.to_string())
        .ok_or("Effect length must be 0.3 to 4 seconds".into())
}
pub fn parse_volume(text: &str) -> Result<String, String> {
    text.trim()
        .parse::<f32>()
        .ok()
        .filter(|v| (0.0..=1.0).contains(v))
        .map(|v| format!("{v:.2}"))
        .ok_or("Volume must be between 0 and 1".into())
}
fn capitalised(name: &str) -> String {
    let mut chars = name.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}
/// Names for a sound dropdown: the first entry stands for "no value", then built-ins, then imports.
pub fn sound_names(first: &str, sounds: &[SoundAsset]) -> Vec<String> {
    std::iter::once(first.to_string())
        .chain(BUILTINS.iter().map(|n| capitalised(n)))
        .chain(sounds.iter().map(|s| s.name.clone()))
        .collect()
}
/// Dropdown index for a stored reference; unknown or missing sounds show as the first entry.
pub fn sound_index(text: &str, sounds: &[SoundAsset]) -> i32 {
    match SoundRef::parse(text) {
        Some(SoundRef::Builtin(name)) => BUILTINS.iter().position(|b| *b == name).map_or(0, |i| i as i32 + 1),
        Some(SoundRef::Asset(id)) => {
            sounds.iter().position(|s| s.id == id).map_or(0, |i| (BUILTINS.len() + i) as i32 + 1)
        }
        None => 0,
    }
}
/// Stored reference for a dropdown index; index 0 is the empty reference.
pub fn sound_value(index: &str, sounds: &[SoundAsset]) -> Result<String, String> {
    let i: usize = index.parse().map_err(|_| "Unknown sound choice")?;
    match i {
        0 => Ok(String::new()),
        i if i <= BUILTINS.len() => Ok(SoundRef::Builtin(BUILTINS[i - 1].into()).to_text()),
        i => sounds
            .get(i - BUILTINS.len() - 1)
            .map(|s| SoundRef::Asset(s.id).to_text())
            .ok_or("Unknown sound choice".into()),
    }
}
/// Sets the style on every layer of the design that has an effect key; returns how many.
pub fn apply_style(design: &mut Design, style: FxStyle) -> usize {
    let mut count = 0;
    for e in design.elements.iter_mut().filter(|e| !e.fx_key.is_empty()) {
        e.fx_style = style;
        count += 1;
    }
    count
}
fn design_style(design: &Design) -> (FxStyle, usize) {
    let layers = design.elements.iter().filter(|e| !e.fx_key.is_empty());
    (layers.clone().next().map(|e| e.fx_style).unwrap_or_default(), layers.count())
}

fn ensure_defaults(s: &mut Desktop) {
    let teams: Vec<u64> = s.document.project.teams.iter().map(|t| t.id).collect();
    let values = &mut s.document.project.values;
    let before = values.len();
    for (key, value) in DEFAULTS {
        values.entry(key.into()).or_insert_with(|| value.into());
    }
    for id in teams {
        for dir in [Dir::Add, Dir::Sub] {
            values.entry(sound::team_key(id, dir)).or_default();
        }
    }
    if values.len() != before {
        s.document.mark_external_edit();
    }
}
fn set(s: &mut Desktop, key: &str, value: &str) -> Result<(), String> {
    ensure_defaults(s);
    s.execute(Action::SetValue { field: key.into(), value: value.into() })
}
fn edit(s: &mut Desktop, key: &str, value: &str) -> Result<(), String> {
    let sounds = &s.document.project.sounds;
    let team_key = |rest: &str, dir| {
        let id: u64 = rest.parse().map_err(|_| "Unknown team")?;
        s.document.project.teams.iter().any(|t| t.id == id).then(|| sound::team_key(id, dir)).ok_or("Unknown team")
    };
    match key {
        "add_color" => set(s, fx::ADD_COLOR, &normalize_color(value)?),
        "sub_color" => set(s, fx::SUB_COLOR, &normalize_color(value)?),
        "seconds" => set(s, fx::SECONDS, &parse_seconds(value)?),
        "volume" => set(s, sound::VOLUME, &parse_volume(value)?),
        "sound_add" | "sound_sub" | "wheel_spin" | "wheel_land" | "wheel_remove" => {
            let target = PICKS[["sound_add", "sound_sub", "wheel_spin", "wheel_land", "wheel_remove"]
                .iter()
                .position(|k| *k == key)
                .unwrap_or(0)];
            let value = sound_value(value, sounds)?;
            set(s, target, &value)
        }
        _ if key.starts_with("team_add.") || key.starts_with("team_sub.") => {
            let (kind, id) = key.split_once('.').unwrap_or_default();
            let target = team_key(id, if kind == "team_add" { Dir::Add } else { Dir::Sub })?;
            let value = sound_value(value, sounds)?;
            set(s, &target, &value)
        }
        _ if key.starts_with("style.") => {
            let i: usize = key[6..].parse().map_err(|_| "Unknown layout")?;
            let style = value
                .parse::<usize>()
                .ok()
                .and_then(|i| FxStyle::ALL.get(i).copied())
                .ok_or("Unknown effect style")?;
            let design = s.document.project.designs.get_mut(i).ok_or("Unknown layout")?;
            if apply_style(design, style) == 0 {
                return Err("This layout has no team layers to animate".into());
            }
            s.document.mark_external_edit();
            Ok(())
        }
        _ => Err("Unknown effects property".into()),
    }
}

fn request_for(values: &std::collections::BTreeMap<String, String>, key: &str, fallback: Option<&str>) -> Result<SoundRequest, String> {
    let pick = |k: &str| values.get(k).and_then(|v| SoundRef::parse(v));
    pick(key)
        .or_else(|| fallback.and_then(pick))
        .map(|sound| SoundRequest { sound, volume: sound::volume(values) })
        .ok_or("Choose a sound first".into())
}
fn test(s: &mut Desktop, which: &str) -> Result<(), String> {
    let values = &s.document.project.values;
    let request = match which {
        "add" => request_for(values, sound::ADD, None),
        "sub" => request_for(values, sound::SUB, None),
        "spin" => request_for(values, sound::WHEEL_SPIN, None),
        "land" => request_for(values, sound::WHEEL_LAND, None),
        "remove" => request_for(values, sound::WHEEL_REMOVE, None),
        _ => {
            let (kind, id) = which.split_once(':').unwrap_or_default();
            let id: u64 = id.parse().map_err(|_| "Unknown team")?;
            let dir = if kind == "add" { Dir::Add } else { Dir::Sub };
            request_for(values, &sound::team_key(id, dir), Some(if dir == Dir::Add { sound::ADD } else { sound::SUB }))
        }
    }?;
    let Desktop { document, audio, .. } = s;
    audio.test_play(&request, &document.project.sounds)
}

fn add_sound(s: &mut Desktop, path: &Path) -> Result<String, String> {
    let id = s.document.project.next_id();
    let asset = audio_import::import_sound(path, id)?;
    crate::audio::Engine::validate(&asset.bytes)?;
    let name = asset.name.clone();
    s.document.project.sounds.push(asset);
    s.document.mark_external_edit();
    ensure_defaults(s);
    if s.document.project.values.get(sound::ADD).is_none_or(|v| v.is_empty()) {
        set(s, sound::ADD, &SoundRef::Asset(id).to_text())?;
    }
    Ok(name)
}
/// Refuses while any rule still plays the sound.
fn remove_sound(s: &mut Desktop, index: &str) -> Result<(), String> {
    let sounds = &s.document.project.sounds;
    let asset = index.parse::<usize>().ok().and_then(|i| sounds.get(i)).ok_or("Unknown sound")?;
    let used = SoundRef::Asset(asset.id).to_text();
    if s.document.project.values.values().any(|v| v.trim() == used) {
        return Err(format!("\"{}\" is still used by a sound choice; pick another sound first", asset.name));
    }
    let i = index.parse::<usize>().unwrap_or(0);
    s.document.project.sounds.remove(i);
    s.document.mark_external_edit();
    Ok(())
}
fn preview(s: &mut Desktop, dir: &str) -> Result<(), String> {
    let id = s.document.project.teams.first().ok_or("Add a team first")?.id;
    let prefix = fx::team_prefix(id);
    let values = &mut s.document.project.values;
    let next = fx::read_state(values, &prefix).0.saturating_add(1);
    values.insert(fx::id_key(&prefix), next.to_string());
    values.insert(fx::dir_key(&prefix), if dir == "sub" { "sub" } else { "add" }.into());
    s.document.revision += 1;
    Ok(())
}

pub fn refresh_panel(ui: &StudioWindow, s: &Desktop) {
    let p = &s.document.project;
    let cfg = FxConfig::read(&p.values);
    let get = |k: &str| p.values.get(k).map(String::as_str).unwrap_or("");
    let snap = Snap {
        colors: (wheel::hex(cfg.add_color), wheel::hex(cfg.sub_color)),
        seconds: cfg.seconds.to_string(),
        volume: sound::volume(&p.values),
        global: sound_names("None", &p.sounds),
        team: sound_names("Use global", &p.sounds),
        imported: p.sounds.iter().map(|a| a.name.clone()).collect(),
        teams: p
            .teams
            .iter()
            .map(|t| {
                let at = |dir| sound_index(get(&sound::team_key(t.id, dir)), &p.sounds);
                (t.id.to_string(), t.label.clone(), at(Dir::Add), at(Dir::Sub))
            })
            .collect(),
        designs: p
            .designs
            .iter()
            .map(|d| {
                let (style, layers) = design_style(d);
                (d.name.clone(), FxStyle::ALL.iter().position(|s| *s == style).unwrap_or(0) as i32, layers as i32)
            })
            .collect(),
        picks: PICKS.map(|k| sound_index(get(k), &p.sounds)),
        has_team: !p.teams.is_empty(),
    };
    if s.fx.snap.borrow().as_ref() == Some(&snap) {
        return;
    }
    let strings = |v: &[String]| ModelRc::new(VecModel::from(v.iter().map(SharedString::from).collect::<Vec<_>>()));
    let swatch = |c: [u8; 3]| slint::Color::from_rgb_u8(c[0], c[1], c[2]);
    ui.set_fx_view(FxView {
        add_color: snap.colors.0.clone().into(),
        sub_color: snap.colors.1.clone().into(),
        add_swatch: swatch(cfg.add_color),
        sub_swatch: swatch(cfg.sub_color),
        seconds: snap.seconds.clone().into(),
        volume: snap.volume,
        global_sounds: strings(&snap.global),
        team_sounds: strings(&snap.team),
        imported: strings(&snap.imported),
        styles: strings(&FxStyle::ALL.map(|s| s.label().to_string())),
        teams: ModelRc::new(VecModel::from(
            snap.teams
                .iter()
                .map(|(id, label, add, sub)| FxTeam { id: id.into(), label: label.into(), add: *add, sub: *sub })
                .collect::<Vec<_>>(),
        )),
        designs: ModelRc::new(VecModel::from(
            snap.designs
                .iter()
                .enumerate()
                .map(|(index, (name, style, layers))| FxDesign {
                    index: index as i32,
                    name: name.into(),
                    style: *style,
                    layers: *layers,
                })
                .collect::<Vec<_>>(),
        )),
        sound_add: snap.picks[0],
        sound_sub: snap.picks[1],
        wheel_spin: snap.picks[2],
        wheel_land: snap.picks[3],
        wheel_remove: snap.picks[4],
        has_team: snap.has_team,
    });
    s.fx.snap.replace(Some(snap));
}

fn refresh_now(ui: &StudioWindow, s: &Desktop) {
    s.fx.snap.replace(None);
    crate::refresh(ui, s);
    ui.window().request_redraw();
}
fn keep_redrawing(ui: &StudioWindow, s: &Desktop) {
    let seconds = FxConfig::read(&s.document.project.values).seconds + 0.3;
    let weak = ui.as_weak();
    s.fx.redraw.start(slint::TimerMode::Repeated, Duration::from_millis(16), move || {
        if let Some(ui) = weak.upgrade() {
            ui.window().request_redraw();
        }
    });
    let timer = s.fx.redraw.clone();
    slint::Timer::single_shot(Duration::from_secs_f32(seconds), move || timer.stop());
}
fn import(ui: &StudioWindow, state: Rc<RefCell<Desktop>>) {
    let weak = ui.as_weak();
    let _ = slint::spawn_local(async move {
        let Some(file) = rfd::AsyncFileDialog::new().add_filter("Sound", &audio_import::EXTENSIONS).pick_file().await
        else {
            return;
        };
        let mut s = state.borrow_mut();
        let result = add_sound(&mut s, file.path());
        if let Some(ui) = weak.upgrade() {
            let status = result.as_ref().map(|n| format!("Imported \"{n}\"")).map_err(Clone::clone);
            refresh_now(&ui, &s);
            match status {
                Ok(text) => ui.set_status(text.into()),
                Err(_) => report(&ui, result.map(|_| ())),
            }
        }
    });
}

pub fn connect(ui: &StudioWindow, state: Rc<RefCell<Desktop>>) {
    let shared = state.clone();
    let weak = ui.as_weak();
    ui.on_fx_edit(move |key, value| {
        if let Some(ui) = weak.upgrade() {
            let mut s = shared.borrow_mut();
            let result = edit(&mut s, &key, &value);
            report(&ui, result);
            refresh_now(&ui, &s);
        }
    });
    let weak = ui.as_weak();
    ui.on_fx_command(move |command, value| {
        let Some(ui) = weak.upgrade() else { return };
        if command.as_str() == "import" {
            return import(&ui, state.clone());
        }
        let mut s = state.borrow_mut();
        let result = match command.as_str() {
            "test" => test(&mut s, &value),
            "test_team_add" => test(&mut s, &format!("add:{value}")),
            "test_team_sub" => test(&mut s, &format!("sub:{value}")),
            "remove" => remove_sound(&mut s, &value),
            "preview" => preview(&mut s, &value).inspect(|_| keep_redrawing(&ui, &s)),
            _ => Err("Unknown effects command".into()),
        };
        report(&ui, result);
        refresh_now(&ui, &s);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use overlay_studio::model::Element;
    use std::sync::Arc;

    fn asset(id: u64, name: &str) -> SoundAsset {
        SoundAsset { id, name: name.into(), bytes: Arc::new(vec![1]) }
    }

    #[test]
    fn colors_are_validated_and_normalised() {
        assert_eq!(normalize_color(" #ff8000 "), Ok("#FF8000".into()));
        assert_eq!(normalize_color("00ff00"), Ok("#00FF00".into()));
        assert!(normalize_color("red").is_err());
        assert!(normalize_color("#12345").is_err());
    }
    #[test]
    fn seconds_and_volume_must_be_in_range() {
        assert_eq!(parse_seconds(" 1.5 "), Ok("1.5".into()));
        assert!(parse_seconds("0.2").is_err());
        assert!(parse_seconds("4.1").is_err());
        assert!(parse_seconds("x").is_err());
        assert_eq!(parse_volume("0.5"), Ok("0.50".into()));
        assert!(parse_volume("1.1").is_err());
        assert!(parse_volume("-0.1").is_err());
    }
    #[test]
    fn sound_choices_list_builtins_then_imports_and_round_trip() {
        let sounds = [asset(7, "Horn"), asset(9, "Clap")];
        let names = sound_names("None", &sounds);
        assert_eq!(names.len(), 1 + BUILTINS.len() + 2);
        assert_eq!((names[0].as_str(), names[1].as_str(), names[7].as_str()), ("None", "Ding", "Horn"));
        assert_eq!(sound_names("Use global", &[])[0], "Use global");
        for i in 0..names.len() {
            let value = sound_value(&i.to_string(), &sounds).unwrap();
            assert_eq!(sound_index(&value, &sounds), i as i32);
        }
        assert_eq!(sound_value("7", &sounds), Ok("asset:7".into()));
        assert_eq!(sound_value("0", &sounds), Ok(String::new()));
        assert!(sound_value("9", &sounds).is_err());
        assert!(sound_value("x", &sounds).is_err());
        assert_eq!(sound_index("asset:404", &sounds), 0);
        assert_eq!(sound_index("builtin:nope", &sounds), 0);
    }
    #[test]
    fn style_is_applied_only_to_layers_with_an_effect_key() {
        let mut keyed = Element::new(1, overlay_studio::model::Kind::Rectangle);
        keyed.fx_key = "team.1".into();
        let plain = Element::new(2, overlay_studio::model::Kind::Rectangle);
        let mut design = Design { id: 1, name: "D".into(), width: 10, height: 10, elements: vec![keyed, plain] };
        assert_eq!(design_style(&design), (FxStyle::None, 1));
        assert_eq!(apply_style(&mut design, FxStyle::Ring), 1);
        assert_eq!((design.elements[0].fx_style, design.elements[1].fx_style), (FxStyle::Ring, FxStyle::None));
        assert_eq!(design_style(&design), (FxStyle::Ring, 1));
    }
    #[test]
    fn imported_sound_ids_do_not_collide_with_image_ids() {
        let mut p = overlay_studio::model::Project::fresh();
        p.assets.push(overlay_studio::model::Asset { id: 5, name: "a".into(), bytes: std::sync::Arc::new(vec![]) });
        let id = p.next_id();
        assert!(p.assets.iter().all(|a| a.id != id));
        assert!(p.sounds.iter().all(|a| a.id != id));
    }
}
