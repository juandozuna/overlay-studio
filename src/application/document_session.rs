//! Validated canvas edits and gesture transactions, independent of the desktop toolkit.
//! History stores designs, so dragging does not clone embedded media on each frame.
use crate::{
    domain::controls::{Action, ControllerLayout},
    model::{CueRule, CueTarget, Design, Element, Kind, Project, Route, validate_design},
};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
mod animation;
mod cues;
mod media;
mod setup;
use cues::CueConfig;
mod manipulation;
pub use manipulation::Alignment;

#[derive(Clone)]
struct Edit {
    design: usize,
    before: Design,
    after: Design,
}
#[derive(Clone)]
struct DesignLinks {
    routes: Vec<Route>,
    cues: Vec<CueRule>,
    controller: ControllerLayout,
}
impl DesignLinks {
    fn capture(p: &Project) -> Self {
        Self {
            routes: p.routes.clone(),
            cues: p.cue_rules.clone(),
            controller: p.controller.clone(),
        }
    }
    fn restore(self, p: &mut Project) {
        p.routes = self.routes;
        p.cue_rules = self.cues;
        p.controller = self.controller;
    }
}
struct CatalogEdit {
    before: Vec<Design>,
    after: Vec<Design>,
    before_active: usize,
    after_active: usize,
    undo_links: Option<DesignLinks>,
}
enum History {
    Setup(Box<setup::SetupEdit>),
    Cues {
        before: CueConfig,
        after: CueConfig,
    },
    Layer(Edit),
    Catalog(Box<CatalogEdit>),
    ProjectName {
        before: String,
        after: String,
    },
    Controller {
        before: ControllerLayout,
        after: ControllerLayout,
    },
}
struct Gesture {
    design: usize,
    before: Design,
    origin: Design,
}

pub enum LayerEdit {
    Geometry {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        rotation: f32,
    },
    Name(String),
    Text(String),
    Binding(String),
    Font(String),
    FontSize(f32),
    Fill([u8; 4]),
    Opacity(f32),
    Hidden(bool),
    Locked(bool),
    Stroke([u8; 4]),
    StrokeWidth(f32),
    Corners(f32),
    TextAlign(crate::model::TextAlign),
}

pub struct DocumentSession {
    pub project: Project,
    pub design: usize,
    pub selected: BTreeSet<u64>,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub revision: u64,
    pub generation: u64,
    pub catalog_revision: u64,
    undo: Vec<History>,
    redo: Vec<History>,
    gesture: Option<Gesture>,
}

impl Default for DocumentSession {
    fn default() -> Self {
        Self::new(Project::game_night())
    }
}

