//! Triggers effects and sounds from score changes (FX-2).
use super::game_commands::CommandOutcome;
use crate::{
    domain::{
        fx::{Dir, dir_key, id_key, team_prefix},
        sound,
    },
    model::Project,
};
use std::time::{Duration, Instant};

/// Records an effect request and queues the sound when `field` is a team's score and it moved.
pub fn score_changed(project: &mut Project, field: &str, step: i64, outcome: &mut CommandOutcome) {
    if step == 0 {
        return;
    }
    let Some(team_id) = project.teams.iter().find(|t| t.score_key == field).map(|t| t.id) else {
        return;
    };
    let dir = if step > 0 { Dir::Add } else { Dir::Sub };
    let prefix = team_prefix(team_id);
    let key = id_key(&prefix);
    let next = project.values.get(&key).and_then(|v| v.trim().parse::<u64>().ok()).unwrap_or(0) + 1;
    project.values.insert(key, next.to_string());
    project.values.insert(dir_key(&prefix), if dir == Dir::Add { "add" } else { "sub" }.into());
    outcome.sounds.extend(sound::for_score(&project.values, team_id, dir));
    outcome.preview_requested = true;
}

/// Tracks whether an effect is still playing, so callers can keep rendering until it ends.
#[derive(Default)]
pub struct FxBusy(Option<Instant>);
impl FxBusy {
    pub fn note(&mut self, seconds: f32) {
        let end = Instant::now() + Duration::from_secs_f32(seconds.max(0.));
        if self.0.is_none_or(|d| end > d) {
            self.0 = Some(end);
        }
    }
    pub fn busy(&self) -> bool {
        self.0.is_some_and(|d| Instant::now() < d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::sound::SoundRef;

    fn change(p: &mut Project, field: &str, step: i64) -> CommandOutcome {
        let mut o = CommandOutcome::default();
        score_changed(p, field, step, &mut o);
        o
    }
    fn first_team(p: &Project) -> (u64, String) {
        (p.teams[0].id, p.teams[0].score_key.clone())
    }

    #[test]
    fn add_and_subtract_bump_counter_and_set_direction() {
        let mut p = Project::fresh();
        let (id, key) = first_team(&p);
        let prefix = team_prefix(id);
        let o = change(&mut p, &key, 1);
        assert!(o.preview_requested);
        assert_eq!(p.values[&id_key(&prefix)], "1");
        assert_eq!(p.values[&dir_key(&prefix)], "add");
        change(&mut p, &key, -2);
        assert_eq!(p.values[&id_key(&prefix)], "2");
        assert_eq!(p.values[&dir_key(&prefix)], "sub");
        change(&mut p, &key, 0);
        assert_eq!(p.values[&id_key(&prefix)], "2");
    }

    #[test]
    fn sound_is_queued_only_when_configured() {
        let mut p = Project::fresh();
        let (id, key) = first_team(&p);
        assert!(change(&mut p, &key, 1).sounds.is_empty());
        p.values.insert(sound::ADD.into(), "builtin:ding".into());
        let o = change(&mut p, &key, 1);
        assert_eq!(o.sounds.len(), 1);
        assert_eq!(o.sounds[0].sound, SoundRef::Builtin("ding".into()));
        assert!(change(&mut p, &key, -1).sounds.is_empty());
        p.values.insert(sound::team_key(id, Dir::Sub), "builtin:buzz".into());
        assert_eq!(change(&mut p, &key, -1).sounds[0].sound, SoundRef::Builtin("buzz".into()));
    }

    #[test]
    fn team_override_beats_global() {
        let mut p = Project::fresh();
        let (id, key) = first_team(&p);
        p.values.insert(sound::ADD.into(), "builtin:ding".into());
        p.values.insert(sound::team_key(id, Dir::Add), "builtin:pop".into());
        assert_eq!(change(&mut p, &key, 3).sounds[0].sound, SoundRef::Builtin("pop".into()));
    }

    #[test]
    fn legacy_default_project_keys_work() {
        let mut p = Project::fresh();
        assert!(p.teams.iter().any(|t| t.score_key == "home.score"));
        for field in ["home.score", "away.score"] {
            change(&mut p, field, 1);
        }
        let ids: Vec<_> = p.teams.iter().map(|t| p.values[&id_key(&team_prefix(t.id))].clone()).collect();
        assert!(ids.iter().all(|v| v == "1"));
    }

    #[test]
    fn non_team_fields_do_nothing() {
        let mut p = Project::fresh();
        p.values.insert("custom.count".into(), "4".into());
        let before = p.values.clone();
        let o = change(&mut p, "custom.count", 1);
        assert_eq!(p.values, before);
        assert!(!o.preview_requested && o.sounds.is_empty());
    }

    #[test]
    fn busy_follows_deadline() {
        let mut b = FxBusy::default();
        assert!(!b.busy());
        b.note(5.);
        assert!(b.busy());
        b.note(0.);
        assert!(b.busy());
        let mut c = FxBusy::default();
        c.note(0.);
        std::thread::sleep(Duration::from_millis(5));
        assert!(!c.busy());
    }
}
