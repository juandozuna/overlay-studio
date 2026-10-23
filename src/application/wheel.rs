//! Wheel commands over `domain::wheel` values (WN-2).
use super::game_commands::CommandError;
use crate::domain::wheel::{self as keys, WheelConfig, lines};
use std::{
    collections::BTreeMap,
    hash::{BuildHasher, Hasher, RandomState},
    time::{SystemTime, UNIX_EPOCH},
};

const SAMPLE: &str = "Trivia round\nCharades\nKaraoke\nPictionary\nSpeed quiz\nMystery box\nBonus points\nLightning round";

/// Inserts any missing wheel keys with defaults, leaving existing values untouched.
pub fn seed_defaults(values: &mut BTreeMap<String, String>) {
    for (key, value) in [
        (keys::ENTRIES, SAMPLE),
        (keys::ORIGINAL, SAMPLE),
        (keys::POINTER, "arrow"),
        (keys::SLICE, "solid"),
        (keys::PALETTE, "vivid"),
        (keys::CONFETTI, "1"),
        (keys::SPIN_SECONDS, "5.5"),
        (keys::SPIN_ID, "0"),
    ] {
        values.entry(key.into()).or_insert_with(|| value.into());
    }
}

/// Live list edit: replaces both the current list and the list Restore returns to.
/// Clearing the list keeps the previous restore point so Restore can bring it back.
pub fn set_entries(values: &mut BTreeMap<String, String>, text: &str) {
    values.insert(keys::ENTRIES.into(), text.into());
    if !lines(text).is_empty() {
        values.insert(keys::ORIGINAL.into(), text.into());
    }
    clear_winner(values);
}

pub fn spin(values: &mut BTreeMap<String, String>) -> Result<(), CommandError> {
    let config = WheelConfig::read(values);
    if config.entries.is_empty() {
        return Err(CommandError::WheelEmpty);
    }
    let spin_id = config.spin_id + 1;
    let mut rng = Rng::new(spin_id);
    let index = (rng.next() % config.entries.len() as u64) as usize;
    let turns = 5 + (rng.next() % 4) as u32;
    let jitter = (rng.unit() * 2. - 1.) * 0.35;
    values.insert(keys::SPIN_ID.into(), spin_id.to_string());
    values.insert(keys::SPIN_TURNS.into(), turns.to_string());
    values.insert(keys::SPIN_JITTER.into(), format!("{jitter:.3}"));
    values.insert(keys::WINNER_INDEX.into(), index.to_string());
    values.insert(keys::WINNER.into(), String::new()); // hidden until the wheel lands
    Ok(())
}

/// Publishes the winner text once the wheel has landed, so bound text and phones do not spoil it.
pub fn reveal_winner(values: &mut BTreeMap<String, String>) -> bool {
    let config = WheelConfig::read(values);
    let Some(entry) = config.winner_index.and_then(|i| config.entries.get(i)) else {
        return false;
    };
    values.insert(keys::WINNER.into(), entry.clone());
    true
}

pub fn remove_winner(values: &mut BTreeMap<String, String>) -> Result<(), CommandError> {
    let config = WheelConfig::read(values);
    let mut entries = config.entries;
    let shown = values.get(keys::WINNER).map(String::as_str).unwrap_or("");
    let index = config
        .winner_index
        .filter(|i| entries.get(*i).is_some_and(|e| !shown.is_empty() && e == shown))
        .ok_or(CommandError::WheelNoWinner)?;
    entries.remove(index);
    values.insert(keys::ENTRIES.into(), entries.join("\n"));
    clear_winner(values);
    Ok(())
}

pub fn restore(values: &mut BTreeMap<String, String>) -> Result<(), CommandError> {
    let original = values.get(keys::ORIGINAL).map(String::as_str).unwrap_or("");
    if lines(original).is_empty() {
        return Err(CommandError::WheelNoOriginal);
    }
    let original = original.to_string();
    values.insert(keys::ENTRIES.into(), original);
    clear_winner(values);
    Ok(())
}

