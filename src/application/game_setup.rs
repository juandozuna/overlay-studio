//! Validated configuration edits. Runtime clocks retain their identity when lists change.
use super::game_session::GameSession;
use crate::{
    domain::wheel::{hex, team_color_key},
    model::{ClockSpec, Project, Protocol, Route, clock_text},
};
use std::{collections::BTreeMap, time::Instant};

/// Preserve runtime identity after document history changes clock metadata.
pub fn reconcile_clocks(game: &mut GameSession, before: &[ClockSpec], after: &[ClockSpec]) {
    let mut runtimes: BTreeMap<_, _> = before
        .iter()
        .map(|c| c.id)
        .zip(std::mem::take(&mut game.clocks))
        .collect();
    game.clocks = after
        .iter()
        .map(|c| {
            let mut runtime = runtimes.remove(&c.id).unwrap_or_default();
            if before
                .iter()
                .find(|old| old.id == c.id)
                .is_some_and(|old| old.seconds != c.seconds || old.countdown != c.countdown)
            {
                runtime.running = false;
                runtime.last = Instant::now();
            }
            runtime
        })
        .collect();
}

/// Keep `team.<id>.color` equal to each team's colour and drop keys of deleted teams.
pub fn sync_team_colors(project: &mut Project) {
    let live: Vec<_> = project.teams.iter().map(|t| team_color_key(t.id)).collect();
    project.values.retain(|key, _| {
        !(key.starts_with("team.") && key.ends_with(".color")) || live.contains(key)
    });
    for team in &project.teams {
        project
            .values
            .insert(team_color_key(team.id), hex(team.color));
    }
}

pub fn parse_time(text: &str) -> Result<f64, String> {
    let (minutes, seconds) = text
        .trim()
        .split_once(':')
        .ok_or("Enter time as minutes:seconds, for example 10:00")?;
    let minutes = minutes.parse::<u64>().map_err(|_| "Enter valid minutes")?;
    let seconds = seconds.parse::<u64>().map_err(|_| "Enter valid seconds")?;
    if seconds >= 60 {
        return Err("Seconds must be between 00 and 59".into());
    }
    let total = minutes
        .checked_mul(60)
        .and_then(|n| n.checked_add(seconds))
        .ok_or("Time is too large")?;
    if total > 604800 {
        return Err("Maximum clock time is seven days".into());
    }
    Ok(total as f64)
}

