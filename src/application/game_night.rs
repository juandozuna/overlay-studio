//! Five-team Game Night project and layouts (WN-4).
use crate::domain::dice;
use crate::domain::controls::{Action, Control, ControllerLayout};
use crate::domain::fx::{self, FxStyle};
use crate::domain::sound;
use crate::domain::wheel::{self, hex, team_color_key};
use crate::model::{
    Animation, ClockSpec, Design, Element, Kind, Project, Protocol, Route, Team, TextAlign,
    VERSION,
};
use std::collections::BTreeMap;

const COLORS: [[u8; 3]; 5] = [
    [0x1f, 0x6f, 0xd1],
    [0xd1, 0x48, 0x1f],
    [0x1f, 0xa5, 0x7a],
    [0x8a, 0x3f, 0xd1],
    [0xc9, 0x9a, 0x12],
];
const LIGHT: [u8; 4] = [245, 248, 255, 255];

struct Builder {
    style: FxStyle,
    next: u64,
    elements: Vec<Element>,
}
impl Builder {
    fn push(&mut self, name: String, kind: Kind, rect: [f32; 4], fill: [u8; 4]) -> &mut Element {
        let mut e = Element::new(self.next, kind);
        self.next += 1;
        e.name = name;
        [e.x, e.y, e.w, e.h] = rect;
        e.fill = fill;
        self.elements.push(e);
        self.elements.last_mut().expect("just pushed")
    }
    fn panel(&mut self, team: &Team, rect: [f32; 4], alpha: u8, radius: f32, suffix: &str) {
        let style = self.style;
        let e = self.push(
            format!("{} {suffix}", team.label),
            Kind::Rectangle,
            rect,
            [team.color[0], team.color[1], team.color[2], alpha],
        );
        e.fill_binding = team_color_key(team.id);
        e.radius = radius;
        e.fx_key = fx::team_prefix(team.id);
        e.fx_style = style;
    }
    fn text(&mut self, name: String, binding: &str, rect: [f32; 4], size: f32, align: TextAlign) -> &mut Element {
        let e = self.push(name, Kind::Text, rect, LIGHT);
        e.binding = binding.into();
        e.font_size = size;
        e.text_align = align;
        e
    }
    fn team_text(&mut self, team: &Team, name: String, binding: &str, rect: [f32; 4], size: f32, align: TextAlign) -> &mut Element {
        let (style, key) = (self.style, fx::team_prefix(team.id));
        let e = self.text(name, binding, rect, size, align);
        e.fx_key = key;
        e.fx_style = style;
        e
    }
    fn title_and_clock(&mut self) {
        self.text("Event title".into(), "event", [300., 35., 1320., 90.], 60., TextAlign::Center);
        self.text("Game clock".into(), "clock", [760., 940., 400., 100.], 74., TextAlign::Center);
    }
    fn chip(&mut self, team: &Team, [x, y, w, h]: [f32; 4], name_size: f32, score_size: f32) {
        self.panel(team, [x, y, w, h], 230, 16., "panel");
        let pad = h * 0.15;
        let name = self.team_text(team, format!("{} name", team.label), &team.name_key, [x + pad, y, w * 0.55 - pad, h], name_size, TextAlign::Left);
        name.animation = Animation::Slide;
        let score = self.team_text(team, format!("{} score", team.label), &team.score_key, [x + w * 0.55, y, w * 0.45 - pad, h], score_size, TextAlign::Right);
        score.animation = Animation::Pop;
    }
    fn finish(self, id: u64, name: &str) -> Design {
        Design { id, name: name.into(), width: 1920, height: 1080, elements: self.elements }
    }
}

