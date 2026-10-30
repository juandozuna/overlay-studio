//! El Dado host panel wiring for the Slint window.
use crate::{DadoView, Desktop, StudioWindow, refresh, report, view_models};
use overlay_studio::{
    application::dice::{self, BankSummary},
    domain::{
        controls::Action,
        dice::{self as keys, DiceConfig, Face},
    },
    model::Team,
};
use slint::{ComponentHandle, ModelRc, SharedString};
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

#[derive(Default)]
pub struct Session {
    /// Team shown in the participants editor.
    team: usize,
    /// A score was awarded for the participant who is up (desk buttons only).
    awarded: bool,
    bank_error: String,
    roll_seen: RefCell<Option<(u64, Instant)>>,
}

/// What the stepper needs to know about the turn, apart from the project values.
#[derive(Clone, Debug, Default)]
struct Facts {
    has_participants: bool,
    rolling: bool,
    has_result: bool,
    green: bool,
    has_question: bool,
    revealed: bool,
    timer_state: i32,
    paused: bool,
    awarded: bool,
    bank_loaded: bool,
    bank_total: usize,
    bank_used: usize,
    bank_error: String,
    team_name: String,
}
impl Facts {
    fn exhausted(&self) -> bool {
        self.bank_loaded && self.bank_used >= self.bank_total
    }
    fn can_next_question(&self) -> bool {
        self.has_result && !self.rolling && !self.has_question && self.bank_loaded && !self.exhausted()
    }
    /// 0 roll, 1 question, 2 timer, 3 reveal, 4 score, 5 next participant.
    fn next_step(&self) -> i32 {
        if !self.has_participants || self.rolling || !self.has_result {
            0
        } else if !self.has_question {
            if self.green { 0 } else { 1 }
        } else if self.awarded || (self.timer_state == 2 && self.revealed) {
            5
        } else if self.revealed {
            4
        } else if self.timer_state == 0 && !self.paused {
            2
        } else {
            3
        }
    }
    /// One line above the stepper and its kind: 0 info, 1 warning, 2 error, 3 done.
    fn status_line(&self) -> (String, i32) {
        if !self.has_participants {
            return ("No participants yet. Add them in Game setup.".into(), 1);
        }
        if !self.bank_error.is_empty() {
            return (self.bank_error.clone(), 2);
        }
        if self.rolling {
            return ("Rolling…".into(), 0);
        }
        if !self.has_result {
            return ("Ready · roll when the next participant is up".into(), 0);
        }
        if !self.has_question && !self.bank_loaded {
            return ("No questions loaded. Import a CSV to draw questions.".into(), 1);
        }
        if !self.has_question && self.exhausted() {
            return (format!("All {} questions used.", self.bank_total), 1);
        }
        if self.timer_state == 2 {
            return ("Time is up · question annulled".into(), 2);
        }
        if self.awarded {
            return (format!("+10 awarded to {}", self.team_name), 3);
        }
        (String::new(), 0)
    }
    /// Dialog (title, question) to show before the command runs.
    fn confirm(&self, command: &str) -> Option<(&'static str, &'static str)> {
        match command {
            "roll" if self.has_question && !self.awarded && self.timer_state != 2 => {
                Some(("Roll again", "Roll again? The current question will be discarded."))
            }
            "next_participant" if self.has_participants && !self.awarded && self.timer_state != 2 => {
                Some(("Next participant", "Skip this participant without points?"))
            }
            "reset_used" => Some(("Reset used questions", "Reset used questions? This cannot be undone.")),
            "import_bank" if self.bank_loaded && self.bank_used > 0 => Some((
                "Import questions",
                "Replace the question bank? The list of used questions will be cleared.",
            )),
            _ => None,
        }
    }
}

