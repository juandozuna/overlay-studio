use super::{dice, fx, wheel};
use crate::{
    domain::{controls::Action, dice as keys, sound},
    model::{CueEvent, CueTarget, GameClock, Project, clock_text},
};
use std::{fmt, time::Instant};

/// Requests handled by the output adapter after a command has been validated.
#[derive(Clone, Debug, PartialEq)]
pub enum OutputRequest {
    ToggleVisibility { route: usize },
    TakeDesign { route: usize, design: usize },
    ReplayAnimation { route: usize },
}

#[derive(Default)]
pub struct CommandOutcome {
    pub events: Vec<CueEvent>,
    pub outputs: Vec<OutputRequest>,
    pub preview_requested: bool,
    pub document_changed: bool,
    /// Sounds the desktop app should play (score changes, wheel); played by the audio engine.
    pub sounds: Vec<crate::domain::sound::SoundRequest>,
}

#[derive(Debug, PartialEq)]
pub enum CommandError {
    MissingField(String),
    MissingClock(usize),
    MissingCue(u64),
    MissingOutput(usize),
    MissingDesign(usize),
    NonNumericField(String),
    WheelEmpty,
    WheelNoWinner,
    WheelNoOriginal,
    WheelBusy,
    /// El Dado command failed; the text is shown to the host as is.
    Dice(String),
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingField(field) => write!(f, "Value '{field}' does not exist"),
            Self::MissingClock(index) => write!(f, "Clock {} does not exist", index + 1),
            Self::MissingCue(id) => write!(f, "Cue {id} does not exist"),
            Self::MissingOutput(index) => write!(f, "Output {} does not exist", index + 1),
            Self::MissingDesign(index) => write!(f, "Design {} does not exist", index + 1),
            Self::WheelEmpty => write!(f, "The wheel has no entries to spin"),
            Self::WheelNoWinner => write!(f, "The wheel has no winner to remove yet"),
            Self::WheelBusy => write!(f, "The wheel is still spinning"),
            Self::Dice(message) => write!(f, "{message}"),
            Self::WheelNoOriginal => write!(f, "The wheel has no original list to restore"),
            Self::NonNumericField(field) => write!(
                f,
                "Value '{field}' must contain an integer before it can be adjusted"
            ),
        }
    }
}
impl std::error::Error for CommandError {}

/// Performs game/document mutations without UI, network, file, or GPU access.
/// Runtime-specific output work is returned as an explicit request to the caller.
pub fn dispatch(
    project: &mut Project,
    clocks: &mut [GameClock],
    action: Action,
) -> Result<CommandOutcome, CommandError> {
    let mut outcome = CommandOutcome::default();
    match action {
        Action::Adjust { field, step } => {
            ensure_field(project, &field)?;
            if project.values[&field].parse::<i64>().is_err() {
                return Err(CommandError::NonNumericField(field));
            }
            project.adjust_score(&field, step);
            outcome.document_changed = step != 0;
            outcome.preview_requested = true;
            fx::score_changed(project, &field, step, &mut outcome);
            if step > 0 {
                outcome.events.push(CueEvent::ScoreAdded { field });
            } else if step < 0 {
                outcome.events.push(CueEvent::ScoreRemoved { field });
            }
        }
        Action::SetValue { field, value } => {
            ensure_field(project, &field)?;
            project.values.insert(field, value);
            outcome.document_changed = true;
        }
        Action::ClockToggle => toggle_clock(project, clocks, 0, &mut outcome)?,
        Action::ClockToggleAt { clock } => toggle_clock(project, clocks, clock, &mut outcome)?,
        Action::ClockReset => reset_clock(project, clocks, 0, &mut outcome)?,
        Action::ClockResetAt { clock } => reset_clock(project, clocks, clock, &mut outcome)?,
        Action::ClockAdjust { clock, seconds } => {
            let spec = project
                .clocks
                .get_mut(clock)
                .ok_or(CommandError::MissingClock(clock))?;
            spec.seconds = (spec.seconds + seconds as f64).clamp(0., 604800.);
            project
                .values
                .insert(spec.value_key.clone(), clock_text(spec.seconds));
            sync_primary_clock(project);
            outcome.document_changed = seconds != 0;
        }
        Action::ToggleOverlay { route } => {
            ensure_output(project, route)?;
            outcome
                .outputs
                .push(OutputRequest::ToggleVisibility { route });
        }
        Action::Take { route, design } => {
            ensure_output(project, route)?;
            if project.designs.get(design).is_none() {
                return Err(CommandError::MissingDesign(design));
            }
            outcome
                .outputs
                .push(OutputRequest::TakeDesign { route, design });
        }
        Action::Cue { route } => {
            ensure_output(project, route)?;
            outcome
                .outputs
                .push(OutputRequest::ReplayAnimation { route });
        }
        Action::SpinWheel => {
            wheel::spin(&mut project.values)?;
            outcome.sounds.extend(sound::for_key(&project.values, sound::WHEEL_SPIN));
            outcome.document_changed = true;
            outcome.preview_requested = true;
        }
        Action::RemoveWheelWinner => {
            wheel::remove_winner(&mut project.values)?;
            outcome.sounds.extend(sound::for_key(&project.values, sound::WHEEL_REMOVE));
            outcome.document_changed = true;
            outcome.preview_requested = true;
        }
        Action::RestoreWheel => {
            wheel::restore(&mut project.values)?;
            outcome.document_changed = true;
            outcome.preview_requested = true;
        }
        Action::RollDice => {
            dice::roll(&mut project.values)?;
            dice::reset_question_clock(project, clocks);
            outcome.document_changed = true;
            outcome.preview_requested = true;
        }
        Action::NextQuestion => {
            dice::next_question(&mut project.values)?;
            outcome.document_changed = true;
            outcome.preview_requested = true;
        }
        Action::RevealAnswer => {
            dice::toggle_reveal(&mut project.values);
            if project.values.get(keys::REVEALED).is_some_and(|v| v == "1") {
                dice::pause_question_clock(project, clocks);
            }
            outcome.document_changed = true;
            outcome.preview_requested = true;
        }
        Action::NextParticipant => {
            dice::next_participant(project)?;
            dice::reset_question_clock(project, clocks);
            outcome.document_changed = true;
            outcome.preview_requested = true;
        }
        Action::StartQuestionTimer => {
            dice::start_question_timer(project, clocks, &mut outcome)?;
            outcome.document_changed = true;
            outcome.preview_requested = true;
        }
        Action::AwardCurrentTeam { points } => {
            dice::award_current_team(project, points, &mut outcome)?;
            dice::pause_question_clock(project, clocks);
        }
        Action::RunCue { id } => {
            let target = project
                .cue_rules
                .iter()
                .find(|cue| cue.id == id)
                .ok_or(CommandError::MissingCue(id))?
                .target
                .clone();
            apply_cue_target(project, target, &mut outcome)?;
        }
    }
    Ok(outcome)
}