fn full_grid(teams: &[Team], first: u64) -> Builder {
    let mut b = Builder { style: FxStyle::Flash, next: first, elements: vec![] };
    for (i, team) in teams.iter().enumerate() {
        let x = 40. + i as f32 * 368.;
        b.panel(team, [x, 170., 352., 700.], 235, 24., "panel");
        let name = b.team_text(team, format!("{} name", team.label), &team.name_key, [x + 16., 200., 320., 110.], 52., TextAlign::Center);
        name.animation = Animation::Slide;
        let score = b.team_text(team, format!("{} score", team.label), &team.score_key, [x + 16., 380., 320., 360.], 220., TextAlign::Center);
        score.animation = Animation::Pop;
    }
    b.title_and_clock();
    b
}
fn top_bar(teams: &[Team], first: u64) -> Builder {
    let mut b = Builder { style: FxStyle::Sweep, next: first, elements: vec![] };
    for (i, team) in teams.iter().enumerate() {
        b.chip(team, [24. + i as f32 * 374.4, 24., 362., 96.], 34., 56.);
    }
    b
}
fn bottom_strip(teams: &[Team], first: u64) -> Builder {
    let mut b = Builder { style: FxStyle::Wipe, next: first, elements: vec![] };
    for (i, team) in teams.iter().enumerate() {
        let x = 40. + i as f32 * 368.;
        b.chip(team, [x, 900., 352., 150.], 40., 72.);
        b.panel(team, [x, 900., 24., 150.], 255, 8., "tab");
    }
    b
}
fn side_stack(teams: &[Team], first: u64) -> Builder {
    let mut b = Builder { style: FxStyle::Shake, next: first, elements: vec![] };
    for (i, team) in teams.iter().enumerate() {
        let y = 120. + i as f32 * 164.;
        b.panel(team, [24., y, 360., 150.], 235, 18., "panel");
        b.team_text(team, format!("{} name", team.label), &team.name_key, [44., y + 8., 320., 50.], 36., TextAlign::Left).animation = Animation::Slide;
        b.team_text(team, format!("{} score", team.label), &team.score_key, [44., y + 58., 320., 84.], 80., TextAlign::Right).animation = Animation::Pop;
    }
    b
}
fn wheel_stage(teams: &[Team], first: u64) -> Builder {
    let mut b = Builder { style: FxStyle::Burst, next: first, elements: vec![] };
    b.push("Wheel".into(), Kind::Wheel, [580., 170., 760., 760.], [255; 4]).animation = Animation::Pop;
    b.text("Wheel winner".into(), wheel::WINNER, [480., 30., 960., 110.], 72., TextAlign::Center);
    for (i, team) in teams.iter().enumerate() {
        b.chip(team, [40. + i as f32 * 368., 950., 352., 100.], 32., 56.);
    }
    b
}
const CARD: [u8; 4] = [20, 22, 32, 235];
// TODO: switch to dice::DICE_COLOR once task A commits it
const DICE_COLOR_KEY: &str = "dice.color";
const CARD_LIGHT: [u8; 4] = [34, 38, 56, 235];

impl Builder {
    fn card(&mut self, name: &str, rect: [f32; 4], fill: [u8; 4], radius: f32) -> &mut Element {
        let e = self.push(name.into(), Kind::Rectangle, rect, fill);
        e.radius = radius;
        e
    }
}
fn dado_stage(teams: &[Team], first: u64) -> Builder {
    let mut b = Builder { style: FxStyle::Burst, next: first, elements: vec![] };
    b.card("Question card", [160., 30., 1600., 190.], CARD, 24.);
    b.text("Question".into(), dice::QUESTION_TEXT, [190., 40., 1540., 170.], 46., TextAlign::Center);
    b.push("Dice".into(), Kind::Dice, [700., 250., 520., 520.], [255; 4]).animation = Animation::Pop;
    // option rows are text only (values already read "A · text"): without conditional visibility an empty panel would still show
    // 34 pt wraps to ~4 lines in 180 px, ending at y=670 well above the rule banner
    for (i, key) in dice::OPTION_KEYS.iter().enumerate() {
        let x = if i % 2 == 0 { 60. } else { 1260. };
        let y = 290. + (i / 2) as f32 * 200.;
        b.text(format!("Option {}", ["A", "B", "C", "D"][i]), key, [x, y, 600., 180.], 34., TextAlign::Left);
    }
    b.card("Rule card", [520., 780., 880., 90.], CARD_LIGHT, 20.).fill_binding = DICE_COLOR_KEY.into();
    b.text("Dice rule".into(), dice::RULE, [540., 780., 840., 90.], 40., TextAlign::Center).fill = [20, 22, 32, 255];
    b.card("Status card", [520., 876., 880., 64.], [20, 22, 32, 200], 16.);
    b.text("Dice status".into(), dice::STATUS, [520., 876., 880., 64.], 44., TextAlign::Center).fill = [255, 77, 94, 255];
    b.card("Turn card", [40., 780., 460., 160.], CARD, 20.);
    b.text("Turn team".into(), dice::TURN_TEAM_NAME, [56., 786., 428., 50.], 30., TextAlign::Left).fill = [170, 180, 205, 255];
    b.text("Turn name".into(), dice::TURN_NAME, [56., 836., 428., 96.], 54., TextAlign::Left);
    b.card("Question clock card", [1420., 780., 460., 160.], CARD, 20.);
    b.text("Question clock".into(), dice::QUESTION_CLOCK_KEY, [1420., 780., 460., 160.], 110., TextAlign::Center);
    // deferred: the approved white outline on the current team's chip needs conditional visibility, not possible with static layers
    for (i, team) in teams.iter().enumerate() {
        b.chip(team, [40. + i as f32 * 368., 950., 352., 100.], 32., 56.);
    }
    b
}

