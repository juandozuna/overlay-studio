use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::{collections::BTreeMap, path::Path, time::Instant};

pub const VERSION: u32 = 1;
pub const MAX_ID: u64 = u64::MAX - 4096;
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct Element {
    pub id: u64,
    pub name: String,
    pub kind: Kind,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub rotation: f32,
    pub fill: [u8; 4],
    pub stroke: [u8; 4],
    pub stroke_width: f32,
    pub radius: f32,
    pub opacity: f32,
    pub text: String,
    pub binding: String,
    pub font: String,
    pub font_size: f32,
    #[serde(default)]
    pub text_align: TextAlign,
    pub hidden: bool,
    pub locked: bool,
    pub group: Option<u64>,
    pub asset: Option<u64>,
    #[serde(default)]
    pub motion: Option<u64>,
    pub animation: Animation,
    pub duration: f32,
    #[serde(default)]
    pub keyframes: Vec<Keyframe>,
    #[serde(default)]
    pub loop_animation: bool,
    /// Value key holding `#RRGGBB`; when set and present, replaces the RGB of `fill`.
    #[serde(default)]
    pub fill_binding: String,
    /// Value-key prefix (for example `team.1`) whose `fx_id` restarts this layer's effect.
    #[serde(default)]
    pub fx_key: String,
    #[serde(default)]
    pub fx_style: crate::domain::fx::FxStyle,
}
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug, Default)]
pub enum TextAlign {
    Left,
    #[default]
    Center,
    Right,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Keyframe {
    pub time: f32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub rotation: f32,
    pub opacity: f32,
    pub easing: Easing,
}
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug, Default)]
pub enum Easing {
    Linear,
    #[default]
    EaseOut,
    EaseInOut,
    Hold,
    /// Overshoots then settles. Implemented by WN-6.
    EaseOutBack,
    /// Damped spring settle. Implemented by WN-6.
    Spring,
}
impl Keyframe {
    pub fn capture(element: &Element, time: f32) -> Self {
        Self {
            time,
            x: element.x,
            y: element.y,
            w: element.w,
            h: element.h,
            rotation: element.rotation,
            opacity: element.opacity,
            easing: Easing::EaseOut,
        }
    }
}
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
pub enum Kind {
    Rectangle,
    Ellipse,
    Triangle,
    Diamond,
    Line,
    Text,
    Image,
    Video,
    /// Prize wheel drawn from `domain::wheel` values. Painted by WN-1.
    Wheel,
    /// El Dado 3D cube drawn from `domain::dice` values.
    Dice,
}
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
pub enum Animation {
    None,
    Fade,
    Slide,
    Pop,
}
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct Design {
    #[serde(default)]
    pub id: u64,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub elements: Vec<Element>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct Asset {
    pub id: u64,
    pub name: String,
    pub bytes: Arc<Vec<u8>>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct MotionAsset {
    pub id: u64,
    pub name: String,
    pub gif: Arc<Vec<u8>>,
    pub fps: f32,
    pub looped: bool,
}
/// An imported sound file (wav, mp3, ogg or flac), embedded so the project is self-contained.
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct SoundAsset {
    pub id: u64,
    pub name: String,
    pub bytes: Arc<Vec<u8>>,
}
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
pub enum Protocol {
    Syphon,
    Ndi,
}
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct Route {
    #[serde(default)]
    pub id: u64,
    pub name: String,
    pub design: usize,
    #[serde(default)]
    pub design_id: u64,
    pub protocol: Protocol,
    pub fps: u32,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct StableReferences {
    #[serde(default)]
    pub cue_routes: BTreeMap<u64, u64>,
    #[serde(default)]
    pub cue_designs: BTreeMap<u64, u64>,
    #[serde(default)]
    pub cue_clocks: BTreeMap<u64, u64>,
    #[serde(default)]
    pub control_routes: BTreeMap<u64, u64>,
    #[serde(default)]
    pub control_designs: BTreeMap<u64, u64>,
    #[serde(default)]
    pub control_clocks: BTreeMap<u64, u64>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct Project {
    pub version: u32,
    pub name: String,
    pub designs: Vec<Design>,
    pub assets: Vec<Asset>,
    #[serde(default)]
    pub motions: Vec<MotionAsset>,
    #[serde(default)]
    pub sounds: Vec<SoundAsset>,
    pub values: BTreeMap<String, String>,
    pub routes: Vec<Route>,
    pub clock_seconds: f64,
    pub countdown: bool,
    #[serde(default)]
    pub teams: Vec<Team>,
    #[serde(default)]
    pub clocks: Vec<ClockSpec>,
    #[serde(default)]
    pub cue_rules: Vec<CueRule>,
    #[serde(default)]
    pub controller: crate::domain::controls::ControllerLayout,
    #[serde(default)]
    pub stable_references: StableReferences,
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct Team {
    pub id: u64,
    pub label: String,
    pub name_key: String,
    pub score_key: String,
    pub color: [u8; 3],
    pub steps: Vec<i64>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct ClockSpec {
    pub id: u64,
    pub name: String,
    pub value_key: String,
    pub seconds: f64,
    pub reset_seconds: f64,
    pub countdown: bool,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub enum CueEvent {
    ScoreAdded { field: String },
    ScoreRemoved { field: String },
    ClockStarted { clock: usize },
    ClockReset { clock: usize },
    ClockFinished { clock: usize },
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub enum CueTarget {
    ReplayOutput { route: usize },
    TakeDesign { route: usize, design: usize },
    SetValue { field: String, value: String },
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct CueRule {
    pub id: u64,
    pub name: String,
    pub enabled: bool,
    pub event: CueEvent,
    pub target: CueTarget,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct DesignFile {
    pub version: u32,
    pub design: Design,
    pub assets: Vec<Asset>,
    #[serde(default)]
    pub motions: Vec<MotionAsset>,
}

impl Element {
    pub fn new(id: u64, kind: Kind) -> Self {
        Self {
            id,
            name: format!("{kind:?}"),
            kind,
            x: 300.,
            y: 300.,
            w: 320.,
            h: 140.,
            rotation: 0.,
            fill: [28, 112, 142, 255],
            stroke: [240, 240, 255, 255],
            stroke_width: 0.,
            radius: 16.,
            opacity: 1.,
            text: "Your text".into(),
            binding: String::new(),
            font: "Default".into(),
            font_size: 64.,
            text_align: TextAlign::Center,
            hidden: false,
            locked: false,
            group: None,
            asset: None,
            motion: None,
            animation: Animation::Slide,
            duration: 0.35,
            keyframes: vec![],
            loop_animation: false,
            fill_binding: String::new(),
            fx_key: String::new(),
            fx_style: crate::domain::fx::FxStyle::None,
        }
    }
    /// Fill after applying `fill_binding`; alpha always comes from `fill`.
    pub fn resolved_fill(&self, values: &BTreeMap<String, String>) -> [u8; 4] {
        let rgb = values
            .get(&self.fill_binding)
            .and_then(|v| crate::domain::wheel::parse_hex(v));
        match rgb {
            Some(c) if !self.fill_binding.is_empty() => [c[0], c[1], c[2], self.fill[3]],
            _ => self.fill,
        }
    }
    pub fn content(&self, values: &BTreeMap<String, String>) -> String {
        if self.binding.is_empty() {
            self.text.clone()
        } else {
            values
                .get(&self.binding)
                .cloned()
                .unwrap_or_else(|| format!("{{{}}}", self.binding))
        }
    }
}
impl Project {
    pub fn add_team(&mut self) {
        let id = self.teams.iter().map(|t| t.id).max().unwrap_or(0) + 1;
        let mut n = id.max(3);
        while self.values.contains_key(&format!("team{n}.score"))
            || self.values.contains_key(&format!("team{n}.name"))
        {
            n += 1;
        }
        let name_key = format!("team{n}.name");
        let score_key = format!("team{n}.score");
        self.values.insert(name_key.clone(), format!("TEAM {n}"));
        self.values.insert(score_key.clone(), "0".into());
        self.teams.push(Team {
            id,
            label: format!("Team {n}"),
            name_key,
            score_key,
            color: [35, 105, 146],
            steps: vec![-1, 1],
        });
    }
    pub fn remove_team(&mut self, index: usize) {
        if index >= self.teams.len() || self.teams.len() <= 1 {
            return;
        }
        let team = self.teams.remove(index);
        self.remove_value(&team.name_key);
        self.remove_value(&team.score_key);
    }
    pub(crate) fn remove_value(&mut self, key: &str) {
        self.values.remove(key);
        for element in self.designs.iter_mut().flat_map(|d| &mut d.elements) {
            if element.binding == key {
                element.binding.clear();
                element.text.clear();
            }
        }
        self.controller.controls.retain(|c| !matches!(&c.action,
            crate::domain::controls::Action::Adjust { field, .. } | crate::domain::controls::Action::SetValue { field, .. } if field == key));
        self.cue_rules.retain(|rule| {
            !matches!(&rule.event, CueEvent::ScoreAdded { field } | CueEvent::ScoreRemoved { field } if field == key)
                && !matches!(&rule.target, CueTarget::SetValue { field, .. } if field == key)
        });
        self.controller.controls.retain(|control| {
            !matches!(control.action,
            crate::domain::controls::Action::RunCue { id } if !self.cue_rules.iter().any(|cue|cue.id == id))
        });
    }
    pub fn add_clock(&mut self) {
        let id = self.clocks.iter().map(|c| c.id).max().unwrap_or(0) + 1;
        let mut n = id.max(2);
        while self.values.contains_key(&format!("clock{n}")) {
            n += 1;
        }
        let key = format!("clock{n}");
        self.values.insert(key.clone(), "05:00".into());
        self.clocks.push(ClockSpec {
            id,
            name: format!("Clock {n}"),
            value_key: key,
            seconds: 300.,
            reset_seconds: 300.,
            countdown: true,
        });
    }
    pub fn remove_clock(&mut self, index: usize) {
        use crate::domain::controls::Action;
        if index >= self.clocks.len() || self.clocks.len() <= 1 {
            return;
        }
        let removed = self.clocks.remove(index);
        self.remove_value(&removed.value_key);
        self.controller.controls.retain_mut(|control| {
            if matches!(control.action, Action::ClockToggle | Action::ClockReset) && index == 0 {
                return false;
            }
            match &mut control.action {
                Action::ClockToggleAt { clock }
                | Action::ClockResetAt { clock }
                | Action::ClockAdjust { clock, .. } => {
                    if *clock == index {
                        return false;
                    }
                    if *clock > index {
                        *clock -= 1;
                    }
                }
                _ => {}
            }
            true
        });
        self.cue_rules.retain_mut(|rule| {
            match &mut rule.event {
                CueEvent::ClockStarted { clock }
                | CueEvent::ClockReset { clock }
                | CueEvent::ClockFinished { clock } => {
                    if *clock == index {
                        return false;
                    }
                    if *clock > index {
                        *clock -= 1;
                    }
                }
                _ => {}
            }
            true
        });
        self.controller.controls.retain(|control| {
            !matches!(control.action,
            Action::RunCue { id } if !self.cue_rules.iter().any(|cue|cue.id == id))
        });
    }
    pub fn fresh() -> Self {
        let values = BTreeMap::from([
            ("home.name".into(), "EAGLES".into()),
            ("away.name".into(), "LIONS".into()),
            ("home.score".into(), "12".into()),
            ("away.score".into(), "9".into()),
            ("round".into(), "2".into()),
            ("clock".into(), "08:42".into()),
            ("event".into(), "COMMUNITY CUP".into()),
        ]);
        let mut p = Self {
            version: VERSION,
            name: "Community Cup".into(),
            designs: vec![],
            assets: vec![],
            motions: vec![],
            sounds: vec![],
            values,
            routes: vec![],
            clock_seconds: 522.,
            countdown: true,
            teams: default_teams(),
            clocks: vec![ClockSpec {
                id: 1,
                name: "Game clock".into(),
                value_key: "clock".into(),
                seconds: 522.,
                reset_seconds: 600.,
                countdown: true,
            }],
            cue_rules: vec![],
            controller: Default::default(),
            stable_references: Default::default(),
        };
        p.designs.push(template("Broadcast", 0));
        p.designs.push(template("Corner score", 1));
        p.designs.push(template("Game night", 2));
        p.routes.push(Route {
            id: 1,
            name: "Overlay Studio • Broadcast".into(),
            design: 0,
            design_id: 1,
            protocol: Protocol::Syphon,
            fps: 30,
        });
        p.routes.push(Route {
            id: 2,
            name: "Overlay Studio • Venue".into(),
            design: 2,
            design_id: 3,
            protocol: Protocol::Ndi,
            fps: 30,
        });
        p.sync_stable_references()
            .expect("fresh project references are valid");
        p
    }
    pub fn next_id(&self) -> u64 {
        self.designs
            .iter()
            .flat_map(|d| &d.elements)
            .map(|e| e.id)
            .chain(
                self.designs
                    .iter()
                    .flat_map(|d| &d.elements)
                    .filter_map(|e| e.group),
            )
            .chain(self.assets.iter().map(|a| a.id))
            .chain(self.motions.iter().map(|a| a.id))
            .chain(self.sounds.iter().map(|a| a.id))
            .max()
            .unwrap_or(0)
            .saturating_add(1)
    }
    pub fn validate(&self) -> Result<(), String> {
        self.validate_inner(true)
    }
    /// Validate edits to a previously verified project without decoding immutable
    /// media again on the UI thread. File/import boundaries must use `validate`.
    pub fn validate_metadata(&self) -> Result<(), String> {
        self.validate_inner(false)
    }
    fn validate_inner(&self, validate_media: bool) -> Result<(), String> {
        if self.version != VERSION {
            return Err(format!("Unsupported project version {}", self.version));
        }
        if self.designs.is_empty() || self.designs.len() > 64 {
            return Err("Project must contain 1–64 designs".into());
        }
        if self.routes.len() > 8 {
            return Err("Maximum eight outputs".into());
        }
        if !self.clock_seconds.is_finite() || self.clock_seconds < 0. {
            return Err("Invalid clock value".into());
        }
        if self.teams.len() > 32 || self.clocks.len() > 16 || self.cue_rules.len() > 128 {
            return Err("Project exceeds team, clock, or cue limits".into());
        }
        let mut keys = std::collections::HashSet::new();
        for team in &self.teams {
            if team.id > MAX_ID
                || team.label.trim().is_empty()
                || !self.values.contains_key(&team.name_key)
                || !self.values.contains_key(&team.score_key)
                || team.steps.is_empty()
                || team.steps.len() > 12
                || team.steps.contains(&0)
            {
                return Err(format!("Invalid team control: {}", team.label));
            }
        }
        for clock in &self.clocks {
            if clock.id > MAX_ID
                || clock.name.trim().is_empty()
                || clock.value_key.trim().is_empty()
                || !keys.insert(clock.value_key.clone())
                || !clock.seconds.is_finite()
                || !clock.reset_seconds.is_finite()
                || clock.seconds < 0.
                || clock.reset_seconds < 0.
                || clock.seconds > 604800.
                || clock.reset_seconds > 604800.
            {
                return Err(format!("Invalid clock: {}", clock.name));
            }
        }
        let mut cue_ids = std::collections::HashSet::new();
        for rule in &self.cue_rules {
            let event_valid = match &rule.event {
                CueEvent::ScoreAdded { field } | CueEvent::ScoreRemoved { field } => {
                    self.values.contains_key(field)
                }
                CueEvent::ClockStarted { clock }
                | CueEvent::ClockReset { clock }
                | CueEvent::ClockFinished { clock } => *clock < self.clocks.len(),
            };
            let target_valid = match &rule.target {
                CueTarget::ReplayOutput { route } => *route < self.routes.len(),
                CueTarget::TakeDesign { route, design } => {
                    *route < self.routes.len() && *design < self.designs.len()
                }
                CueTarget::SetValue { field, .. } => self.values.contains_key(field),
            };
            if rule.id > MAX_ID
                || !cue_ids.insert(rule.id)
                || rule.name.trim().is_empty()
                || !event_valid
                || !target_valid
            {
                return Err(format!("Invalid cue rule: {}", rule.name));
            }
        }
        for d in &self.designs {
            validate_design(d)?;
        }
        self.controller.validate(self)?;
        for r in &self.routes {
            if r.design >= self.designs.len()
                || !matches!(r.fps, 24 | 25 | 30 | 50 | 60)
                || r.name.contains('\0')
            {
                return Err("Invalid output route".into());
            }
        }
        let mut ids = std::collections::HashSet::new();
        for a in &self.assets {
            if a.id > MAX_ID || !ids.insert(a.id) || a.bytes.len() > 32 * 1024 * 1024 {
                return Err("Invalid or oversized image asset".into());
            }
            if validate_media {
                let img = image::load_from_memory(&a.bytes)
                    .map_err(|e| format!("Invalid image {}: {e}", a.name))?;
                if img.width() > 8192 || img.height() > 8192 {
                    return Err("Image exceeds 8192px".into());
                }
            }
        }
        if self.sounds.len() > 64 {
            return Err("Maximum 64 imported sounds".into());
        }
        for sound in &self.sounds {
            if sound.id > MAX_ID
                || !ids.insert(sound.id)
                || sound.name.trim().is_empty()
                || sound.bytes.is_empty()
                || sound.bytes.len() > 16 * 1024 * 1024
            {
                return Err(format!("Invalid sound: {}", sound.name));
            }
        }
        for motion in &self.motions {
            if motion.id > MAX_ID
                || !ids.insert(motion.id)
                || motion.gif.len() > 64 * 1024 * 1024
                || !motion.fps.is_finite()
                || !(1. ..=60.).contains(&motion.fps)
                || validate_media && image::load_from_memory(&motion.gif).is_err()
            {
                return Err(format!("Invalid motion clip: {}", motion.name));
            }
        }
        for d in &self.designs {
            for e in &d.elements {
                if let Some(id) = e.asset
                    && !self.assets.iter().any(|asset| asset.id == id)
                {
                    return Err(format!("Missing image for {}", e.name));
                }
                if let Some(id) = e.motion
                    && !self.motions.iter().any(|motion| motion.id == id)
                {
                    return Err(format!("Missing video for {}", e.name));
                }
            }
        }
        Ok(())
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        if bytes.len() > 512 * 1024 * 1024 {
            return Err("Project exceeds 512 MB".into());
        }
        let mut p: Self = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        p.normalize_runtime_schema();
        p.validate()?;
        Ok(p)
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let mut snapshot = self.clone();
        snapshot.sync_stable_references()?;
        snapshot.validate()?;
        atomic_json(path, &snapshot)
    }
    pub fn adjust_score(&mut self, key: &str, delta: i64) {
        let n = self
            .values
            .get(key)
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0);
        self.values
            .insert(key.into(), n.saturating_add(delta).to_string());
    }
    pub fn normalize_runtime_schema(&mut self) {
        if self.teams.is_empty() {
            self.teams = default_teams();
        }
        if self.clocks.is_empty() {
            self.clocks.push(ClockSpec {
                id: 1,
                name: "Game clock".into(),
                value_key: "clock".into(),
                seconds: self.clock_seconds,
                reset_seconds: 600.,
                countdown: self.countdown,
            });
        }
        for clock in &self.clocks {
            self.values
                .entry(clock.value_key.clone())
                .or_insert_with(|| clock_text(clock.seconds));
        }
        self.resolve_stable_references();
    }

    fn ensure_stable_ids(&mut self) {
        let mut used = std::collections::HashSet::new();
        let mut next = self
            .designs
            .iter()
            .map(|design| design.id)
            .max()
            .unwrap_or(0)
            .saturating_add(1)
            .max(1);
        for design in &mut self.designs {
            if design.id == 0 || design.id > MAX_ID || !used.insert(design.id) {
                while used.contains(&next) && next < MAX_ID {
                    next += 1;
                }
                design.id = next;
                used.insert(next);
                next = next.saturating_add(1);
            }
        }
        used.clear();
        next = self
            .routes
            .iter()
            .map(|route| route.id)
            .max()
            .unwrap_or(0)
            .saturating_add(1)
            .max(1);
        for route in &mut self.routes {
            if route.id == 0 || route.id > MAX_ID || !used.insert(route.id) {
                while used.contains(&next) && next < MAX_ID {
                    next += 1;
                }
                route.id = next;
                used.insert(next);
                next = next.saturating_add(1);
            }
        }
    }

    fn resolve_stable_references(&mut self) {
        self.ensure_stable_ids();
        let design_index: BTreeMap<_, _> = self
            .designs
            .iter()
            .enumerate()
            .map(|(index, design)| (design.id, index))
            .collect();
        let route_index: BTreeMap<_, _> = self
            .routes
            .iter()
            .enumerate()
            .map(|(index, route)| (route.id, index))
            .collect();
        let clock_index: BTreeMap<_, _> = self
            .clocks
            .iter()
            .enumerate()
            .map(|(index, clock)| (clock.id, index))
            .collect();
        for route in &mut self.routes {
            if let Some(index) = design_index.get(&route.design_id) {
                route.design = *index;
            }
        }
        for rule in &mut self.cue_rules {
            if let Some(id) = self.stable_references.cue_clocks.get(&rule.id)
                && let Some(index) = clock_index.get(id)
            {
                match &mut rule.event {
                    CueEvent::ClockStarted { clock }
                    | CueEvent::ClockReset { clock }
                    | CueEvent::ClockFinished { clock } => *clock = *index,
                    _ => {}
                }
            }
            if let Some(id) = self.stable_references.cue_routes.get(&rule.id)
                && let Some(index) = route_index.get(id)
            {
                match &mut rule.target {
                    CueTarget::ReplayOutput { route } | CueTarget::TakeDesign { route, .. } => {
                        *route = *index
                    }
                    _ => {}
                }
            }
            if let Some(id) = self.stable_references.cue_designs.get(&rule.id)
                && let Some(index) = design_index.get(id)
                && let CueTarget::TakeDesign { design, .. } = &mut rule.target
            {
                *design = *index;
            }
        }
        for control in &mut self.controller.controls {
            if let Some(id) = self.stable_references.control_clocks.get(&control.id)
                && let Some(index) = clock_index.get(id)
            {
                match &mut control.action {
                    crate::domain::controls::Action::ClockToggleAt { clock }
                    | crate::domain::controls::Action::ClockResetAt { clock }
                    | crate::domain::controls::Action::ClockAdjust { clock, .. } => *clock = *index,
                    _ => {}
                }
            }
            if let Some(id) = self.stable_references.control_routes.get(&control.id)
                && let Some(index) = route_index.get(id)
            {
                match &mut control.action {
                    crate::domain::controls::Action::ToggleOverlay { route }
                    | crate::domain::controls::Action::Cue { route }
                    | crate::domain::controls::Action::Take { route, .. } => *route = *index,
                    _ => {}
                }
            }
            if let Some(id) = self.stable_references.control_designs.get(&control.id)
                && let Some(index) = design_index.get(id)
                && let crate::domain::controls::Action::Take { design, .. } = &mut control.action
            {
                *design = *index;
            }
        }
        let _ = self.sync_stable_references();
    }

    pub fn sync_stable_references(&mut self) -> Result<(), String> {
        self.ensure_stable_ids();
        let mut refs = StableReferences::default();
        for route in &mut self.routes {
            route.design_id = self
                .designs
                .get(route.design)
                .ok_or("Output references missing design")?
                .id;
        }
        for rule in &self.cue_rules {
            match &rule.event {
                CueEvent::ClockStarted { clock }
                | CueEvent::ClockReset { clock }
                | CueEvent::ClockFinished { clock } => {
                    refs.cue_clocks.insert(
                        rule.id,
                        self.clocks
                            .get(*clock)
                            .ok_or("Cue references missing clock")?
                            .id,
                    );
                }
                _ => {}
            }
            match &rule.target {
                CueTarget::ReplayOutput { route } => {
                    refs.cue_routes.insert(
                        rule.id,
                        self.routes
                            .get(*route)
                            .ok_or("Cue references missing output")?
                            .id,
                    );
                }
                CueTarget::TakeDesign { route, design } => {
                    refs.cue_routes.insert(
                        rule.id,
                        self.routes
                            .get(*route)
                            .ok_or("Cue references missing output")?
                            .id,
                    );
                    refs.cue_designs.insert(
                        rule.id,
                        self.designs
                            .get(*design)
                            .ok_or("Cue references missing design")?
                            .id,
                    );
                }
                _ => {}
            }
        }
        for control in &self.controller.controls {
            use crate::domain::controls::Action;
            match &control.action {
                Action::ClockToggleAt { clock }
                | Action::ClockResetAt { clock }
                | Action::ClockAdjust { clock, .. } => {
                    refs.control_clocks.insert(
                        control.id,
                        self.clocks
                            .get(*clock)
                            .ok_or("Remote button references missing clock")?
                            .id,
                    );
                }
                _ => {}
            }
            match &control.action {
                Action::ToggleOverlay { route } | Action::Cue { route } => {
                    refs.control_routes.insert(
                        control.id,
                        self.routes
                            .get(*route)
                            .ok_or("Remote button references missing output")?
                            .id,
                    );
                }
                Action::Take { route, design } => {
                    refs.control_routes.insert(
                        control.id,
                        self.routes
                            .get(*route)
                            .ok_or("Remote button references missing output")?
                            .id,
                    );
                    refs.control_designs.insert(
                        control.id,
                        self.designs
                            .get(*design)
                            .ok_or("Remote button references missing design")?
                            .id,
                    );
                }
                _ => {}
            }
        }
        self.stable_references = refs;
        Ok(())
    }
}

pub fn default_teams() -> Vec<Team> {
    vec![
        Team {
            id: 1,
            label: "Home".into(),
            name_key: "home.name".into(),
            score_key: "home.score".into(),
            color: [19, 95, 123],
            steps: vec![-1, 1, 2, 3],
        },
        Team {
            id: 2,
            label: "Away".into(),
            name_key: "away.name".into(),
            score_key: "away.score".into(),
            color: [134, 72, 34],
            steps: vec![-1, 1, 2, 3],
        },
    ]
}
pub fn validate_design(d: &Design) -> Result<(), String> {
    if !(64..=4096).contains(&d.width)
        || !(64..=4096).contains(&d.height)
        || d.elements.len() > 1000
    {
        return Err("Canvas must be 64–4096 pixels per side; maximum 1000 layers".into());
    }
    let mut ids = std::collections::HashSet::new();
    for e in &d.elements {
        if e.keyframes.len() > 256
            || e.keyframes.iter().any(|frame| {
                [
                    frame.time,
                    frame.x,
                    frame.y,
                    frame.w,
                    frame.h,
                    frame.rotation,
                    frame.opacity,
                ]
                .iter()
                .any(|n| !n.is_finite())
                    || !(0. ..=300.).contains(&frame.time)
                    || !(0. ..=1.).contains(&frame.opacity)
                    || frame.w < 1.
                    || frame.h < 1.
                    || frame.w > 16384.
                    || frame.h > 16384.
            })
        {
            return Err(format!("Invalid animation keyframes: {}", e.name));
        }
        if e.id > MAX_ID || e.group.is_some_and(|id| id > MAX_ID) || !ids.insert(e.id) {
            return Err("Duplicate layer ID".into());
        }
        if [
            e.x,
            e.y,
            e.w,
            e.h,
            e.rotation,
            e.radius,
            e.opacity,
            e.stroke_width,
            e.font_size,
            e.duration,
        ]
        .iter()
        .any(|n| !n.is_finite())
            || e.w < 1.
            || e.h < 1.
            || e.w > 16384.
            || e.h > 16384.
            || !(0. ..=1.).contains(&e.opacity)
            || e.duration < 0.01
            || e.duration > 10.
            || e.font_size < 1.
            || e.font_size > 2048.
        {
            return Err(format!("Invalid layer geometry: {}", e.name));
        }
    }
    Ok(())
}
pub fn atomic_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    let temp = path.with_extension("saving");
    use std::io::Write;
    let mut f = std::fs::File::create(&temp).map_err(|e| e.to_string())?;
    f.write_all(&bytes)
        .and_then(|_| f.sync_all())
        .map_err(|e| e.to_string())?;
    std::fs::rename(temp, path).map_err(|e| e.to_string())
}
pub fn clock_text(seconds: f64) -> String {
    let n = seconds.max(0.).ceil() as u64;
    format!("{:02}:{:02}", n / 60, n % 60)
}
pub struct GameClock {
    pub running: bool,
    pub last: Instant,
}
impl Default for GameClock {
    fn default() -> Self {
        Self::new()
    }
}
impl GameClock {
    pub fn new() -> Self {
        Self {
            running: false,
            last: Instant::now(),
        }
    }
    pub fn tick_spec(&mut self, clock: &mut ClockSpec) -> bool {
        let now = Instant::now();
        let dt = now.duration_since(self.last).as_secs_f64();
        self.last = now;
        let was_running = self.running;
        if self.running {
            clock.seconds = if clock.countdown {
                (clock.seconds - dt).max(0.)
            } else {
                (clock.seconds + dt).min(604800.)
            };
            if clock.countdown && clock.seconds == 0. {
                self.running = false;
            }
            if !clock.countdown && clock.seconds == 604800. {
                self.running = false;
            }
        }
        was_running && !self.running && clock.countdown && clock.seconds == 0.
    }
}
pub fn template(name: &str, style: usize) -> Design {
    let mut elements = vec![];
    let mut add =
        |kind: Kind, x: f32, y: f32, w: f32, h: f32, fill: [u8; 4], binding: &str, size: f32| {
            let mut e = Element::new(elements.len() as u64 + 1, kind);
            e.x = x;
            e.y = y;
            e.w = w;
            e.h = h;
            e.fill = fill;
            e.binding = binding.into();
            e.name = if binding.is_empty() {
                format!("{kind:?} panel")
            } else {
                binding.into()
            };
            e.font_size = size;
            e.text = String::new();
            elements.push(e);
        };
    let ink = [241, 247, 255, 255];
    let blue = [14, 93, 122, 255];
    let amber = [140, 74, 32, 255];
    let dark = [18, 28, 43, 255];
    if style == 2 {
        add(Kind::Rectangle, 260., 280., 600., 500., blue, "", 0.);
        add(Kind::Rectangle, 1060., 280., 600., 500., amber, "", 0.);
        add(Kind::Text, 300., 320., 520., 120., ink, "home.name", 70.);
        add(Kind::Text, 1100., 320., 520., 120., ink, "away.name", 70.);
        add(Kind::Text, 300., 450., 520., 270., ink, "home.score", 210.);
        add(Kind::Text, 1100., 450., 520., 270., ink, "away.score", 210.);
        add(
            Kind::Text,
            800.,
            820.,
            320.,
            90.,
            [45, 40, 62, 255],
            "clock",
            70.,
        );
        add(
            Kind::Text,
            560.,
            80.,
            800.,
            110.,
            [45, 40, 62, 255],
            "event",
            64.,
        );
    } else {
        let (x, y, w, h) = if style == 1 {
            (90., 80., 960., 120.)
        } else {
            (220., 820., 1480., 140.)
        };
        let unit = w / 8.;
        add(Kind::Rectangle, x, y, w, h, dark, "", 0.);
        add(Kind::Rectangle, x, y, unit * 3., h, blue, "", 0.);
        add(
            Kind::Rectangle,
            x + unit * 5.,
            y,
            unit * 3.,
            h,
            amber,
            "",
            0.,
        );
        add(Kind::Text, x + 12., y, unit * 2., h, ink, "home.name", 48.);
        add(
            Kind::Text,
            x + unit * 2.,
            y,
            unit,
            h,
            ink,
            "home.score",
            72.,
        );
        add(
            Kind::Text,
            x + unit * 3.,
            y,
            unit * 2.,
            h,
            ink,
            "clock",
            58.,
        );
        add(
            Kind::Text,
            x + unit * 5.,
            y,
            unit,
            h,
            ink,
            "away.score",
            72.,
        );
        add(
            Kind::Text,
            x + unit * 6.,
            y,
            unit * 2. - 12.,
            h,
            ink,
            "away.name",
            48.,
        );
    }
    // Text has a valid nominal font size even when the helper receives zero for shapes.
    for e in &mut elements {
        if e.font_size == 0. {
            e.font_size = 64.;
        }
    }
    Design {
        id: style as u64 + 1,
        name: name.into(),
        width: 1920,
        height: 1080,
        elements,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip_and_routes() {
        let p = Project::fresh();
        p.validate().unwrap();
        let q: Project = serde_json::from_slice(&serde_json::to_vec(&p).unwrap()).unwrap();
        assert!(p == q);
        assert_eq!(q.routes[0].design, 0);
        assert_eq!(q.routes[1].design, 2);
    }
    #[test]
    fn bindings_share_live_state() {
        let mut p = Project::fresh();
        p.adjust_score("home.score", 3);
        for d in &p.designs {
            let e = d
                .elements
                .iter()
                .find(|e| e.binding == "home.score")
                .unwrap();
            assert_eq!(e.content(&p.values), "15");
        }
    }
    #[test]
    fn reject_bad_project() {
        let mut p = Project::fresh();
        p.routes[0].design = 99;
        assert!(p.validate().is_err());
        p.routes[0].design = 0;
        p.designs[0].elements[0].w = f32::NAN;
        assert!(p.validate().is_err());
    }
    #[test]
    fn clock_tracks_elapsed_time() {
        let mut clock = ClockSpec {
            id: 1,
            name: "Test".into(),
            value_key: "clock".into(),
            seconds: 10.,
            reset_seconds: 10.,
            countdown: true,
        };
        let mut c = GameClock::new();
        c.running = true;
        c.last = Instant::now() - std::time::Duration::from_secs(3);
        c.tick_spec(&mut clock);
        assert!((clock.seconds - 7.).abs() < 0.05);
        assert_eq!(clock_text(65.), "01:05");
    }
    #[test]
    fn old_project_gets_teams_and_primary_clock_on_load() {
        let p = Project::fresh();
        let mut value = serde_json::to_value(&p).unwrap();
        let object = value.as_object_mut().unwrap();
        object.remove("teams");
        object.remove("clocks");
        object.remove("cue_rules");
        object.remove("stable_references");
        for design in object["designs"].as_array_mut().unwrap() {
            design.as_object_mut().unwrap().remove("id");
        }
        for route in object["routes"].as_array_mut().unwrap() {
            let route = route.as_object_mut().unwrap();
            route.remove("id");
            route.remove("design_id");
        }
        let path = std::env::temp_dir().join(format!(
            "overlay-studio-migration-{}-{}.overlay",
            std::process::id(),
            rand::random::<u64>()
        ));
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        let loaded = Project::load(&path).unwrap();
        assert_eq!(loaded.teams.len(), 2);
        assert_eq!(loaded.clocks.len(), 1);
        assert_eq!(loaded.clocks[0].seconds, p.clock_seconds);
        assert!(loaded.designs.iter().all(|design| design.id != 0));
        assert!(loaded.routes.iter().all(|route| route.id != 0));
        let _ = std::fs::remove_file(path);
    }
    #[test]
    fn stable_references_survive_array_reordering_on_load() {
        use crate::domain::controls::Action;
        let mut project = Project::fresh();
        project.clocks.push(ClockSpec {
            id: 2,
            name: "Second clock".into(),
            value_key: "clock.2".into(),
            seconds: 30.,
            reset_seconds: 30.,
            countdown: true,
        });
        project.values.insert("clock.2".into(), "00:30".into());
        project.cue_rules.push(CueRule {
            id: 900,
            name: "Stable take".into(),
            enabled: true,
            event: CueEvent::ClockStarted { clock: 1 },
            target: CueTarget::TakeDesign {
                route: 1,
                design: 1,
            },
        });
        let path = std::env::temp_dir().join(format!(
            "overlay-studio-stable-refs-{}-{}.overlay",
            std::process::id(),
            rand::random::<u64>()
        ));
        project.save(&path).unwrap();
        let mut value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        value["designs"].as_array_mut().unwrap().reverse();
        value["routes"].as_array_mut().unwrap().reverse();
        value["clocks"].as_array_mut().unwrap().reverse();
        std::fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
        let loaded = Project::load(&path).unwrap();
        let cue = loaded.cue_rules.iter().find(|cue| cue.id == 900).unwrap();
        assert!(matches!(cue.event, CueEvent::ClockStarted { clock: 0 }));
        assert!(matches!(
            cue.target,
            CueTarget::TakeDesign {
                route: 0,
                design: 1
            }
        ));
        let take = loaded
            .controller
            .controls
            .iter()
            .find(|control| matches!(control.action, Action::Take { .. }))
            .unwrap();
        assert!(matches!(
            take.action,
            Action::Take {
                route: 1,
                design: 0
            }
        ));
        assert_eq!(loaded.routes[1].name, "Overlay Studio • Broadcast");
        assert_eq!(loaded.routes[1].design, 2);
        let _ = std::fs::remove_file(path);
    }
    #[test]
    fn removing_controls_preserves_references_and_new_fields_are_unique() {
        use crate::domain::controls::Action;
        let mut project = Project::fresh();
        project.add_team();
        project.add_team();
        project.values.insert("team4.score".into(), "37".into());
        project.controller = crate::domain::controls::ControllerLayout::for_game(&project).unwrap();
        project.cue_rules.push(CueRule {
            id: 1,
            name: "Removed team cue".into(),
            enabled: true,
            event: CueEvent::ScoreAdded {
                field: "team3.score".into(),
            },
            target: CueTarget::SetValue {
                field: "event".into(),
                value: "Goal".into(),
            },
        });
        project.remove_team(2);
        project.add_team();
        assert_eq!(project.values["team4.score"], "37");
        assert!(
            !project.controller.controls.iter().any(
                |c| matches!(&c.action, Action::Adjust { field, .. } if field == "team3.score")
            )
        );
        assert!(project.cue_rules.is_empty());
        project.add_clock();
        project.add_clock();
        let mut control = project.controller.controls[0].clone();
        control.id = 999;
        control.action = Action::ClockAdjust {
            clock: 2,
            seconds: 10,
        };
        project.controller.controls.push(control);
        project.cue_rules.push(CueRule {
            id: 2,
            name: "Clock cue".into(),
            enabled: true,
            event: CueEvent::ClockStarted { clock: 2 },
            target: CueTarget::ReplayOutput { route: 0 },
        });
        project.remove_clock(1);
        assert!(matches!(
            project.controller.controls.last().unwrap().action,
            Action::ClockAdjust { clock: 1, .. }
        ));
        assert!(matches!(
            project.cue_rules[0].event,
            CueEvent::ClockStarted { clock: 1 }
        ));
        project.add_clock();
        project.validate().unwrap();
    }
    #[test]
    fn portable_project_and_design_files() {
        let mut p = Project::fresh();
        p.controller.controls[0].label = "Home +3".into();
        p.controller.controls[0].action = crate::domain::controls::Action::Adjust {
            field: "home.score".into(),
            step: 3,
        };
        p.designs[0].elements[0].group = Some(500);
        assert_eq!(p.next_id(), 501);
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(2, 2)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        p.assets.push(Asset {
            id: 501,
            name: "Embedded logo".into(),
            bytes: bytes.into_inner().into(),
        });
        let mut logo = Element::new(502, Kind::Image);
        logo.asset = Some(501);
        p.designs[0].elements.push(logo);
        let path = std::env::temp_dir().join(format!(
            "overlay-studio-{}-{}.overlay",
            std::process::id(),
            rand::random::<u64>()
        ));
        p.save(&path).unwrap();
        let loaded = Project::load(&path).unwrap();
        assert!(loaded == p);
        let design_path = path.with_extension("scoredesign");
        let design = DesignFile {
            version: VERSION,
            design: p.designs[0].clone(),
            assets: p.assets.clone(),
            motions: p.motions.clone(),
        };
        atomic_json(&design_path, &design).unwrap();
        let loaded: DesignFile =
            serde_json::from_slice(&std::fs::read(&design_path).unwrap()).unwrap();
        assert!(loaded.design == design.design);
        assert!(loaded.assets == design.assets);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(design_path).unwrap();
    }
}