/// `m:ss` as the audience timer shows it.
fn timer_text(seconds: f64) -> String {
    let n = seconds.max(0.).ceil() as u64;
    format!("{}:{:02}", n / 60, n % 60)
}
/// The roll tumbles until the result is published; the margin keeps a lost publish from sticking forever.
fn is_rolling(result: &str, since_roll: Option<Duration>, roll_seconds: f32) -> bool {
    result.is_empty() && since_roll.is_some_and(|d| d.as_secs_f32() < roll_seconds + 0.5)
}
fn normalize_names(text: &str) -> String {
    text.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n")
}
fn book_filter(books: &[String], index: usize) -> String {
    index.checked_sub(1).and_then(|i| books.get(i)).cloned().unwrap_or_default()
}
fn book_index(books: &[String], filter: &str) -> i32 {
    books.iter().position(|b| b == filter).map_or(0, |i| i as i32 + 1)
}
fn host_note(face: Option<Face>, team: &str, who: &str) -> String {
    let who = if who.is_empty() { "the participant" } else { who };
    match face {
        Some(Face::Azul) => format!("Pick someone from the audience to answer. Points go to {team}."),
        Some(Face::Naranja) => format!("Pick a teammate to answer for {who}. Points go to {team}."),
        Some(Face::Morado) => "Bible lookup allowed — the 30 s keep running.".into(),
        Some(Face::Verde) => "Extra question: handle it manually, then roll again for this participant.".into(),
        Some(Face::Amarillo) => "Only questions with options A to D are drawn.".into(),
        _ => String::new(),
    }
}
fn import_message(summary: &BankSummary) -> String {
    let books = summary.per_book.iter().map(|(b, n)| format!("{b} {n}")).collect::<Vec<_>>().join(" · ");
    let noun = if summary.rows == 1 { "question" } else { "questions" };
    if books.is_empty() {
        format!("Imported {} {noun}.", summary.rows)
    } else {
        format!("Imported {} {noun} · {books}", summary.rows)
    }
}
fn decode_csv(bytes: Vec<u8>) -> Result<String, String> {
    String::from_utf8(bytes)
        .map_err(|_| "Could not import: the file is not UTF-8 text. Save it as CSV UTF-8 and try again.".into())
}
fn rgb(c: [u8; 3]) -> slint::Color {
    slint::Color::from_rgb_u8(c[0], c[1], c[2])
}
fn strings(items: impl IntoIterator<Item = String>) -> Vec<SharedString> {
    items.into_iter().map(Into::into).collect()
}

fn question_clock(s: &Desktop) -> Option<usize> {
    s.document.project.clocks.iter().position(|c| c.value_key == keys::QUESTION_CLOCK_KEY)
}
fn facts(s: &Desktop, cfg: &DiceConfig, rolling: bool) -> Facts {
    let p = &s.document.project;
    let get = |k: &str| p.values.get(k).map(String::as_str).unwrap_or("");
    let turn = dice::turn_view(p);
    let bank = dice::bank_status(&p.values);
    let clock = question_clock(s);
    let running = clock.and_then(|i| s.game.clocks.get(i)).is_some_and(|c| c.running);
    let timer_state = dice::timer_state(p, running);
    let paused = clock.and_then(|i| p.clocks.get(i)).is_some_and(|c| {
        timer_state == 0 && !running && c.seconds > 0. && c.seconds < c.reset_seconds
    });
    Facts {
        has_participants: turn.has_participants,
        rolling,
        has_result: !cfg.result.is_empty() && !rolling,
        green: cfg.face == Some(Face::Verde),
        has_question: !get(keys::QUESTION_TEXT).is_empty(),
        revealed: get(keys::REVEALED) == "1",
        timer_state,
        paused,
        awarded: s.dado.awarded,
        bank_loaded: bank.loaded,
        bank_total: bank.total,
        bank_used: bank.used,
        bank_error: s.dado.bank_error.clone(),
        team_name: turn.team_name,
    }
}

