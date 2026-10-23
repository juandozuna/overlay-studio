use super::{DocumentSession, validate_design};

pub enum Alignment {
    Left,
    Top,
    CenterX,
}
impl DocumentSession {
    pub fn group(&mut self) -> Result<(), String> {
        if self.selected.len() < 2 {
            return Err("Select at least two layers to group".into());
        }
        let ids = self.selected.clone();
        let group = self.project.next_id();
        self.edit(|d| {
            for e in &mut d.elements {
                if ids.contains(&e.id) && !e.locked {
                    e.group = Some(group);
                }
            }
        })
    }
    pub fn ungroup(&mut self) -> Result<(), String> {
        let ids = self.selected.clone();
        self.edit(|d| {
            for e in &mut d.elements {
                if ids.contains(&e.id) && !e.locked {
                    e.group = None;
                }
            }
        })
    }
    pub fn reorder(&mut self, forward: bool) -> Result<(), String> {
        let ids = self.selected.clone();
        self.edit(|d| {
            if forward {
                for i in (0..d.elements.len().saturating_sub(1)).rev() {
                    if ids.contains(&d.elements[i].id)
                        && !d.elements[i].locked
                        && !ids.contains(&d.elements[i + 1].id)
                    {
                        d.elements.swap(i, i + 1);
                    }
                }
            } else {
                for i in 1..d.elements.len() {
                    if ids.contains(&d.elements[i].id)
                        && !d.elements[i].locked
                        && !ids.contains(&d.elements[i - 1].id)
                    {
                        d.elements.swap(i, i - 1);
                    }
                }
            }
        })
    }
    pub fn bounds(&self) -> Option<[f32; 4]> {
        bounds(&self.active().elements, &self.selected)
    }
    pub fn align(&mut self, alignment: Alignment) -> Result<(), String> {
        let b = self.bounds().ok_or("Select layers to align")?;
        let ids = self.selected.clone();
        self.edit(|d| {
            for e in &mut d.elements {
                if ids.contains(&e.id) && !e.locked {
                    match alignment {
                        Alignment::Left => e.x = b[0],
                        Alignment::Top => e.y = b[1],
                        Alignment::CenterX => e.x = (d.width as f32 - e.w) / 2.,
                    }
                }
            }
        })
    }
    pub fn nudge(&mut self, dx: f32, dy: f32) -> Result<(), String> {
        self.begin_gesture()?;
        self.translate_gesture(dx, dy, false)?;
        self.end_gesture()
    }
    pub fn resize_gesture(
        &mut self,
        corner: u8,
        dx: f32,
        dy: f32,
        proportional: bool,
    ) -> Result<(), String> {
        if !dx.is_finite() || !dy.is_finite() || corner > 3 {
            return Err("Invalid resize gesture".into());
        }
        let gesture = self.gesture.as_ref().ok_or("No active gesture")?;
        let mut d = gesture.origin.clone();
        let b = bounds(&d.elements, &self.selected).ok_or("Select a layer")?;
        let left = matches!(corner, 0 | 3);
        let top = matches!(corner, 0 | 1);
        let mut w = (b[2] + if left { -dx } else { dx }).max(1.);
        let mut h = (b[3] + if top { -dy } else { dy }).max(1.);
        if proportional {
            let ratio = if (w / b[2] - 1.).abs() > (h / b[3] - 1.).abs() {
                w / b[2]
            } else {
                h / b[3]
            };
            w = (b[2] * ratio).max(1.);
            h = (b[3] * ratio).max(1.);
        }
        let single = self.selected.len() == 1;
        for e in &mut d.elements {
            if !self.selected.contains(&e.id) || e.locked {
                continue;
            }
            if single {
                let angle = e.rotation.to_radians();
                let (s, c) = angle.sin_cos();
                let ax = if left { e.w / 2. } else { -e.w / 2. };
                let ay = if top { e.h / 2. } else { -e.h / 2. };
                let anchor = (
                    e.x + e.w / 2. + ax * c - ay * s,
                    e.y + e.h / 2. + ax * s + ay * c,
                );
                let ax = if left { w / 2. } else { -w / 2. };
                let ay = if top { h / 2. } else { -h / 2. };
                e.x = anchor.0 - ax * c + ay * s - w / 2.;
                e.y = anchor.1 - ax * s - ay * c - h / 2.;
                e.w = w;
                e.h = h;
            } else {
                let x = if left { b[0] + b[2] - w } else { b[0] };
                let y = if top { b[1] + b[3] - h } else { b[1] };
                e.x = x + (e.x - b[0]) * w / b[2];
                e.y = y + (e.y - b[1]) * h / b[3];
                e.w *= w / b[2];
                e.h *= h / b[3];
            }
        }
        validate_design(&d)?;
        self.project.designs[self.design] = d;
        self.revision += 1;
        Ok(())
    }
    pub fn rotate_gesture(&mut self, degrees: f32) -> Result<(), String> {
        if !degrees.is_finite() {
            return Err("Invalid rotation".into());
        }
        let gesture = self.gesture.as_ref().ok_or("No active gesture")?;
        let mut d = gesture.origin.clone();
        let b = bounds(&d.elements, &self.selected).ok_or("Select a layer")?;
        let center = (b[0] + b[2] / 2., b[1] + b[3] / 2.);
        let (s, c) = degrees.to_radians().sin_cos();
        for e in &mut d.elements {
            if self.selected.contains(&e.id) && !e.locked {
                let dx = e.x + e.w / 2. - center.0;
                let dy = e.y + e.h / 2. - center.1;
                e.x = center.0 + dx * c - dy * s - e.w / 2.;
                e.y = center.1 + dx * s + dy * c - e.h / 2.;
                e.rotation += degrees;
            }
        }
        validate_design(&d)?;
        self.project.designs[self.design] = d;
        self.revision += 1;
        Ok(())
    }
}
fn bounds(
    elements: &[crate::model::Element],
    ids: &std::collections::BTreeSet<u64>,
) -> Option<[f32; 4]> {
    let mut layers = elements.iter().filter(|e| ids.contains(&e.id));
    let first = layers.next()?;
    let mut x = first.x;
    let mut y = first.y;
    let mut r = x + first.w;
    let mut bottom = y + first.h;
    for e in layers {
        x = x.min(e.x);
        y = y.min(e.y);
        r = r.max(e.x + e.w);
        bottom = bottom.max(e.y + e.h);
    }
    Some([x, y, (r - x).max(1.), (bottom - y).max(1.)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Kind;
    #[test]
    fn rotated_resize_preserves_opposite_world_corner_and_undo() {
        let mut s = DocumentSession::new(crate::model::Project::fresh());
        s.project.designs[0].elements.clear();
        let id = s.add(Kind::Rectangle, 100., 100., 100., 40.).unwrap();
        s.apply(
            id,
            super::super::LayerEdit::Geometry {
                x: 100.,
                y: 100.,
                width: 100.,
                height: 40.,
                rotation: 90.,
            },
        )
        .unwrap();
        s.begin_gesture().unwrap();
        s.resize_gesture(2, 50., 20., false).unwrap();
        s.end_gesture().unwrap();
        let e = s.selected_layer().unwrap();
        assert!((e.x + e.w / 2. + e.h / 2. - 170.).abs() < 0.01);
        assert!((e.y + e.h / 2. - e.w / 2. - 70.).abs() < 0.01);
        s.undo().unwrap();
        assert_eq!(s.active().elements[0].w, 100.);
    }
    #[test]
    fn copying_multiple_groups_does_not_merge_them() {
        let mut s = DocumentSession::new(crate::model::Project::fresh());
        s.project.designs[0].elements.clear();
        let a = s.add(Kind::Rectangle, 0., 0., 50., 50.).unwrap();
        let b = s.add(Kind::Rectangle, 70., 0., 50., 50.).unwrap();
        s.project.designs[0].elements[0].group = Some(100);
        s.project.designs[0].elements[1].group = Some(101);
        s.select(Some(a), false);
        s.select(Some(b), true);
        s.duplicate().unwrap();
        let copies: Vec<_> = s
            .active()
            .elements
            .iter()
            .filter(|e| s.selected.contains(&e.id))
            .collect();
        assert_ne!(copies[0].group, copies[1].group);
    }
}