pub fn apply_cue_target(
    project: &mut Project,
    target: CueTarget,
    outcome: &mut CommandOutcome,
) -> Result<(), CommandError> {
    match target {
        CueTarget::ReplayOutput { route } => {
            ensure_output(project, route)?;
            outcome
                .outputs
                .push(OutputRequest::ReplayAnimation { route });
        }
        CueTarget::TakeDesign { route, design } => {
            ensure_output(project, route)?;
            if project.designs.get(design).is_none() {
                return Err(CommandError::MissingDesign(design));
            }
            outcome
                .outputs
                .push(OutputRequest::TakeDesign { route, design });
        }
        CueTarget::SetValue { field, value } => {
            ensure_field(project, &field)?;
            project.values.insert(field, value);
            outcome.document_changed = true;
        }
    }
    Ok(())
}

fn ensure_field(project: &Project, field: &str) -> Result<(), CommandError> {
    if project.values.contains_key(field) {
        Ok(())
    } else {
        Err(CommandError::MissingField(field.into()))
    }
}
fn ensure_output(project: &Project, route: usize) -> Result<(), CommandError> {
    if project.routes.get(route).is_some() {
        Ok(())
    } else {
        Err(CommandError::MissingOutput(route))
    }
}
fn toggle_clock(
    project: &Project,
    clocks: &mut [GameClock],
    index: usize,
    outcome: &mut CommandOutcome,
) -> Result<(), CommandError> {
    if project.clocks.get(index).is_none() {
        return Err(CommandError::MissingClock(index));
    }
    let runtime = clocks
        .get_mut(index)
        .ok_or(CommandError::MissingClock(index))?;
    runtime.last = Instant::now();
    runtime.running = !runtime.running;
    if runtime.running {
        outcome.events.push(CueEvent::ClockStarted { clock: index });
    }
    Ok(())
}
fn reset_clock(
    project: &mut Project,
    clocks: &mut [GameClock],
    index: usize,
    outcome: &mut CommandOutcome,
) -> Result<(), CommandError> {
    if project.clocks.get(index).is_none() || clocks.get(index).is_none() {
        return Err(CommandError::MissingClock(index));
    }
    let runtime = &mut clocks[index];
    let spec = &mut project.clocks[index];
    runtime.running = false;
    runtime.last = Instant::now();
    spec.seconds = spec.reset_seconds;
    project
        .values
        .insert(spec.value_key.clone(), clock_text(spec.seconds));
    sync_primary_clock(project);
    outcome.document_changed = true;
    outcome.events.push(CueEvent::ClockReset { clock: index });
    Ok(())
}
pub(super) fn sync_primary_clock(project: &mut Project) {
    if let Some(clock) = project.clocks.first() {
        project.clock_seconds = clock.seconds;
        project.countdown = clock.countdown;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn commands_are_usable_without_desktop_or_network_adapters() {
        let mut project = Project::fresh();
        let mut clocks = vec![GameClock::new()];
        let outcome = dispatch(
            &mut project,
            &mut clocks,
            Action::Adjust {
                field: "home.score".into(),
                step: -2,
            },
        )
        .unwrap();
        assert_eq!(project.values["home.score"], "10");
        assert_eq!(
            outcome.events,
            vec![CueEvent::ScoreRemoved {
                field: "home.score".into()
            }]
        );
        let outcome = dispatch(
            &mut project,
            &mut clocks,
            Action::Take {
                route: 0,
                design: 2,
            },
        )
        .unwrap();
        assert_eq!(
            outcome.outputs,
            vec![OutputRequest::TakeDesign {
                route: 0,
                design: 2
            }]
        );
    }
    #[test]
    fn invalid_commands_preserve_document_and_clocks() {
        let mut project = Project::fresh();
        let before = project.clone();
        let mut clocks = vec![GameClock::new()];
        assert!(matches!(
            dispatch(
                &mut project,
                &mut clocks,
                Action::ClockResetAt { clock: 99 }
            ),
            Err(CommandError::MissingClock(99))
        ));
        assert!(matches!(
            dispatch(
                &mut project,
                &mut clocks,
                Action::Take {
                    route: 0,
                    design: 99
                }
            ),
            Err(CommandError::MissingDesign(99))
        ));
        assert!(project == before);
        assert!(!clocks[0].running);
        assert!(matches!(
            dispatch(
                &mut project,
                &mut clocks,
                Action::Adjust {
                    field: "home.name".into(),
                    step: 1
                }
            ),
            Err(CommandError::NonNumericField(_))
        ));
        assert!(project == before);
    }
}