fn clear_winner(values: &mut BTreeMap<String, String>) {
    values.insert(keys::WINNER_INDEX.into(), String::new());
    values.insert(keys::WINNER.into(), String::new());
}

/// Small splitmix64 stream seeded from the clock, a per-process random key and the spin id.
struct Rng(u64);
impl Rng {
    fn new(spin_id: u64) -> Self {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u64(nanos);
        hasher.write_u64(spin_id);
        Self(hasher.finish())
    }
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn seeded() -> BTreeMap<String, String> {
        let mut v = BTreeMap::new();
        seed_defaults(&mut v);
        v
    }
    #[test]
    fn spin_picks_valid_winner_and_increments_id() {
        let mut v = seeded();
        for expected in 1..=20u64 {
            spin(&mut v).unwrap();
            let c = WheelConfig::read(&v);
            assert_eq!(c.spin_id, expected);
            let i = c.winner_index.unwrap();
            assert!(i < c.entries.len());
            assert_eq!(v[keys::WINNER], "");
            assert!(reveal_winner(&mut v));
            assert_eq!(v[keys::WINNER], c.entries[i]);
            assert!((5..=8).contains(&c.spin_turns));
            assert!(c.spin_jitter.abs() <= 0.35);
        }
    }
    #[test]
    fn spin_on_empty_list_errors_and_changes_nothing() {
        let mut v = BTreeMap::new();
        let before = v.clone();
        assert_eq!(spin(&mut v), Err(CommandError::WheelEmpty));
        assert_eq!(v, before);
    }
    #[test]
    fn remove_shrinks_list_and_clears_winner_but_keeps_original() {
        let mut v = seeded();
        spin(&mut v).unwrap();
        let winner = v[keys::WINNER].clone();
        reveal_winner(&mut v);
        remove_winner(&mut v).unwrap();
        let c = WheelConfig::read(&v);
        assert_eq!(c.entries.len(), 7);
        assert!(!c.entries.contains(&winner));
        assert_eq!(c.winner_index, None);
        assert_eq!(v[keys::WINNER], "");
        assert_eq!(lines(&v[keys::ORIGINAL]).len(), 8);
        assert_eq!(remove_winner(&mut v), Err(CommandError::WheelNoWinner));
    }
    #[test]
    fn restore_returns_original_and_clears_winner() {
        let mut v = seeded();
        spin(&mut v).unwrap();
        reveal_winner(&mut v);
        remove_winner(&mut v).unwrap();
        spin(&mut v).unwrap();
        restore(&mut v).unwrap();
        assert_eq!(v[keys::ENTRIES], v[keys::ORIGINAL]);
        assert_eq!(WheelConfig::read(&v).winner_index, None);
        assert_eq!(v[keys::WINNER], "");
    }
    #[test]
    fn restore_without_original_errors() {
        let mut v = BTreeMap::new();
        assert_eq!(restore(&mut v), Err(CommandError::WheelNoOriginal));
    }
    #[test]
    fn set_entries_replaces_original_so_restore_returns_edit() {
        let mut v = seeded();
        set_entries(&mut v, "A\nB\nC");
        spin(&mut v).unwrap();
        reveal_winner(&mut v);
        remove_winner(&mut v).unwrap();
        assert_eq!(WheelConfig::read(&v).entries.len(), 2);
        restore(&mut v).unwrap();
        assert_eq!(WheelConfig::read(&v).entries, ["A", "B", "C"]);
    }
    #[test]
    fn seed_defaults_keeps_existing_values() {
        let mut v = BTreeMap::new();
        v.insert(keys::POINTER.into(), "bar".into());
        seed_defaults(&mut v);
        assert_eq!(v[keys::POINTER], "bar");
        assert_eq!(WheelConfig::read(&v).entries.len(), 8);
    }
}
