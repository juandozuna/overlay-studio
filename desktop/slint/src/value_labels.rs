use overlay_studio::model::Project;

pub fn label(project: &Project, key: &str) -> String {
    for team in &project.teams {
        if key == team.score_key {
            return format!(
                "{} → Score",
                project.values.get(&team.name_key).unwrap_or(&team.label)
            );
        }
        if key == team.name_key {
            return format!("{} → Name", team.label);
        }
    }
    if let Some(clock) = project.clocks.iter().find(|c| c.value_key == key) {
        return format!("{} → Time", clock.name);
    }
    match key {
        "event" => "Event name".into(),
        "round" => "Round".into(),
        _ => key.into(),
    }
}
