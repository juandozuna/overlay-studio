//! El Dado commands over `domain::dice` values: rolls, question draws, turns and the question clock.
use super::{fx, game_commands::{self, CommandError, CommandOutcome}};
use crate::{
    domain::{
        dice::{self as keys, DiceConfig, Face},
        question_bank::{self, Bank, Question},
    },
    model::{CueEvent, GameClock, Project, clock_text},
};
use std::{
    collections::BTreeMap,
    hash::{BuildHasher, Hasher, RandomState},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

type Values = BTreeMap<String, String>;

pub struct BankSummary {
    pub rows: usize,
    pub per_book: Vec<(String, usize)>,
}
pub struct BankStatus {
    pub loaded: bool,
    pub total: usize,
    pub used: usize,
    /// Distinct books present, file order.
    pub books: Vec<String>,
    pub filter: String,
}
pub struct TurnView {
    pub team_index: usize,
    pub team_name: String,
    /// Empty when the team has nobody.
    pub participant: String,
    /// "Participante 2 de 4"
    pub position: String,
    /// True when any team has participants.
    pub has_participants: bool,
}

fn get<'a>(values: &'a Values, key: &str) -> &'a str {
    values.get(key).map_or("", |v| v.trim())
}
fn set(values: &mut Values, key: &str, value: impl Into<String>) {
    values.insert(key.into(), value.into());
}

/// Inserts the bindable text keys and the first turn when missing, leaving existing values untouched.
pub fn ensure_defaults(project: &mut Project) {
    let values = &mut project.values;
    for key in [keys::RESULT, keys::RULE, keys::STATUS, keys::QUESTION_TEXT, keys::HOST_ANSWER, keys::HOST_REF, keys::REVEALED]
        .into_iter()
        .chain(keys::OPTION_KEYS)
    {
        values.entry(key.into()).or_default();
    }
    if get(values, keys::TURN_TEAM).parse::<usize>().ok().is_none_or(|i| i >= project.teams.len().max(1)) {
        set(values, keys::TURN_TEAM, "0");
    }
    refresh_turn(project);
}

// ---- rolling ----

pub fn roll(values: &mut Values) -> Result<(), CommandError> {
    let id = DiceConfig::read(values).roll_id + 1;
    roll_with(values, &mut Rng::new(id));
    Ok(())
}

fn roll_with(values: &mut Values, rng: &mut Rng) {
    let id = DiceConfig::read(values).roll_id + 1;
    set(values, keys::ROLL_ID, id.to_string());
    set(values, keys::FACE, (rng.next() % Face::ALL.len() as u64).to_string());
    for key in [keys::RESULT, keys::RULE, keys::STATUS] {
        set(values, key, "");
    }
    clear_question(values);
}

/// Publishes the landed face and its rule once the roll is over; fills A-D if a question is already up.
pub fn reveal_roll(values: &mut Values) -> bool {
    let config = DiceConfig::read(values);
    let Some(face) = config.face else {
        return false;
    };
    set(values, keys::RESULT, face.name());
    set(values, keys::RULE, config.rules[face.index()].clone());
    if face.shows_options() {
        if let Some(question) = current_question(values) {
            fill_options(values, &question);
        }
    }
    true
}

/// Stops the question clock and sets it back to the full time; no-op without that clock.
pub fn reset_question_clock(project: &mut Project, clocks: &mut [GameClock]) {
    if let Some(index) = question_clock(project) {
        if let Some(runtime) = clocks.get_mut(index) {
            runtime.running = false;
            runtime.last = Instant::now();
        }
        set_clock_seconds(project, index, keys::QUESTION_SECONDS);
    }
}

/// Pauses the question clock where it is (the answer was revealed or judged).
pub fn pause_question_clock(project: &Project, clocks: &mut [GameClock]) {
    if let Some(runtime) = question_clock(project).and_then(|i| clocks.get_mut(i)) {
        runtime.running = false;
    }
}

