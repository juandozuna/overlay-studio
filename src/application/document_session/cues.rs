use super::{DocumentSession, History};
use crate::{
    domain::controls::{Action, ControllerLayout},
    model::{CueRule, Project},
};

#[derive(Clone)]
pub(super) struct CueConfig {
    rules: Vec<CueRule>,
    controller: ControllerLayout,
}
impl CueConfig {
    fn capture(project: &Project) -> Self {
        Self {
            rules: project.cue_rules.clone(),
            controller: project.controller.clone(),
        }
    }
    pub(super) fn restore(&self, project: &mut Project) {
        project.cue_rules = self.rules.clone();
        project.controller = self.controller.clone();
    }
}
impl DocumentSession {
    pub(super) fn validate_cue_config(&mut self, config: &CueConfig) -> Result<(), String> {
        let current = CueConfig::capture(&self.project);
        config.restore(&mut self.project);
        let result = self.project.validate_metadata();
        current.restore(&mut self.project);
        result
    }
    pub fn set_cue_rules(&mut self, rules: Vec<CueRule>) -> Result<(), String> {
        self.end_gesture()?;
        if self.project.cue_rules == rules {
            return Ok(());
        }
        let before = CueConfig::capture(&self.project);
        let mut after = CueConfig {
            rules,
            controller: before.controller.clone(),
        };
        after.controller.controls.retain(|button|!matches!(button.action,Action::RunCue{id} if !after.rules.iter().any(|rule|rule.id==id)));
        self.validate_cue_config(&after)?;
        after.restore(&mut self.project);
        self.push_history(History::Cues { before, after });
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CueEvent, CueTarget};
    #[test]
    fn removing_a_linked_rule_is_one_undo_without_reverting_live_values() {
        let mut s = DocumentSession::default();
        let rule = CueRule {
            id: 41,
            name: "Winner".into(),
            enabled: true,
            event: CueEvent::ScoreAdded {
                field: "home.score".into(),
            },
            target: CueTarget::SetValue {
                field: "round".into(),
                value: "WINNER".into(),
            },
        };
        s.set_cue_rules(vec![rule.clone()]).unwrap();
        s.project.controller.controls[0].action = Action::RunCue { id: 41 };
        let control = s.project.controller.controls[0].clone();
        s.set_cue_rules(vec![]).unwrap();
        assert!(
            !s.project
                .controller
                .controls
                .iter()
                .any(|c| c.id == control.id)
        );
        s.project.values.insert("home.score".into(), "99".into());
        s.undo().unwrap();
        assert_eq!(s.project.cue_rules[0].name, "Winner");
        assert!(s.project.controller.controls[0] == control);
        assert_eq!(s.project.values["home.score"], "99");
        s.redo();
        assert!(s.project.cue_rules.is_empty());
        s.project.validate_metadata().unwrap();
        let mut bad = rule;
        bad.target = CueTarget::ReplayOutput { route: 99 };
        assert!(s.set_cue_rules(vec![bad]).is_err());
        assert!(s.project.cue_rules.is_empty());
    }
}
