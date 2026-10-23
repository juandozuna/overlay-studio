//! Wheel panel wiring for the Slint window (WN-5).
use crate::{Desktop, StudioWindow, WheelView, refresh, report};
use overlay_studio::domain::{
    controls::Action,
    wheel::{self, WheelConfig},
};
use slint::ComponentHandle;
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

#[derive(Default)]
pub struct Session {
    pub last_spin: Option<Instant>,
    shown: RefCell<(String, String)>,
}

const DEFAULTS: [(&str, &str); 11] = [
    (wheel::ENTRIES, ""),
    (wheel::ORIGINAL, ""),
    (wheel::POINTER, "arrow"),
    (wheel::SLICE, "solid"),
    (wheel::PALETTE, "vivid"),
    (wheel::COLORS, ""),
    (wheel::CONFETTI, "1"),
    (wheel::SPIN_SECONDS, "5.5"),
    (wheel::WINNER, ""),
    (wheel::WINNER_INDEX, ""),
    (wheel::SPIN_ID, "0"),
];
const POINTERS: [&str; 3] = ["arrow", "pin", "bar"];
const SLICES: [&str; 3] = ["solid", "grad", "alt"];
const PALETTES: [&str; 4] = ["vivid", "neon", "gold", "pastel"];

pub fn normalize_entries(text: &str) -> String {
    wheel::lines(text).join("\n")
}
/// The winner is hidden until the wheel has had `spin_seconds` to land.
pub fn revealed(since_spin: Option<Duration>, spin_seconds: f32) -> bool {
    since_spin.is_none_or(|d| d.as_secs_f32() >= spin_seconds)
}
/// Normalised comma-separated `#RRGGBB` list; empty text means "use the palette".
pub fn normalize_colors(text: &str) -> Result<String, String> {
    let parts: Vec<&str> = text.split(',').map(str::trim).filter(|p| !p.is_empty()).collect();
    parts
        .iter()
        .map(|p| wheel::parse_hex(p).map(wheel::hex).ok_or(format!("\"{p}\" is not a #RRGGBB color")))
        .collect::<Result<Vec<_>, _>>()
        .map(|c| c.join(","))
}
pub fn parse_seconds(text: &str) -> Result<String, String> {
    text.trim()
        .parse::<f32>()
        .ok()
        .filter(|s| (0.5..=30.).contains(s))
        .map(|s| s.to_string())
        .ok_or("Spin time must be 0.5 to 30 seconds".into())
}

