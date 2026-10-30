//! El Dado (Olimpiadas Bíblicas): settings, turn state and question state, stored in
//! `Project::values` so every renderer (preview and each running output) sees live changes.
//! Shared contract between the dice logic, the cube painter and the Dado panel.
use std::collections::BTreeMap;

// ---- dice ----
pub const ROLL_ID: &str = "dice.roll_id"; // increments on every roll; renderers restart on change
pub const ROLL_SECONDS: &str = "dice.roll_seconds"; // 0.5..=10, default 2.0
pub const FACE: &str = "dice.face"; // 0..=5 face chosen at roll time; painter lands on it, empty before the first roll
pub const RESULT: &str = "dice.result"; // face name (ROJO...), published only after the roll lands, empty otherwise
pub const RULE: &str = "dice.rule"; // rule text of the landed face, empty before landing
pub const STATUS: &str = "dice.status"; // "" or STATUS_ANNULLED
pub const DICE_COLOR: &str = "dice.color"; // #RRGGBB of the landed face, published with RESULT, empty otherwise
pub const COLORS: &str = "dice.colors"; // optional comma-separated #RRGGBB x6, overrides default face colours
pub const RULES_PREFIX: &str = "dice.rules."; // dice.rules.0 .. dice.rules.5, editable Spanish rule text
pub const STATUS_ANNULLED: &str = "PREGUNTA ANULADA";

// ---- question shown to the audience ----
pub const QUESTION_TEXT: &str = "question.text";
pub const OPTION_KEYS: [&str; 4] = ["question.a", "question.b", "question.c", "question.d"]; // filled ONLY on yellow

// ---- host-only: never bind these to a canvas layer ----
pub const HOST_ANSWER: &str = "dado.host.answer";
pub const HOST_REF: &str = "dado.host.ref";
pub const REVEALED: &str = "dado.revealed"; // "1" once the host revealed the answer (host panel only)
pub const AWARDED: &str = "dado.awarded"; // "1" once points were awarded this turn, cleared by roll / next participant / import

// ---- turns ----
pub const TURN_TEAM: &str = "dado.turn.team"; // id of the team that is up; empty or "0" = nobody yet
pub const TURN_NAME: &str = "dado.turn.name"; // current participant, bindable ("TURNO")
pub const TURN_TEAM_NAME: &str = "dado.turn.team_name";
pub fn participants_key(team_id: u64) -> String {
    format!("dado.team.{team_id}.participants") // one name per line
}
pub fn turn_index_key(team_id: u64) -> String {
    format!("dado.team.{team_id}.turn") // index of the participant who is up
}

// ---- question bank ----
pub const BANK: &str = "dado.bank"; // imported CSV text, UTF-8
pub const BANK_USED: &str = "dado.bank.used"; // comma-separated row numbers already drawn
pub const BANK_FILTER: &str = "dado.bank.filter"; // "" = all, otherwise a book name
pub const BANK_CURRENT: &str = "dado.bank.current"; // row number of the question on screen, empty if none
pub const BANKS_BOOKS: [&str; 4] = ["1 Samuel", "2 Samuel", "1 Reyes", "2 Reyes"];