fn controller(project: &mut Project) -> Result<(), String> {
    project.controller.controls.clear();
    let mut layout = ControllerLayout::for_game(project)?;
    let mut id = layout.controls.iter().map(|c| c.id).max().unwrap_or(0) + 1;
    let row = layout.controls.iter().map(|c| c.row + c.height).max().unwrap_or(0);
    let mut actions: Vec<(String, Action, bool)> = project
        .designs
        .iter()
        .enumerate()
        .map(|(design, d)| (format!("Show {}", d.name), Action::Take { route: 0, design }, false))
        .collect();
    actions.extend([
        ("Spin wheel".into(), Action::SpinWheel, false),
        ("Remove winner".into(), Action::RemoveWheelWinner, false),
        ("Restore wheel".into(), Action::RestoreWheel, false),
        ("Roll dice".into(), Action::RollDice, false),
        ("Next question".into(), Action::NextQuestion, false),
        ("Reveal answer".into(), Action::RevealAnswer, false),
        ("Start 30 s".into(), Action::StartQuestionTimer, false),
        ("Next participant".into(), Action::NextParticipant, true),
        ("+10 team".into(), Action::AwardCurrentTeam { points: dice::POINTS }, false),
        ("-10 team".into(), Action::AwardCurrentTeam { points: -dice::POINTS }, true),
    ]);
    for (n, (label, action, confirm)) in actions.into_iter().enumerate() {
        layout.controls.push(Control {
            id,
            label,
            col: (n % 3) as u8 * 4,
            row: row + (n / 3) as u8 * 2,
            width: 4,
            height: 2,
            color: [19, 95, 123],
            confirm,
            action,
        });
        id += 1;
    }
    layout.validate(project)?;
    project.controller = layout;
    Ok(())
}

