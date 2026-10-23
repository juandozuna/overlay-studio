use crate::Desktop;
use overlay_studio::domain::controls::Action;

#[test]
fn configuration_history_keeps_running_clock_identity_and_live_scores() {
    let mut s = Desktop::two_team_demo();
    s.execute(Action::ClockToggleAt { clock: 0 }).unwrap();
    let last = s.game.clocks[0].last;
    s.document
        .configure(&mut s.game, "clock", 0, "reset", "12:00")
        .unwrap();
    s.document.project.clocks[0].seconds = 400.;
    s.execute(Action::Adjust {
        field: "home.score".into(),
        step: 5,
    })
    .unwrap();
    s.history(false).unwrap();
    assert!(s.game.clocks[0].running);
    assert_eq!(s.game.clocks[0].last, last);
    assert_eq!(s.document.project.clocks[0].seconds, 400.);
    assert_eq!(s.document.project.values["home.score"], "17");
    s.history(true).unwrap();
    assert!(s.game.clocks[0].running);
    assert_eq!(s.document.project.clocks[0].reset_seconds, 720.);
    s.document
        .configure(&mut s.game, "clock", 0, "current", "01:30")
        .unwrap();
    assert!(!s.game.clocks[0].running);
    s.history(false).unwrap();
    assert!(!s.game.clocks[0].running);
    assert_eq!(s.document.project.clocks[0].seconds, 400.);
}

#[test]
fn removing_and_restoring_clock_keeps_other_timer_running() {
    let mut s = Desktop::two_team_demo();
    s.document
        .configure(&mut s.game, "clock", 0, "add", "")
        .unwrap();
    s.execute(Action::ClockToggleAt { clock: 1 }).unwrap();
    let id = s.document.project.clocks[1].id;
    let last = s.game.clocks[1].last;
    s.document
        .configure(&mut s.game, "clock", 0, "remove", "")
        .unwrap();
    assert!(s.game.clocks[0].running);
    s.history(false).unwrap();
    assert_eq!(s.document.project.clocks[1].id, id);
    assert!(s.game.clocks[1].running);
    assert_eq!(s.game.clocks[1].last, last);
    assert!(!s.game.clocks[0].running);
    s.history(true).unwrap();
    assert!(s.game.clocks[0].running);
    assert_eq!(s.game.clocks[0].last, last);
}