fn view(s: &Desktop, old: Option<&DadoView>) -> DadoView {
    let p = &s.document.project;
    let get = |k: &str| p.values.get(k).map(String::as_str).unwrap_or("");
    let cfg = DiceConfig::read(&p.values);
    let since_roll = {
        let mut seen = s.dado.roll_seen.borrow_mut();
        if seen.is_none_or(|(id, _)| id != cfg.roll_id) {
            // The first sight of a project (or of a loaded one) is not a roll in progress.
            let started = if seen.is_none() { Instant::now() - Duration::from_secs(3600) } else { Instant::now() };
            *seen = Some((cfg.roll_id, started));
        }
        seen.map(|(_, at)| at.elapsed())
    };
    let rolling = is_rolling(&cfg.result, since_roll, cfg.roll_seconds);
    let f = facts(s, &cfg, rolling);
    let turn = dice::turn_view(p);
    let bank = dice::bank_status(&p.values);
    let face = if f.has_result { cfg.face } else { None };
    let (status, status_kind) = f.status_line();
    let clock = question_clock(s).and_then(|i| p.clocks.get(i));
    let seconds = clock.map_or(keys::QUESTION_SECONDS, |c| c.seconds);
    let team: Option<&Team> = p.teams.get(turn.team_index);
    let team_index = s.dado.team.min(p.teams.len().saturating_sub(1));
    let stored = dice::participants_text(p, team_index);
    // Keep what is being typed while the stored text only differs by blank lines or spaces.
    let participants_text = match old {
        Some(o) if o.team_index == team_index as i32 && normalize_names(&o.participants_text) == stored => {
            o.participants_text.to_string()
        }
        _ => stored,
    };
    let books = std::iter::once("All".to_string()).chain(bank.books.iter().cloned()).collect::<Vec<_>>();
    let option = |i: usize| get(keys::OPTION_KEYS[i]).to_string();
    let next_step = f.next_step();
    let model = |old: Option<ModelRc<SharedString>>, items: Vec<SharedString>| {
        view_models::sync(old.unwrap_or_default(), items)
    };
    DadoView {
        team_name: turn.team_name.clone().into(),
        participant_name: turn.participant.clone().into(),
        participant_pos: turn.position.clone().into(),
        has_participants: f.has_participants,
        can_roll: f.has_participants && !f.rolling,
        rolling,
        result_name: if f.has_result { cfg.result.clone() } else { String::new() }.into(),
        result_rule: if f.has_result { cfg.rule.clone() } else { String::new() }.into(),
        result_color: face.map_or(slint::Color::default(), |face| rgb(cfg.colors[face.index()])),
        bank_loaded: bank.loaded,
        bank_used: if bank.loaded { format!("used {}/{}", bank.used, bank.total) } else { "no bank".into() }.into(),
        bank_error: s.dado.bank_error.clone().into(),
        book_index: book_index(&books, &bank.filter),
        books: model(old.map(|o| o.books.clone()), strings(books)),
        question_text: get(keys::QUESTION_TEXT).into(),
        options_on_screen: face.is_some_and(Face::shows_options),
        option_a: option(0).into(),
        option_b: option(1).into(),
        option_c: option(2).into(),
        option_d: option(3).into(),
        answer_text: get(keys::HOST_ANSWER).into(),
        ref_text: get(keys::HOST_REF).into(),
        revealed: f.revealed,
        timer_text: timer_text(seconds).into(),
        timer_state: f.timer_state,
        score_text: team.map_or("0", |t| get(&t.score_key)).into(),
        next_step,
        teams: model(old.map(|o| o.teams.clone()), strings(p.teams.iter().map(|t| t.label.clone()))),
        team_index: team_index as i32,
        participants_text: participants_text.into(),
        status: status.into(),
        status_kind,
        host_note: if f.has_result { host_note(face, &turn.team_name, &turn.participant) } else { String::new() }.into(),
        score_note: if f.timer_state == 2 {
            "After time-up: +10 is still your call."
        } else if f.awarded {
            "Awarded this turn."
        } else {
            ""
        }
        .into(),
        team_color: team.map_or(slint::Color::default(), |t| rgb(t.color)),
        has_question: f.has_question,
        bank_exhausted: f.exhausted(),
        bank_summary: if bank.loaded {
            format!("BANK · {} loaded · {} used", bank.total, bank.used)
        } else {
            "BANK · not loaded".into()
        }
        .into(),
        can_next_question: f.can_next_question(),
        can_reveal: f.has_question,
        can_start_timer: f.has_question && f.timer_state == 0 && !f.paused,
        can_pause: f.timer_state == 1 || f.paused,
        timer_paused: f.paused,
        can_reset_timer: f.has_question && (f.timer_state != 0 || f.paused),
        timer_seconds: seconds.max(0.).ceil() as i32,
        can_score: f.has_participants && f.has_result,
        can_next_participant: f.has_participants && !f.rolling,
        can_reset_used: bank.used > 0,
    }
}

