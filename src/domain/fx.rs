//! Score-change effects, stored in `Project::values` so every renderer sees them live.
//! A team's effect restarts whenever `team.<id>.fx_id` changes; `fx_dir` says which way.
use super::wheel::parse_hex;
use std::collections::BTreeMap;

pub const ADD_COLOR: &str = "fx.add_color"; // #RRGGBB, default green
pub const SUB_COLOR: &str = "fx.sub_color"; // #RRGGBB, default black
pub const SECONDS: &str = "fx.seconds"; // 0.3..=4, default 1.2
/// How strongly the subtract colour covers the tile at its peak (keeps digits readable).
pub const SUB_STRENGTH: f32 = 0.85;

pub fn id_key(prefix: &str) -> String {
    format!("{prefix}.fx_id")
}
pub fn dir_key(prefix: &str) -> String {
    format!("{prefix}.fx_dir")
}
/// Value-key prefix for a team's effect, for example `team.3`.
pub fn team_prefix(team_id: u64) -> String {
    format!("team.{team_id}")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum FxStyle {
    #[default]
    None,
    /// Tint flash with a number pop.
    Flash,
    /// Light bar sweeping across the tile.
    Sweep,
    /// Color wipes in from the left and out to the right.
    Wipe,
    /// Horizontal shake with a tint flash.
    Shake,
    /// Expanding ring from the centre with a number pop.
    Ring,
    /// Sparkle dots burst from the centre with a tint flash.
    Burst,
}
impl FxStyle {
    pub const ALL: [FxStyle; 7] = [
        Self::None,
        Self::Flash,
        Self::Sweep,
        Self::Wipe,
        Self::Shake,
        Self::Ring,
        Self::Burst,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Flash => "Flash and pop",
            Self::Sweep => "Light sweep",
            Self::Wipe => "Color wipe",
            Self::Shake => "Shake",
            Self::Ring => "Shockwave ring",
            Self::Burst => "Sparkle burst",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Add,
    Sub,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxConfig {
    pub add_color: [u8; 3],
    pub sub_color: [u8; 3],
    pub seconds: f32,
}
impl Default for FxConfig {
    fn default() -> Self {
        Self { add_color: [0x22, 0xC5, 0x5E], sub_color: [0, 0, 0], seconds: 1.2 }
    }
}
impl FxConfig {
    pub fn read(values: &BTreeMap<String, String>) -> Self {
        let d = Self::default();
        let get = |k: &str| values.get(k).map(|v| v.trim()).unwrap_or("");
        Self {
            add_color: parse_hex(get(ADD_COLOR)).unwrap_or(d.add_color),
            sub_color: parse_hex(get(SUB_COLOR)).unwrap_or(d.sub_color),
            seconds: get(SECONDS).parse().ok().filter(|s: &f32| (0.3..=4.).contains(s)).unwrap_or(d.seconds),
        }
    }
}

/// The latest effect request for one value-key prefix. `id == 0` means none yet.
pub fn read_state(values: &BTreeMap<String, String>, prefix: &str) -> (u64, Dir) {
    let id = values.get(&id_key(prefix)).and_then(|v| v.trim().parse().ok()).unwrap_or(0);
    let dir = match values.get(&dir_key(prefix)).map(|v| v.trim()) {
        Some("sub") => Dir::Sub,
        _ => Dir::Add,
    };
    (id, dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn config_and_state_fall_back_to_defaults() {
        let mut v = BTreeMap::new();
        assert_eq!(FxConfig::read(&v), FxConfig::default());
        assert_eq!(read_state(&v, "team.1"), (0, Dir::Add));
        v.insert(ADD_COLOR.into(), "#112233".into());
        v.insert(SECONDS.into(), "9".into());
        v.insert(id_key("team.1"), "4".into());
        v.insert(dir_key("team.1"), "sub".into());
        let c = FxConfig::read(&v);
        assert_eq!((c.add_color, c.seconds), ([0x11, 0x22, 0x33], 1.2));
        assert_eq!(read_state(&v, "team.1"), (4, Dir::Sub));
    }
}
