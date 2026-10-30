//! El Dado commands over `domain::dice` values: rolls, question draws, turns and the question clock.
use super::{fx, game_commands::{self, CommandError, CommandOutcome}};
use crate::{
    domain::{
        dice::{self as keys, DiceConfig, Face},
        wheel::hex,
        question_bank::{self, Bank, Question},
    },
    model::{CueEvent, GameClock, Project, clock_text},
};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    hash::{BuildHasher, Hasher, RandomState},
    rc::Rc,
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
    /// Unused rows that match the book filter (and have all four options when yellow landed).
    pub remaining: usize,
    /// Distinct books present, file order.
    pub books: Vec<String>,
    pub filter: String,
}
pub struct TurnView {
    /// Index into `project.teams`.
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

/// True once points were awarded this turn (cleared by roll, next participant, back one and import).
pub fn awarded(values: &Values) -> bool {
    get(values, keys::AWARDED) == "1"
}

/// Inserts the bindable text keys and the first turn when missing, leaving existing values untouched.
pub fn ensure_defaults(project: &mut Project) {
    let values = &mut project.values;
    for key in [keys::RESULT, keys::RULE, keys::STATUS, keys::DICE_COLOR, keys::QUESTION_TEXT, keys::HOST_ANSWER, keys::HOST_REF, keys::REVEALED]
        .into_iter()
        .chain(keys::OPTION_KEYS)
    {
        values.entry(key.into()).or_default();
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
    for key in [keys::RESULT, keys::RULE, keys::STATUS, keys::DICE_COLOR, keys::AWARDED] {
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
    set(values, keys::DICE_COLOR, hex(config.colors[face.index()]));
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

thread_local! {
    /// The last bank text seen and what it parsed to, so refreshes and draws do not re-parse it.
    static BANK_CACHE: RefCell<Option<(String, Option<Rc<Bank>>)>> = const { RefCell::new(None) };
}

fn load_bank(values: &Values) -> Option<Rc<Bank>> {
    let text = values.get(keys::BANK)?;
    if text.trim().is_empty() {
        return None;
    }
    BANK_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((csv, bank)) = cache.as_ref() {
            if csv == text {
                return bank.clone();
            }
        }
        let bank = question_bank::parse(text).ok().map(Rc::new);
        *cache = Some((text.clone(), bank.clone()));
        bank
    })
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
    for ((key, letter), option) in keys::OPTION_KEYS.iter().zip(["A", "B", "C", "D"]).zip(&question.options) {
        set(values, key, if option.is_empty() { String::new() } else { format!("{letter} · {option}") });
    }
}

fn clear_question(values: &mut Values) {
    for key in [keys::QUESTION_TEXT, keys::HOST_ANSWER, keys::HOST_REF, keys::REVEALED, keys::BANK_CURRENT].into_iter().chain(keys::OPTION_KEYS) {
        set(values, key, "");
    }
}

/// Replaces the bank with `csv`, or leaves everything untouched and explains why not.
pub fn import_bank(values: &mut Values, csv: &str) -> Result<BankSummary, String> {
    if csv.len() > question_bank::MAX_BYTES {
        return Err(format!("The file is too large ({} MB). The limit is {} MB.", csv.len().div_ceil(1 << 20), question_bank::MAX_BYTES >> 20));
    }
    let bank = question_bank::parse(csv)?;
    if bank.rows.len() > question_bank::MAX_ROWS {
        return Err(format!("The file has {} questions. The limit is {}.", bank.rows.len(), question_bank::MAX_ROWS));
    }
    set(values, keys::BANK, csv.strip_prefix('\u{feff}').unwrap_or(csv));
    set(values, keys::BANK_USED, "");
    set(values, keys::AWARDED, "");
    clear_question(values);
    if !get(values, keys::BANK_FILTER).is_empty() && !bank.books().iter().any(|b| b == get(values, keys::BANK_FILTER)) {
        set(values, keys::BANK_FILTER, "");
    }
    Ok(BankSummary { rows: bank.rows.len(), per_book: bank.per_book() })
}

pub fn bank_status(values: &Values) -> BankStatus {
    let filter = get(values, keys::BANK_FILTER).to_string();
    let Some(bank) = load_bank(values) else {
        return BankStatus { loaded: false, total: 0, used: 0, remaining: 0, books: vec![], filter };
    };
    let mut used = used_rows(values);
    used.sort_unstable();
    used.dedup();
    let yellow = get(values, keys::RESULT) == Face::Amarillo.name();
    let remaining = (1..=bank.rows.len())
        .filter(|n| used.binary_search(n).is_err())
        .filter(|n| filter.is_empty() || bank.rows[n - 1].book == filter)
        .filter(|n| !yellow || bank.rows[n - 1].has_options())
        .count();
    BankStatus { loaded: true, total: bank.rows.len(), used: used.iter().filter(|n| (1..=bank.rows.len()).contains(*n)).count(), remaining, books: bank.books(), filter }
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
    if get(values, keys::RESULT).is_empty() {
        let rolling = DiceConfig::read(values).face.is_some();
        return Err(CommandError::Dice(if rolling { "The dice is still rolling" } else { "Roll the dice first" }.into()));
    }
    let yellow = get(values, keys::RESULT) == Face::Amarillo.name();
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
    if yellow {
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
/// Index of the team that is up, from the team id stored in `TURN_TEAM`; `None` when nobody is up or that team is gone.
fn turn_team(project: &Project) -> Option<usize> {
    let id: u64 = get(&project.values, keys::TURN_TEAM).parse().ok().filter(|id| *id != 0)?;
    project.teams.iter().position(|t| t.id == id)
}
/// True when `TURN_TEAM` names a team that no longer exists (as opposed to nobody being up yet).
fn turn_team_gone(project: &Project) -> bool {
    !matches!(get(&project.values, keys::TURN_TEAM), "" | "0") && turn_team(project).is_none()
}
/// Where the turn lands when nobody is up or the team is gone: the first team with participants.
fn fallback_team(project: &Project) -> usize {
    (0..project.teams.len()).find(|t| has_participants(project, *t)).unwrap_or(0)
}
fn up_team(project: &Project) -> usize {
    turn_team(project).unwrap_or_else(|| fallback_team(project))
}
/// Points `TURN_TEAM` at the fallback team when nobody is up or the team is gone, dropping the round of a vanished team.
fn repair_turn(project: &mut Project) {
    if turn_team(project).is_some() || project.teams.is_empty() {
        return;
    }
    if turn_team_gone(project) {
        clear_round(&mut project.values);
    }
    let id = project.teams[fallback_team(project)].id;
    set(&mut project.values, keys::TURN_TEAM, id.to_string());
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
    repair_turn(project);
    let team = up_team(project);
    let (who, team_label) = (current_participant(project, team), team_name(project, team));
    set(&mut project.values, keys::TURN_NAME, who);
    set(&mut project.values, keys::TURN_TEAM_NAME, team_label);
}

/// Read-only: when the team that was up is gone this reports the fallback team; `ensure_defaults` and the next turn action make it stick.
pub fn turn_view(project: &Project) -> TurnView {
    let team = up_team(project);
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

/// A new turn: no roll, rule, die colour, question, options, annulled banner or award left over.
fn clear_round(values: &mut Values) {
    clear_question(values);
    for key in [keys::RESULT, keys::RULE, keys::FACE, keys::DICE_COLOR, keys::STATUS, keys::AWARDED] {
        set(values, key, "");
    }
}

/// Advances the team that just played, then hands the turn to the next team that has participants.
pub fn next_participant(project: &mut Project) -> Result<(), CommandError> {
    let teams = project.teams.len();
    if !(0..teams).any(|t| has_participants(project, t)) {
        return Err(CommandError::Dice("Add participants first".into()));
    }
    let gone = turn_team_gone(project);
    ensure_defaults(project);
    if gone {
        return Ok(()); // landed on the first team with participants; nothing was played yet
    }
    let current = up_team(project);
    if let Some(count) = project.teams.get(current).map(|t| participants(&project.values, t.id).len()).filter(|c| *c > 0) {
        let next = (turn_index(project, current) + 1) % count;
        set_turn_index(project, current, next);
    }
    if let Some(next) = (1..=teams).map(|step| (current + step) % teams).find(|t| has_participants(project, *t)) {
        set(&mut project.values, keys::TURN_TEAM, project.teams[next].id.to_string());
    }
    clear_round(&mut project.values);
    refresh_turn(project);
    Ok(())
}

/// "Back one": undoes the last `next_participant`. Callers with a running session use `back_one`, which also resets the question clock.
pub fn previous_participant(project: &mut Project) -> Result<(), CommandError> {
    let teams = project.teams.len();
    if !(0..teams).any(|t| has_participants(project, t)) {
        return Err(CommandError::Dice("Add participants first".into()));
    }
    let gone = turn_team_gone(project);
    ensure_defaults(project);
    if gone {
        return Ok(());
    }
    let current = up_team(project);
    let Some(previous) = (1..=teams).map(|step| (current + teams - step % teams) % teams).find(|t| has_participants(project, *t)) else {
        return Ok(());
    };
    let count = participants(&project.values, project.teams[previous].id).len();
    let index = (turn_index(project, previous) + count - 1) % count;
    set_turn_index(project, previous, index);
    set(&mut project.values, keys::TURN_TEAM, project.teams[previous].id.to_string());
    clear_round(&mut project.values);
    refresh_turn(project);
    Ok(())
}

/// `previous_participant` plus a stopped, full question clock so the turn we go back to starts clean.
pub fn back_one(project: &mut Project, clocks: &mut [GameClock]) -> Result<(), CommandError> {
    previous_participant(project)?;
    reset_question_clock(project, clocks);
    Ok(())
}

/// Adds (or removes, when negative) points for the team that is up, like `Action::Adjust` does.
pub fn award_current_team(project: &mut Project, points: i64, outcome: &mut CommandOutcome) -> Result<(), CommandError> {
    if turn_team_gone(project) {
        return Err(CommandError::Dice("The team that was up no longer exists. Click Next participant.".into()));
    }
    ensure_defaults(project);
    let team = turn_team(project).ok_or_else(|| CommandError::Dice("Nobody is up yet".into()))?;
    let field = project.teams[team].score_key.clone();
    if project.values.get(&field).is_some_and(|v| v.trim().parse::<i64>().is_err()) {
        return Err(CommandError::NonNumericField(field));
    }
    project.adjust_score(&field, points);
    if points != 0 {
        set(&mut project.values, keys::AWARDED, if points > 0 { "1" } else { "" });
    }
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

    const COLORS_KEY: &str = keys::COLORS;
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
    /// A finished roll of face `index`, as `roll` + `reveal_roll` leave the values.
    fn land(v: &mut Values, index: usize) {
        v.insert(keys::FACE.into(), index.to_string());
        assert!(reveal_roll(v));
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
        land(&mut v, 0);
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
        land(&mut v, 0);
        assert_eq!(bank_status(&v).remaining, 1);
        next_question(&mut v).unwrap();
        let s = bank_status(&v);
        assert_eq!((s.loaded, s.total, s.used, s.remaining, s.filter.as_str()), (true, 3, 1, 0, "2 Reyes"));
        assert_eq!(s.books, ["1 Samuel", "2 Reyes"]);
        reset_used(&mut v);
        assert_eq!(bank_status(&v).used, 0);
        set_bank_filter(&mut v, "");
        assert_eq!(bank_status(&v).remaining, 3);
        land(&mut v, 1);
        assert_eq!(bank_status(&v).remaining, 2); // yellow needs all four options
    }
    #[test]
    fn draws_never_repeat_until_exhausted_then_error() {
        let mut v = bank_values(&[("A", "q1", true), ("A", "q2", false), ("B", "q3", true), ("B", "q4", true)]);
        land(&mut v, 0);
        let mut seen = std::collections::HashSet::new();
        for drawn in 1..=4 {
            next_question(&mut v).unwrap();
            let raw = used_rows(&v); // draw order, not sorted
            assert_eq!(raw.len(), drawn);
            assert_eq!(raw.iter().collect::<std::collections::HashSet<_>>().len(), drawn, "a row was drawn twice: {raw:?}");
            let current: usize = v[BANK_CURRENT].parse().unwrap();
            assert!(seen.insert(current), "row {current} came up again");
            assert_eq!(raw.last(), Some(&current));
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
        land(&mut v, 0);
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
        land(&mut v, 1);
        next_question(&mut v).unwrap();
        assert_eq!(v[keys::QUESTION_TEXT], "mc");
        let err = next_question(&mut v).unwrap_err().to_string();
        assert!(err.contains("four options"));
        assert_eq!(used(&v), [3]);
        land(&mut v, 0);
        next_question(&mut v).unwrap();
        assert!(v[keys::QUESTION_TEXT].starts_with("open"));
    }
    #[test]
    fn options_only_fill_when_landed_on_yellow_and_carry_their_letter() {
        let mut v = bank_values(&[("A", "q1", true)]);
        // a pending roll (face chosen, nothing landed) cannot draw
        v.insert(keys::FACE.into(), "1".into());
        assert_eq!(next_question(&mut v), Err(CommandError::Dice("The dice is still rolling".into())));
        // landed on yellow, then draw: filled immediately
        land(&mut v, 1);
        next_question(&mut v).unwrap();
        assert_eq!(keys::OPTION_KEYS.map(|k| v[k].clone()), ["A · a", "B · b", "C · c", "D · d"]);
        // a redraw is allowed ("Skip question")
        reset_used(&mut v);
        next_question(&mut v).unwrap();
        assert_eq!(v[keys::OPTION_KEYS[2]], "C · c");
        // any other colour leaves them empty
        let mut v = bank_values(&[("A", "q1", true)]);
        land(&mut v, 3);
        next_question(&mut v).unwrap();
        assert!(keys::OPTION_KEYS.iter().all(|k| v[*k].is_empty()));
        assert_eq!(v[keys::HOST_ANSWER], "B · b");
        assert_eq!(v[keys::HOST_REF], "r");
    }
    #[test]
    fn a_stale_face_cannot_put_options_on_screen() {
        let mut v = bank_values(&[("A", "q1", true)]);
        land(&mut v, 3); // landed naranja
        v.insert(keys::FACE.into(), "1".into()); // FACE says yellow, the landed RESULT does not
        next_question(&mut v).unwrap();
        assert!(keys::OPTION_KEYS.iter().all(|k| v[*k].is_empty()));
    }
    #[test]
    fn next_question_needs_a_landed_roll() {
        let mut v = bank_values(&[("A", "q1", true)]);
        assert_eq!(next_question(&mut v), Err(CommandError::Dice("Roll the dice first".into())));
        roll(&mut v).unwrap();
        assert_eq!(next_question(&mut v), Err(CommandError::Dice("The dice is still rolling".into())));
        assert!(used(&v).is_empty());
        assert!(reveal_roll(&mut v));
        assert!(next_question(&mut v).is_ok());
    }
    #[test]
    fn bank_text_is_parsed_once_and_limits_are_enforced() {
        let mut v = bank_values(&[("A", "q1", true), ("A", "q2", true)]);
        assert!(Rc::ptr_eq(&load_bank(&v).unwrap(), &load_bank(&v).unwrap()));
        let other = bank_values(&[("B", "q9", true)]);
        assert_eq!(load_bank(&other).unwrap().rows.len(), 1);
        assert_eq!(load_bank(&v).unwrap().rows.len(), 2);
        let big = format!("{HEAD}{}", "a,r,q,1,2,3,4,A\n".repeat(question_bank::MAX_ROWS + 1));
        let err = import_bank(&mut v, &big).err().unwrap();
        assert!(err.contains("5000"), "{err}");
        let huge = format!("{HEAD}{}", "x".repeat(question_bank::MAX_BYTES));
        assert!(import_bank(&mut v, &huge).err().unwrap().contains("too large"));
        assert_eq!(bank_status(&v).total, 2);
    }
    #[test]
    fn roll_sets_face_hides_result_and_clears_question() {
        let mut v = bank_values(&[("A", "q1", true)]);
        land(&mut v, 0);
        next_question(&mut v).unwrap();
        v.insert(keys::STATUS.into(), keys::STATUS_ANNULLED.into());
        v.insert(keys::REVEALED.into(), "1".into());
        v.insert(keys::AWARDED.into(), "1".into());
        roll(&mut v).unwrap();
        let c = DiceConfig::read(&v);
        assert_eq!(c.roll_id, 1);
        assert!(c.face.is_some());
        for k in [keys::RESULT, keys::RULE, keys::STATUS, keys::DICE_COLOR, keys::AWARDED, keys::QUESTION_TEXT, keys::HOST_ANSWER, keys::HOST_REF, keys::REVEALED, BANK_CURRENT] {
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
        assert_eq!(v[keys::DICE_COLOR], "#62F0A0");
        v.insert(COLORS_KEY.into(), "#000001".into());
        v.insert(keys::FACE.into(), "0".into());
        reveal_roll(&mut v);
        assert_eq!(v[keys::RULE], Face::Rojo.default_rule());
        assert_eq!(v[keys::DICE_COLOR], "#000001");
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
        assert_eq!(p.values[keys::TURN_TEAM], "1"); // the team id, not its index
        let view = turn_view(&p);
        assert_eq!((view.team_index, view.participant.as_str(), view.position.as_str()), (0, "T0P1", "Participante 2 de 2"));
        assert!(view.has_participants);
    }
    #[test]
    fn team_without_participants_is_skipped_and_none_errors() {
        let mut p = team_project(&[0, 2, 0]);
        assert_eq!(turn_view(&p).participant, "");
        next_participant(&mut p).unwrap();
        assert_eq!((p.values[keys::TURN_TEAM].as_str(), who(&p).as_str()), ("2", "T1P0"));
        next_participant(&mut p).unwrap();
        assert_eq!(who(&p), "T1P1");
        let mut empty = team_project(&[0, 0]);
        assert_eq!(next_participant(&mut empty), Err(CommandError::Dice("Add participants first".into())));
        assert_eq!(previous_participant(&mut empty), Err(CommandError::Dice("Add participants first".into())));
        assert!(!turn_view(&empty).has_participants);
    }
    #[test]
    fn next_participant_clears_the_whole_round() {
        let mut p = team_project(&[1, 1]);
        p.values.insert(BANK.into(), csv(&[("A", "q", true)]));
        land(&mut p.values, 1);
        next_question(&mut p.values).unwrap();
        p.values.insert(keys::STATUS.into(), keys::STATUS_ANNULLED.into());
        p.values.insert(keys::AWARDED.into(), "1".into());
        next_participant(&mut p).unwrap();
        for k in [keys::QUESTION_TEXT, keys::HOST_ANSWER, keys::HOST_REF, keys::REVEALED, keys::STATUS, BANK_CURRENT, keys::OPTION_KEYS[0], keys::RESULT, keys::RULE, keys::FACE, keys::DICE_COLOR, keys::AWARDED] {
            assert_eq!(p.values[k], "", "{k}");
        }
        assert_eq!(used(&p.values), [1]);
        assert!(!awarded(&p.values));
        // "Back one" clears it too
        land(&mut p.values, 2);
        previous_participant(&mut p).unwrap();
        assert_eq!((p.values[keys::RESULT].as_str(), p.values[keys::FACE].as_str()), ("", ""));
    }
    #[test]
    fn next_and_back_reset_the_question_clock() {
        let mut p = team_project(&[1, 1]);
        p.clocks[0].value_key = keys::QUESTION_CLOCK_KEY.into();
        let mut clocks = vec![GameClock::new()];
        start_question_timer(&mut p, &mut clocks, &mut CommandOutcome::default()).unwrap();
        p.clocks[0].seconds = 12.;
        back_one(&mut p, &mut clocks).unwrap();
        assert!(!clocks[0].running);
        assert_eq!((p.clocks[0].seconds, p.values[keys::QUESTION_CLOCK_KEY].as_str()), (keys::QUESTION_SECONDS, "00:30"));
    }
    #[test]
    fn a_vanished_team_falls_back_and_refuses_points() {
        let mut p = team_project(&[1, 1, 1]);
        next_participant(&mut p).unwrap();
        next_participant(&mut p).unwrap();
        assert_eq!(p.values[keys::TURN_TEAM], "3");
        p.values.insert(keys::STATUS.into(), keys::STATUS_ANNULLED.into());
        p.teams.pop();
        assert_eq!(turn_view(&p).team_index, 0);
        assert_eq!(
            award_current_team(&mut p, 10, &mut CommandOutcome::default()),
            Err(CommandError::Dice("The team that was up no longer exists. Click Next participant.".into()))
        );
        assert_eq!((p.values["team0.score"].as_str(), p.values["team1.score"].as_str()), ("0", "0"));
        ensure_defaults(&mut p);
        assert_eq!((p.values[keys::TURN_TEAM].as_str(), p.values[keys::STATUS].as_str(), who(&p).as_str()), ("1", "", "T0P0"));
    }
    #[test]
    fn nobody_up_yet_starts_on_the_first_team() {
        for seed in ["", "0", "x"] {
            let mut p = team_project(&[1, 1]);
            p.values.insert(keys::TURN_TEAM.into(), seed.into());
            assert_eq!(turn_view(&p).team_index, 0);
            ensure_defaults(&mut p);
            assert_eq!(p.values[keys::TURN_TEAM], "1", "{seed:?}");
        }
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
        assert!(awarded(&p.values));
        let mut outcome = CommandOutcome::default();
        award_current_team(&mut p, -10, &mut outcome).unwrap();
        assert!(!awarded(&p.values));
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
    // ---- QA: failure kinds, resets, limits and distribution ----

    /// A bank with a question on screen and one used row, as a host mid-game would have it.
    fn mid_game() -> Values {
        let mut v = bank_values(&[("1 Samuel", "q1", true), ("2 Reyes", "q2", true), ("2 Reyes", "q3", false)]);
        set_bank_filter(&mut v, "2 Reyes");
        land(&mut v, 3);
        next_question(&mut v).unwrap();
        v
    }
    #[test]
    fn every_kind_of_failed_import_leaves_bank_used_and_question_untouched() {
        let rows = |n: usize| "a,r,q,1,2,3,4,A\n".repeat(n);
        let cases: Vec<(&str, String, &str)> = vec![
            ("semicolons", "book;reference;question;A;B;C;D;correct\nx;r;q;a;b;c;d;A\n".into(), "semicolons"),
            ("semicolons with a comma header", format!("{HEAD}x;r;q;a;b;c;d;A\n"), ""),
            ("missing header column", "book,reference,question,A,B,C,correct\nx,r,q,a,b,c,A\n".into(), "d"),
            ("empty question", format!("{HEAD}x,r,,a,b,c,d,A\n"), "Line 2"),
            ("empty correct", format!("{HEAD}x,r,q,a,b,c,d,\n"), "Line 2"),
            ("correct points at an empty option", format!("{HEAD}x,r,q,a,b,,d,C\n"), "Line 2"),
            ("unclosed quote", format!("{HEAD}x,r,\"q,a,b,c,d,A\n"), ""),
            ("over MAX_BYTES", format!("{HEAD}{}", "x".repeat(question_bank::MAX_BYTES)), "too large"),
            ("over MAX_ROWS", format!("{HEAD}{}", rows(question_bank::MAX_ROWS + 1)), "limit"),
            ("header only", HEAD.to_string(), "no questions"),
            ("empty", String::new(), "empty"),
            ("whitespace only", "  \n\n \n".to_string(), ""),
        ];
        let mut v = mid_game();
        let before = v.clone();
        let (bank, used_before, current, question) = (v[BANK].clone(), v[BANK_USED].clone(), v[BANK_CURRENT].clone(), v[keys::QUESTION_TEXT].clone());
        assert!(!used_before.is_empty() && !current.is_empty() && !question.is_empty());
        for (name, text, mentions) in cases {
            let err = import_bank(&mut v, &text).err().unwrap_or_else(|| panic!("{name} was accepted"));
            assert!(!err.is_empty() && err.contains(mentions), "{name}: {err}");
            assert_eq!(v, before, "{name} changed the values");
            assert_eq!((&v[BANK], &v[BANK_USED], &v[BANK_CURRENT], &v[keys::QUESTION_TEXT]), (&bank, &used_before, &current, &question), "{name}");
        }
        // non-UTF-8 bytes die in `decode`, before an import can even start
        for bytes in [&b"book,reference,question,A,B,C,D,correct\nx,r,Se\xf1or,a,b,c,d,A\n"[..], &[0xff, 0xfe, b'b', 0, b'o', 0][..]] {
            let err = question_bank::decode(bytes).unwrap_err();
            assert!(err.contains("UTF-8"), "{err}");
        }
        assert_eq!(v, before);
        // and the same bank still imports afterwards
        assert!(import_bank(&mut v, &csv(&[("A", "fresh", true)])).is_ok());
        assert_eq!(v[BANK_USED], "");
    }
    #[test]
    fn a_failed_import_keeps_the_book_filter_too() {
        let mut v = mid_game();
        assert!(import_bank(&mut v, "garbage").is_err());
        assert_eq!(get(&v, keys::BANK_FILTER), "2 Reyes");
        // a good import drops a filter naming a book the new file lacks
        import_bank(&mut v, &csv(&[("1 Samuel", "q", true)])).unwrap();
        assert_eq!(get(&v, keys::BANK_FILTER), "");
    }
    #[test]
    fn trailing_separator_rows_and_a_bom_import_cleanly() {
        let mut v = Values::new();
        let text = format!("\u{feff}{HEAD}A,r,q1,a,b,c,d,B\n,,,,,,,\nA,r,q2,,,,,open\n,,,,,,,\n , ,,,,,,  \n");
        let summary = import_bank(&mut v, &text).unwrap();
        assert_eq!(summary.rows, 2);
        assert!(!v[BANK].starts_with('\u{feff}'));
        assert_eq!(bank_status(&v).total, 2);
    }
    #[test]
    fn next_participant_and_back_one_reset_clock_and_clear_the_round() {
        use crate::domain::controls::Action;
        for back in [false, true] {
            let mut p = team_project(&[2, 2]);
            p.clocks[0].value_key = keys::QUESTION_CLOCK_KEY.into();
            import_bank(&mut p.values, &csv(&[("A", "q", true)])).unwrap();
            next_participant(&mut p).unwrap(); // so "back" has somewhere to go
            let mut clocks = vec![GameClock::new()];
            land(&mut p.values, 1);
            next_question(&mut p.values).unwrap();
            start_question_timer(&mut p, &mut clocks, &mut CommandOutcome::default()).unwrap();
            p.clocks[0].seconds = 12.;
            p.values.insert(keys::STATUS.into(), keys::STATUS_ANNULLED.into());
            p.values.insert(keys::REVEALED.into(), "1".into());
            award_current_team(&mut p, keys::POINTS, &mut CommandOutcome::default()).unwrap();
            assert!(awarded(&p.values));
            if back {
                back_one(&mut p, &mut clocks).unwrap();
            } else {
                game_commands::dispatch(&mut p, &mut clocks, Action::NextParticipant).unwrap();
            }
            assert!(!clocks[0].running, "back={back}");
            assert_eq!((p.clocks[0].seconds, p.values[keys::QUESTION_CLOCK_KEY].as_str()), (keys::QUESTION_SECONDS, "00:30"), "back={back}");
            for k in [keys::RESULT, keys::RULE, keys::FACE, keys::DICE_COLOR, keys::STATUS, keys::AWARDED, keys::QUESTION_TEXT, keys::HOST_ANSWER, keys::HOST_REF, keys::REVEALED, BANK_CURRENT]
                .into_iter()
                .chain(keys::OPTION_KEYS)
            {
                assert_eq!(p.values[k], "", "back={back} {k}");
            }
            assert_eq!(timer_state(&p, false), 0);
            assert_eq!(used(&p.values), [1]); // the used list is not part of the round
        }
    }
    #[test]
    fn yellow_options_carry_their_letter_and_vanish_on_other_colours() {
        let mut v = bank_values(&[("A", "q1", true)]);
        land(&mut v, 1);
        next_question(&mut v).unwrap();
        let shown = keys::OPTION_KEYS.map(|k| v[k].clone());
        for (option, prefix) in shown.iter().zip(["A · ", "B · ", "C · ", "D · "]) {
            assert!(option.starts_with(prefix), "{option}");
        }
        // landing on another colour for the same question wipes them
        reset_used(&mut v);
        land(&mut v, 4);
        next_question(&mut v).unwrap();
        assert!(keys::OPTION_KEYS.iter().all(|k| v[*k].is_empty()), "{:?}", keys::OPTION_KEYS.map(|k| &v[k]));
        // the host answer stays host-only and lettered
        assert_eq!(v[keys::HOST_ANSWER], "B · b");
        assert!(keys::OPTION_KEYS.iter().all(|k| !k.contains("host")));
    }
    #[test]
    fn awarded_is_set_by_plus_points_and_cleared_by_roll_next_participant_and_import() {
        let mut p = team_project(&[1, 1]);
        let win = |p: &mut Project| {
            award_current_team(p, keys::POINTS, &mut CommandOutcome::default()).unwrap();
            assert!(awarded(&p.values));
        };
        win(&mut p);
        roll(&mut p.values).unwrap();
        assert!(!awarded(&p.values), "roll");
        win(&mut p);
        next_participant(&mut p).unwrap();
        assert!(!awarded(&p.values), "next participant");
        win(&mut p);
        previous_participant(&mut p).unwrap();
        assert!(!awarded(&p.values), "back one");
        win(&mut p);
        import_bank(&mut p.values, &csv(&[("A", "q", true)])).unwrap();
        assert!(!awarded(&p.values), "import");
        // taking points away does not count as awarding
        award_current_team(&mut p, -keys::POINTS, &mut CommandOutcome::default()).unwrap();
        assert!(!awarded(&p.values));
    }
    #[test]
    fn a_deleted_team_never_receives_points_by_falling_back_to_team_zero() {
        // team ids 1,2,3; team id 2 (index 1) is up, then it is deleted
        let mut p = team_project(&[1, 1, 1]);
        next_participant(&mut p).unwrap();
        assert_eq!(p.values[keys::TURN_TEAM], "2");
        let scores = |p: &Project| p.teams.iter().map(|t| p.values[&t.score_key].clone()).collect::<Vec<_>>();
        let before = scores(&p);
        p.teams.remove(1);
        for points in [10, -10] {
            let err = award_current_team(&mut p, points, &mut CommandOutcome::default()).unwrap_err();
            assert_eq!(err, CommandError::Dice("The team that was up no longer exists. Click Next participant.".into()));
        }
        assert_eq!(scores(&p), [before[0].clone(), before[2].clone()]);
        assert!(!awarded(&p.values));
        // deleting a team ahead of the one that is up keeps pointing at the same team by id
        let mut q = team_project(&[1, 1, 1]);
        next_participant(&mut q).unwrap();
        next_participant(&mut q).unwrap();
        assert_eq!(q.values[keys::TURN_TEAM], "3");
        q.teams.remove(0);
        award_current_team(&mut q, 10, &mut CommandOutcome::default()).unwrap();
        assert_eq!((q.values["team1.score"].as_str(), q.values["team2.score"].as_str()), ("0", "10"));
    }
    #[test]
    fn remaining_honours_filter_yellow_and_out_of_range_used_rows() {
        let mut v = bank_values(&[("A", "open", false), ("A", "mc1", true), ("B", "mc2", true), ("B", "open2", false)]);
        let remaining = |v: &Values| bank_status(v).remaining;
        assert_eq!(remaining(&v), 4);
        set_bank_filter(&mut v, "A");
        assert_eq!(remaining(&v), 2);
        set_bank_filter(&mut v, "B");
        land(&mut v, 1); // yellow: only rows with options count
        assert_eq!(remaining(&v), 1);
        set_bank_filter(&mut v, "");
        assert_eq!(remaining(&v), 2);
        land(&mut v, 0); // any other colour
        assert_eq!(remaining(&v), 4);
        // duplicate and out-of-range used entries neither double count nor go negative
        v.insert(BANK_USED.into(), "2,2, 99,x".into());
        let s = bank_status(&v);
        assert_eq!((s.used, s.remaining), (1, 3));
        v.insert(BANK_USED.into(), "1,2,3,4".into());
        let s = bank_status(&v);
        assert_eq!((s.used, s.remaining), (4, 0));
    }
    #[test]
    fn rolls_through_the_real_rng_are_uniform() {
        const N: usize = 6000;
        let mut v = Values::new();
        let mut counts = [0usize; 6];
        let mut faces = Vec::with_capacity(N);
        for n in 1..=N {
            roll(&mut v).unwrap(); // clock-seeded, so the outcome differs per run: the bounds below must never flake
            assert_eq!(v[keys::ROLL_ID], n.to_string());
            let face: usize = v[keys::FACE].parse().unwrap();
            counts[face] += 1;
            faces.push(face);
        }
        // expected 1000 each, sd ~29: 750..1250 is +-8.6 sd; chi-square (5 dof) < 40 has p < 1e-7
        for (face, count) in counts.iter().enumerate() {
            assert!((750..=1250).contains(count), "face {face} came up {count} times: {counts:?}");
        }
        let chi2: f64 = counts.iter().map(|c| (*c as f64 - 1000.).powi(2) / 1000.).sum();
        assert!(chi2 < 40., "chi-square {chi2} for {counts:?}");
        // consecutive rolls repeat about 1 in 6 (expected ~1000, sd ~29)
        let repeats = faces.windows(2).filter(|w| w[0] == w[1]).count();
        assert!((700..=1300).contains(&repeats), "{repeats} repeats");
    }
}