fn edit(s: &mut Desktop, key: &str, value: &str) -> Result<(), String> {
    match key {
        "book" => {
            let index: usize = value.parse().map_err(|_| "Unknown book")?;
            let books = dice::bank_status(&s.document.project.values).books;
            if index > books.len() {
                return Err("Unknown book".into());
            }
            dice::set_bank_filter(&mut s.document.project.values, &book_filter(&books, index));
        }
        "team" => {
            let index: usize = value.parse().map_err(|_| "Unknown team")?;
            if index >= s.document.project.teams.len() {
                return Err("Unknown team".into());
            }
            s.dado.team = index;
            return Ok(());
        }
        "participants" => dice::set_participants(&mut s.document.project, s.dado.team, value),
        _ => return Err("Unknown El Dado property".into()),
    }
    s.document.mark_external_edit();
    Ok(())
}
fn command(s: &mut Desktop, command: &str, value: &str) -> Result<(), String> {
    match command {
        "roll" => {
            s.execute(Action::RollDice)?;
            s.dado.awarded = false;
            Ok(())
        }
        "next_question" | "skip_question" => s.execute(Action::NextQuestion),
        "start_timer" => s.execute(Action::StartQuestionTimer),
        "pause_timer" => {
            let clock = question_clock(s).ok_or("The question timer does not exist yet")?;
            s.execute(Action::ClockToggleAt { clock })
        }
        "reset_timer" => {
            let clock = question_clock(s).ok_or("The question timer does not exist yet")?;
            s.execute(Action::ClockResetAt { clock })?;
            // The clock reset leaves PREGUNTA ANULADA up; clearing it re-arms Start.
            s.execute(Action::SetValue { field: keys::STATUS.into(), value: String::new() })
        }
        "reveal" => s.execute(Action::RevealAnswer), // toggles: Reveal / Hide
        "award" => {
            let points = match value {
                "10" => keys::POINTS,
                "-10" => -keys::POINTS,
                _ => return Err("Unknown score step".into()),
            };
            s.execute(Action::AwardCurrentTeam { points })?;
            s.dado.awarded |= points > 0;
            Ok(())
        }
        "next_participant" => {
            s.execute(Action::NextParticipant)?;
            s.dado.awarded = false;
            Ok(())
        }
        "back_participant" => {
            dice::previous_participant(&mut s.document.project).map_err(|e| e.to_string())?;
            s.document.mark_external_edit();
            s.dado.awarded = false;
            Ok(())
        }
        "reset_used" => {
            dice::reset_used(&mut s.document.project.values);
            s.document.mark_external_edit();
            Ok(())
        }
        _ => Err("Unknown El Dado command".into()),
    }
}
fn apply_import(s: &mut Desktop, csv: &str) -> Result<String, String> {
    match dice::import_bank(&mut s.document.project.values, csv) {
        Ok(summary) => {
            s.dado.bank_error.clear();
            s.document.mark_external_edit();
            Ok(import_message(&summary))
        }
        Err(error) => {
            s.dado.bank_error = error.clone();
            Err(error)
        }
    }
}

fn current_facts(s: &Desktop) -> Facts {
    let cfg = DiceConfig::read(&s.document.project.values);
    let since = s.dado.roll_seen.borrow().filter(|(id, _)| *id == cfg.roll_id).map(|(_, at)| at.elapsed());
    facts(s, &cfg, is_rolling(&cfg.result, since, cfg.roll_seconds))
}
async fn ask(title: &str, text: &str) -> bool {
    matches!(
        rfd::AsyncMessageDialog::new()
            .set_title(title)
            .set_description(text)
            .set_buttons(rfd::MessageButtons::YesNo)
            .show()
            .await,
        rfd::MessageDialogResult::Yes
    )
}
fn finish(ui: &StudioWindow, state: &Rc<RefCell<Desktop>>) {
    refresh(ui, &state.borrow());
    ui.window().request_redraw();
}
fn run(ui: &StudioWindow, state: &Rc<RefCell<Desktop>>, name: &str, value: &str) {
    let result = command(&mut state.borrow_mut(), name, value);
    report(ui, result);
    finish(ui, state);
}
async fn import_bank(weak: slint::Weak<StudioWindow>, state: Rc<RefCell<Desktop>>) {
    let confirm = current_facts(&state.borrow()).confirm("import_bank");
    if let Some((title, text)) = confirm
        && !ask(title, text).await
    {
        return;
    }
    let Some(file) = rfd::AsyncFileDialog::new().add_filter("Question bank (CSV)", &["csv"]).pick_file().await else {
        return;
    };
    let result = decode_csv(file.read().await).and_then(|csv| apply_import(&mut state.borrow_mut(), &csv));
    let Some(ui) = weak.upgrade() else {
        return;
    };
    match result {
        Ok(message) => {
            report(&ui, Ok(()));
            ui.set_status(message.into());
        }
        Err(error) => {
            state.borrow_mut().dado.bank_error = error.clone();
            report(&ui, Err(error));
        }
    }
    finish(&ui, &state);
}

