use super::{DocumentSession, History};
use crate::{
    application::{game_session::GameSession, game_setup},
    domain::controls::ControllerLayout,
    model::{ClockSpec, CueRule, Project, Route, Team, clock_text},
};
use std::collections::BTreeMap;

#[derive(Clone, PartialEq)]
struct Snapshot {
    teams: Vec<Team>,
    clocks: Vec<ClockSpec>,
    routes: Vec<Route>,
    cues: Vec<CueRule>,
    controller: ControllerLayout,
    values: BTreeMap<String, String>,
    bindings: BTreeMap<(usize, u64), (String, String)>,
    primary: (f64, bool),
}
impl Snapshot {
    fn capture(p: &Project) -> Self {
        Self {
            teams: p.teams.clone(),
            clocks: p.clocks.clone(),
            routes: p.routes.clone(),
            cues: p.cue_rules.clone(),
            controller: p.controller.clone(),
            values: p.values.clone(),
            primary: (p.clock_seconds, p.countdown),
            bindings: p
                .designs
                .iter()
                .enumerate()
                .flat_map(|(d, design)| {
                    design
                        .elements
                        .iter()
                        .map(move |e| ((d, e.id), (e.binding.clone(), e.text.clone())))
                })
                .collect(),
        }
    }
    fn exact(&self, p: &mut Project) {
        p.teams = self.teams.clone();
        p.clocks = self.clocks.clone();
        p.routes = self.routes.clone();
        p.cue_rules = self.cues.clone();
        p.controller = self.controller.clone();
        p.values = self.values.clone();
        for (d, design) in p.designs.iter_mut().enumerate() {
            for e in &mut design.elements {
                if let Some((binding, text)) = self.bindings.get(&(d, e.id)) {
                    e.binding = binding.clone();
                    e.text = text.clone();
                }
            }
        }
        p.clock_seconds = self.primary.0;
        p.countdown = self.primary.1;
    }
}
fn primary(p: &mut Project) {
    if let Some(c) = p.clocks.first() {
        p.clock_seconds = c.seconds;
        p.countdown = c.countdown;
    }
}

