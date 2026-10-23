//! Wheel settings and spin state, stored in `Project::values` so every renderer
//! (preview and each running output) sees live changes without re-taking a design.
use std::collections::BTreeMap;

pub const MAX_ENTRIES: usize = 64; // longer lists are cut when read, keeping painting fast
pub const ENTRIES: &str = "wheel.entries"; // current list, one entry per line
pub const ORIGINAL: &str = "wheel.original"; // list restored by RestoreWheel
pub const POINTER: &str = "wheel.pointer"; // arrow | pin | bar
pub const SLICE: &str = "wheel.slice"; // solid | grad | alt
pub const PALETTE: &str = "wheel.palette"; // vivid | neon | gold | pastel
pub const COLORS: &str = "wheel.colors"; // optional comma-separated #RRGGBB, overrides palette
pub const CONFETTI: &str = "wheel.confetti"; // 1 | 0
pub const SPIN_SECONDS: &str = "wheel.spin_seconds";
pub const SPIN_ID: &str = "wheel.spin_id"; // increments on every spin; renderers restart on change
pub const SPIN_TURNS: &str = "wheel.spin_turns"; // whole turns before landing (5..=8)
pub const SPIN_JITTER: &str = "wheel.spin_jitter"; // -0.35..=0.35 of a slice, off-centre landing
pub const WINNER_INDEX: &str = "wheel.winner_index"; // index into ENTRIES, empty before a spin
pub const WINNER: &str = "wheel.winner"; // winner text, empty before a spin or after removal

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Pointer {
    #[default]
    Arrow,
    Pin,
    Bar,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Slice {
    #[default]
    Solid,
    Gradient,
    TwoTone,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Palette {
    #[default]
    Vivid,
    Neon,
    Gold,
    Pastel,
}
impl Palette {
    pub fn colors(self) -> [[u8; 3]; 8] {
        let hex = match self {
            Self::Vivid => ["e8505b", "f5b942", "4cd6a0", "4aa8ff", "a778ff", "ff8a4c", "35c9d9", "ec6bb5"],
            Self::Neon => ["ff2e93", "00e5ff", "b6ff00", "ff9100", "7c4dff", "00e676", "ffea00", "ff5252"],
            Self::Gold => ["f5b942", "1c2a5e", "e0a22e", "26397d", "fbd27a", "162149", "c88a1c", "34498f"],
            Self::Pastel => ["f7a8b8", "ffd8a8", "b8e6c9", "a8d4f7", "d4b8f7", "fff3a8", "a8f0ea", "f7c8e8"],
        };
        hex.map(|h| parse_hex(h).expect("built-in palette is valid"))
    }
}

/// Everything a painter or command needs, parsed once from `values`.
#[derive(Clone, Debug, PartialEq)]
pub struct WheelConfig {
    pub entries: Vec<String>,
    pub pointer: Pointer,
    pub slice: Slice,
    pub palette: Palette,
    pub custom_colors: Vec<[u8; 3]>,
    pub confetti: bool,
    pub spin_seconds: f32,
    pub spin_id: u64,
    pub spin_turns: u32,
    pub spin_jitter: f32,
    pub winner_index: Option<usize>,
}
impl Default for WheelConfig {
    fn default() -> Self {
        Self {
            entries: vec![],
            pointer: Pointer::Arrow,
            slice: Slice::Solid,
            palette: Palette::Vivid,
            custom_colors: vec![],
            confetti: true,
            spin_seconds: 5.5,
            spin_id: 0,
            spin_turns: 6,
            spin_jitter: 0.,
            winner_index: None,
        }
    }
}
impl WheelConfig {
    pub fn read(values: &BTreeMap<String, String>) -> Self {
        let get = |k: &str| values.get(k).map(|v| v.trim()).unwrap_or("");
        let d = Self::default();
        Self {
            entries: lines(get_raw(values, ENTRIES)).into_iter().take(MAX_ENTRIES).collect(),
            pointer: match get(POINTER) {
                "pin" => Pointer::Pin,
                "bar" => Pointer::Bar,
                _ => Pointer::Arrow,
            },
            slice: match get(SLICE) {
                "grad" => Slice::Gradient,
                "alt" => Slice::TwoTone,
                _ => Slice::Solid,
            },
            palette: match get(PALETTE) {
                "neon" => Palette::Neon,
                "gold" => Palette::Gold,
                "pastel" => Palette::Pastel,
                _ => Palette::Vivid,
            },
            custom_colors: get(COLORS).split(',').filter_map(|h| parse_hex(h.trim())).collect(),
            confetti: get(CONFETTI) != "0",
            spin_seconds: get(SPIN_SECONDS).parse().ok().filter(|s: &f32| (0.5..=30.).contains(s)).unwrap_or(d.spin_seconds),
            spin_id: get(SPIN_ID).parse().unwrap_or(0),
            spin_turns: get(SPIN_TURNS).parse().ok().filter(|t| (1..=20).contains(t)).unwrap_or(d.spin_turns),
            spin_jitter: get(SPIN_JITTER).parse().ok().filter(|j: &f32| j.abs() <= 0.45).unwrap_or(0.),
            winner_index: get(WINNER_INDEX).parse().ok(),
        }
    }
    /// Colour for slice `i`, honouring custom colours first, then the palette and slice style.
    pub fn slice_color(&self, i: usize) -> [u8; 3] {
        let pool: Vec<[u8; 3]> = if self.custom_colors.is_empty() {
            self.palette.colors().to_vec()
        } else {
            self.custom_colors.clone()
        };
        match self.slice {
            Slice::TwoTone => pool[(i % 2) * 3 % pool.len()],
            _ => pool[i % pool.len()],
        }
    }
}
fn get_raw<'a>(values: &'a BTreeMap<String, String>, key: &str) -> &'a str {
    values.get(key).map(String::as_str).unwrap_or("")
}
pub fn lines(text: &str) -> Vec<String> {
    text.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect()
}
pub fn parse_hex(text: &str) -> Option<[u8; 3]> {
    let t = text.trim().trim_start_matches('#');
    (t.len() == 6).then(|| u32::from_str_radix(t, 16).ok()).flatten().map(|n| [(n >> 16) as u8, (n >> 8) as u8, n as u8])
}
pub fn hex(color: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2])
}
/// Value key that holds a team's colour, kept in sync with `Team::color`.
pub fn team_color_key(team_id: u64) -> String {
    format!("team.{team_id}.color")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_defaults_and_overrides() {
        let mut v = BTreeMap::new();
        assert_eq!(WheelConfig::read(&v), WheelConfig::default());
        v.insert(ENTRIES.into(), "A\n\n B \nC".into());
        v.insert(POINTER.into(), "pin".into());
        v.insert(SPIN_ID.into(), "3".into());
        v.insert(WINNER_INDEX.into(), "1".into());
        let c = WheelConfig::read(&v);
        assert_eq!(c.entries, ["A", "B", "C"]);
        assert_eq!(c.pointer, Pointer::Pin);
        assert_eq!((c.spin_id, c.winner_index), (3, Some(1)));
    }
    #[test]
    fn hex_round_trips_and_rejects_bad_input() {
        assert_eq!(parse_hex("#FF8000"), Some([255, 128, 0]));
        assert_eq!(hex([255, 128, 0]), "#FF8000");
        assert_eq!(parse_hex("zzz"), None);
    }
}