pub fn start_question_timer(project: &mut Project, clocks: &mut [GameClock], outcome: &mut CommandOutcome) -> Result<(), CommandError> {
    let index = question_clock(project)
        .filter(|i| clocks.get(*i).is_some())
        .ok_or_else(|| CommandError::Dice("There is no question timer. Add a clock whose value is 'dado.clock'.".into()))?;
    set_clock_seconds(project, index, keys::QUESTION_SECONDS);
    clocks[index].last = Instant::now();
    clocks[index].running = true;
    set(&mut project.values, keys::STATUS, "");
    outcome.events.push(CueEvent::ClockStarted { clock: index });
    Ok(())
}

fn question_clock(project: &Project) -> Option<usize> {
    project.clocks.iter().position(|c| c.value_key == keys::QUESTION_CLOCK_KEY)
}
fn set_clock_seconds(project: &mut Project, index: usize, seconds: f64) {
    let spec = &mut project.clocks[index];
    spec.seconds = seconds;
    let (key, text) = (spec.value_key.clone(), clock_text(seconds));
    project.values.insert(key, text);
    game_commands::sync_primary_clock(project);
}

/// 0 idle, 1 running, 2 annulled.
pub fn timer_state(project: &Project, clocks_running: bool) -> i32 {
    if get(&project.values, keys::STATUS) == keys::STATUS_ANNULLED {
        2
    } else {
        i32::from(clocks_running)
    }
}

// ---- questions ----

fn load_bank(values: &Values) -> Option<Bank> {
    let text = values.get(keys::BANK)?;
    if text.trim().is_empty() {
        return None;
    }
    question_bank::parse(text).ok()
}

fn used_rows(values: &Values) -> Vec<usize> {
    get(values, keys::BANK_USED).split(',').filter_map(|n| n.trim().parse().ok()).collect()
}

/// The question on screen (its 1-based row number is `BANK_CURRENT`).
fn current_question(values: &Values) -> Option<Question> {
    let row: usize = get(values, keys::BANK_CURRENT).parse().ok()?;
    load_bank(values)?.rows.get(row.checked_sub(1)?).cloned()
}

fn fill_options(values: &mut Values, question: &Question) {
    for (key, option) in keys::OPTION_KEYS.iter().zip(&question.options) {
        set(values, key, option.clone());
    }
}

fn clear_question(values: &mut Values) {
    for key in [keys::QUESTION_TEXT, keys::HOST_ANSWER, keys::HOST_REF, keys::REVEALED, keys::BANK_CURRENT].into_iter().chain(keys::OPTION_KEYS) {
        set(values, key, "");
    }
}

/// Replaces the bank with `csv`, or leaves everything untouched and explains why not.
pub fn import_bank(values: &mut Values, csv: &str) -> Result<BankSummary, String> {
    let bank = question_bank::parse(csv)?;
    set(values, keys::BANK, csv.strip_prefix('\u{feff}').unwrap_or(csv));
    set(values, keys::BANK_USED, "");
    clear_question(values);
    if !get(values, keys::BANK_FILTER).is_empty() && !bank.books().iter().any(|b| b == get(values, keys::BANK_FILTER)) {
        set(values, keys::BANK_FILTER, "");
    }
    Ok(BankSummary { rows: bank.rows.len(), per_book: bank.per_book() })
}

pub fn bank_status(values: &Values) -> BankStatus {
    let filter = get(values, keys::BANK_FILTER).to_string();
    let Some(bank) = load_bank(values) else {
        return BankStatus { loaded: false, total: 0, used: 0, books: vec![], filter };
    };
    let mut used = used_rows(values);
    used.sort_unstable();
    used.dedup();
    BankStatus { loaded: true, total: bank.rows.len(), used: used.iter().filter(|n| (1..=bank.rows.len()).contains(*n)).count(), books: bank.books(), filter }
}

pub fn reset_used(values: &mut Values) {
    set(values, keys::BANK_USED, "");
}

pub fn set_bank_filter(values: &mut Values, book: &str) {
    set(values, keys::BANK_FILTER, book.trim());
}