impl DocumentSession {
    pub fn new(project: Project) -> Self {
        Self {
            project,
            design: 0,
            selected: BTreeSet::new(),
            path: None,
            dirty: false,
            revision: 1,
            generation: 1,
            catalog_revision: 1,
            undo: vec![],
            redo: vec![],
            gesture: None,
        }
    }
    pub fn active(&self) -> &Design {
        &self.project.designs[self.design]
    }
    pub fn selected_layer(&self) -> Option<&Element> {
        self.active()
            .elements
            .iter()
            .find(|e| self.selected.contains(&e.id))
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn select_design(&mut self, index: usize) -> Result<(), String> {
        if index >= self.project.designs.len() {
            return Err("Design no longer exists.".into());
        }
        self.end_gesture()?;
        self.design = index;
        self.selected.clear();
        self.revision += 1;
        Ok(())
    }
    pub fn select(&mut self, id: Option<u64>, extend: bool) {
        if !extend {
            self.selected.clear();
        }
        if let Some(id) = id {
            if !self.active().elements.iter().any(|e| e.id == id) {
                return;
            }
            let group = self
                .active()
                .elements
                .iter()
                .find(|e| e.id == id)
                .and_then(|e| e.group);
            let ids: Vec<_> = self
                .active()
                .elements
                .iter()
                .filter(|e| e.id == id || group.is_some() && e.group == group)
                .map(|e| e.id)
                .collect();
            for id in ids {
                self.selected.insert(id);
            }
        }
        self.revision += 1;
    }
    pub fn hit_test(&self, x: f32, y: f32) -> Option<u64> {
        self.active()
            .elements
            .iter()
            .rev()
            .find(|e| {
                if e.hidden || e.locked {
                    return false;
                }
                let angle = -e.rotation.to_radians();
                let dx = x - e.x - e.w / 2.;
                let dy = y - e.y - e.h / 2.;
                let rx = dx * angle.cos() - dy * angle.sin();
                let ry = dx * angle.sin() + dy * angle.cos();
                rx.abs() <= e.w / 2. && ry.abs() <= e.h / 2.
            })
            .map(|e| e.id)
    }
    fn record(&mut self, before: Design) {
        let after = self.active().clone();
        if before == after {
            return;
        }
        self.push_history(History::Layer(Edit {
            design: self.design,
            before,
            after,
        }));
    }
    fn push_history(&mut self, edit: History) {
        self.undo.push(edit);
        if self.undo.len() > 64 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
        self.revision += 1;
    }
    pub fn mark_external_edit(&mut self) {
        self.redo.clear();
        self.dirty = true;
        self.revision += 1;
    }
    pub fn rename_project(&mut self, name: String) -> Result<(), String> {
        if name.trim().is_empty() {
            return Err("Enter a project name".into());
        }
        if self.project.name != name {
            let before = std::mem::replace(&mut self.project.name, name.clone());
            self.push_history(History::ProjectName {
                before,
                after: name,
            });
        }
        Ok(())
    }
    pub fn append_design(&mut self, mut design: Design) -> Result<(), String> {
        self.end_gesture()?;
        if self.project.designs.len() >= 64 {
            return Err("Maximum 64 designs".into());
        }
        if design.id == 0 || self.project.designs.iter().any(|item| item.id == design.id) {
            design.id = self
                .project
                .designs
                .iter()
                .map(|item| item.id)
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or("Design ID limit reached")?;
        }
        validate_design(&design)?;
        let before = self.project.designs.clone();
        let before_active = self.design;
        self.project.designs.push(design);
        if let Err(error) = self.project.validate_metadata() {
            self.project.designs = before;
            return Err(error);
        }
        self.design = self.project.designs.len() - 1;
        self.selected.clear();
        self.push_history(History::Catalog(Box::new(CatalogEdit {
            before,
            after: self.project.designs.clone(),
            before_active,
            after_active: self.design,
            undo_links: None,
        })));
        Ok(())
    }
    pub fn set_controller(&mut self, after: ControllerLayout) -> Result<(), String> {
        after.validate(&self.project)?;
        self.end_gesture()?;
        if self.project.controller != after {
            let before = std::mem::replace(&mut self.project.controller, after.clone());
            self.push_history(History::Controller { before, after });
        }
        Ok(())
    }
    pub fn blank_design(&mut self) -> Result<(), String> {
        self.append_design(Design {
            id: 0,
            name: "Untitled scoreboard".into(),
            width: 1920,
            height: 1080,
            elements: vec![],
        })
    }
    pub fn duplicate_design(&mut self) -> Result<(), String> {
        let mut d = self.active().clone();
        d.name = format!("{} copy", d.name);
        let mut next = self.project.next_id();
        let mut groups = std::collections::BTreeMap::new();
        for e in &mut d.elements {
            e.id = next;
            next = next.checked_add(1).ok_or("Layer ID limit reached")?;
        }
        for e in &mut d.elements {
            if let Some(group) = e.group {
                let id = if let Some(id) = groups.get(&group) {
                    *id
                } else {
                    let id = next;
                    next = next.checked_add(1).ok_or("Group ID limit reached")?;
                    groups.insert(group, id);
                    id
                };
                e.group = Some(id);
            }
        }
        self.append_design(d)
    }
    pub fn import_design(
        &mut self,
        verified: crate::infrastructure::portable_design::VerifiedDesign,
    ) -> Result<(), String> {
        let mut file = verified.into_file();
        let mut next = self.project.next_id();
        let mut allocate = || -> Result<u64, String> {
            let id = next;
            next = next.checked_add(1).ok_or("ID limit reached")?;
            Ok(id)
        };
        let mut images = std::collections::BTreeMap::new();
        let mut motions = std::collections::BTreeMap::new();
        let mut groups = std::collections::BTreeMap::new();
        for asset in &mut file.assets {
            let old = asset.id;
            asset.id = allocate()?;
            images.insert(old, asset.id);
        }
        for motion in &mut file.motions {
            let old = motion.id;
            motion.id = allocate()?;
            motions.insert(old, motion.id);
        }
        for e in &mut file.design.elements {
            e.id = allocate()?;
            e.asset = e.asset.and_then(|id| images.get(&id).copied());
            e.motion = e.motion.and_then(|id| motions.get(&id).copied());
            if let Some(old) = e.group {
                let id = if let Some(id) = groups.get(&old) {
                    *id
                } else {
                    let id = allocate()?;
                    groups.insert(old, id);
                    id
                };
                e.group = Some(id);
            }
        }
        let assets_len = self.project.assets.len();
        let motions_len = self.project.motions.len();
        let values = self.project.values.clone();
        self.project.assets.extend(file.assets);
        self.project.motions.extend(file.motions);
        for e in &file.design.elements {
            if !e.binding.is_empty() {
                self.project
                    .values
                    .entry(e.binding.clone())
                    .or_insert("0".into());
            }
        }
        if let Err(error) = self.append_design(file.design) {
            self.project.assets.truncate(assets_len);
            self.project.motions.truncate(motions_len);
            self.project.values = values;
            return Err(error);
        }
        Ok(())
    }
    pub fn canvas_setup(&mut self, name: String, width: u32, height: u32) -> Result<(), String> {
        if name.trim().is_empty() {
            return Err("Enter a design name".into());
        }
        self.edit(|d| {
            d.name = name;
            d.width = width;
            d.height = height;
        })
    }
    /// Commit a reviewed layout proposal as one undoable edit.
    ///
    /// The proposal may only change layer and keyframe geometry, and is rejected
    /// if the document or source design changed while the background job ran.
    pub fn apply_layout_proposal(
        &mut self,
        generation: u64,
        design: usize,
        source: &Design,
        proposal: Design,
    ) -> Result<(), String> {
        self.end_gesture()?;
        if self.generation != generation || self.project.designs.get(design) != Some(source) {
            return Err(
                "The source design changed while the suggestion was generated. Try again.".into(),
            );
        }
        if source.elements.len() != proposal.elements.len() {
            return Err("The layout suggestion changed the layer list and was rejected.".into());
        }
        let mut expected = source.clone();
        for (expected, proposed) in expected.elements.iter_mut().zip(&proposal.elements) {
            if expected.id != proposed.id || expected.keyframes.len() != proposed.keyframes.len() {
                return Err(
                    "The layout suggestion changed layer identity and was rejected.".into(),
                );
            }
            expected.x = proposed.x;
            expected.y = proposed.y;
            expected.w = proposed.w;
            expected.h = proposed.h;
            for (expected, proposed) in expected.keyframes.iter_mut().zip(&proposed.keyframes) {
                expected.x = proposed.x;
                expected.y = proposed.y;
                expected.w = proposed.w;
                expected.h = proposed.h;
            }
        }
        if expected != proposal {
            return Err("The layout suggestion changed content and was rejected.".into());
        }
        validate_design(&proposal)?;
        if proposal == *source {
            return Ok(());
        }
        self.project.designs[design] = proposal.clone();
        self.push_history(History::Layer(Edit {
            design,
            before: source.clone(),
            after: proposal,
        }));
        Ok(())
    }
    fn edit(&mut self, change: impl FnOnce(&mut Design)) -> Result<(), String> {
        let before = self.active().clone();
        change(&mut self.project.designs[self.design]);
        if let Err(error) = validate_design(self.active()) {
            self.project.designs[self.design] = before;
            return Err(error);
        }
        if self.gesture.is_none() {
            self.record(before);
        } else {
            self.revision += 1;
        }
        Ok(())
    }
    pub fn add(&mut self, kind: Kind, x: f32, y: f32, w: f32, h: f32) -> Result<u64, String> {
        let id = self.project.next_id();
        let mut layer = Element::new(id, kind);
        layer.x = x;
        layer.y = y;
        layer.w = w;
        layer.h = h;
        if kind == Kind::Text {
            layer.fill = [241, 247, 255, 255];
        }
        self.edit(|d| d.elements.push(layer))?;
        self.select(Some(id), false);
        Ok(id)
    }
    pub fn apply(&mut self, id: u64, edit: LayerEdit) -> Result<(), String> {
        if !self.active().elements.iter().any(|e| e.id == id) {
            return Err("Layer no longer exists.".into());
        }
        if let LayerEdit::Binding(ref key) = edit
            && !key.is_empty()
            && !self.project.values.contains_key(key)
        {
            return Err("Choose an existing live value.".into());
        }
        self.edit(|d| {
            let Some(e) = d.elements.iter_mut().find(|e| e.id == id) else {
                return;
            };
            match edit {
                LayerEdit::Geometry {
                    x,
                    y,
                    width,
                    height,
                    rotation,
                } => {
                    e.x = x;
                    e.y = y;
                    e.w = width;
                    e.h = height;
                    e.rotation = rotation;
                }
                LayerEdit::Name(v) => e.name = v,
                LayerEdit::Text(v) => e.text = v,
                LayerEdit::Binding(v) => e.binding = v,
                LayerEdit::Font(v) => e.font = v,
                LayerEdit::FontSize(v) => e.font_size = v,
                LayerEdit::Fill(v) => e.fill = v,
                LayerEdit::Opacity(v) => e.opacity = v,
                LayerEdit::Hidden(v) => e.hidden = v,
                LayerEdit::Locked(v) => e.locked = v,
                LayerEdit::Stroke(v) => e.stroke = v,
                LayerEdit::StrokeWidth(v) => e.stroke_width = v,
                LayerEdit::Corners(v) => e.radius = v,
                LayerEdit::TextAlign(v) => e.text_align = v,
            }
        })
    }
    pub fn delete_selected(&mut self) -> Result<(), String> {
        let ids = self.selected.clone();
        self.edit(|d| d.elements.retain(|e| !ids.contains(&e.id) || e.locked))?;
        let existing: BTreeSet<_> = self.active().elements.iter().map(|e| e.id).collect();
        self.selected.retain(|id| existing.contains(id));
        Ok(())
    }
    pub fn duplicate(&mut self) -> Result<(), String> {
        self.duplicate_offset(20., 20.)
    }
    pub fn duplicate_offset(&mut self, dx: f32, dy: f32) -> Result<(), String> {
        let mut next = self.project.next_id();
        let mut groups = std::collections::BTreeMap::new();
        let copies: Vec<_> = self
            .active()
            .elements
            .iter()
            .filter(|e| self.selected.contains(&e.id))
            .map(|e| {
                let mut e = e.clone();
                e.id = next;
                next += 1;
                e.x += dx;
                e.y += dy;
                if let Some(group) = e.group {
                    e.group = Some(*groups.entry(group).or_insert_with(|| {
                        let id = next;
                        next += 1;
                        id
                    }));
                }
                e.locked = false;
                e
            })
            .collect();
        let ids = copies.iter().map(|e| e.id).collect();
        self.edit(|d| d.elements.extend(copies))?;
        self.selected = ids;
        Ok(())
    }
    pub fn begin_gesture(&mut self) -> Result<(), String> {
        self.end_gesture()?;
        self.gesture = Some(Gesture {
            design: self.design,
            before: self.active().clone(),
            origin: self.active().clone(),
        });
        Ok(())
    }
    pub fn reset_gesture_origin(&mut self) {
        let origin = self.active().clone();
        if let Some(gesture) = &mut self.gesture {
            gesture.origin = origin;
        }
    }
    pub fn translate_gesture(&mut self, dx: f32, dy: f32, snap: bool) -> Result<(), String> {
        if !dx.is_finite() || !dy.is_finite() {
            return Err("Position must be finite.".into());
        }
        let Some(gesture) = &self.gesture else {
            return Err("No active gesture.".into());
        };
        if gesture.design != self.design {
            return Err("Design changed during drag.".into());
        }
        let mut design = gesture.origin.clone();
        let (dx, dy) = if snap {
            let anchor = design
                .elements
                .iter()
                .filter(|e| self.selected.contains(&e.id) && !e.locked)
                .map(|e| (e.x, e.y))
                .reduce(|a, b| (a.0.min(b.0), a.1.min(b.1)));
            anchor
                .map(|(x, y)| {
                    (
                        ((x + dx) / 10.).round() * 10. - x,
                        ((y + dy) / 10.).round() * 10. - y,
                    )
                })
                .unwrap_or((dx, dy))
        } else {
            (dx, dy)
        };
        for e in &mut design.elements {
            if self.selected.contains(&e.id) && !e.locked {
                e.x += dx;
                e.y += dy;
            }
        }
        validate_design(&design)?;
        self.project.designs[self.design] = design;
        self.revision += 1;
        Ok(())
    }
    pub fn end_gesture(&mut self) -> Result<(), String> {
        if let Some(gesture) = self.gesture.take() {
            if gesture.design != self.design {
                return Err("Design changed during drag.".into());
            }
            self.record(gesture.before);
        }
        Ok(())
    }
    pub fn undo(&mut self) -> Result<(), String> {
        self.end_gesture()?;
        if let Some(History::Setup(edit)) = self.undo.last() {
            edit.clone().validate(&mut self.project, false)?;
        }
        let cue_before = match self.undo.last() {
            Some(History::Cues { before, .. }) => Some(before.clone()),
            _ => None,
        };
        if let Some(before) = cue_before {
            self.validate_cue_config(&before)?;
        }
        if let Some(History::Controller { before, .. }) = self.undo.last() {
            before.validate(&self.project)?;
        }
        if let Some(mut edit) = self.undo.pop() {
            match &mut edit {
                History::Setup(edit) => {
                    edit.remember_removed(&self.project, false);
                    edit.restore(&mut self.project, false);
                }
                History::Cues { before, .. } => before.restore(&mut self.project),
                History::ProjectName { before, .. } => self.project.name = before.clone(),
                History::Controller { before, .. } => self.project.controller = before.clone(),
                History::Layer(edit) => {
                    self.design = edit.design;
                    self.project.designs[edit.design] = edit.before.clone();
                }
                History::Catalog(edit) => {
                    self.catalog_revision += 1;
                    edit.undo_links = Some(DesignLinks::capture(&self.project));
                    self.project.designs = edit.before.clone();
                    self.design = edit.before_active;
                    repair_design_links(&mut self.project);
                }
            }
            self.redo.push(edit);
            self.selected.clear();
            self.dirty = true;
            self.revision += 1;
        }
        Ok(())
    }
    pub fn redo(&mut self) {
        if let Some(History::Setup(edit)) = self.redo.last() {
            if edit.clone().validate(&mut self.project, true).is_err() {
                return;
            }
        }
        let cue_after = match self.redo.last() {
            Some(History::Cues { after, .. }) => Some(after.clone()),
            _ => None,
        };
        if let Some(after) = cue_after
            && self.validate_cue_config(&after).is_err()
        {
            self.redo.clear();
            return;
        }
        if let Some(History::Controller { after, .. }) = self.redo.last()
            && after.validate(&self.project).is_err()
        {
            self.redo.clear();
            return;
        }
        if let Some(mut edit) = self.redo.pop() {
            match &mut edit {
                History::Setup(edit) => {
                    edit.remember_removed(&self.project, true);
                    edit.restore(&mut self.project, true);
                }
                History::Cues { after, .. } => after.restore(&mut self.project),
                History::ProjectName { after, .. } => self.project.name = after.clone(),
                History::Controller { after, .. } => self.project.controller = after.clone(),
                History::Layer(edit) => {
                    self.design = edit.design;
                    self.project.designs[edit.design] = edit.after.clone();
                }
                History::Catalog(edit) => {
                    self.catalog_revision += 1;
                    self.project.designs = edit.after.clone();
                    self.design = edit.after_active;
                    if let Some(links) = edit.undo_links.take() {
                        links.restore(&mut self.project);
                    }
                }
            }
            self.undo.push(edit);
            self.selected.clear();
            self.dirty = true;
            self.revision += 1;
        }
    }
    pub fn save(&mut self, path: &Path) -> Result<(), String> {
        self.end_gesture()?;
        self.project.save(path)?;
        self.path = Some(path.into());
        self.dirty = false;
        Ok(())
    }
    pub fn load(&mut self, path: &Path) -> Result<(), String> {
        let project = Project::load(path)?;
        self.replace(project, Some(path.into()))
    }
    pub fn replace(&mut self, project: Project, path: Option<PathBuf>) -> Result<(), String> {
        project.validate()?;
        let revision = self.revision + 1;
        let generation = self.generation + 1;
        *self = Self::new(project);
        self.path = path;
        self.revision = revision;
        self.generation = generation;
        Ok(())
    }
}
fn repair_design_links(project: &mut Project) {
    let count = project.designs.len();
    for route in &mut project.routes {
        if route.design >= count {
            route.design = 0;
        }
    }
    for cue in &mut project.cue_rules {
        if let CueTarget::TakeDesign { design, .. } = &mut cue.target
            && *design >= count
        {
            *design = 0;
        }
    }
    for control in &mut project.controller.controls {
        if let Action::Take { design, .. } = &mut control.action
            && *design >= count
        {
            *design = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reviewed_layout_proposal_is_atomic_and_rejects_stale_or_content_changes() {
        let mut session = DocumentSession::default();
        let source = session.active().clone();
        let mut proposal = source.clone();
        proposal.elements[0].x += 40.;
        let generation = session.generation;
        session
            .apply_layout_proposal(generation, 0, &source, proposal.clone())
            .unwrap();
        assert!(session.active() == &proposal);
        session.undo().unwrap();
        assert!(session.active() == &source);
        session.redo();
        assert!(session.active() == &proposal);

        let mut altered = proposal.clone();
        altered.elements[0].text = "Unexpected content".into();
        assert!(
            session
                .apply_layout_proposal(generation, 0, &proposal, altered)
                .unwrap_err()
                .contains("content")
        );
        session.project.designs[0].elements[0].y += 1.;
        assert!(
            session
                .apply_layout_proposal(generation, 0, &proposal, source.clone())
                .unwrap_err()
                .contains("source design changed")
        );
    }
    #[test]
    fn snapped_group_translation_preserves_relative_positions() {
        let mut s = DocumentSession::default();
        s.project.designs[0].elements.clear();
        let a = s.add(Kind::Rectangle, 10.3, 20.4, 20., 20.).unwrap();
        let b = s.add(Kind::Rectangle, 42.7, 67.2, 20., 20.).unwrap();
        s.select(Some(a), false);
        s.select(Some(b), true);
        s.begin_gesture().unwrap();
        s.translate_gesture(23., 31., true).unwrap();
        s.end_gesture().unwrap();
        let d = s.active();
        assert!((d.elements[1].x - d.elements[0].x - 32.4).abs() < 0.01);
        assert!((d.elements[1].y - d.elements[0].y - 46.8).abs() < 0.01);
    }
    #[test]
    fn catalog_undo_repairs_refs_without_rolling_back_live_scores() {
        let mut s = DocumentSession::default();
        let count = s.project.designs.len();
        s.blank_design().unwrap();
        s.project.routes[0].design = count;
        s.project.values.insert("home.score".into(), "99".into());
        s.undo().unwrap();
        assert_eq!(s.project.designs.len(), count);
        assert_eq!(s.project.routes[0].design, 0);
        assert_eq!(s.project.values["home.score"], "99");
        s.project.validate().unwrap();
        s.redo();
        assert_eq!(s.project.routes[0].design, count);
        s.project.validate().unwrap();
    }
    #[test]
    fn imported_asset_remapping_does_not_rewrite_already_mapped_ids() {
        let mut s = DocumentSession::default();
        let collision = s.project.next_id();
        let mut cursor = std::io::Cursor::new(vec![]);
        image::DynamicImage::new_rgba8(2, 2)
            .write_to(&mut cursor, image::ImageFormat::Png)
            .unwrap();
        let bytes = cursor.into_inner();
        let mut a = Element::new(1, Kind::Image);
        a.asset = Some(1);
        let mut b = Element::new(2, Kind::Image);
        b.asset = Some(collision);
        let file = crate::model::DesignFile {
            version: crate::model::VERSION,
            design: Design {
                id: 0,
                name: "Imported".into(),
                width: 1920,
                height: 1080,
                elements: vec![a, b],
            },
            assets: vec![
                crate::model::Asset {
                    id: 1,
                    name: "First".into(),
                    bytes: bytes.clone().into(),
                },
                crate::model::Asset {
                    id: collision,
                    name: "Second".into(),
                    bytes: bytes.into(),
                },
            ],
            motions: vec![],
        };
        let verified = crate::infrastructure::portable_design::verify(file).unwrap();
        s.import_design(verified).unwrap();
        assert_ne!(s.active().elements[0].asset, s.active().elements[1].asset);
        assert_eq!(s.active().elements[0].asset, Some(collision));
        s.project.validate().unwrap();
        s.undo().unwrap();
        s.redo();
        s.project.validate().unwrap();
    }
    #[test]
    fn replacing_project_invalidates_preview_cache_and_history() {
        let mut s = DocumentSession::default();
        s.add(Kind::Rectangle, 10., 10., 40., 40.).unwrap();
        let revision = s.revision;
        let generation = s.generation;
        s.replace(Project::fresh(), None).unwrap();
        assert!(s.revision > revision);
        assert!(s.generation > generation);
        assert!(!s.can_undo());
        assert!(s.selected.is_empty());
    }
    #[test]
    fn drag_is_one_undo_and_preserves_media() {
        let mut s = DocumentSession::default();
        let id = s.active().elements[0].id;
        let before = s.active().clone();
        s.select(Some(id), false);
        s.begin_gesture().unwrap();
        for dx in [1., 12., 50.] {
            s.translate_gesture(dx, 10., false).unwrap();
        }
        s.end_gesture().unwrap();
        assert_eq!(s.undo.len(), 1);
        s.undo().unwrap();
        assert!(s.active() == &before);
        s.redo();
        assert_eq!(s.active().elements[0].x, before.elements[0].x + 50.);
    }
    #[test]
    fn invalid_edit_rolls_back_and_cannot_enter_history() {
        let mut s = DocumentSession::default();
        let e = s.active().elements[0].clone();
        assert!(
            s.apply(
                e.id,
                LayerEdit::Geometry {
                    x: f32::NAN,
                    y: e.y,
                    width: e.w,
                    height: e.h,
                    rotation: 0.
                }
            )
            .is_err()
        );
        assert!(s.active().elements[0] == e);
        assert!(!s.can_undo());
        assert!(s.apply(e.id, LayerEdit::Binding("missing".into())).is_err());
    }
    #[test]
    fn reversed_stacking_hidden_locked_and_rotated_hit_testing() {
        let mut s = DocumentSession::default();
        s.project.designs[0].elements.clear();
        let a = s.add(Kind::Rectangle, 10., 10., 100., 40.).unwrap();
        let b = s.add(Kind::Rectangle, 10., 10., 100., 40.).unwrap();
        assert_eq!(s.hit_test(20., 20.), Some(b));
        s.apply(b, LayerEdit::Hidden(true)).unwrap();
        assert_eq!(s.hit_test(20., 20.), Some(a));
        s.apply(a, LayerEdit::Locked(true)).unwrap();
        assert_eq!(s.hit_test(20., 20.), None);
        s.apply(a, LayerEdit::Locked(false)).unwrap();
        s.apply(
            a,
            LayerEdit::Geometry {
                x: 10.,
                y: 10.,
                width: 100.,
                height: 40.,
                rotation: 90.,
            },
        )
        .unwrap();
        assert_eq!(s.hit_test(60., 70.), Some(a));
        assert_eq!(s.hit_test(20., 20.), None);
    }
    #[test]
    fn controller_history_preserves_live_scores_and_rejects_invalid_targets() {
        let mut s = DocumentSession::default();
        let before = s.project.controller.clone();
        let mut after = before.clone();
        after.controls[0].label = "Score five".into();
        after.controls[0].action = Action::Adjust {
            field: "home.score".into(),
            step: 5,
        };
        s.set_controller(after.clone()).unwrap();
        s.project.values.insert("home.score".into(), "77".into());
        s.undo().unwrap();
        assert_eq!(
            s.project.controller.controls[0].label,
            before.controls[0].label
        );
        assert_eq!(s.project.values["home.score"], "77");
        s.redo();
        assert_eq!(s.project.controller.controls[0].label, "Score five");
        after.controls[0].action = Action::Take {
            route: 500,
            design: 0,
        };
        assert!(s.set_controller(after).is_err());
        s.project.validate().unwrap();
    }
}