#[derive(Clone)]
pub(super) struct SetupEdit {
    before: Snapshot,
    after: Snapshot,
}
impl SetupEdit {
    pub(super) fn remember_removed(&mut self, p: &Project, redo: bool) {
        let (source, target) = if redo {
            (&mut self.before, &self.after)
        } else {
            (&mut self.after, &self.before)
        };
        for (key, value) in &p.values {
            if source.values.contains_key(key) && !target.values.contains_key(key) {
                source.values.insert(key.clone(), value.clone());
            }
        }
        for clock in &mut source.clocks {
            if !target.clocks.iter().any(|c| c.id == clock.id) {
                if let Some(live) = p.clocks.iter().find(|c| c.id == clock.id) {
                    clock.seconds = live.seconds;
                }
            }
        }
    }
    pub(super) fn restore(&self, p: &mut Project, redo: bool) {
        let (source, target) = if redo {
            (&self.before, &self.after)
        } else {
            (&self.after, &self.before)
        };
        if source.teams != target.teams {
            p.teams = target.teams.clone();
        }
        if source.clocks != target.clocks {
            let live: BTreeMap<_, _> = p.clocks.iter().map(|c| (c.id, c.seconds)).collect();
            p.clocks = target
                .clocks
                .iter()
                .map(|c| {
                    let mut c = c.clone();
                    if source
                        .clocks
                        .iter()
                        .find(|old| old.id == c.id)
                        .is_some_and(|old| old.seconds == c.seconds)
                    {
                        if let Some(seconds) = live.get(&c.id) {
                            c.seconds = *seconds;
                        }
                    }
                    c
                })
                .collect();
        }
        if source.routes != target.routes {
            p.routes.truncate(target.routes.len());
            for (index, route) in target.routes.iter().enumerate() {
                if let (Some(old), Some(live)) = (source.routes.get(index), p.routes.get_mut(index))
                {
                    if old.name != route.name {
                        live.name = route.name.clone();
                    }
                    if old.protocol != route.protocol {
                        live.protocol = route.protocol.clone();
                    }
                    if old.fps != route.fps {
                        live.fps = route.fps;
                    }
                    if old.design != route.design {
                        live.design = route.design;
                    }
                } else if index >= p.routes.len() {
                    p.routes.push(route.clone());
                }
            }
        }
        if source.cues != target.cues {
            p.cue_rules = target.cues.clone();
        }
        if source.controller != target.controller {
            p.controller = target.controller.clone();
        }
        for key in source.values.keys().chain(target.values.keys()) {
            if source.values.get(key) != target.values.get(key) {
                if let Some(value) = target.values.get(key) {
                    p.values.insert(key.clone(), value.clone());
                } else {
                    p.values.remove(key);
                }
            }
        }
        for (d, design) in p.designs.iter_mut().enumerate() {
            for e in &mut design.elements {
                let key = (d, e.id);
                if source.bindings.get(&key) != target.bindings.get(&key) {
                    if let Some((binding, text)) = target.bindings.get(&key) {
                        e.binding = binding.clone();
                        e.text = text.clone();
                    }
                }
            }
        }
        for clock in &p.clocks {
            p.values
                .insert(clock.value_key.clone(), clock_text(clock.seconds));
        }
        primary(p);
    }
    pub(super) fn validate(&self, p: &mut Project, redo: bool) -> Result<(), String> {
        let original = Snapshot::capture(p);
        self.restore(p, redo);
        let result = p.validate_metadata();
        original.exact(p);
        result
    }
    fn stream_changes(&self) -> Vec<usize> {
        let count = self.before.routes.len().max(self.after.routes.len());
        (0..count)
            .filter(
                |&i| match (self.before.routes.get(i), self.after.routes.get(i)) {
                    (Some(a), Some(b)) => {
                        a.name != b.name || a.protocol != b.protocol || a.fps != b.fps
                    }
                    _ => true,
                },
            )
            .collect()
    }
    fn removed_fields(&self, redo: bool) -> Vec<String> {
        let (source, target) = if redo {
            (&self.before, &self.after)
        } else {
            (&self.after, &self.before)
        };
        source
            .values
            .keys()
            .filter(|key| !target.values.contains_key(*key))
            .cloned()
            .collect()
    }
}
impl DocumentSession {
    pub fn configure(
        &mut self,
        game: &mut GameSession,
        kind: &str,
        index: usize,
        field: &str,
        value: &str,
    ) -> Result<(), String> {
        self.end_gesture()?;
        let before = Snapshot::capture(&self.project);
        game_setup::apply(&mut self.project, game, kind, index, field, value)?;
        let after = Snapshot::capture(&self.project);
        if before != after {
            self.push_history(History::Setup(Box::new(SetupEdit { before, after })));
        }
        Ok(())
    }
    pub fn history_stream_changes(&self, redo: bool) -> Vec<usize> {
        match if redo {
            self.redo.last()
        } else {
            self.undo.last()
        } {
            Some(History::Setup(edit)) => edit.stream_changes(),
            _ => vec![],
        }
    }
    pub fn history_removed_fields(&self, redo: bool) -> Vec<String> {
        match if redo {
            self.redo.last()
        } else {
            self.undo.last()
        } {
            Some(History::Setup(edit)) => edit.removed_fields(redo),
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn custom_field_removal_restores_references_and_latest_live_value() {
        use crate::{
            domain::controls::Action,
            model::{CueEvent, CueTarget},
        };
        let mut s = DocumentSession::new(crate::model::Project::fresh());
        let mut g = GameSession::new(&s.project);
        s.configure(&mut g, "value", 0, "add", "Question").unwrap();
        s.project.values.insert("Question".into(), "BONUS".into());
        s.project.designs[0].elements[3].binding = "Question".into();
        s.project.cue_rules.push(CueRule {
            id: 91,
            name: "Bonus".into(),
            enabled: false,
            event: CueEvent::ScoreAdded {
                field: "Question".into(),
            },
            target: CueTarget::SetValue {
                field: "Question".into(),
                value: "NEXT".into(),
            },
        });
        s.project.controller.controls[0].action = Action::SetValue {
            field: "Question".into(),
            value: "NEXT".into(),
        };
        s.project.controller.controls[1].action = Action::RunCue { id: 91 };
        let controller = s.project.controller.clone();
        let designs = s.project.designs.clone();
        s.configure(&mut g, "value", 0, "remove", "Question")
            .unwrap();
        assert!(!s.project.values.contains_key("Question"));
        assert!(s.project.cue_rules.is_empty());
        assert_eq!(
            s.project.controller.controls.len(),
            controller.controls.len() - 2
        );
        s.project.values.insert("home.score".into(), "99".into());
        s.undo().unwrap();
        assert_eq!(s.project.values["Question"], "BONUS");
        assert!(s.project.controller == controller);
        assert!(s.project.designs == designs);
        s.project.values.insert("Question".into(), "EXTRA".into());
        s.redo();
        s.undo().unwrap();
        assert_eq!(s.project.values["Question"], "EXTRA");
        assert_eq!(s.project.values["home.score"], "99");
        s.project.validate_metadata().unwrap();
    }
    #[test]
    fn new_field_history_keeps_operator_changes_and_protects_owned_keys() {
        let mut s = DocumentSession::new(crate::model::Project::fresh());
        let mut g = GameSession::new(&s.project);
        s.configure(&mut g, "value", 0, "add", "Custom text")
            .unwrap();
        s.project.values.insert("Custom text".into(), "7".into());
        assert_eq!(s.history_removed_fields(false), vec!["Custom text"]);
        s.undo().unwrap();
        s.redo();
        assert_eq!(s.project.values["Custom text"], "7");
        let revision = s.revision;
        assert!(
            s.configure(&mut g, "value", 0, "add", "Custom text")
                .is_err()
        );
        assert!(
            s.configure(&mut g, "value", 0, "remove", "home.score")
                .is_err()
        );
        assert_eq!(s.revision, revision);
    }
    #[test]
    fn stream_history_preserves_later_live_take_selection() {
        let mut s = DocumentSession::new(crate::model::Project::fresh());
        let mut g = GameSession::new(&s.project);
        s.configure(&mut g, "output", 0, "name", "Main scoreboard")
            .unwrap();
        s.project.routes[0].design = 2;
        s.project.routes[1].design = 1;
        assert_eq!(s.history_stream_changes(false), vec![0]);
        s.undo().unwrap();
        assert_eq!(s.project.routes[0].design, 2);
        assert_eq!(s.project.routes[1].design, 1);
        s.redo();
        assert_eq!(s.project.routes[0].name, "Main scoreboard");
        assert_eq!(s.project.routes[0].design, 2);
    }
    #[test]
    fn undo_setup_preserves_live_scores_and_elapsed_clock() {
        let mut s = DocumentSession::new(crate::model::Project::fresh());
        let mut g = GameSession::new(&s.project);
        s.configure(&mut g, "clock", 0, "reset", "12:00").unwrap();
        s.project.clocks[0].seconds = 410.;
        s.project.values.insert("home.score".into(), "33".into());
        s.undo().unwrap();
        assert_eq!(s.project.clocks[0].reset_seconds, 600.);
        assert_eq!(s.project.clocks[0].seconds, 410.);
        assert_eq!(s.project.values["home.score"], "33");
        s.redo();
        assert_eq!(s.project.clocks[0].reset_seconds, 720.);
        assert_eq!(s.project.clocks[0].seconds, 410.);
    }
    #[test]
    fn removed_team_bindings_return_and_invalid_setup_has_no_history() {
        let mut s = DocumentSession::new(crate::model::Project::fresh());
        let mut g = GameSession::new(&s.project);
        let bindings = s.project.designs.clone();
        let controller = s.project.controller.clone();
        s.configure(&mut g, "team", 1, "remove", "").unwrap();
        assert!(!s.project.values.contains_key("away.score"));
        s.project.values.insert("home.score".into(), "44".into());
        s.undo().unwrap();
        assert_eq!(s.project.values["home.score"], "44");
        assert_eq!(s.project.values["away.score"], "9");
        assert!(s.project.designs == bindings);
        assert!(s.project.controller == controller);
        s.redo();
        assert_eq!(s.project.teams.len(), 1);
        s.project.validate_metadata().unwrap();
        let revision = s.revision;
        assert!(s.configure(&mut g, "team", 0, "steps", "0").is_err());
        assert_eq!(s.revision, revision);
    }
    #[test]
    fn undo_recolor_restores_color_value() {
        let mut s = DocumentSession::new(crate::model::Project::fresh());
        let mut g = GameSession::new(&s.project);
        let key = crate::domain::wheel::team_color_key(s.project.teams[0].id);
        let old = s.project.values[&key].clone();
        s.configure(&mut g, "team", 0, "color", "#010203").unwrap();
        assert_eq!(s.project.values[&key], "#010203");
        s.undo().unwrap();
        assert_eq!(s.project.values[&key], old);
        s.redo();
        assert_eq!(s.project.values[&key], "#010203");
    }
}