pub fn refresh_panel(ui: &StudioWindow, s: &Desktop) {
    let old = ui.get_dado_view();
    let new = view(s, Some(&old));
    if new != old {
        ui.set_dado_view(new);
    }
}

pub fn connect(ui: &StudioWindow, state: Rc<RefCell<Desktop>>) {
    let shared = state.clone();
    let weak = ui.as_weak();
    ui.on_dado_edit(move |key, value| {
        if let Some(ui) = weak.upgrade() {
            let result = edit(&mut shared.borrow_mut(), &key, &value);
            report(&ui, result);
            finish(&ui, &shared);
        }
    });
    let weak = ui.as_weak();
    ui.on_dado_command(move |name, value| {
        let Some(ui) = weak.upgrade() else {
            return;
        };
        if name.as_str() == "import_bank" {
            let _ = slint::spawn_local(import_bank(weak.clone(), state.clone()));
            return;
        }
        let confirm = current_facts(&state.borrow()).confirm(&name);
        let Some((title, text)) = confirm else {
            run(&ui, &state, &name, &value);
            return;
        };
        let (weak, state) = (weak.clone(), state.clone());
        let _ = slint::spawn_local(async move {
            if ask(title, text).await
                && let Some(ui) = weak.upgrade()
            {
                run(&ui, &state, &name, &value);
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn() -> Facts {
        Facts { has_participants: true, bank_loaded: true, bank_total: 8, team_name: "Equipo 2".into(), ..Facts::default() }
    }

    #[test]
    fn the_accent_follows_the_turn() {
        let mut f = turn();
        assert_eq!(f.next_step(), 0);
        f.has_result = true;
        assert_eq!(f.next_step(), 1);
        f.has_question = true;
        assert_eq!(f.next_step(), 2);
        f.paused = true;
        assert_eq!(f.next_step(), 3);
        f.paused = false;
        f.revealed = true;
        assert_eq!(f.next_step(), 4);
        f.awarded = true;
        assert_eq!(f.next_step(), 5);
    }
    #[test]
    fn green_rolls_again_and_annulment_needs_the_reveal_before_moving_on() {
        let mut f = Facts { has_result: true, green: true, ..turn() };
        assert_eq!(f.next_step(), 0);
        f = Facts { has_result: true, has_question: true, timer_state: 2, ..turn() };
        assert_eq!(f.next_step(), 3);
        f.revealed = true;
        assert_eq!(f.next_step(), 5);
    }
    #[test]
    fn nothing_is_highlighted_for_a_roll_that_cannot_happen_yet() {
        assert_eq!(Facts::default().next_step(), 0);
        assert_eq!(Facts { rolling: true, has_result: true, ..turn() }.next_step(), 0);
    }
    #[test]
    fn next_question_needs_a_result_a_bank_and_an_unused_question() {
        let rolled = Facts { has_result: true, ..turn() };
        assert!(rolled.can_next_question());
        assert!(!Facts { has_result: false, ..rolled.clone() }.can_next_question());
        assert!(!Facts { has_question: true, ..rolled.clone() }.can_next_question());
        assert!(!Facts { bank_loaded: false, ..rolled.clone() }.can_next_question());
        assert!(!Facts { bank_used: 8, ..rolled.clone() }.can_next_question());
        assert!(Facts { bank_used: 8, ..rolled }.exhausted());
    }
    #[test]
    fn status_line_says_what_is_blocking() {
        assert_eq!(Facts::default().status_line().1, 1);
        assert_eq!(turn().status_line().0, "Ready · roll when the next participant is up");
        let rolled = Facts { has_result: true, ..turn() };
        assert_eq!(Facts { bank_loaded: false, ..rolled.clone() }.status_line().0, "No questions loaded. Import a CSV to draw questions.");
        assert_eq!(Facts { bank_used: 8, ..rolled.clone() }.status_line().0, "All 8 questions used.");
        let annulled = Facts { has_question: true, timer_state: 2, ..rolled.clone() };
        assert_eq!(annulled.status_line(), ("Time is up · question annulled".into(), 2));
        let awarded = Facts { has_question: true, awarded: true, ..rolled.clone() };
        assert_eq!(awarded.status_line(), ("+10 awarded to Equipo 2".into(), 3));
        let broken = Facts { bank_error: "Could not import: row 14 is missing a correct answer.".into(), ..rolled };
        assert_eq!(broken.status_line().1, 2);
    }
    #[test]
    fn dangerous_commands_ask_first() {
        let f = turn();
        assert_eq!(f.confirm("roll"), None);
        assert_eq!(f.confirm("next_participant").map(|c| c.0), Some("Next participant"));
        assert!(f.confirm("reset_used").is_some());
        assert_eq!(f.confirm("import_bank"), None);
        assert!(Facts { bank_used: 1, ..f.clone() }.confirm("import_bank").is_some());
        let open = Facts { has_question: true, has_result: true, ..f };
        assert!(open.confirm("roll").is_some());
        assert!(Facts { awarded: true, ..open.clone() }.confirm("roll").is_none());
        assert!(Facts { awarded: true, ..open.clone() }.confirm("next_participant").is_none());
        assert!(Facts { timer_state: 2, ..open.clone() }.confirm("next_participant").is_none());
        assert_eq!(open.confirm("reveal"), None);
    }
    #[test]
    fn timer_shows_minutes_and_rounds_up() {
        assert_eq!(timer_text(30.), "0:30");
        assert_eq!(timer_text(9.2), "0:10");
        assert_eq!(timer_text(0.), "0:00");
        assert_eq!(timer_text(-3.), "0:00");
        assert_eq!(timer_text(75.), "1:15");
    }
    #[test]
    fn rolling_ends_when_the_result_lands_or_the_margin_runs_out() {
        assert!(is_rolling("", Some(Duration::from_secs(1)), 2.));
        assert!(!is_rolling("ROJO", Some(Duration::from_secs(1)), 2.));
        assert!(!is_rolling("", Some(Duration::from_secs(3)), 2.));
        assert!(!is_rolling("", None, 2.));
    }
    #[test]
    fn book_filter_maps_to_the_combo_index_and_back() {
        let books = vec!["1 Samuel".to_string(), "2 Reyes".to_string()];
        assert_eq!(book_filter(&books, 0), "");
        assert_eq!(book_filter(&books, 2), "2 Reyes");
        assert_eq!(book_filter(&books, 9), "");
        assert_eq!(book_index(&books, ""), 0);
        assert_eq!(book_index(&books, "2 Reyes"), 2);
        assert_eq!(book_index(&books, "Salmos"), 0);
    }
    #[test]
    fn participant_names_are_trimmed_and_blank_lines_dropped() {
        assert_eq!(normalize_names(" Ana \n\n Luis\r\n \n"), "Ana\nLuis");
        assert_eq!(normalize_names("\n"), "");
    }
    #[test]
    fn host_notes_name_the_team_that_scores() {
        assert!(host_note(Some(Face::Azul), "Equipo 2", "Ana").contains("Points go to Equipo 2"));
        assert!(host_note(Some(Face::Naranja), "Equipo 2", "Ana").contains("answer for Ana"));
        assert!(host_note(Some(Face::Verde), "Equipo 2", "Ana").contains("roll again"));
        assert_eq!(host_note(Some(Face::Rojo), "Equipo 2", "Ana"), "");
        assert_eq!(host_note(None, "Equipo 2", "Ana"), "");
    }
    #[test]
    fn import_result_and_bad_encoding_are_reported_in_plain_text() {
        let summary = BankSummary { rows: 84, per_book: vec![("1 Samuel".into(), 40), ("2 Reyes".into(), 44)] };
        assert_eq!(import_message(&summary), "Imported 84 questions · 1 Samuel 40 · 2 Reyes 44");
        assert_eq!(import_message(&BankSummary { rows: 1, per_book: vec![] }), "Imported 1 question.");
        assert_eq!(decode_csv(b"book,reference".to_vec()).as_deref(), Ok("book,reference"));
        assert!(decode_csv(vec![0xff, 0xfe, 0x41]).unwrap_err().contains("not UTF-8"));
    }
    #[test]
    fn the_team_edit_only_accepts_existing_teams() {
        let mut s = Desktop::two_team_demo();
        assert!(edit(&mut s, "team", "1").is_ok());
        assert_eq!(s.dado.team, 1);
        assert!(edit(&mut s, "team", "99").is_err());
        assert!(edit(&mut s, "book", "x").is_err());
        assert!(edit(&mut s, "nope", "").is_err());
    }
}
