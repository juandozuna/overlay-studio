//! El Dado flows that span modules: what reaches the audience canvas, old project files and save/load.
use overlay_studio::{
    application::{
        dice::{self, bank_status, ensure_defaults, import_bank, turn_view},
        game_session::GameSession,
    },
    domain::{controls::Action, dice as keys},
    model::{Kind, Project},
};
use std::path::PathBuf;

const TEMPLATE: &str = include_str!("../examples/El Dado Questions Template.csv");
const COMMUNITY_CUP: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/Community Cup.overlay");

fn scratch_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("overlay-studio-dado-flow-{}-{}", std::process::id(), rand::random::<u64>()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Every key an element reads from `Project::values`.
fn bound_keys(e: &overlay_studio::model::Element) -> Vec<&str> {
    [e.binding.as_str(), e.fill_binding.as_str()].into_iter().filter(|k| !k.is_empty()).collect()
}

#[test]
fn host_keys_are_never_bound_on_any_canvas() {
    let p = Project::game_night();
    let forbidden = ["dado.host.", keys::HOST_ANSWER, keys::HOST_REF, keys::REVEALED, keys::BANK, keys::BANK_USED, keys::BANK_FILTER, keys::BANK_CURRENT, keys::AWARDED];
    let mut bound_anywhere = std::collections::BTreeSet::new();
    for design in &p.designs {
        for element in &design.elements {
            for key in bound_keys(element) {
                bound_anywhere.insert(key.to_string());
                for bad in forbidden {
                    assert!(!key.starts_with(bad) && key != bad, "{} / {} is bound to host-only key {key}", design.name, element.name);
                }
                assert!(!key.starts_with("dado.host") && !key.starts_with("dado.revealed") && !key.starts_with("dado.bank") && !key.starts_with("dado.awarded"), "{key}");
            }
            // static text is never a substitute for a binding, but a host key typed into it would still be on air
            for bad in ["dado.host", "dado.revealed", "dado.bank", "dado.awarded"] {
                assert!(!element.text.contains(bad), "{} / {} text mentions {bad}", design.name, element.name);
            }
        }
    }
    // positive control: the audience keys ARE bound, so the scan above looks at real bindings
    for key in [keys::QUESTION_TEXT, "question.a", "question.b", "question.c", "question.d"] {
        assert!(bound_anywhere.contains(key), "{key} is not bound on any layer");
    }
    // and a secret in the host answer never shows up in any resolved text
    let mut values = p.values.clone();
    values.insert(keys::HOST_ANSWER.into(), "SECRET-ANSWER".into());
    values.insert(keys::HOST_REF.into(), "SECRET-REF".into());
    values.insert(keys::BANK.into(), "SECRET-BANK".into());
    for design in &p.designs {
        for element in &design.elements {
            let shown = element.content(&values);
            assert!(!shown.contains("SECRET"), "{} / {} would show {shown:?}", design.name, element.name);
        }
    }
}

#[test]
fn a_pre_dice_project_loads_and_every_dado_action_answers_gracefully() {
    let old = Project::load(&PathBuf::from(COMMUNITY_CUP)).unwrap();
    assert!(old.values.keys().all(|k| !k.starts_with("dado.") && !k.starts_with("dice.")), "the fixture is supposed to predate El Dado");
    assert!(old.designs.iter().all(|d| d.elements.iter().all(|e| e.kind != Kind::Dice)));

    // reads on a project without any dice values
    let mut p = old.clone();
    assert!(!bank_status(&p.values).loaded);
    let view = turn_view(&p);
    assert!(!view.has_participants);
    assert_eq!(view.participant, "");
    ensure_defaults(&mut p);
    p.validate().unwrap();
    assert_eq!(p.values[keys::QUESTION_TEXT], "");

    // each action on its own copy: none may panic, the outcome is pinned
    let outcome = |action: Action| {
        let mut p = old.clone();
        let mut game = GameSession::new(&p);
        let result = game.execute(&mut p, action).map(|_| ());
        p.validate().expect("project stays valid after the action");
        (result.map_err(|e| e.to_string()), p)
    };
    assert!(outcome(Action::RollDice).0.is_ok());
    assert!(outcome(Action::RevealAnswer).0.is_ok());
    assert_eq!(outcome(Action::NextQuestion).0.unwrap_err(), "No questions loaded. Import a CSV to draw questions.");
    assert_eq!(outcome(Action::NextParticipant).0.unwrap_err(), "Add participants first");
    assert_eq!(outcome(Action::StartQuestionTimer).0.unwrap_err(), "There is no question timer. Add a clock whose value is 'dado.clock'.");
    // nobody was set up: points go to the first team (nobody-up falls back to team 0) or are refused, but never to a missing key
    for points in [10, -10] {
        let (result, after) = outcome(Action::AwardCurrentTeam { points });
        match result {
            Ok(()) => {
                let team = &after.teams[0];
                assert_eq!(after.values[&team.score_key].parse::<i64>().unwrap() - old.values.get(&team.score_key).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0), points);
            }
            Err(message) => assert!(!message.is_empty()),
        }
    }

    // and in sequence on one session, like the phone would
    let mut p = old.clone();
    let mut game = GameSession::new(&p);
    for action in [Action::RollDice, Action::NextQuestion, Action::RevealAnswer, Action::NextParticipant, Action::StartQuestionTimer, Action::AwardCurrentTeam { points: 10 }, Action::RevealAnswer] {
        let _ = game.execute(&mut p, action);
        let _ = game.tick(&mut p);
    }
    p.validate().unwrap();
}

#[test]
fn a_game_night_project_keeps_the_die_the_bank_and_the_used_list_through_save_and_load() {
    let mut p = Project::game_night();
    let rows = import_bank(&mut p.values, TEMPLATE).unwrap().rows;
    assert!(rows > 1);
    dice::roll(&mut p.values).unwrap();
    assert!(dice::reveal_roll(&mut p.values));
    dice::next_question(&mut p.values).unwrap();
    dice::next_question(&mut p.values).unwrap();
    p.values.insert(keys::COLORS.into(), "#010203,#040506".into());
    let before = bank_status(&p.values);
    assert_eq!((before.total, before.used), (rows, 2));
    assert_eq!(p.values[keys::BANK_USED].split(',').count(), 2);

    let dir = scratch_dir();
    let path = dir.join("night.overlay");
    p.save(&path).unwrap();
    let loaded = Project::load(&path).unwrap();
    let _ = std::fs::remove_dir_all(&dir);

    let dados: Vec<_> = loaded.designs.iter().flat_map(|d| &d.elements).filter(|e| e.kind == Kind::Dice).collect();
    assert_eq!(dados.len(), 1, "exactly one Dice layer survives");
    assert_eq!(loaded.designs.len(), p.designs.len());
    for key in [keys::BANK, keys::BANK_USED, keys::BANK_CURRENT, keys::BANK_FILTER, keys::QUESTION_TEXT, keys::HOST_ANSWER, keys::FACE, keys::ROLL_ID, keys::RESULT, keys::COLORS] {
        assert_eq!(loaded.values.get(key), p.values.get(key), "{key}");
    }
    assert!(!loaded.values[keys::BANK].is_empty());
    let after = bank_status(&loaded.values);
    assert_eq!((after.loaded, after.total, after.used, after.remaining), (before.loaded, before.total, before.used, before.remaining));
    assert_eq!(after.books, before.books);
    assert_eq!(keys::DiceConfig::read(&loaded.values), keys::DiceConfig::read(&p.values));
    loaded.controller.validate(&loaded).unwrap();
}