pub fn next_question(values: &mut Values) -> Result<(), CommandError> {
    next_question_with(values, &mut Rng::new(used_rows(values).len() as u64))
}

fn next_question_with(values: &mut Values, rng: &mut Rng) -> Result<(), CommandError> {
    let bank = load_bank(values).ok_or_else(|| CommandError::Dice("No questions loaded. Import a CSV to draw questions.".into()))?;
    let face = DiceConfig::read(values).face;
    let yellow = face.is_some_and(Face::shows_options);
    let filter = get(values, keys::BANK_FILTER).to_string();
    let in_book = if filter.is_empty() { String::new() } else { format!(" for {filter}") };
    let used = used_rows(values);
    let matching: Vec<usize> = (1..=bank.rows.len()).filter(|n| filter.is_empty() || bank.rows[n - 1].book == filter).collect();
    if matching.is_empty() {
        return Err(CommandError::Dice(format!("The question bank has no questions{in_book}.")));
    }
    let unused: Vec<usize> = matching.into_iter().filter(|n| !used.contains(n)).collect();
    if unused.is_empty() {
        return Err(CommandError::Dice(format!("All questions{in_book} have been used. Reset the used questions or change the book filter.")));
    }
    let candidates: Vec<usize> = unused.into_iter().filter(|n| !yellow || bank.rows[n - 1].has_options()).collect();
    if candidates.is_empty() {
        return Err(CommandError::Dice(format!(
            "No unused questions{in_book} have all four options A-D, and yellow needs them. Roll again, reset the used questions or change the book filter."
        )));
    }
    let row = candidates[(rng.next() % candidates.len() as u64) as usize];
    let question = &bank.rows[row - 1];
    set(values, keys::QUESTION_TEXT, question.text.clone());
    set(values, keys::HOST_ANSWER, question.answer.clone());
    set(values, keys::HOST_REF, question.reference.clone());
    set(values, keys::BANK_CURRENT, row.to_string());
    let used_text = values.get(keys::BANK_USED).map(|u| u.trim().to_string()).filter(|u| !u.is_empty());
    set(values, keys::BANK_USED, used_text.map_or(row.to_string(), |u| format!("{u},{row}")));
    let landed_yellow = yellow && !get(values, keys::RESULT).is_empty();
    if landed_yellow {
        fill_options(values, question);
    } else {
        for key in keys::OPTION_KEYS {
            set(values, key, "");
        }
    }
    for key in [keys::REVEALED, keys::STATUS] {
        set(values, key, "");
    }
    Ok(())
}

pub fn toggle_reveal(values: &mut Values) {
    let shown = get(values, keys::REVEALED) == "1";
    set(values, keys::REVEALED, if shown { "" } else { "1" });
}

// ---- turns ----

fn participants(values: &Values, team_id: u64) -> Vec<String> {
    lines(values.get(&keys::participants_key(team_id)).map_or("", String::as_str))
}
fn lines(text: &str) -> Vec<String> {
    text.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect()
}
fn turn_team(project: &Project) -> usize {
    get(&project.values, keys::TURN_TEAM).parse().ok().filter(|i| *i < project.teams.len()).unwrap_or(0)
}
fn turn_index(project: &Project, team: usize) -> usize {
    let Some(t) = project.teams.get(team) else { return 0 };
    let count = participants(&project.values, t.id).len().max(1);
    get(&project.values, &keys::turn_index_key(t.id)).parse::<usize>().unwrap_or(0) % count
}
fn set_turn_index(project: &mut Project, team: usize, index: usize) {
    if let Some(t) = project.teams.get(team) {
        let key = keys::turn_index_key(t.id);
        set(&mut project.values, &key, index.to_string());
    }
}
fn has_participants(project: &Project, team: usize) -> bool {
    project.teams.get(team).is_some_and(|t| !participants(&project.values, t.id).is_empty())
}
fn team_name(project: &Project, team: usize) -> String {
    project.teams.get(team).map_or_else(String::new, |t| {
        let name = get(&project.values, &t.name_key);
        if name.is_empty() { t.label.clone() } else { name.to_string() }
    })
}
fn current_participant(project: &Project, team: usize) -> String {
    let Some(t) = project.teams.get(team) else { return String::new() };
    participants(&project.values, t.id).get(turn_index(project, team)).cloned().unwrap_or_default()
}