/// The question clock is a normal countdown `ClockSpec` whose `value_key` is this and whose reset is 30 s.
pub const QUESTION_CLOCK_KEY: &str = "dado.clock";
pub const QUESTION_SECONDS: f64 = 30.;
pub const POINTS: i64 = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    Rojo,
    Amarillo,
    Azul,
    Naranja,
    Morado,
    Verde,
}
impl Face {
    pub const ALL: [Face; 6] = [Face::Rojo, Face::Amarillo, Face::Azul, Face::Naranja, Face::Morado, Face::Verde];
    pub fn from_index(i: usize) -> Option<Face> {
        Self::ALL.get(i).copied()
    }
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|f| *f == self).unwrap_or(0)
    }
    pub fn name(self) -> &'static str {
        ["ROJO", "AMARILLO", "AZUL", "NARANJA", "MORADO", "VERDE"][self.index()]
    }
    /// 3-letter code printed on the face, so colour is never the only cue.
    pub fn code(self) -> &'static str {
        ["ROJ", "AMA", "AZU", "NAR", "MOR", "VER"][self.index()]
    }
    pub fn default_color(self) -> [u8; 3] {
        [[255, 77, 94], [255, 233, 77], [77, 181, 255], [255, 159, 61], [185, 139, 255], [98, 240, 160]][self.index()]
    }
    pub fn default_rule(self) -> &'static str {
        [
            "ROJO – Respuesta directa",
            "AMARILLO – 4 opciones",
            "AZUL – Alguien del público",
            "NARANJA – Alguien de tu equipo",
            "MORADO – Con la Biblia",
            "VERDE – Pregunta extra",
        ][self.index()]
    }
    /// Only yellow shows the A-D options on the audience screen.
    pub fn shows_options(self) -> bool {
        self == Face::Amarillo
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DiceConfig {
    pub roll_id: u64,
    pub roll_seconds: f32,
    pub face: Option<Face>,
    pub colors: [[u8; 3]; 6],
    pub rules: [String; 6],
    pub result: String,
    pub rule: String,
    pub status: String,
}
impl Default for DiceConfig {
    fn default() -> Self {
        Self {
            roll_id: 0,
            roll_seconds: 2.0,
            face: None,
            colors: Face::ALL.map(Face::default_color),
            rules: Face::ALL.map(|f| f.default_rule().to_string()),
            result: String::new(),
            rule: String::new(),
            status: String::new(),
        }
    }
}
impl DiceConfig {
    pub fn read(values: &BTreeMap<String, String>) -> Self {
        let get = |k: &str| values.get(k).map(|v| v.trim()).unwrap_or("");
        let d = Self::default();
        let custom: Vec<&str> = get(COLORS).split(',').collect();
        let colors = std::array::from_fn(|i| custom.get(i).and_then(|h| super::wheel::parse_hex(h.trim())).unwrap_or(d.colors[i]));
        let rules = std::array::from_fn(|i| {
            let text = get(&format!("{RULES_PREFIX}{i}"));
            if text.is_empty() { d.rules[i].clone() } else { text.to_string() }
        });
        Self {
            roll_id: get(ROLL_ID).parse().unwrap_or(0),
            roll_seconds: get(ROLL_SECONDS).parse().ok().filter(|s: &f32| (0.5..=10.).contains(s)).unwrap_or(d.roll_seconds),
            face: get(FACE).parse().ok().and_then(Face::from_index),
            colors,
            rules,
            result: get(RESULT).to_string(),
            rule: get(RULE).to_string(),
            status: get(STATUS).to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_and_overrides() {
        let mut v = BTreeMap::new();
        assert_eq!(DiceConfig::read(&v), DiceConfig::default());
        v.insert(FACE.into(), "1".into());
        v.insert(ROLL_ID.into(), "4".into());
        v.insert(format!("{RULES_PREFIX}1"), "Amarillo – cuatro".into());
        let c = DiceConfig::read(&v);
        assert_eq!((c.roll_id, c.face), (4, Some(Face::Amarillo)));
        assert_eq!(c.rules[1], "Amarillo – cuatro");
        assert_eq!(c.rules[0], Face::Rojo.default_rule());
    }
    #[test]
    fn a_bad_colour_only_replaces_its_own_slot() {
        let mut v = BTreeMap::new();
        v.insert(COLORS.into(), "#010203, nope ,#0A0B0C".into());
        let c = DiceConfig::read(&v).colors;
        let d = DiceConfig::default().colors;
        assert_eq!((c[0], c[1], c[2]), ([1, 2, 3], d[1], [10, 11, 12]));
        assert_eq!(&c[3..], &d[3..]);
    }
    #[test]
    fn only_yellow_shows_options() {
        assert!(Face::ALL.iter().filter(|f| f.shows_options()).count() == 1 && Face::Amarillo.shows_options());
    }
}
