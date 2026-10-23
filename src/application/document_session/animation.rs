//! Animation transactions share canvas history and never change live game values.
use super::DocumentSession;
use crate::model::{Animation, Keyframe};

impl DocumentSession {
    pub fn animation_setup(
        &mut self,
        id: u64,
        preset: Animation,
        duration: f32,
        looping: bool,
    ) -> Result<(), String> {
        self.animation_layer(id)?;
        self.edit(|d| {
            if let Some(e) = d.elements.iter_mut().find(|e| e.id == id) {
                e.animation = preset;
                e.duration = duration;
                e.loop_animation = looping;
            }
        })
    }
    fn animation_layer(&self, id: u64) -> Result<(), String> {
        let e = self
            .active()
            .elements
            .iter()
            .find(|e| e.id == id)
            .ok_or("Select a layer")?;
        if e.locked {
            return Err("Unlock the layer before editing its animation".into());
        }
        Ok(())
    }
    pub fn insert_keyframes(
        &mut self,
        frames: Vec<(u64, Keyframe)>,
    ) -> Result<Vec<(u64, usize)>, String> {
        for (id, _) in &frames {
            self.animation_layer(*id)?;
        }
        let mut inserted = vec![];
        self.edit(|d| {
            for (id, frame) in frames {
                if let Some(e) = d.elements.iter_mut().find(|e| e.id == id) {
                    inserted.push((id, e.keyframes.len()));
                    e.keyframes.push(frame);
                }
            }
        })?;
        Ok(inserted)
    }
    pub fn edit_keyframe(&mut self, id: u64, index: usize, frame: Keyframe) -> Result<(), String> {
        self.animation_layer(id)?;
        if !self
            .active()
            .elements
            .iter()
            .any(|e| e.id == id && index < e.keyframes.len())
        {
            return Err("Keyframe no longer exists".into());
        }
        self.edit(|d| {
            if let Some(e) = d.elements.iter_mut().find(|e| e.id == id) {
                e.keyframes[index] = frame;
            }
        })
    }
    pub fn move_keyframes(&mut self, frames: &[(u64, usize)], delta: f32) -> Result<(), String> {
        if !delta.is_finite() {
            return Err("Enter a finite time offset".into());
        }
        for (id, index) in frames {
            self.animation_layer(*id)?;
            if !self
                .active()
                .elements
                .iter()
                .any(|e| e.id == *id && *index < e.keyframes.len())
            {
                return Err("Keyframe no longer exists".into());
            }
        }
        self.edit(|d| {
            for (id, index) in frames {
                if let Some(e) = d.elements.iter_mut().find(|e| e.id == *id) {
                    e.keyframes[*index].time += delta;
                }
            }
        })
    }
    pub fn remove_keyframes(&mut self, frames: &[(u64, usize)]) -> Result<(), String> {
        for (id, _) in frames {
            self.animation_layer(*id)?;
        }
        self.edit(|d| {
            for e in &mut d.elements {
                let mut index = 0;
                e.keyframes.retain(|_| {
                    let keep = !frames.contains(&(e.id, index));
                    index += 1;
                    keep
                });
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bulk_timing_is_atomic_and_one_undo_without_reverting_scores() {
        let mut s = DocumentSession::new(crate::model::Project::fresh());
        let e = s.active().elements[0].clone();
        let mut a = Keyframe::capture(&e, 0.);
        let b = Keyframe::capture(&e, 2.);
        a.x = 0.;
        let keys = s.insert_keyframes(vec![(e.id, a), (e.id, b)]).unwrap();
        assert!(s.move_keyframes(&keys, -1.).is_err());
        assert_eq!(s.active().elements[0].keyframes[0].time, 0.);
        s.move_keyframes(&keys, 3.).unwrap();
        s.project.values.insert("home.score".into(), "99".into());
        s.undo().unwrap();
        assert_eq!(s.active().elements[0].keyframes[1].time, 2.);
        assert_eq!(s.project.values["home.score"], "99");
        s.redo();
        assert_eq!(s.active().elements[0].keyframes[1].time, 5.);
        s.remove_keyframes(&keys).unwrap();
        assert!(s.active().elements[0].keyframes.is_empty());
        s.undo().unwrap();
        assert_eq!(s.active().elements[0].keyframes.len(), 2);
    }
    #[test]
    fn bad_pose_cannot_enter_saved_animation() {
        let mut s = DocumentSession::new(crate::model::Project::fresh());
        let e = s.active().elements[0].clone();
        let f = Keyframe::capture(&e, 1.);
        s.insert_keyframes(vec![(e.id, f.clone())]).unwrap();
        let mut invalid = f.clone();
        invalid.w = 0.;
        assert!(s.edit_keyframe(e.id, 0, invalid).is_err());
        assert_eq!(s.active().elements[0].keyframes[0], f);
        s.project.designs[0].elements[0].locked = true;
        assert!(s.animation_setup(e.id, Animation::Pop, 0.5, false).is_err());
    }
}
