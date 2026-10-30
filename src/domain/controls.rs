use crate::model::Project;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub enum Action {
    Adjust { field: String, step: i64 },
    ClockToggle,
    ClockReset,
    ClockToggleAt { clock: usize },
    ClockResetAt { clock: usize },
    ClockAdjust { clock: usize, seconds: i64 },
    ToggleOverlay { route: usize },
    Take { route: usize, design: usize },
    Cue { route: usize },
    RunCue { id: u64 },
    SetValue { field: String, value: String },
    SpinWheel,
    RemoveWheelWinner,
    RestoreWheel,
    // El Dado
    RollDice,
    NextQuestion,
    RevealAnswer,
    NextParticipant,
    StartQuestionTimer,
    /// +/- points for the team whose participant is up.
    AwardCurrentTeam { points: i64 },
}
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct Control {
    pub id: u64,
    pub label: String,
    pub col: u8,
    pub row: u8,
    pub width: u8,
    pub height: u8,
    pub color: [u8; 3],
    pub confirm: bool,
    pub action: Action,
}
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct ControllerLayout {
    pub name: String,
    pub controls: Vec<Control>,
}
impl Default for ControllerLayout {
    fn default() -> Self {
        let controls = vec![
            (
                "Eagles +1",
                0,
                0,
                Action::Adjust {
                    field: "home.score".into(),
                    step: 1,
                },
            ),
            (
                "Lions +1",
                6,
                0,
                Action::Adjust {
                    field: "away.score".into(),
                    step: 1,
                },
            ),
            (
                "Eagles −1",
                0,
                2,
                Action::Adjust {
                    field: "home.score".into(),
                    step: -1,
                },
            ),
            (
                "Lions −1",
                6,
                2,
                Action::Adjust {
                    field: "away.score".into(),
                    step: -1,
                },
            ),
            ("Start / pause", 0, 4, Action::ClockToggle),
            (
                "Show / hide overlay",
                6,
                4,
                Action::ToggleOverlay { route: 0 },
            ),
            ("Play entrance", 0, 6, Action::Cue { route: 0 }),
            (
                "Show game night",
                6,
                6,
                Action::Take {
                    route: 0,
                    design: 2,
                },
            ),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (label, col, row, action))| Control {
            id: i as u64 + 1,
            label: label.into(),
            col,
            row,
            width: 6,
            height: 2,
            color: if i % 2 == 0 {
                [19, 95, 123]
            } else {
                [134, 72, 34]
            },
            confirm: false,
            action,
        })
        .collect();
        Self {
            name: "Game controls".into(),
            controls,
        }
    }
}
impl ControllerLayout {
    pub fn for_game(p: &Project) -> Result<Self, String> {
        let score_keys: HashSet<_> = p.teams.iter().map(|t| &t.score_key).collect();
        let mut extras: Vec<_> = p.controller.controls.iter().filter(|control| {
            !matches!(&control.action, Action::Adjust { field, .. } if score_keys.contains(field))
        }).cloned().collect();
        let needed = p.teams.iter().map(|t| t.steps.len()).sum::<usize>() + extras.len();
        if needed > 100 {
            return Err(
                "This control set exceeds 100 buttons. Reduce the scoring steps first.".into(),
            );
        }
        let mut id = p
            .controller
            .controls
            .iter()
            .map(|c| c.id)
            .max()
            .unwrap_or(0)
            + 1;
        let mut controls = vec![];
        let columns = p.teams.len().clamp(1, 3);
        let width = (12 / columns) as u8;
        let mut row = 0_u8;
        for chunk in p.teams.chunks(columns) {
            let height = chunk.iter().map(|t| t.steps.len()).max().unwrap_or(0) as u8 * 2;
            for (column, team) in chunk.iter().enumerate() {
                for (step_index, step) in team.steps.iter().enumerate() {
                    controls.push(Control {
                        id,
                        label: format!("{} {step:+}", team.label),
                        col: column as u8 * width,
                        row: row + step_index as u8 * 2,
                        width,
                        height: 2,
                        color: team.color,
                        confirm: false,
                        action: Action::Adjust {
                            field: team.score_key.clone(),
                            step: *step,
                        },
                    });
                    id += 1;
                }
            }
            row = row.checked_add(height).ok_or("Controller is too tall")?;
        }
        // Preserve custom actions and their sizes, packing them below all team controls.
        let mut column = 0_u8;
        let mut line_height = 0_u8;
        for extra in &mut extras {
            if column + extra.width > 12 {
                row += line_height;
                column = 0;
                line_height = 0;
            }
            extra.col = column;
            extra.row = row;
            column += extra.width;
            line_height = line_height.max(extra.height);
        }
        controls.extend(extras);
        let result = Self {
            name: p.controller.name.clone(),
            controls,
        };
        result.validate(p)?;
        Ok(result)
    }
    pub fn validate(&self, p: &Project) -> Result<(), String> {
        if self.controls.len() > 100 {
            return Err("Maximum 100 remote buttons".into());
        }
        let mut ids = HashSet::new();
        for b in &self.controls {
            if b.id > crate::model::MAX_ID
                || !ids.insert(b.id)
                || b.width == 0
                || b.height == 0
                || b.col as u16 + b.width as u16 > 12
                || b.row as u16 + b.height as u16 > 48
            {
                return Err("Invalid remote button placement".into());
            }
            match &b.action {
                Action::RunCue { id } if !p.cue_rules.iter().any(|cue| cue.id == *id) => {
                    return Err("Remote button references missing cue".into());
                }
                Action::Adjust { field, .. } | Action::SetValue { field, .. }
                    if !p.values.contains_key(field) =>
                {
                    return Err(format!("Remote button references missing field {field}"));
                }
                Action::Take { route, design }
                    if *route >= p.routes.len() || *design >= p.designs.len() =>
                {
                    return Err("Remote button references missing output/design".into());
                }
                Action::ToggleOverlay { route } | Action::Cue { route }
                    if *route >= p.routes.len() =>
                {
                    return Err("Remote button references missing output".into());
                }
                Action::ClockToggleAt { clock }
                | Action::ClockResetAt { clock }
                | Action::ClockAdjust { clock, .. }
                    if *clock >= p.clocks.len() =>
                {
                    return Err("Remote button references missing clock".into());
                }
                _ => {}
            }
        }
        Ok(())
    }
}