/// Keeps the bindable "TURNO" texts in step with the turn state.
fn refresh_turn(project: &mut Project) {
    let team = turn_team(project);
    let (who, team_label) = (current_participant(project, team), team_name(project, team));
    set(&mut project.values, keys::TURN_NAME, who);
    set(&mut project.values, keys::TURN_TEAM_NAME, team_label);
}

pub fn turn_view(project: &Project) -> TurnView {
    let team = turn_team(project);
    let count = project.teams.get(team).map_or(0, |t| participants(&project.values, t.id).len());
    TurnView {
        team_index: team,
        team_name: team_name(project, team),
        participant: current_participant(project, team),
        position: if count == 0 { String::new() } else { format!("Participante {} de {count}", turn_index(project, team) + 1) },
        has_participants: (0..project.teams.len()).any(|t| has_participants(project, t)),
    }
}

/// One name per line; blanks dropped.
pub fn set_participants(project: &mut Project, team_index: usize, text: &str) {
    let Some(id) = project.teams.get(team_index).map(|t| t.id) else { return };
    set(&mut project.values, &keys::participants_key(id), lines(text).join("\n"));
    let index = turn_index(project, team_index);
    set_turn_index(project, team_index, index);
    refresh_turn(project);
}

pub fn participants_text(project: &Project, team_index: usize) -> String {
    project.teams.get(team_index).map_or_else(String::new, |t| participants(&project.values, t.id).join("\n"))
}

fn clear_round(values: &mut Values) {
    clear_question(values);
    set(values, keys::STATUS, "");
}

/// Advances the team that just played, then hands the turn to the next team that has participants.
pub fn next_participant(project: &mut Project) -> Result<(), CommandError> {
    ensure_defaults(project);
    let teams = project.teams.len();
    if !(0..teams).any(|t| has_participants(project, t)) {
        return Err(CommandError::Dice("Add participants first".into()));
    }
    let current = turn_team(project);
    if let Some(count) = project.teams.get(current).map(|t| participants(&project.values, t.id).len()).filter(|c| *c > 0) {
        let next = (turn_index(project, current) + 1) % count;
        set_turn_index(project, current, next);
    }
    if let Some(next) = (1..=teams).map(|step| (current + step) % teams).find(|t| has_participants(project, *t)) {
        set(&mut project.values, keys::TURN_TEAM, next.to_string());
    }
    clear_round(&mut project.values);
    refresh_turn(project);
    Ok(())
}

/// "Back one": undoes the last `next_participant`.
pub fn previous_participant(project: &mut Project) -> Result<(), CommandError> {
    ensure_defaults(project);
    let teams = project.teams.len();
    if !(0..teams).any(|t| has_participants(project, t)) {
        return Err(CommandError::Dice("Add participants first".into()));
    }
    let current = turn_team(project);
    let Some(previous) = (1..=teams).map(|step| (current + teams - step % teams) % teams).find(|t| has_participants(project, *t)) else {
        return Ok(());
    };
    let count = participants(&project.values, project.teams[previous].id).len();
    let index = (turn_index(project, previous) + count - 1) % count;
    set_turn_index(project, previous, index);
    set(&mut project.values, keys::TURN_TEAM, previous.to_string());
    clear_round(&mut project.values);
    refresh_turn(project);
    Ok(())
}