/// The field names are a small adapter protocol used by the desktop view models.
/// Edits are validated before replacing the live project, including file-loaded data.
pub fn apply(
    project: &mut Project,
    game: &mut GameSession,
    kind: &str,
    index: usize,
    field: &str,
    value: &str,
) -> Result<(), String> {
    let mut candidate = project.clone();
    match kind {
        "team" => match field {
            "add" => {
                if candidate.teams.len() >= 32 {
                    return Err("Maximum 32 teams".into());
                }
                candidate.add_team();
            }
            "remove" => {
                if candidate.teams.len() <= 1 {
                    return Err("Keep at least one team".into());
                }
                if index >= candidate.teams.len() {
                    return Err("Team does not exist".into());
                }
                candidate.remove_team(index);
            }
            _ => {
                let team = candidate
                    .teams
                    .get_mut(index)
                    .ok_or("Team does not exist")?;
                match field {
                    "name" => {
                        candidate.values.insert(team.name_key.clone(), value.into());
                    }
                    "label" => team.label = value.into(),
                    "color" => {
                        let hex = value.trim().trim_start_matches('#');
                        if hex.len() != 6 || !hex.is_ascii() {
                            return Err("Use a six digit color such as #176C86".into());
                        }
                        let color = u32::from_str_radix(hex, 16)
                            .map_err(|_| "Use a valid hexadecimal color")?;
                        team.color = [(color >> 16) as u8, (color >> 8) as u8, color as u8];
                    }
                    "steps" => {
                        let steps: Vec<i64> = value
                            .split(',')
                            .map(|v| {
                                v.trim()
                                    .parse::<i64>()
                                    .map_err(|_| "Separate integer scoring steps with commas")
                            })
                            .collect::<Result<_, _>>()?;
                        if steps.is_empty() || steps.len() > 12 || steps.contains(&0) {
                            return Err("Use 1–12 nonzero scoring steps".into());
                        }
                        team.steps = steps;
                    }
                    _ => return Err("Unknown team setting".into()),
                }
            }
        },
        "clock" => match field {
            "add" => {
                if candidate.clocks.len() >= 16 {
                    return Err("Maximum 16 clocks".into());
                }
                candidate.add_clock();
            }
            "remove" => {
                if candidate.clocks.len() <= 1 {
                    return Err("Keep at least one clock".into());
                }
                if index >= candidate.clocks.len() {
                    return Err("Clock does not exist".into());
                }
                candidate.remove_clock(index);
            }
            _ => {
                let clock = candidate
                    .clocks
                    .get_mut(index)
                    .ok_or("Clock does not exist")?;
                match field {
                    "name" => clock.name = value.into(),
                    "current" => {
                        clock.seconds = parse_time(value)?;
                        candidate
                            .values
                            .insert(clock.value_key.clone(), clock_text(clock.seconds));
                    }
                    "reset" => clock.reset_seconds = parse_time(value)?,
                    "direction" => {
                        clock.countdown = match value {
                            "down" => true,
                            "up" => false,
                            _ => return Err("Choose count up or count down".into()),
                        }
                    }
                    _ => return Err("Unknown clock setting".into()),
                }
            }
        },
        "value" => {
            let key = value.trim();
            match field {
                "add" => {
                    if key.is_empty()
                        || key.chars().count() > 128
                        || key.chars().any(char::is_control)
                    {
                        return Err(
                            "Enter a field name of 1–128 characters without line breaks".into()
                        );
                    }
                    if candidate.values.contains_key(key) {
                        return Err("A field with that name already exists".into());
                    }
                    candidate.values.insert(key.into(), "0".into());
                }
                "remove" => {
                    if candidate
                        .teams
                        .iter()
                        .any(|t| t.name_key == key || t.score_key == key)
                        || candidate.clocks.iter().any(|c| c.value_key == key)
                    {
                        return Err("Use team or clock setup to remove their fields".into());
                    }
                    if !candidate.values.contains_key(key) {
                        return Err("Field no longer exists".into());
                    }
                    candidate.remove_value(key);
                }
                _ => return Err("Unknown custom field setting".into()),
            }
        }
        "output" => {
            if field == "add" {
                if candidate.routes.len() >= 8 {
                    return Err("Maximum eight outputs".into());
                }
                candidate.routes.push(Route {
                    id: 0,
                    name: format!("Overlay Studio • Output {}", candidate.routes.len() + 1),
                    design: 0,
                    design_id: 0,
                    protocol: Protocol::Syphon,
                    fps: 30,
                });
            } else {
                let output = candidate
                    .routes
                    .get_mut(index)
                    .ok_or("Output does not exist")?;
                match field {
                    "name" => output.name = value.into(),
                    "fps" => {
                        output.fps = value.parse().map_err(|_| "Choose a valid frame rate")?;
                        if ![24, 25, 30, 50, 60].contains(&output.fps) {
                            return Err("Choose 24, 25, 30, 50 or 60 fps".into());
                        }
                    }
                    "protocol" => {
                        output.protocol = match value {
                            "Syphon" => Protocol::Syphon,
                            "NDI" => Protocol::Ndi,
                            _ => return Err("Choose Syphon or NDI".into()),
                        }
                    }
                    "design" => output.design = value.parse().map_err(|_| "Choose a design")?,
                    _ => return Err("Unknown output setting".into()),
                }
            }
        }
        _ => return Err("Unknown configuration group".into()),
    }
    if let Some(primary) = candidate.clocks.first() {
        candidate.clock_seconds = primary.seconds;
        candidate.countdown = primary.countdown;
    }
    sync_team_colors(&mut candidate);
    candidate.sync_stable_references()?;
    candidate.validate_metadata()?;
    let old_ids: Vec<_> = project.clocks.iter().map(|c| c.id).collect();
    let runtimes = std::mem::take(&mut game.clocks);
    let mut by_id: BTreeMap<_, _> = old_ids.into_iter().zip(runtimes).collect();
    game.clocks = candidate
        .clocks
        .iter()
        .map(|c| by_id.remove(&c.id).unwrap_or_default())
        .collect();
    if kind == "clock"
        && matches!(field, "current" | "direction")
        && let Some(clock) = game.clocks.get_mut(index)
    {
        clock.running = false;
        clock.last = Instant::now();
    }
    *project = candidate;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_setup_does_not_change_project_or_running_clocks() {
        let mut p = Project::fresh();
        let mut g = GameSession::new(&p);
        g.clocks[0].running = true;
        let before = p.clone();
        assert!(apply(&mut p, &mut g, "clock", 0, "current", "8:99").is_err());
        assert!(apply(&mut p, &mut g, "team", 0, "steps", "0,1").is_err());
        assert!(p == before);
        assert!(g.clocks[0].running);
    }
    #[test]
    fn adding_clock_preserves_other_runtime_and_five_team_custom_steps() {
        let mut p = Project::fresh();
        let mut g = GameSession::new(&p);
        g.clocks[0].running = true;
        apply(&mut p, &mut g, "clock", 0, "add", "").unwrap();
        assert!(g.clocks[0].running);
        assert!(!g.clocks[1].running);
        for _ in 0..3 {
            apply(&mut p, &mut g, "team", 0, "add", "").unwrap();
        }
        apply(&mut p, &mut g, "team", 4, "steps", "-1,1,5,10").unwrap();
        assert_eq!(p.teams.len(), 5);
        assert_eq!(p.teams[4].steps, vec![-1, 1, 5, 10]);
        apply(&mut p, &mut g, "clock", 1, "current", "02:30").unwrap();
        assert_eq!(p.clocks[1].seconds, 150.);
        assert!(g.clocks[0].running);
    }
    #[test]
    fn team_color_key_follows_add_recolor_and_remove() {
        let mut p = Project::fresh();
        let mut g = GameSession::new(&p);
        p.values.remove(&team_color_key(p.teams[0].id));
        apply(&mut p, &mut g, "team", 0, "add", "").unwrap();
        for t in &p.teams {
            assert_eq!(p.values[&team_color_key(t.id)], hex(t.color));
        }
        apply(&mut p, &mut g, "team", 0, "color", "#112233").unwrap();
        assert_eq!(p.values[&team_color_key(p.teams[0].id)], "#112233");
        let id = p.teams[2].id;
        apply(&mut p, &mut g, "team", 2, "remove", "").unwrap();
        assert!(!p.values.contains_key(&team_color_key(id)));
    }
    #[test]
    fn sync_creates_missing_keys_for_older_projects() {
        let mut p = Project::fresh();
        p.values.retain(|k, _| !k.ends_with(".color"));
        p.values.insert("team.99.color".into(), "#000000".into());
        sync_team_colors(&mut p);
        assert!(!p.values.contains_key("team.99.color"));
        for t in &p.teams {
            assert_eq!(p.values[&team_color_key(t.id)], hex(t.color));
        }
    }
}
