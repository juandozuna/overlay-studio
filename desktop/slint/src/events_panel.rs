use crate::{CueRow, Desktop, StudioWindow, refresh, report, strings, view_models};
use overlay_studio::{
    domain::controls::Action,
    model::{CueEvent, CueRule, CueTarget, MAX_ID, Project},
};
use slint::ComponentHandle;
use std::{cell::RefCell, rc::Rc};
#[derive(Default)]
pub struct Session {
    pub selected: Option<u64>,
}
fn keys(p: &Project) -> Vec<String> {
    p.values.keys().cloned().collect()
}
fn selected(s: &Desktop) -> Option<&CueRule> {
    s.events
        .selected
        .and_then(|id| s.document.project.cue_rules.iter().find(|c| c.id == id))
        .or(s.document.project.cue_rules.first())
}
pub fn refresh_panel(ui: &StudioWindow, s: &Desktop) {
    let p = &s.document.project;
    let fields = keys(p);
    let index = |key: &str| fields.iter().position(|v| v == key).unwrap_or(0) as i32;
    ui.set_event_fields(strings(
        fields.iter().map(|key| crate::value_labels::label(p, key)),
    ));
    ui.set_event_clocks(strings(p.clocks.iter().map(|c| c.name.clone())));
    let rows = p
        .cue_rules
        .iter()
        .map(|c| {
            let (event, source) = match &c.event {
                CueEvent::ScoreAdded { field } => (0, index(field)),
                CueEvent::ScoreRemoved { field } => (1, index(field)),
                CueEvent::ClockStarted { clock } => (2, *clock as i32),
                CueEvent::ClockReset { clock } => (3, *clock as i32),
                CueEvent::ClockFinished { clock } => (4, *clock as i32),
            };
            let (target, output, design, field, value) = match &c.target {
                CueTarget::ReplayOutput { route } => (0, *route as i32, 0, 0, String::new()),
                CueTarget::TakeDesign { route, design } => {
                    (1, *route as i32, *design as i32, 0, String::new())
                }
                CueTarget::SetValue { field, value } => (2, 0, 0, index(field), value.clone()),
            };
            let events = [
                "Score added",
                "Score removed",
                "Clock started",
                "Clock reset",
                "Clock finished",
            ];
            let actions = ["Replay animation", "Take design", "Set value"];
            CueRow {
                id: c.id.to_string().into(),
                name: c.name.clone().into(),
                enabled: c.enabled,
                event,
                source,
                target,
                output,
                design,
                field,
                value: value.into(),
                summary: format!("{} → {}", events[event as usize], actions[target as usize])
                    .into(),
                linked: p
                    .controller
                    .controls
                    .iter()
                    .filter(|b| matches!(b.action,Action::RunCue{id} if id==c.id))
                    .count() as i32,
            }
        })
        .collect::<Vec<_>>();
    let current = selected(s)
        .and_then(|c| rows.iter().find(|row| row.id.as_str() == c.id.to_string()))
        .cloned()
        .unwrap_or_default();
    ui.set_event_rules(view_models::sync(ui.get_event_rules(), rows));
    ui.set_event_current(current);
}
fn edit(s: &mut Desktop, key: &str, value: &str) -> Result<(), String> {
    let id = selected(s).ok_or("Select a rule")?.id;
    let mut rules = s.document.project.cue_rules.clone();
    let c = rules
        .iter_mut()
        .find(|c| c.id == id)
        .ok_or("Rule no longer exists")?;
    let fields = keys(&s.document.project);
    let number = || {
        value
            .parse::<usize>()
            .map_err(|_| "Choose a valid target".to_string())
    };
    let field_at = |i: usize| {
        fields
            .get(i)
            .cloned()
            .ok_or("Field no longer exists".to_string())
    };
    match key {
        "name" => c.name = value.into(),
        "enabled" => c.enabled = value == "true",
        "event" => {
            c.event = match number()? {
                0 => CueEvent::ScoreAdded {
                    field: s
                        .document
                        .project
                        .teams
                        .first()
                        .map(|t| t.score_key.clone())
                        .unwrap_or(field_at(0)?),
                },
                1 => CueEvent::ScoreRemoved {
                    field: s
                        .document
                        .project
                        .teams
                        .first()
                        .map(|t| t.score_key.clone())
                        .unwrap_or(field_at(0)?),
                },
                2 => CueEvent::ClockStarted { clock: 0 },
                3 => CueEvent::ClockReset { clock: 0 },
                4 => CueEvent::ClockFinished { clock: 0 },
                _ => return Err("Choose an event".into()),
            }
        }
        "source" => match &mut c.event {
            CueEvent::ScoreAdded { field } | CueEvent::ScoreRemoved { field } => {
                *field = field_at(number()?)?
            }
            CueEvent::ClockStarted { clock }
            | CueEvent::ClockReset { clock }
            | CueEvent::ClockFinished { clock } => *clock = number()?,
        },
        "target" => {
            c.target = match number()? {
                0 => CueTarget::ReplayOutput { route: 0 },
                1 => CueTarget::TakeDesign {
                    route: 0,
                    design: s.document.design,
                },
                2 => CueTarget::SetValue {
                    field: if s.document.project.values.contains_key("round") {
                        "round".into()
                    } else {
                        field_at(0)?
                    },
                    value: "WINNER".into(),
                },
                _ => return Err("Choose an action".into()),
            }
        }
        "output" => match &mut c.target {
            CueTarget::ReplayOutput { route } | CueTarget::TakeDesign { route, .. } => {
                *route = number()?
            }
            _ => return Err("This action has no output".into()),
        },
        "design" => match &mut c.target {
            CueTarget::TakeDesign { design, .. } => *design = number()?,
            _ => return Err("Choose Take design first".into()),
        },
        "field" => match &mut c.target {
            CueTarget::SetValue { field, .. } => *field = field_at(number()?)?,
            _ => return Err("Choose Set value first".into()),
        },
        "value" => match &mut c.target {
            CueTarget::SetValue { value: current, .. } => *current = value.into(),
            _ => return Err("Choose Set value first".into()),
        },
        _ => return Err("Unknown rule property".into()),
    }
    s.document.set_cue_rules(rules)
}
fn command(s: &mut Desktop, command: &str) -> Result<(), String> {
    let mut rules = s.document.project.cue_rules.clone();
    if command == "run" {
        let id = selected(s).ok_or("Select a rule")?.id;
        return s.execute(Action::RunCue { id });
    }
    if command == "delete" {
        let id = selected(s).ok_or("Select a rule")?.id;
        rules.retain(|c| c.id != id);
        s.document.set_cue_rules(rules)?;
        s.events.selected = None;
        return Ok(());
    }
    let id = rules
        .iter()
        .map(|c| c.id)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .filter(|id| *id <= MAX_ID)
        .ok_or("Rule IDs are exhausted")?;
    let c = match command {
        "add" => CueRule {
            id,
            name: format!("Rule {}", rules.len() + 1),
            enabled: false,
            event: CueEvent::ScoreAdded {
                field: s
                    .document
                    .project
                    .teams
                    .first()
                    .ok_or("Add a team first")?
                    .score_key
                    .clone(),
            },
            target: CueTarget::ReplayOutput { route: 0 },
        },
        "duplicate" => {
            let mut c = selected(s).ok_or("Select a rule")?.clone();
            c.id = id;
            c.name.push_str(" copy");
            c.enabled = false;
            c
        }
        _ => return Err("Unknown rule command".into()),
    };
    rules.push(c);
    s.document.set_cue_rules(rules)?;
    s.events.selected = Some(id);
    Ok(())
}
pub fn connect(ui: &StudioWindow, state: Rc<RefCell<Desktop>>) {
    let shared = state.clone();
    let weak = ui.as_weak();
    ui.on_event_select(move |id| {
        if let Some(ui) = weak.upgrade() {
            let mut s = shared.borrow_mut();
            s.events.selected = id.parse().ok();
            refresh_panel(&ui, &s);
        }
    });
    let shared = state.clone();
    let weak = ui.as_weak();
    ui.on_event_edit(move |key, value| {
        if let Some(ui) = weak.upgrade() {
            let mut s = shared.borrow_mut();
            let result = edit(&mut s, &key, &value);
            report(&ui, result);
            refresh(&ui, &s);
            ui.window().request_redraw();
        }
    });
    let weak = ui.as_weak();
    ui.on_event_command(move |value| {
        if let Some(ui) = weak.upgrade() {
            let mut s = state.borrow_mut();
            let result = command(&mut s, &value);
            report(&ui, result);
            refresh(&ui, &s);
            ui.window().request_redraw();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_rules_trigger_only_when_enabled_and_manual_run_always_works() {
        let mut s = Desktop::new();
        command(&mut s, "add").unwrap();
        edit(&mut s, "target", "2").unwrap();
        edit(&mut s, "value", "GOAL").unwrap();
        let add = Action::Adjust {
            field: "home.score".into(),
            step: 1,
        };
        s.execute(add.clone()).unwrap();
        assert_ne!(s.document.project.values["round"], "GOAL");
        command(&mut s, "run").unwrap();
        assert_eq!(s.document.project.values["round"], "GOAL");
        s.document
            .project
            .values
            .insert("round".into(), "PLAY".into());
        edit(&mut s, "enabled", "true").unwrap();
        s.execute(add).unwrap();
        assert_eq!(s.document.project.values["round"], "GOAL");
        command(&mut s, "duplicate").unwrap();
        assert!(!selected(&s).unwrap().enabled);
        assert_eq!(s.document.project.cue_rules.len(), 2);
        command(&mut s, "delete").unwrap();
        s.document.undo().unwrap();
        assert_eq!(s.document.project.cue_rules.len(), 2);
        assert_eq!(s.document.project.values["home.score"], "14");
    }
}
