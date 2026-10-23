//! Running clocks and cue coordination shared by desktop and remote entry points.
use std::time::{Duration, Instant};
use super::game_commands::{self, CommandError, CommandOutcome};
use crate::{
    domain::controls::Action,
    model::{CueEvent, GameClock, Project, clock_text},
};

pub struct GameSession {
    pub clocks: Vec<GameClock>,
    wheel_reveal: Option<Instant>,
    wheel_animation: Option<Instant>,
}
const WHEEL_TAIL: Duration = Duration::from_secs(3);
impl GameSession {
    pub fn new(project: &Project) -> Self {
        Self {
            clocks: project.clocks.iter().map(|_| GameClock::new()).collect(),
            wheel_reveal: None,
            wheel_animation: None,
        }
    }
    pub fn execute(
        &mut self,
        project: &mut Project,
        action: Action,
    ) -> Result<CommandOutcome, CommandError> {
        let wheel_action = matches!(
            action,
            Action::SpinWheel | Action::RemoveWheelWinner | Action::RestoreWheel
        );
        if wheel_action && self.wheel_reveal.is_some() {
            return Err(CommandError::WheelBusy);
        }
        let spun = matches!(action, Action::SpinWheel);
        let mut outcome = game_commands::dispatch(project, &mut self.clocks, action)?;
        if spun {
            let seconds = crate::domain::wheel::WheelConfig::read(&project.values).spin_seconds;
            let now = Instant::now();
            self.wheel_reveal = Some(now + Duration::from_secs_f32(seconds + 0.1));
            self.wheel_animation = Some(now + Duration::from_secs_f32(seconds) + WHEEL_TAIL);
        }
        self.apply_events(project, &mut outcome)?;
        Ok(outcome)
    }
    fn apply_events(
        &self,
        project: &mut Project,
        outcome: &mut CommandOutcome,
    ) -> Result<(), CommandError> {
        let targets: Vec<_> = project
            .cue_rules
            .iter()
            .filter(|rule| rule.enabled && outcome.events.contains(&rule.event))
            .map(|rule| rule.target.clone())
            .collect();
        if targets.is_empty() {
            return Ok(());
        }
        let before = project.values.clone();
        for target in targets {
            game_commands::apply_cue_target(project, target, outcome)?;
        }
        outcome.preview_requested |= project.values != before;
        Ok(())
    }
    /// True while a wheel spin, its winner glow or its confetti needs frames.
    pub fn animation_busy(&self) -> bool {
        self.wheel_animation.is_some_and(|end| Instant::now() < end)
    }
    /// Publishes the winner once the spin has landed; cheap enough to call every frame.
    pub fn reveal(&mut self, project: &mut Project) -> CommandOutcome {
        let mut outcome = CommandOutcome::default();
        if self.wheel_reveal.is_some_and(|at| Instant::now() >= at) {
            self.wheel_reveal = None;
            if crate::application::wheel::reveal_winner(&mut project.values) {
                outcome.document_changed = true;
                outcome.preview_requested = true;
            }
        }
        outcome
    }
    pub fn tick(&mut self, project: &mut Project) -> Result<CommandOutcome, CommandError> {
        let mut outcome = self.reveal(project);
        for (index, (runtime, spec)) in self.clocks.iter_mut().zip(&mut project.clocks).enumerate()
        {
            outcome.document_changed |= runtime.running;
            if runtime.tick_spec(spec) {
                outcome
                    .events
                    .push(CueEvent::ClockFinished { clock: index });
            }
            let value = clock_text(spec.seconds);
            if project.values.get(&spec.value_key) != Some(&value) {
                project.values.insert(spec.value_key.clone(), value);
                outcome.preview_requested = true;
            }
        }
        if let Some(primary) = project.clocks.first() {
            project.clock_seconds = primary.seconds;
            project.countdown = primary.countdown;
        }
        self.apply_events(project, &mut outcome)?;
        Ok(outcome)
    }
    pub fn pause_all(&mut self) {
        for clock in &mut self.clocks {
            clock.running = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CueRule, CueTarget};
    use std::time::{Duration, Instant};
    #[test]
    fn count_up_stops_at_valid_persistence_limit() {
        let mut p = Project::fresh();
        p.clocks[0].countdown = false;
        p.clocks[0].seconds = 604799.;
        let mut g = GameSession::new(&p);
        g.clocks[0].running = true;
        g.clocks[0].last = Instant::now() - Duration::from_secs(2);
        g.tick(&mut p).unwrap();
        assert_eq!(p.clocks[0].seconds, 604800.);
        assert!(!g.clocks[0].running);
        p.validate().unwrap();
    }
    #[test]
    fn clock_finishes_once_and_runs_enabled_rule() {
        let mut p = Project::fresh();
        p.clocks[0].seconds = 0.01;
        p.cue_rules.push(CueRule {
            id: 1,
            name: "End".into(),
            enabled: true,
            event: CueEvent::ClockFinished { clock: 0 },
            target: CueTarget::SetValue {
                field: "round".into(),
                value: "FINAL".into(),
            },
        });
        let mut game = GameSession::new(&p);
        game.execute(&mut p, Action::ClockToggleAt { clock: 0 })
            .unwrap();
        game.clocks[0].last = Instant::now() - Duration::from_secs(1);
        let outcome = game.tick(&mut p).unwrap();
        assert_eq!(outcome.events, vec![CueEvent::ClockFinished { clock: 0 }]);
        assert_eq!(p.values["round"], "FINAL");
        assert_eq!(p.values["clock"], "00:00");
        assert!(game.tick(&mut p).unwrap().events.is_empty());
    }
    #[test]
    fn score_event_returns_output_intent_without_native_adapter() {
        let mut p = Project::fresh();
        p.cue_rules.push(CueRule {
            id: 1,
            name: "Score".into(),
            enabled: true,
            event: CueEvent::ScoreAdded {
                field: "home.score".into(),
            },
            target: CueTarget::ReplayOutput { route: 0 },
        });
        let mut game = GameSession::new(&p);
        let outcome = game
            .execute(
                &mut p,
                Action::Adjust {
                    field: "home.score".into(),
                    step: 5,
                },
            )
            .unwrap();
        assert_eq!(p.values["home.score"], "17");
        assert_eq!(
            outcome.outputs,
            vec![game_commands::OutputRequest::ReplayAnimation { route: 0 }]
        );
    }
    #[test]
    fn wheel_winner_is_hidden_until_landing_and_actions_wait() {
        let mut p = Project::game_night();
        let mut game = GameSession::new(&p);
        game.execute(&mut p, Action::SpinWheel).unwrap();
        assert_eq!(p.values["wheel.winner"], "");
        assert!(!p.values["wheel.winner_index"].is_empty());
        for action in [Action::SpinWheel, Action::RemoveWheelWinner, Action::RestoreWheel] {
            assert_eq!(game.execute(&mut p, action).err(), Some(CommandError::WheelBusy));
        }
        game.wheel_reveal = Some(Instant::now() - Duration::from_millis(1));
        game.tick(&mut p).unwrap();
        assert!(!p.values["wheel.winner"].is_empty());
        game.execute(&mut p, Action::RemoveWheelWinner).unwrap();
        assert_eq!(p.values["wheel.winner"], "");
    }
    #[test]
    fn reveal_runs_without_clocks_and_animation_covers_spin_and_tail() {
        let mut p = Project::game_night();
        let mut game = GameSession::new(&p);
        assert!(!game.animation_busy());
        game.execute(&mut p, Action::SpinWheel).unwrap();
        assert!(game.animation_busy());
        assert!(!game.reveal(&mut p).preview_requested);
        game.wheel_reveal = Some(Instant::now() - Duration::from_millis(1));
        assert!(game.reveal(&mut p).preview_requested);
        assert!(!p.values["wheel.winner"].is_empty());
        assert!(game.animation_busy());
        game.wheel_animation = Some(Instant::now() - Duration::from_millis(1));
        assert!(!game.animation_busy());
    }
}
