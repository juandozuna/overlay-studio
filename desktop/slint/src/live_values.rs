use crate::{Desktop, LiveValueRow, StudioWindow, refresh, report, view_models};
use overlay_studio::{domain::controls::Action, model::Project};
use slint::{ComponentHandle, Model};
use std::{cell::RefCell, rc::Rc};

fn custom(p: &Project, key: &str) -> bool {
    !p.teams
        .iter()
        .any(|t| t.score_key == key || t.name_key == key)
        && !p.clocks.iter().any(|c| c.value_key == key)
}

pub fn refresh_panel(ui: &StudioWindow, s: &Desktop) {
    let old = ui.get_live_values();
    let generation = s.document.generation.to_string();
    let rows = s
        .document
        .project
        .values
        .iter()
        .filter(|(key, _)| custom(&s.document.project, key))
        .map(|(key, value)| {
            let previous = old
                .iter()
                .find(|r| r.key.as_str() == key && r.generation.as_str() == generation);
            let draft = previous
                .filter(|r| r.draft != r.value)
                .map(|r| r.draft.to_string())
                .unwrap_or_else(|| value.clone());
            let label = match key.as_str() {
                "event" => "Event name",
                "round" => "Round",
                _ => key,
            };
            LiveValueRow {
                key: key.clone().into(),
                label: label.into(),
                value: value.clone().into(),
                draft: draft.into(),
                generation: generation.clone().into(),
            }
        })
        .collect();
    ui.set_live_values(view_models::sync(old, rows));
}

fn apply(s: &mut Desktop, key: &str, value: &str) -> Result<(), String> {
    if !custom(&s.document.project, key) {
        return Err("Use team or clock controls for this field".into());
    }
    if value.len() > 4096 {
        return Err("Live text must be at most 4096 bytes".into());
    }
    s.execute(Action::SetValue {
        field: key.into(),
        value: value.into(),
    })
}

pub fn connect(ui: &StudioWindow, state: Rc<RefCell<Desktop>>) {
    let shared = state.clone();
    let weak = ui.as_weak();
    ui.on_live_field_command(move |command, key| {
        let Some(ui) = weak.upgrade() else {
            return;
        };
        let mut s = shared.borrow_mut();
        let result = s.configure("value", 0, &command, &key);
        if result.is_ok() && command == "add" {
            ui.set_new_field_name("".into());
        }
        report(&ui, result);
        refresh(&ui, &s);
        ui.window().request_redraw();
    });
    let weak = ui.as_weak();
    ui.on_live_value_apply(move |key, value| {
        let Some(ui) = weak.upgrade() else {
            return;
        };
        let mut s = state.borrow_mut();
        let result = apply(&mut s, &key, &value);
        report(&ui, result);
        refresh(&ui, &s);
        ui.window().request_redraw();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn custom_text_updates_without_stopping_clocks_or_changing_scores() {
        let mut s = Desktop::two_team_demo();
        s.execute(Action::ClockToggleAt { clock: 0 }).unwrap();
        apply(&mut s, "event", "Church games").unwrap();
        assert_eq!(s.document.project.values["event"], "Church games");
        assert_eq!(s.document.project.values["home.score"], "12");
        assert!(s.game.clocks[0].running);
        assert!(apply(&mut s, "clock", "bad time").is_err());
        assert!(apply(&mut s, "missing", "text").is_err());
        assert!(apply(&mut s, "round", &"x".repeat(4097)).is_err());
    }
}