/// Adds (or removes, when negative) points for the team that is up, like `Action::Adjust` does.
pub fn award_current_team(project: &mut Project, points: i64, outcome: &mut CommandOutcome) -> Result<(), CommandError> {
    ensure_defaults(project);
    let team = turn_team(project);
    let field = project.teams.get(team).map(|t| t.score_key.clone()).ok_or_else(|| CommandError::Dice("Nobody is up yet".into()))?;
    if project.values.get(&field).is_some_and(|v| v.trim().parse::<i64>().is_err()) {
        return Err(CommandError::NonNumericField(field));
    }
    project.adjust_score(&field, points);
    outcome.document_changed |= points != 0;
    outcome.preview_requested = true;
    fx::score_changed(project, &field, points, outcome);
    if points > 0 {
        outcome.events.push(CueEvent::ScoreAdded { field });
    } else if points < 0 {
        outcome.events.push(CueEvent::ScoreRemoved { field });
    }
    Ok(())
}

/// Small splitmix64 stream seeded from the clock, a per-process random key and a salt (roll id).
struct Rng(u64);
impl Rng {
    fn new(salt: u64) -> Self {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u64(nanos);
        hasher.write_u64(salt);
        Self(hasher.finish())
    }
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::dice::{BANK, BANK_CURRENT, BANK_USED};

    const HEAD: &str = "book,reference,question,A,B,C,D,correct\n";
    fn csv(rows: &[(&str, &str, bool)]) -> String {
        let body: String = rows
            .iter()
            .map(|(book, q, options)| if *options { format!("{book},r,{q},a,b,c,d,B\n") } else { format!("{book},r,{q},,,,,open\n") })
            .collect();
        format!("{HEAD}{body}")
    }
    fn bank_values(rows: &[(&str, &str, bool)]) -> Values {
        let mut v = Values::new();
        import_bank(&mut v, &csv(rows)).unwrap();
        v
    }
    fn used(v: &Values) -> Vec<usize> {
        let mut u = used_rows(v);
        u.sort_unstable();
        u
    }
    fn team_project(sizes: &[usize]) -> Project {
        let mut p = Project::fresh();
        p.teams.truncate(0);
        let template = Project::fresh().teams[0].clone();
        for (i, size) in sizes.iter().enumerate() {
            let mut t = template.clone();
            t.id = i as u64 + 1;
            t.label = format!("Team {i}");
            t.name_key = format!("team{i}.name");
            t.score_key = format!("team{i}.score");
            p.values.insert(t.score_key.clone(), "0".into());
            p.teams.push(t);
            let names: Vec<String> = (0..*size).map(|n| format!("T{i}P{n}")).collect();
            set_participants(&mut p, i, &names.join("\n"));
        }
        ensure_defaults(&mut p);
        p
    }
    fn who(p: &Project) -> String {
        p.values[keys::TURN_NAME].clone()
    }

