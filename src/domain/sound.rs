//! Sound choices, stored in `Project::values`. A reference is "" (none), "builtin:<name>"
//! or "asset:<id>" for an imported file in `Project::sounds`.
use super::fx::Dir;
use std::collections::BTreeMap;

pub const VOLUME: &str = "sound.volume"; // 0..=1, default 0.6
pub const ADD: &str = "sound.add"; // default for point added
pub const SUB: &str = "sound.sub"; // default for point removed
pub const WHEEL_SPIN: &str = "sound.wheel_spin";
pub const WHEEL_LAND: &str = "sound.wheel_land";
pub const WHEEL_REMOVE: &str = "sound.wheel_remove";
/// Per-team override keys: `team.<id>.sound_add` and `team.<id>.sound_sub`.
pub fn team_key(team_id: u64, dir: Dir) -> String {
    match dir {
        Dir::Add => format!("team.{team_id}.sound_add"),
        Dir::Sub => format!("team.{team_id}.sound_sub"),
    }
}

pub const BUILTINS: [&str; 6] = ["ding", "chime", "pop", "buzz", "thud", "whoosh"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SoundRef {
    Builtin(String),
    Asset(u64),
}
impl SoundRef {
    pub fn parse(text: &str) -> Option<Self> {
        let t = text.trim();
        if let Some(name) = t.strip_prefix("builtin:") {
            return BUILTINS.contains(&name).then(|| Self::Builtin(name.into()));
        }
        t.strip_prefix("asset:").and_then(|id| id.parse().ok()).map(Self::Asset)
    }
    pub fn to_text(&self) -> String {
        match self {
            Self::Builtin(name) => format!("builtin:{name}"),
            Self::Asset(id) => format!("asset:{id}"),
        }
    }
}

/// A sound the desktop app should play, with the volume already read from the project.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundRequest {
    pub sound: SoundRef,
    pub volume: f32,
}

pub fn volume(values: &BTreeMap<String, String>) -> f32 {
    values.get(VOLUME).and_then(|v| v.trim().parse().ok()).filter(|v: &f32| (0.0..=1.0).contains(v)).unwrap_or(0.6)
}

/// Sound for a score change: the team's own choice wins, then the global one.
pub fn for_score(values: &BTreeMap<String, String>, team_id: u64, dir: Dir) -> Option<SoundRequest> {
    let pick = |key: &str| values.get(key).and_then(|v| SoundRef::parse(v));
    let global = if dir == Dir::Add { ADD } else { SUB };
    pick(&team_key(team_id, dir))
        .or_else(|| pick(global))
        .map(|sound| SoundRequest { sound, volume: volume(values) })
}
pub fn for_key(values: &BTreeMap<String, String>, key: &str) -> Option<SoundRequest> {
    values
        .get(key)
        .and_then(|v| SoundRef::parse(v))
        .map(|sound| SoundRequest { sound, volume: volume(values) })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_refs_and_prefers_team_choice() {
        assert_eq!(SoundRef::parse("builtin:ding"), Some(SoundRef::Builtin("ding".into())));
        assert_eq!(SoundRef::parse("asset:7"), Some(SoundRef::Asset(7)));
        assert_eq!(SoundRef::parse("builtin:nope"), None);
        let mut v = BTreeMap::new();
        v.insert(ADD.into(), "builtin:ding".into());
        v.insert(team_key(2, Dir::Add), "asset:9".into());
        assert_eq!(for_score(&v, 1, Dir::Add).unwrap().sound, SoundRef::Builtin("ding".into()));
        assert_eq!(for_score(&v, 2, Dir::Add).unwrap().sound, SoundRef::Asset(9));
        assert!(for_score(&v, 1, Dir::Sub).is_none());
    }
}