fn ensure_defaults(s: &mut Desktop) {
    let values = &mut s.document.project.values;
    let before = values.len();
    for (key, value) in DEFAULTS {
        values.entry(key.into()).or_insert_with(|| value.into());
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
/// Replaces the list and the restore point, and clears the winner.
fn set_entries(s: &mut Desktop, text: &str) -> Result<(), String> {
    let text = normalize_entries(text);
    for (key, value) in [
        (wheel::ENTRIES, text.as_str()),
        (wheel::ORIGINAL, text.as_str()),
        (wheel::WINNER, ""),
        (wheel::WINNER_INDEX, ""),
    ] {
        set(s, key, value)?;
    }
    s.wheel.last_spin = None;
    Ok(())
}
fn choice(list: &[&str], value: &str) -> Result<String, String> {
    value
        .parse::<usize>()
        .ok()
        .and_then(|i| list.get(i))
        .map(|v| v.to_string())
        .ok_or("Unknown wheel option".into())
}
fn edit(s: &mut Desktop, key: &str, value: &str) -> Result<(), String> {
    match key {
        "entries" => set_entries(s, value),
        "pointer" => set(s, wheel::POINTER, &choice(&POINTERS, value)?),
        "slice" => set(s, wheel::SLICE, &choice(&SLICES, value)?),
        "palette" => set(s, wheel::PALETTE, &choice(&PALETTES, value)?),
        "confetti" => set(s, wheel::CONFETTI, if value == "1" { "1" } else { "0" }),
        "spin_seconds" => set(s, wheel::SPIN_SECONDS, &parse_seconds(value)?),
        "colors" => set(s, wheel::COLORS, &normalize_colors(value)?),
        _ => Err("Unknown wheel property".into()),
    }
}
fn spinning(s: &Desktop) -> bool {
    let cfg = WheelConfig::read(&s.document.project.values);
    !revealed(s.wheel.last_spin.map(|t| t.elapsed()), cfg.spin_seconds)
}
fn command(s: &mut Desktop, command: &str) -> Result<(), String> {
    ensure_defaults(s);
    match command {
        "spin" => {
            if spinning(s) {
                return Err("The wheel is still spinning".into());
            }
            if WheelConfig::read(&s.document.project.values).entries.is_empty() {
                return Err("Add wheel entries first".into());
            }
            s.execute(Action::SpinWheel)?;
            s.wheel.last_spin = Some(Instant::now());
            Ok(())
        }
        "remove" => s.execute(Action::RemoveWheelWinner),
        "restore" => {
            s.wheel.last_spin = None;
            s.execute(Action::RestoreWheel)
        }
        _ => Err("Unknown wheel command".into()),
    }
}

pub fn refresh_panel(ui: &StudioWindow, s: &Desktop) {
    let v = &s.document.project.values;
    let get = |k: &str| v.get(k).map(String::as_str).unwrap_or("");
    let cfg = WheelConfig::read(v);
    let spinning = spinning(s);
    let winner = get(wheel::WINNER).to_string();
    let index = |list: &[&str], key: &str| list.iter().position(|o| *o == get(key)).unwrap_or(0) as i32;
    let old = ui.get_wheel_view();
    let entries = get(wheel::ENTRIES);
    if normalize_entries(&ui.get_wheel_entries()) != entries {
        ui.set_wheel_entries(entries.into());
    }
    let stored = (cfg.spin_seconds.to_string(), get(wheel::COLORS).to_string());
    let mut shown = s.wheel.shown.borrow_mut();
    let (seconds, colors) = if *shown != stored {
        *shown = stored.clone();
        stored
    } else {
        (old.spin_seconds.to_string(), old.colors.to_string())
    };
    ui.set_wheel_view(WheelView {
        pointer: index(&POINTERS, wheel::POINTER),
        slice: index(&SLICES, wheel::SLICE),
        palette: index(&PALETTES, wheel::PALETTE),
        confetti: cfg.confetti,
        spin_seconds: seconds.into(),
        colors: colors.into(),
        can_remove: !spinning && !winner.is_empty(),
        winner: winner.into(),
        count: cfg.entries.len() as i32,
        spinning,
    });
}

pub fn connect(ui: &StudioWindow, state: Rc<RefCell<Desktop>>) {
    let shared = state.clone();
    let weak = ui.as_weak();
    ui.on_wheel_edit(move |key, value| {
        if let Some(ui) = weak.upgrade() {
            let mut s = shared.borrow_mut();
            let result = edit(&mut s, &key, &value);
            report(&ui, result);
            if key.as_str() == "entries" {
                refresh_panel(&ui, &s);
            } else {
                refresh(&ui, &s);
            }
            ui.window().request_redraw();
        }
    });
    let weak = ui.as_weak();
    ui.on_wheel_command(move |value| {
        if let Some(ui) = weak.upgrade() {
            let mut s = state.borrow_mut();
            let result = command(&mut s, &value);
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
    fn entries_are_trimmed_and_blank_lines_dropped() {
        assert_eq!(normalize_entries(" A \n\n B\r\n\nC \n"), "A\nB\nC");
        assert_eq!(normalize_entries("\n \n"), "");
    }
    #[test]
    fn winner_is_revealed_only_after_the_spin_time() {
        assert!(revealed(None, 5.5));
        assert!(!revealed(Some(Duration::from_secs(2)), 5.5));
        assert!(revealed(Some(Duration::from_millis(5500)), 5.5));
    }
    #[test]
    fn custom_colors_are_validated_and_normalised() {
        assert_eq!(normalize_colors(""), Ok(String::new()));
        assert_eq!(normalize_colors(" #ff8000 , 00ff00,"), Ok("#FF8000,#00FF00".into()));
        assert!(normalize_colors("#ff8000, red").is_err());
        assert!(normalize_colors("#12345").is_err());
    }
    #[test]
    fn spin_seconds_must_be_in_range() {
        assert_eq!(parse_seconds(" 4 "), Ok("4".into()));
        assert!(parse_seconds("0.1").is_err());
        assert!(parse_seconds("abc").is_err());
    }
    #[test]
    fn editing_entries_sets_the_restore_point_and_options_map_to_values() {
        let mut s = Desktop::new();
        edit(&mut s, "entries", " A\n\nB ").unwrap();
        edit(&mut s, "pointer", "2").unwrap();
        edit(&mut s, "palette", "3").unwrap();
        edit(&mut s, "confetti", "0").unwrap();
        let v = &s.document.project.values;
        assert_eq!((v[wheel::ENTRIES].as_str(), v[wheel::ORIGINAL].as_str()), ("A\nB", "A\nB"));
        assert_eq!((v[wheel::POINTER].as_str(), v[wheel::PALETTE].as_str(), v[wheel::CONFETTI].as_str()), ("bar", "pastel", "0"));
        assert!(edit(&mut s, "pointer", "9").is_err());
    }
}
