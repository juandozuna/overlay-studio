//! Remote presentation adapter; server and command validation remain in the shared engine.
use crate::{Desktop, RemoteRow, StudioWindow, refresh, report, strings};
use overlay_studio::{
    application::style_values,
    domain::controls::{Action, Control, ControllerLayout},
    remote::{self, Remote, Snapshot},
};
use slint::ComponentHandle;
use std::{cell::RefCell, collections::VecDeque, rc::Rc};

#[derive(Default)]
pub struct Session {
    pub server: Option<Remote>,
    pub selected: Option<u64>,
    seen: VecDeque<(String, Result<(), String>)>,
}
impl Session {
    pub fn stop(&mut self) {
        self.server = None;
        self.seen.clear();
    }
}
fn snapshot(s: &Desktop) -> Snapshot {
    Snapshot::from_project(
        &s.document.project,
        &s.game.clocks.iter().map(|c| c.running).collect::<Vec<_>>(),
        s.document
            .project
            .routes
            .iter()
            .enumerate()
            .map(|(i, r)| {
                format!(
                    "{} · {}",
                    r.name,
                    if s.outputs.is_running(i) {
                        "Running"
                    } else {
                        "Stopped"
                    }
                )
            })
            .collect(),
    )
}
pub fn poll(s: &mut Desktop) -> bool {
    let commands = s
        .remote
        .server
        .as_ref()
        .map(|r| r.commands.try_iter().collect::<Vec<_>>())
        .unwrap_or_default();
    let changed = !commands.is_empty();
    for c in commands {
        let result =
            if let Some((_, result)) = s.remote.seen.iter().find(|(id, _)| id == &c.request_id) {
                result.clone()
            } else {
                s.document
                    .project
                    .controller
                    .controls
                    .iter()
                    .find(|b| b.id == c.button)
                    .map(|b| b.action.clone())
                    .ok_or("Button no longer exists".into())
                    .and_then(|a| s.execute(a))
            };
        if !s.remote.seen.iter().any(|(id, _)| id == &c.request_id) {
            s.remote.seen.push_back((c.request_id, result.clone()));
            if s.remote.seen.len() > 256 {
                s.remote.seen.pop_front();
            }
        }
        let _ = c.reply.send(result);
    }
    if let Some(r) = &s.remote.server
        && let Ok(mut state) = r.snapshot.lock()
    {
        *state = snapshot(s);
    }
    changed
}
fn row(c: &Control, s: &Desktop) -> RemoteRow {
    let p = &s.document.project;
    let mut r = RemoteRow {
        id: c.id.to_string().into(),
        label: c.label.clone().into(),
        col: c.col as i32,
        row: c.row as i32,
        span: c.width as i32,
        height: c.height as i32,
        tint: slint::Color::from_rgb_u8(c.color[0], c.color[1], c.color[2]),
        color_text: style_values::color_text([c.color[0], c.color[1], c.color[2], 255]).into(),
        confirm: c.confirm,
        ..Default::default()
    };
    match &c.action {
        Action::SpinWheel => r.action = 9,
        Action::RemoveWheelWinner => r.action = 10,
        Action::RestoreWheel => r.action = 11,
        Action::RollDice => r.action = 12,
        Action::NextQuestion => r.action = 13,
        Action::RevealAnswer => r.action = 14,
        Action::NextParticipant => r.action = 15,
        Action::StartQuestionTimer => r.action = 16,
        Action::AwardCurrentTeam { points } => {
            r.action = 17;
            r.amount = points.to_string().into();
        }
        Action::Adjust { field, step } => {
            r.action = 0;
            r.field = p.values.keys().position(|x| x == field).unwrap_or(0) as i32;
            r.amount = step.to_string().into();
        }
        Action::ClockToggle => r.action = 1,
        Action::ClockToggleAt { clock } => {
            r.action = 1;
            r.clock = *clock as i32;
        }
        Action::ClockReset => r.action = 2,
        Action::ClockResetAt { clock } => {
            r.action = 2;
            r.clock = *clock as i32;
        }
        Action::ClockAdjust { clock, seconds } => {
            r.action = 3;
            r.clock = *clock as i32;
            r.amount = seconds.to_string().into();
        }
        Action::ToggleOverlay { route } => {
            r.action = 4;
            r.output = *route as i32;
        }
        Action::Take { route, design } => {
            r.action = 5;
            r.output = *route as i32;
            r.design = *design as i32;
        }
        Action::Cue { route } => {
            r.action = 6;
            r.output = *route as i32;
        }
        Action::SetValue { field, value } => {
            r.action = 7;
            r.field = p.values.keys().position(|x| x == field).unwrap_or(0) as i32;
            r.value = value.clone().into();
        }
        Action::RunCue { id } => {
            r.action = 8;
            r.cue = p.cue_rules.iter().position(|c| c.id == *id).unwrap_or(0) as i32;
        }
    }
    r
}
pub fn refresh_panel(ui: &StudioWindow, s: &Desktop) {
    let p = &s.document.project;
    ui.set_remote_name(p.controller.name.clone().into());
    ui.set_remote_fields(strings(
        p.values
            .keys()
            .map(|key| crate::value_labels::label(p, key)),
    ));
    ui.set_remote_clocks(strings(p.clocks.iter().map(|c| c.name.clone())));
    ui.set_remote_cues(strings(p.cue_rules.iter().map(|c| c.name.clone())));
    ui.set_remote_controls(crate::view_models::sync(
        ui.get_remote_controls(),
        p.controller.controls.iter().map(|c| row(c, s)).collect(),
    ));
    ui.set_remote_current(
        s.remote
            .selected
            .and_then(|id| p.controller.controls.iter().find(|c| c.id == id))
            .map(|c| row(c, s))
            .unwrap_or_default(),
    );
    ui.set_remote_running(s.remote.server.is_some());
    ui.set_remote_address(
        s.remote
            .server
            .as_ref()
            .map(|r| r.address.clone())
            .unwrap_or_default()
            .into(),
    );
    if s.remote.server.is_none() {
        ui.set_remote_qr(slint::Image::default());
    }
}
fn action(r: &RemoteRow, s: &Desktop) -> Result<Action, String> {
    let index = |i: i32| usize::try_from(i).map_err(|_| "Select a valid action target".to_string());
    let field = || {
        s.document
            .project
            .values
            .keys()
            .nth(index(r.field)?)
            .cloned()
            .ok_or_else(|| "Select a live field".to_string())
    };
    let amount = || {
        r.amount
            .parse::<i64>()
            .map_err(|_| "Enter a signed whole-number amount".to_string())
    };
    Ok(match r.action {
        0 => Action::Adjust {
            field: field()?,
            step: amount()?,
        },
        1 => Action::ClockToggleAt {
            clock: index(r.clock)?,
        },
        2 => Action::ClockResetAt {
            clock: index(r.clock)?,
        },
        3 => Action::ClockAdjust {
            clock: index(r.clock)?,
            seconds: amount()?,
        },
        4 => Action::ToggleOverlay {
            route: index(r.output)?,
        },
        5 => Action::Take {
            route: index(r.output)?,
            design: index(r.design)?,
        },
        6 => Action::Cue {
            route: index(r.output)?,
        },
        7 => Action::SetValue {
            field: field()?,
            value: r.value.to_string(),
        },
        8 => Action::RunCue {
            id: s
                .document
                .project
                .cue_rules
                .get(index(r.cue)?)
                .ok_or("Create an event cue first")?
                .id,
        },
        9 => Action::SpinWheel,
        10 => Action::RemoveWheelWinner,
        11 => Action::RestoreWheel,
        12 => Action::RollDice,
        13 => Action::NextQuestion,
        14 => Action::RevealAnswer,
        15 => Action::NextParticipant,
        16 => Action::StartQuestionTimer,
        17 => Action::AwardCurrentTeam { points: amount()? },
        _ => return Err("Select an action".into()),
    })
}
fn edit(ui: &StudioWindow, s: &mut Desktop, key: &str, value: &str) -> Result<(), String> {
    let mut layout = s.document.project.controller.clone();
    if key == "layout" {
        layout.name = value.trim().into();
        return s.document.set_controller(layout);
    }
    let id = s.remote.selected.ok_or("Select a remote button")?;
    let c = layout
        .controls
        .iter_mut()
        .find(|c| c.id == id)
        .ok_or("Button no longer exists")?;
    match key {
        "label" => c.label = value.into(),
        "col" => c.col = value.parse().map_err(|_| "Enter a column from 0 to 11")?,
        "row" => c.row = value.parse().map_err(|_| "Enter a row from 0 to 47")?,
        "width" => c.width = value.parse().map_err(|_| "Enter a width from 1 to 12")?,
        "height" => c.height = value.parse().map_err(|_| "Enter a height from 1 to 8")?,
        "color" => {
            let rgb = style_values::parse_color(value)?;
            c.color = [rgb[0], rgb[1], rgb[2]];
        }
        "confirm" => c.confirm = value == "true",
        _ => {
            let mut r = ui.get_remote_current();
            if key == "action" {
                r.action = value.parse().map_err(|_| "Invalid action")?;
                if r.amount.is_empty() {
                    r.amount = "1".into();
                }
            }
            c.action = action(&r, s)?;
            if key == "action" && matches!(r.action, 2 | 10) {
                c.confirm = true;
            }
        }
    }
    s.document.set_controller(layout)
}
fn add(s: &mut Desktop, duplicate: bool) -> Result<(), String> {
    let mut layout = s.document.project.controller.clone();
    let id = layout
        .controls
        .iter()
        .map(|c| c.id)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .filter(|id| *id <= overlay_studio::model::MAX_ID)
        .ok_or("Button ID limit reached")?;
    let row = layout
        .controls
        .iter()
        .map(|c| c.row as u16 + c.height as u16)
        .max()
        .unwrap_or(0);
    if row + 2 > 48 {
        return Err(
            "No free row below the controls. Move existing buttons or delete one first.".into(),
        );
    }
    let mut c = if duplicate {
        layout
            .controls
            .iter()
            .find(|c| Some(c.id) == s.remote.selected)
            .cloned()
            .ok_or("Select a button to duplicate")?
    } else {
        Control {
            id,
            label: "New button".into(),
            col: 0,
            row: 0,
            width: 6,
            height: 2,
            color: [19, 95, 123],
            confirm: false,
            action: Action::Adjust {
                field: s.document.project.teams[0].score_key.clone(),
                step: 1,
            },
        }
    };
    c.id = id;
    c.col = 0;
    c.row = row as u8;
    if row + c.height as u16 > 48 {
        return Err("The duplicate would exceed the controller grid".into());
    }
    if duplicate {
        c.label = format!("{} copy", c.label);
    }
    layout.controls.push(c);
    s.document.set_controller(layout)?;
    s.remote.selected = Some(id);
    Ok(())
}
pub fn connect(ui: &StudioWindow, state: Rc<RefCell<Desktop>>) {
    let addresses = remote::addresses();
    ui.set_remote_ip(
        addresses
            .first()
            .cloned()
            .unwrap_or_else(|| "127.0.0.1".into())
            .into(),
    );
    ui.set_remote_addresses(strings(addresses));
    let shared = state.clone();
    let weak = ui.as_weak();
    ui.on_remote_select(move |id| {
        if let Some(ui) = weak.upgrade() {
            let mut s = shared.borrow_mut();
            s.remote.selected = id.parse().ok();
            refresh_panel(&ui, &s);
        }
    });
    let shared = state.clone();
    let weak = ui.as_weak();
    ui.on_remote_edit(move |key, value| {
        if let Some(ui) = weak.upgrade() {
            let mut s = shared.borrow_mut();
            let result = edit(&ui, &mut s, &key, &value);
            report(&ui, result);
            refresh(&ui, &s);
        }
    });
    let shared = state.clone();
    let weak = ui.as_weak();
    ui.on_remote_place(move |id, col, row| {
        if let Some(ui) = weak.upgrade() {
            let mut s = shared.borrow_mut();
            let mut layout = s.document.project.controller.clone();
            if let Some(c) = layout.controls.iter_mut().find(|c| id == c.id.to_string()) {
                c.col = col.clamp(0, 12 - c.width as i32) as u8;
                c.row = row.clamp(0, 48 - c.height as i32) as u8;
                let result = s.document.set_controller(layout);
                report(&ui, result);
            }
            refresh(&ui, &s);
        }
    });
    let weak = ui.as_weak();
    ui.on_remote_command(move |command| {
        if let Some(ui) = weak.upgrade() {
            let mut s = state.borrow_mut();
            let result = (|| {
                match command.as_str() {
                    "start" => {
                        let port = ui
                            .get_remote_port()
                            .parse::<u16>()
                            .map_err(|_| "Enter a port from 1024 to 65535")?;
                        if port < 1024 {
                            return Err("Choose a port from 1024 to 65535".into());
                        }
                        let server = Remote::start(
                            &ui.get_remote_ip(),
                            port,
                            snapshot(&s),
                            egui::Context::default(),
                        )?;
                        let qr = remote::qr_image(&server.url)?;
                        let bytes = qr
                            .pixels
                            .iter()
                            .flat_map(|p| p.to_array())
                            .collect::<Vec<_>>();
                        let buffer =
                            slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
                                &bytes,
                                qr.size[0] as u32,
                                qr.size[1] as u32,
                            );
                        ui.set_remote_qr(slint::Image::from_rgba8(buffer));
                        s.remote.server = Some(server);
                    }
                    "stop" => s.remote.stop(),
                    "copy-link" => {
                        let url = s
                            .remote
                            .server
                            .as_ref()
                            .ok_or("Start remote control first")?
                            .url
                            .clone();
                        arboard::Clipboard::new()
                            .and_then(|mut c| c.set_text(url))
                            .map_err(|e| e.to_string())?;
                    }
                    "add" => add(&mut s, false)?,
                    "duplicate" => add(&mut s, true)?,
                    "teams" => {
                        let layout = ControllerLayout::for_game(&s.document.project)?;
                        s.document.set_controller(layout)?;
                        s.remote.selected = None;
                    }
                    "delete" => {
                        let id = s.remote.selected.ok_or("Select a button")?;
                        let mut layout = s.document.project.controller.clone();
                        layout.controls.retain(|c| c.id != id);
                        s.document.set_controller(layout)?;
                        s.remote.selected = None;
                    }
                    _ => return Err("Unknown remote action".into()),
                }
                Ok(())
            })();
            report(&ui, result);
            refresh(&ui, &s);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use overlay_studio::domain::controls::{Control, ControllerLayout};

    fn control(action: Action) -> Control {
        let project = overlay_studio::model::Project::game_night();
        let mut c = project.controller.controls[0].clone();
        c.action = action;
        c
    }

    #[test]
    fn el_dado_actions_round_trip_through_remote_ids_12_to_17() {
        let s = Desktop::two_team_demo();
        let table = [
            (Action::RollDice, 12, ""),
            (Action::NextQuestion, 13, ""),
            (Action::RevealAnswer, 14, ""),
            (Action::NextParticipant, 15, ""),
            (Action::StartQuestionTimer, 16, ""),
            (Action::AwardCurrentTeam { points: 10 }, 17, "10"),
            (Action::AwardCurrentTeam { points: -10 }, 17, "-10"),
            (Action::AwardCurrentTeam { points: 0 }, 17, "0"),
        ];
        for (a, id, amount) in &table {
            let r = row(&control(a.clone()), &s);
            assert_eq!((r.action, r.amount.as_str()), (*id, *amount), "{a:?}");
            assert_eq!(action(&r, &s).as_ref(), Ok(a), "{a:?}");
        }
        // the ids are distinct from every older action and from each other
        for older in [Action::SpinWheel, Action::RemoveWheelWinner, Action::RestoreWheel, Action::ClockToggle, Action::ClockReset] {
            assert!(!(12..=17).contains(&row(&control(older), &s).action));
        }
        assert_eq!(table.iter().map(|(_, id, _)| *id).collect::<std::collections::BTreeSet<_>>().len(), 6);
    }

    #[test]
    fn el_dado_ids_reject_a_bad_amount_and_unknown_ids() {
        let s = Desktop::two_team_demo();
        let mut r = row(&control(Action::AwardCurrentTeam { points: 10 }), &s);
        r.amount = "abc".into();
        assert_eq!(action(&r, &s), Err("Enter a signed whole-number amount".to_string()));
        r.amount = "".into();
        assert!(action(&r, &s).is_err());
        r.action = 18;
        assert_eq!(action(&r, &s), Err("Select an action".to_string()));
        // actions without an amount ignore a stray one
        let mut r = row(&control(Action::RollDice), &s);
        r.amount = "junk".into();
        assert_eq!(action(&r, &s), Ok(Action::RollDice));
    }

    #[test]
    fn every_el_dado_control_of_the_game_night_layout_survives_the_remote_row() {
        let s = Desktop::two_team_demo();
        let layout: ControllerLayout = overlay_studio::model::Project::game_night().controller;
        let mut seen = 0;
        for c in &layout.controls {
            let r = row(c, &s);
            if (12..=17).contains(&r.action) {
                seen += 1;
                assert_eq!(action(&r, &s).as_ref(), Ok(&c.action), "{}", c.label);
            }
        }
        assert!(seen >= 7, "only {seen} El Dado buttons found");
    }
}
