//! Preserve repeater identity and focus while live values refresh.
use crate::{ClockRow, TeamRow};
use overlay_studio::model::{ClockSpec, GameClock, clock_text};
use slint::{Model, ModelRc, VecModel};
use std::rc::Rc;

pub fn clock_runtime(mut row: ClockRow, spec: &ClockSpec, runtime: &GameClock) -> ClockRow {
    let was_running = row.running;
    row.running = runtime.running;
    if row.running || was_running {
        row.time = clock_text(spec.seconds).into();
    }
    row
}
pub fn team_runtime(mut row: TeamRow, name: &str, score: &str, editing: bool) -> TeamRow {
    row.score = score.into();
    if !editing {
        row.name = name.into();
    }
    row
}
pub fn sync<T: Clone + PartialEq + 'static>(model: ModelRc<T>, values: Vec<T>) -> ModelRc<T> {
    if let Some(rows) = model.as_any().downcast_ref::<VecModel<T>>() {
        while rows.row_count() > values.len() {
            rows.remove(rows.row_count() - 1);
        }
        for (index, value) in values.into_iter().enumerate() {
            if index >= rows.row_count() {
                rows.push(value);
            } else if rows.row_data(index).as_ref() != Some(&value) {
                rows.set_row_data(index, value);
            }
        }
        model
    } else {
        Rc::new(VecModel::from(values)).into()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn runtime_scores_refresh_without_erasing_team_name_drafts() {
        let row = TeamRow {
            name: "Draft".into(),
            score: "12".into(),
            ..Default::default()
        };
        let next = team_runtime(row, "Eagles", "17", true);
        assert_eq!(next.name, "Draft");
        assert_eq!(next.score, "17");
        let next = team_runtime(next, "Eagles", "17", false);
        assert_eq!(next.name, "Eagles");
    }
    #[test]
    fn clock_finish_updates_time_without_erasing_edit_drafts() {
        let p = overlay_studio::model::Project::fresh();
        let mut spec = p.clocks[0].clone();
        spec.seconds = 0.;
        let row = ClockRow {
            name: "Draft name".into(),
            reset: "01:30".into(),
            time: "00:01".into(),
            running: true,
            ..Default::default()
        };
        let next = clock_runtime(row, &spec, &GameClock::new());
        assert_eq!(next.time, "00:00");
        assert_eq!(next.name, "Draft name");
        assert_eq!(next.reset, "01:30");
        assert!(!next.running);
    }
    #[test]
    fn syncing_retains_existing_model_identity() {
        let model: ModelRc<i32> = Rc::new(VecModel::from(vec![1, 2])).into();
        let original = model.as_any() as *const _;
        let model = sync(model, vec![1, 3, 4]);
        assert!(std::ptr::eq(original, model.as_any() as *const _));
        assert_eq!(model.row_data(1), Some(3));
        assert_eq!(model.row_count(), 3);
    }
}