    #[test]
    fn import_is_all_or_nothing_and_resets_used() {
        let mut v = bank_values(&[("1 Samuel", "q1", true), ("2 Reyes", "q2", true)]);
        next_question(&mut v).unwrap();
        let old = v.clone();
        assert!(import_bank(&mut v, &format!("{HEAD}x,r,,,,,,\n")).is_err());
        assert_eq!(v, old);
        let summary = import_bank(&mut v, &csv(&[("A", "q", true)])).unwrap();
        assert_eq!((summary.rows, summary.per_book.len()), (1, 1));
        assert_eq!((v[BANK_USED].as_str(), v[BANK_CURRENT].as_str(), v[keys::QUESTION_TEXT].as_str()), ("", "", ""));
    }
    #[test]
    fn bank_status_reports_books_filter_and_usage() {
        let mut v = bank_values(&[("1 Samuel", "q1", true), ("2 Reyes", "q2", true), ("1 Samuel", "q3", false)]);
        assert!(!bank_status(&Values::new()).loaded);
        set_bank_filter(&mut v, "2 Reyes");
        next_question(&mut v).unwrap();
        let s = bank_status(&v);
        assert_eq!((s.loaded, s.total, s.used, s.filter.as_str()), (true, 3, 1, "2 Reyes"));
        assert_eq!(s.books, ["1 Samuel", "2 Reyes"]);
        reset_used(&mut v);
        assert_eq!(bank_status(&v).used, 0);
    }
    #[test]
    fn draws_never_repeat_until_exhausted_then_error() {
        let mut v = bank_values(&[("A", "q1", true), ("A", "q2", false), ("B", "q3", true), ("B", "q4", true)]);
        for drawn in 1..=4 {
            next_question(&mut v).unwrap();
            assert_eq!(used(&v).len(), drawn);
            assert_eq!(used(&v), { let mut u = used_rows(&v); u.dedup(); u.sort_unstable(); u });
        }
        assert_eq!(used(&v), [1, 2, 3, 4]);
        let err = next_question(&mut v).unwrap_err();
        assert!(err.to_string().contains("used"));
        assert_eq!(used(&v).len(), 4);
        reset_used(&mut v);
        assert!(next_question(&mut v).is_ok());
    }
    #[test]
    fn draw_without_bank_errors_plainly() {
        let mut v = Values::new();
        assert_eq!(next_question(&mut v), Err(CommandError::Dice("No questions loaded. Import a CSV to draw questions.".into())));
    }
    #[test]
    fn filter_is_honoured_and_exhaustion_mentions_it() {
        let mut v = bank_values(&[("A", "q1", true), ("B", "q2", true), ("B", "q3", true)]);
        set_bank_filter(&mut v, "A");
        next_question(&mut v).unwrap();
        assert_eq!(v[keys::QUESTION_TEXT], "q1");
        let err = next_question(&mut v).unwrap_err().to_string();
        assert!(err.contains("for A") && err.contains("filter"));
        set_bank_filter(&mut v, "B");
        for _ in 0..2 {
            next_question(&mut v).unwrap();
            assert!(["q2", "q3"].contains(&v[keys::QUESTION_TEXT].as_str()));
        }
        set_bank_filter(&mut v, "Nope");
        assert!(next_question(&mut v).unwrap_err().to_string().contains("no questions for Nope"));
    }
    #[test]
    fn yellow_skips_rows_without_options_and_says_so_when_none_left() {
        let mut v = bank_values(&[("A", "open1", false), ("A", "open2", false), ("A", "mc", true)]);
        v.insert(keys::FACE.into(), "1".into());
        next_question(&mut v).unwrap();
        assert_eq!(v[keys::QUESTION_TEXT], "mc");
        let err = next_question(&mut v).unwrap_err().to_string();
        assert!(err.contains("four options"));
        assert_eq!(used(&v), [3]);
        v.insert(keys::FACE.into(), "0".into());
        next_question(&mut v).unwrap();
        assert!(v[keys::QUESTION_TEXT].starts_with("open"));
    }
    #[test]
    fn options_only_fill_when_landed_on_yellow() {
        let mut v = bank_values(&[("A", "q1", true)]);
        // yellow rolled, still rolling: options wait for the landing
        v.insert(keys::FACE.into(), "1".into());
        next_question(&mut v).unwrap();
        assert!(keys::OPTION_KEYS.iter().all(|k| v[*k].is_empty()));
        assert!(reveal_roll(&mut v));
        assert_eq!(keys::OPTION_KEYS.map(|k| v[k].clone()), ["a", "b", "c", "d"]);
        // landed on yellow, then draw: filled immediately
        reset_used(&mut v);
        next_question(&mut v).unwrap();
        assert_eq!(v[keys::OPTION_KEYS[2]], "c");
        // any other colour leaves them empty
        let mut v = bank_values(&[("A", "q1", true)]);
        v.insert(keys::FACE.into(), "3".into());
        next_question(&mut v).unwrap();
        reveal_roll(&mut v);
        assert!(keys::OPTION_KEYS.iter().all(|k| v[*k].is_empty()));
        assert_eq!(v[keys::HOST_ANSWER], "B · b");
        assert_eq!(v[keys::HOST_REF], "r");
    }
    #[test]
    fn roll_sets_face_hides_result_and_clears_question() {
        let mut v = bank_values(&[("A", "q1", true)]);
        next_question(&mut v).unwrap();
        v.insert(keys::STATUS.into(), keys::STATUS_ANNULLED.into());
        v.insert(keys::REVEALED.into(), "1".into());
        roll(&mut v).unwrap();
        let c = DiceConfig::read(&v);
        assert_eq!(c.roll_id, 1);
        assert!(c.face.is_some());
        for k in [keys::RESULT, keys::RULE, keys::STATUS, keys::QUESTION_TEXT, keys::HOST_ANSWER, keys::HOST_REF, keys::REVEALED, BANK_CURRENT] {
            assert_eq!(v[k], "", "{k}");
        }
        roll(&mut v).unwrap();
        assert_eq!(DiceConfig::read(&v).roll_id, 2);
        assert_eq!(used(&v), [1]); // used list survives rolls
    }
    #[test]
    fn every_face_is_reachable_and_always_in_range() {
        let mut seen = [false; 6];
        let mut v = Values::new();
        for seed in 0..400u64 {
            roll_with(&mut v, &mut Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            let face: usize = v[keys::FACE].parse().unwrap();
            assert!(face <= 5);
            seen[face] = true;
        }
        assert!(seen.iter().all(|s| *s), "{seen:?}");
    }
    #[test]
    fn reveal_publishes_result_and_editable_rule() {
        let mut v = Values::new();
        assert!(!reveal_roll(&mut v));
        v.insert(keys::FACE.into(), "5".into());
        v.insert(format!("{}5", keys::RULES_PREFIX), "Verde – otra".into());
        assert!(reveal_roll(&mut v));
        assert_eq!((v[keys::RESULT].as_str(), v[keys::RULE].as_str()), ("VERDE", "Verde – otra"));
        v.insert(keys::FACE.into(), "0".into());
        reveal_roll(&mut v);
        assert_eq!(v[keys::RULE], Face::Rojo.default_rule());
    }
    #[test]
    fn reveal_answer_toggles() {
        let mut v = Values::new();
        toggle_reveal(&mut v);
        assert_eq!(v[keys::REVEALED], "1");
        toggle_reveal(&mut v);
        assert_eq!(v[keys::REVEALED], "");
    }
    #[test]
    fn turns_rotate_across_teams_of_different_sizes() {
        let mut p = team_project(&[2, 1, 3]);
        assert_eq!(who(&p), "T0P0");
        let expected = ["T1P0", "T2P0", "T0P1", "T1P0", "T2P1", "T0P0", "T1P0", "T2P2", "T0P1"];
        for name in expected {
            next_participant(&mut p).unwrap();
            assert_eq!(who(&p), name);
        }
        assert_eq!(p.values[keys::TURN_TEAM_NAME], "Team 0");
        let view = turn_view(&p);
        assert_eq!((view.team_index, view.participant.as_str(), view.position.as_str()), (0, "T0P1", "Participante 2 de 2"));
        assert!(view.has_participants);
    }
    #[test]
    fn team_without_participants_is_skipped_and_none_errors() {
        let mut p = team_project(&[0, 2, 0]);
        assert_eq!(turn_view(&p).participant, "");
        next_participant(&mut p).unwrap();
        assert_eq!((p.values[keys::TURN_TEAM].as_str(), who(&p).as_str()), ("1", "T1P0"));
        next_participant(&mut p).unwrap();
        assert_eq!(who(&p), "T1P1");
        let mut empty = team_project(&[0, 0]);
        assert_eq!(next_participant(&mut empty), Err(CommandError::Dice("Add participants first".into())));
        assert_eq!(previous_participant(&mut empty), Err(CommandError::Dice("Add participants first".into())));
        assert!(!turn_view(&empty).has_participants);
    }
    #[test]
    fn next_participant_clears_the_question_state() {
        let mut p = team_project(&[1, 1]);
        p.values.insert(BANK.into(), csv(&[("A", "q", true)]));
        next_question(&mut p.values).unwrap();
        p.values.insert(keys::STATUS.into(), keys::STATUS_ANNULLED.into());
        next_participant(&mut p).unwrap();
        for k in [keys::QUESTION_TEXT, keys::HOST_ANSWER, keys::HOST_REF, keys::REVEALED, keys::STATUS, BANK_CURRENT, keys::OPTION_KEYS[0]] {
            assert_eq!(p.values[k], "", "{k}");
        }
        assert_eq!(used(&p.values), [1]);
    }
    #[test]
    fn previous_participant_undoes_next_including_wraps() {
        let mut p = team_project(&[2, 2]);
        let mut trail = vec![who(&p)];
        for _ in 0..5 {
            next_participant(&mut p).unwrap();
            trail.push(who(&p));
        }
        for expected in trail.iter().rev().skip(1) {
            previous_participant(&mut p).unwrap();
            assert_eq!(&who(&p), expected);
        }
        let mut solo = team_project(&[3]);
        next_participant(&mut solo).unwrap();
        assert_eq!(who(&solo), "T0P1");
        previous_participant(&mut solo).unwrap();
        assert_eq!(who(&solo), "T0P0");
        previous_participant(&mut solo).unwrap();
        assert_eq!(who(&solo), "T0P2");
    }
    #[test]
    fn set_participants_trims_drops_blanks_and_refreshes_turn() {
        let mut p = team_project(&[0, 0]);
        set_participants(&mut p, 0, "  Ana \n\n Beto\r\n   \n");
        assert_eq!(participants_text(&p, 0), "Ana\nBeto");
        assert_eq!(who(&p), "Ana");
        set_participants(&mut p, 0, "Cris");
        assert_eq!(who(&p), "Cris");
        assert_eq!(participants_text(&p, 5), "");
    }
    #[test]
    fn award_uses_the_current_team_and_reports_events() {
        let mut p = team_project(&[1, 1]);
        next_participant(&mut p).unwrap();
        let mut outcome = CommandOutcome::default();
        award_current_team(&mut p, keys::POINTS, &mut outcome).unwrap();
        assert_eq!((p.values["team0.score"].as_str(), p.values["team1.score"].as_str()), ("0", "10"));
        assert_eq!(outcome.events, vec![CueEvent::ScoreAdded { field: "team1.score".into() }]);
        assert!(outcome.document_changed);
        let mut outcome = CommandOutcome::default();
        award_current_team(&mut p, -10, &mut outcome).unwrap();
        assert_eq!(p.values["team1.score"], "0");
        assert_eq!(outcome.events, vec![CueEvent::ScoreRemoved { field: "team1.score".into() }]);
        let mut none = team_project(&[]);
        assert_eq!(award_current_team(&mut none, 10, &mut CommandOutcome::default()), Err(CommandError::Dice("Nobody is up yet".into())));
    }
    #[test]
    fn question_timer_starts_full_and_clears_annulled() {
        let mut p = Project::fresh();
        p.clocks[0].value_key = keys::QUESTION_CLOCK_KEY.into();
        p.clocks[0].seconds = 3.;
        p.values.insert(keys::STATUS.into(), keys::STATUS_ANNULLED.into());
        let mut clocks = vec![GameClock::new()];
        let mut outcome = CommandOutcome::default();
        start_question_timer(&mut p, &mut clocks, &mut outcome).unwrap();
        assert!(clocks[0].running);
        assert_eq!((p.clocks[0].seconds, p.values[keys::STATUS].as_str(), p.values[keys::QUESTION_CLOCK_KEY].as_str()), (30., "", "00:30"));
        assert_eq!(timer_state(&p, true), 1);
        assert_eq!(timer_state(&p, false), 0);
        p.values.insert(keys::STATUS.into(), keys::STATUS_ANNULLED.into());
        assert_eq!(timer_state(&p, false), 2);
        reset_question_clock(&mut p, &mut clocks);
        assert!(!clocks[0].running);
        let mut other = Project::fresh();
        assert!(start_question_timer(&mut other, &mut vec![GameClock::new()], &mut CommandOutcome::default()).is_err());
    }
}
