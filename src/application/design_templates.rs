//! Adaptive artwork built from the configured game without presentation dependencies.
use crate::model::{Animation, Design, Element, Kind, Project, validate_design};
pub fn team_board(project: &Project) -> Result<Design, String> {
    if project.teams.is_empty() {
        return Err("Configure at least one team".into());
    }
    let count = project.teams.len();
    let columns = count.min(4);
    let rows = count.div_ceil(columns);
    let cell_w = 1840. / columns as f32;
    let cell_h = 760. / rows as f32;
    let mut next = project.next_id();
    let mut elements = vec![];
    for (index, team) in project.teams.iter().enumerate() {
        let x = 40. + (index % columns) as f32 * cell_w;
        let y = 150. + (index / columns) as f32 * cell_h;
        let mut panel = Element::new(next, Kind::Rectangle);
        next += 1;
        panel.name = format!("{} panel", team.label);
        panel.x = x + 8.;
        panel.y = y + 8.;
        panel.w = cell_w - 16.;
        panel.h = cell_h - 16.;
        panel.fill = [team.color[0], team.color[1], team.color[2], 230];
        elements.push(panel);
        let mut name = Element::new(next, Kind::Text);
        next += 1;
        name.name = format!("{} name", team.label);
        name.binding = team.name_key.clone();
        name.x = x + 30.;
        name.y = y + 30.;
        name.w = cell_w - 60.;
        name.h = cell_h * 0.25;
        name.fill = [245, 248, 255, 255];
        name.font_size = 58.;
        name.animation = Animation::Fade;
        elements.push(name);
        let mut score = Element::new(next, Kind::Text);
        next += 1;
        score.name = format!("{} score", team.label);
        score.binding = team.score_key.clone();
        score.x = x + 30.;
        score.y = y + cell_h * 0.28;
        score.w = cell_w - 60.;
        score.h = cell_h * 0.58;
        score.fill = [255; 4];
        score.font_size = 180.;
        score.animation = Animation::Pop;
        elements.push(score);
    }
    if project.values.contains_key("event") {
        let mut title = Element::new(next, Kind::Text);
        next += 1;
        title.name = "Event title".into();
        title.binding = "event".into();
        title.x = 300.;
        title.y = 35.;
        title.w = 1320.;
        title.h = 90.;
        title.fill = [242, 247, 255, 255];
        title.font_size = 60.;
        elements.push(title);
    }
    if let Some(clock) = project.clocks.first() {
        let mut timer = Element::new(next, Kind::Text);
        timer.name = "Primary clock".into();
        timer.binding = clock.value_key.clone();
        timer.x = 760.;
        timer.y = 940.;
        timer.w = 400.;
        timer.h = 100.;
        timer.fill = [242, 247, 255, 255];
        timer.font_size = 74.;
        elements.push(timer);
    }
    let design = Design {
        id: 0,
        name: format!("{count} team scoreboard"),
        width: 1920,
        height: 1080,
        elements,
    };
    validate_design(&design)?;
    Ok(design)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_team_gets_bound_name_and_score_within_canvas() {
        let mut p = Project::fresh();
        for _ in 0..3 {
            p.add_team();
        }
        let d = team_board(&p).unwrap();
        for team in &p.teams {
            assert_eq!(
                d.elements
                    .iter()
                    .filter(|e| e.binding == team.name_key)
                    .count(),
                1
            );
            assert_eq!(
                d.elements
                    .iter()
                    .filter(|e| e.binding == team.score_key)
                    .count(),
                1
            );
        }
        assert!(
            d.elements
                .iter()
                .all(|e| e.x >= 0. && e.y >= 0. && e.x + e.w <= 1920. && e.y + e.h <= 1080.)
        );
    }
}
