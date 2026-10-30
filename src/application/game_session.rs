//! Running clocks and cue coordination shared by desktop and remote entry points.
use std::time::{Duration, Instant};
use super::game_commands::{self, CommandError, CommandOutcome};
use crate::{
    domain::{controls::Action, dice as dice_keys},
    model::{CueEvent, GameClock, Project, clock_text},
};

pub struct GameSession {
    pub clocks: Vec<GameClock>,
    wheel_reveal: Option<Instant>,
    wheel_animation: Option<Instant>,
    dice_reveal: Option<Instant>,
    dice_animation: Option<Instant>,
    fx_animation: Option<Instant>,
}
const WHEEL_TAIL: Duration = Duration::from_secs(3);
const DICE_TAIL: Duration = Duration::from_millis(600);
impl GameSession {
    pub fn new(project: &Project) -> Self {
        Self {
            clocks: project.clocks.iter().map(|_| GameClock::new()).collect(),
            wheel_reveal: None,
            wheel_animation: None,
            dice_reveal: None,
            dice_animation: None,
            fx_animation: None,
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
        if matches!(action, Action::RollDice) && self.dice_reveal.is_some() {
            return Err(CommandError::Dice("The dice is still rolling".into()));
        }
        let spun = matches!(action, Action::SpinWheel);
        let rolled = matches!(action, Action::RollDice);
        let scored = match &action {
            Action::Adjust { step, .. } => *step != 0,
            Action::AwardCurrentTeam { points } => *points != 0,
            _ => false,
        };
        let new_turn = matches!(action, Action::NextParticipant);
        let mut outcome = game_commands::dispatch(project, &mut self.clocks, action)?;
        if new_turn {
            self.cancel_dice_roll();
        }
        if spun {
            let seconds = crate::domain::wheel::WheelConfig::read(&project.values).spin_seconds;
            let now = Instant::now();
            self.wheel_reveal = Some(now + Duration::from_secs_f32(seconds + 0.1));
            self.wheel_animation = Some(now + Duration::from_secs_f32(seconds) + WHEEL_TAIL);
        }
        if rolled {
            let seconds = crate::domain::dice::DiceConfig::read(&project.values).roll_seconds;
            let now = Instant::now();
            self.dice_reveal = Some(now + Duration::from_secs_f32(seconds + 0.1));
            self.dice_animation = Some(now + Duration::from_secs_f32(seconds) + DICE_TAIL);
        }
        if scored {
            let seconds = crate::domain::fx::FxConfig::read(&project.values).seconds;
            self.fx_animation = Some(Instant::now() + Duration::from_secs_f32(seconds + 0.2));
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
    /// Forgets a roll that is still tumbling so it cannot publish its result into another turn (new turn, "Back one").
    pub fn cancel_dice_roll(&mut self) {
        self.dice_reveal = None;
        self.dice_animation = None;
    }
    /// True while a wheel spin, its winner glow, its confetti, a dice roll or a score effect needs frames.
    pub fn animation_busy(&self) -> bool {
        let now = Instant::now();
        [self.wheel_animation, self.dice_animation, self.fx_animation].iter().flatten().any(|end| now < *end)
    }
    /// Publishes the winner once the spin has landed; cheap enough to call every frame.
    pub fn reveal(&mut self, project: &mut Project) -> CommandOutcome {
        let mut outcome = CommandOutcome::default();
        if self.wheel_reveal.is_some_and(|at| Instant::now() >= at) {
            self.wheel_reveal = None;
            if crate::application::wheel::reveal_winner(&mut project.values) {
                outcome.document_changed = true;
                outcome.preview_requested = true;
                outcome.sounds.extend(crate::domain::sound::for_key(&project.values, crate::domain::sound::WHEEL_LAND));
            }
        }
        if self.dice_reveal.is_some_and(|at| Instant::now() >= at) {
            self.dice_reveal = None;
            if crate::application::dice::reveal_roll(&mut project.values) {
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
                // Revealing or awarding pauses the clock, so reaching zero means nobody answered in time
                // (a Reveal before the timer started must not hide that).
                if spec.value_key == dice_keys::QUESTION_CLOCK_KEY {
                    project.values.insert(dice_keys::STATUS.into(), dice_keys::STATUS_ANNULLED.into());
                    outcome.document_changed = true;
                    outcome.preview_requested = true;
                }
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
    fn dice_project() -> Project {
        let mut p = Project::fresh();
        p.clocks[0].value_key = dice_keys::QUESTION_CLOCK_KEY.into();
        p.clocks[0].countdown = true;
        p
    }
    #[test]
    fn dice_result_is_hidden_until_landing_and_roll_waits() {
        let mut p = dice_project();
        let mut game = GameSession::new(&p);
        assert!(!game.animation_busy());
        game.execute(&mut p, Action::RollDice).unwrap();
        assert!(game.animation_busy());
        assert!(!p.values[dice_keys::FACE].is_empty());
        assert_eq!(p.values[dice_keys::RESULT], "");
        assert_eq!(
            game.execute(&mut p, Action::RollDice).err(),
            Some(CommandError::Dice("The dice is still rolling".into()))
        );
        assert!(!game.reveal(&mut p).preview_requested);
        game.dice_reveal = Some(Instant::now() - Duration::from_millis(1));
        assert!(game.reveal(&mut p).preview_requested);
        assert!(!p.values[dice_keys::RESULT].is_empty());
        assert!(!p.values[dice_keys::RULE].is_empty());
        game.execute(&mut p, Action::RollDice).unwrap();
        assert_eq!(p.values[dice_keys::ROLL_ID], "2");
        game.dice_animation = Some(Instant::now() - Duration::from_millis(1));
        assert!(!game.animation_busy());
    }
    #[test]
    fn question_clock_finish_annuls_and_next_actions_clear_it() {
        for clear in [Action::StartQuestionTimer, Action::RollDice, Action::NextParticipant, Action::NextQuestion] {
            let mut p = dice_project();
            crate::application::dice::set_participants(&mut p, 0, "Ana");
            let mut game = GameSession::new(&p);
            game.execute(&mut p, Action::StartQuestionTimer).unwrap();
            p.clocks[0].seconds = 0.01;
            game.clocks[0].last = Instant::now() - Duration::from_secs(1);
            let outcome = game.tick(&mut p).unwrap();
            assert_eq!(outcome.events, vec![CueEvent::ClockFinished { clock: 0 }]);
            assert_eq!(p.values[dice_keys::STATUS], dice_keys::STATUS_ANNULLED);
            assert!(!game.clocks[0].running);
            // the action may fail (no bank loaded) but a successful one must clear the status
            if game.execute(&mut p, clear.clone()).is_ok() {
                assert_eq!(p.values[dice_keys::STATUS], "", "{clear:?}");
            }
        }
    }
    #[test]
    fn revealed_answer_stops_the_clock_and_is_not_annulled() {
        let mut p = dice_project();
        let mut game = GameSession::new(&p);
        game.execute(&mut p, Action::StartQuestionTimer).unwrap();
        game.execute(&mut p, Action::RevealAnswer).unwrap();
        assert!(!game.clocks[0].running);
        p.clocks[0].seconds = 0.;
        game.clocks[0].last = Instant::now() - Duration::from_secs(1);
        game.tick(&mut p).unwrap();
        assert_ne!(p.values.get(dice_keys::STATUS).map(String::as_str), Some(dice_keys::STATUS_ANNULLED));
    }
    #[test]
    fn reveal_before_the_timer_does_not_hide_the_annulment() {
        let mut p = dice_project();
        let mut game = GameSession::new(&p);
        game.execute(&mut p, Action::RevealAnswer).unwrap();
        assert_eq!(p.values[dice_keys::REVEALED], "1");
        game.execute(&mut p, Action::StartQuestionTimer).unwrap();
        p.clocks[0].seconds = 0.01;
        game.clocks[0].last = Instant::now() - Duration::from_secs(1);
        game.tick(&mut p).unwrap();
        assert_eq!(p.values[dice_keys::STATUS], dice_keys::STATUS_ANNULLED);
    }
    #[test]
    fn next_participant_cancels_a_roll_that_is_still_tumbling() {
        let mut p = dice_project();
        crate::application::dice::set_participants(&mut p, 0, "Ana");
        let mut game = GameSession::new(&p);
        game.execute(&mut p, Action::RollDice).unwrap();
        game.execute(&mut p, Action::StartQuestionTimer).unwrap();
        game.execute(&mut p, Action::NextParticipant).unwrap();
        assert!(!game.clocks[0].running);
        assert_eq!((p.values[dice_keys::FACE].as_str(), p.values[dice_keys::QUESTION_CLOCK_KEY].as_str()), ("", "00:30"));
        assert!(!game.animation_busy());
        assert!(!game.reveal(&mut p).preview_requested);
        assert_eq!(p.values[dice_keys::RESULT], "");
    }
    #[test]
    fn award_runs_the_score_effect_path() {
        let mut p = dice_project();
        let mut game = GameSession::new(&p);
        game.execute(&mut p, Action::AwardCurrentTeam { points: 10 }).unwrap();
        assert_eq!(p.values[&p.teams[0].score_key], "22");
        assert!(game.animation_busy());
    }
}