impl Project {
    pub fn game_night() -> Self {
        let teams: Vec<Team> = COLORS
            .iter()
            .enumerate()
            .map(|(i, color)| Team {
                id: i as u64 + 1,
                label: format!("Team {}", i + 1),
                name_key: format!("team{}.name", i + 1),
                score_key: format!("team{}.score", i + 1),
                color: *color,
                steps: vec![-1, 1, 2, 3],
            })
            .collect();
        let mut values = BTreeMap::from([("event".to_string(), "GAME NIGHT".to_string()), ("clock".into(), "10:00".into())]);
        for team in &teams {
            values.insert(team.name_key.clone(), team.label.to_uppercase());
            values.insert(team.score_key.clone(), "0".into());
            values.insert(team_color_key(team.id), hex(team.color));
        }
        values.extend(
            [
                (fx::ADD_COLOR, "#22C55E"),
                (fx::SUB_COLOR, "#000000"),
                (fx::SECONDS, "1.2"),
                (sound::VOLUME, "0.6"),
                (sound::ADD, "builtin:ding"),
                (sound::SUB, "builtin:buzz"),
                (sound::WHEEL_SPIN, ""),
                (sound::WHEEL_LAND, ""),
                (sound::WHEEL_REMOVE, ""),
            ]
            .map(|(k, v)| (k.to_string(), v.to_string())),
        );
        crate::application::wheel::seed_defaults(&mut values);
        values.insert(wheel::WINNER.into(), String::new());
        values.insert(wheel::WINNER_INDEX.into(), String::new());
        for k in [DICE_COLOR_KEY, dice::STATUS, dice::RULE, dice::RESULT, dice::QUESTION_TEXT, dice::HOST_ANSWER, dice::HOST_REF, dice::REVEALED, dice::BANK, dice::BANK_USED, dice::BANK_FILTER] {
            values.insert(k.into(), String::new());
        }
        for k in dice::OPTION_KEYS {
            values.insert(k.into(), String::new());
        }
        values.insert(dice::ROLL_SECONDS.into(), "2".into());
        values.insert(dice::TURN_TEAM.into(), teams[0].id.to_string());
        values.insert(dice::TURN_NAME.into(), "Participante 1".into());
        values.insert(dice::TURN_TEAM_NAME.into(), teams[0].label.to_uppercase());
        values.insert(dice::QUESTION_CLOCK_KEY.into(), crate::model::clock_text(dice::QUESTION_SECONDS));
        for team in &teams {
            values.insert(dice::participants_key(team.id), "Participante 1\nParticipante 2\nParticipante 3".into());
        }
        let builders = [
            ("1 Full grid", full_grid as fn(&[Team], u64) -> Builder),
            ("2 Top bar", top_bar),
            ("3 Bottom strip", bottom_strip),
            ("4 Side stack", side_stack),
            ("5 Wheel stage", wheel_stage),
            ("6 Dado", dado_stage),
        ];
        let mut designs = vec![];
        let mut next = 1;
        for (i, (name, build)) in builders.into_iter().enumerate() {
            let b = build(&teams, next);
            next = b.next;
            designs.push(b.finish(i as u64 + 1, name));
        }
        let route = |id, name: &str, protocol| Route { id, name: name.into(), design: 0, design_id: 1, protocol, fps: 30 };
        let mut p = Self {
            version: VERSION,
            name: "Game Night".into(),
            designs,
            assets: vec![],
            motions: vec![],
            sounds: vec![],
            values,
            routes: vec![
                route(1, "Overlay Studio • Broadcast", Protocol::Syphon),
                route(2, "Overlay Studio • Venue", Protocol::Ndi),
            ],
            clock_seconds: 600.,
            countdown: true,
            teams,
            clocks: vec![ClockSpec {
                id: 1,
                name: "Game clock".into(),
                value_key: "clock".into(),
                seconds: 600.,
                reset_seconds: 600.,
                countdown: true,
            },
            ClockSpec {
                id: 2,
                name: "Question clock".into(),
                value_key: dice::QUESTION_CLOCK_KEY.into(),
                seconds: dice::QUESTION_SECONDS,
                reset_seconds: dice::QUESTION_SECONDS,
                countdown: true,
            }],
            cue_rules: vec![],
            controller: Default::default(),
            stable_references: Default::default(),
        };
        controller(&mut p).expect("game night controller fits");
        p.sync_stable_references().expect("game night references are valid");
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn project_is_valid_with_five_teams_and_named_layouts() {
        let p = Project::game_night();
        p.validate().unwrap();
        assert_eq!(p.teams.len(), 5);
        let names: Vec<_> = p.designs.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["1 Full grid", "2 Top bar", "3 Bottom strip", "4 Side stack", "5 Wheel stage", "6 Dado"]);
        assert_eq!(p.routes.len(), 2);
        assert!(p.routes.iter().all(|r| r.design_id == p.designs[0].id));
    }
    #[test]
    fn every_element_is_inside_the_canvas() {
        for d in &Project::game_night().designs {
            assert!(
                d.elements.iter().all(|e| e.x >= 0. && e.y >= 0. && e.x + e.w <= 1920. && e.y + e.h <= 1080.),
                "{} leaves the canvas",
                d.name
            );
        }
    }
    #[test]
    fn team_panels_bind_existing_colour_values() {
        let p = Project::game_night();
        for d in &p.designs {
            for team in &p.teams {
                let panel = d.elements.iter().find(|e| e.name == format!("{} panel", team.label)).expect("panel");
                assert_eq!(panel.fill_binding, team_color_key(team.id));
                assert!(p.values.contains_key(&panel.fill_binding));
            }
        }
    }
    #[test]
    fn team_layers_carry_their_layout_effect_and_others_none() {
        let p = Project::game_night();
        let styles = [FxStyle::Flash, FxStyle::Sweep, FxStyle::Wipe, FxStyle::Shake, FxStyle::Burst, FxStyle::Burst];
        for (d, style) in p.designs.iter().zip(styles) {
            for e in &d.elements {
                let team = p.teams.iter().find(|t| e.name.starts_with(&format!("{} ", t.label)));
                match team {
                    Some(t) => {
                        assert_eq!(e.fx_key, fx::team_prefix(t.id), "{} / {}", d.name, e.name);
                        assert_eq!(e.fx_style, style, "{} / {}", d.name, e.name);
                    }
                    None => assert!((e.fx_key.as_str(), e.fx_style) == ("", FxStyle::None), "{} / {}", d.name, e.name),
                }
            }
            assert!(d.elements.iter().any(|e| !e.fx_key.is_empty()), "{} has no team layers", d.name);
        }
    }
    #[test]
    fn seeded_fx_and_sound_values_parse() {
        let v = Project::game_night().values;
        assert_eq!(fx::FxConfig::read(&v), fx::FxConfig { add_color: [0x22, 0xC5, 0x5E], sub_color: [0, 0, 0], seconds: 1.2 });
        assert_eq!(sound::SoundRef::parse(&v[sound::ADD]), Some(sound::SoundRef::Builtin("ding".into())));
        assert_eq!(sound::SoundRef::parse(&v[sound::SUB]), Some(sound::SoundRef::Builtin("buzz".into())));
        assert_eq!(v[sound::VOLUME], "0.6");
        for k in [sound::WHEEL_SPIN, sound::WHEEL_LAND, sound::WHEEL_REMOVE] {
            assert_eq!(v[k], "");
        }
    }
    #[test]
    fn controller_validates_and_covers_layouts_and_wheel() {
        let p = Project::game_night();
        p.controller.validate(&p).unwrap();
        let takes = p.controller.controls.iter().filter(|c| matches!(c.action, Action::Take { .. })).count();
        assert_eq!(takes, 6);
        assert!(p.controller.controls.iter().any(|c| c.action == Action::SpinWheel));
        for action in [Action::RollDice, Action::NextQuestion, Action::RevealAnswer, Action::StartQuestionTimer, Action::NextParticipant, Action::AwardCurrentTeam { points: 10 }, Action::AwardCurrentTeam { points: -10 }] {
            assert!(p.controller.controls.iter().any(|c| c.action == action), "{action:?}");
        }
    }
    #[test]
    fn dado_design_has_one_die_five_chips_and_seeded_values() {
        let p = Project::game_night();
        let d = &p.designs[5];
        assert_eq!(d.name, "6 Dado");
        assert_eq!(d.elements.iter().filter(|e| e.kind == Kind::Dice).count(), 1);
        assert_eq!(d.elements.iter().filter(|e| e.name.ends_with(" panel") && e.name.starts_with("Team ")).count(), 5);
        for e in d.elements.iter().filter(|e| e.kind == Kind::Text && !e.binding.is_empty()) {
            assert!(p.values.contains_key(&e.binding) || e.binding == "event", "{}", e.name);
        }
        assert_eq!(p.values[dice::participants_key(5).as_str()].lines().count(), 3);
        let clock = p.clocks.iter().find(|c| c.value_key == dice::QUESTION_CLOCK_KEY).expect("question clock");
        assert_eq!((clock.seconds, clock.countdown), (30., true));
        p.controller.validate(&p).unwrap();
    }
}
