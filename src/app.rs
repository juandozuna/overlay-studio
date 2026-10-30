use crate::{
    model::*,
    output::Output,
    remote::{self, Action, Control, Remote, Snapshot},
    scene::{self, FontBook, Resources},
};
use egui::{Color32, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use std::{
    collections::{HashSet, VecDeque},
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Design,
    Live,
    Cues,
    Remote,
}
#[derive(Clone)]
enum DragKind {
    Move,
    Create,
    Resize { left: bool, top: bool },
    Rotate,
}
struct Drag {
    start: Pos2,
    elements: Vec<Element>,
    kind: DragKind,
}
struct VideoImport {
    epoch: u64,
    design: usize,
    worker: std::sync::mpsc::Receiver<Result<crate::media::PreparedVideo, String>>,
}
pub struct Studio {
    project: Project,
    design: usize,
    page: Page,
    selected: HashSet<u64>,
    drag: Option<Drag>,
    tool: Option<Kind>,
    fonts: FontBook,
    resources: Resources,
    gpu: Option<egui_wgpu::RenderState>,
    outputs: Vec<Option<Output>>,
    clocks: Vec<GameClock>,
    preview: Option<Instant>,
    preview_time: Option<f32>,
    path: Option<PathBuf>,
    dirty: bool,
    message: String,
    undo: Vec<Project>,
    redo: Vec<Project>,
    pending_history: Option<Project>,
    zoom: f32,
    pan: Vec2,
    snap: bool,
    font_search: String,
    remote: Option<Remote>,
    remote_ip: String,
    remote_port: u16,
    qr: Option<egui::TextureHandle>,
    control: Option<u64>,
    remote_seen: VecDeque<String>,
    new_field: String,
    last_save: Instant,
    recovery: Option<Project>,
    confirm_reset: Option<usize>,
    history_epoch: u64,
    ai_open: bool,
    ai_engine: Option<crate::layout_ai::Engine>,
    ai_worker: Option<std::sync::mpsc::Receiver<crate::layout_ai::Event>>,
    ai_status: String,
    ai_brief: String,
    ai_proposal: Option<(usize, Design, Design)>,
    ai_source: Option<(usize, Design)>,
    video_import: Option<VideoImport>,
}
impl Studio {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self::with_context(&cc.egui_ctx, cc.wgpu_render_state.clone(), true)
    }
    fn with_context(
        ctx: &egui::Context,
        gpu: Option<egui_wgpu::RenderState>,
        recover: bool,
    ) -> Self {
        let mut style = (*ctx.style_of(egui::Theme::Dark)).clone();
        style.spacing.item_spacing = Vec2::new(10., 8.);
        style.spacing.button_padding = Vec2::new(12., 8.);
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(15.));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(14.));
        ctx.set_style_of(egui::Theme::Dark, style);
        ctx.set_visuals(egui::Visuals::dark());
        let project = Project::fresh();
        let recovery = if recover {
            Project::load(&autosave_path()).ok()
        } else {
            None
        };
        Self {
            project,
            design: 0,
            page: Page::Design,
            selected: HashSet::new(),
            drag: None,
            tool: None,
            fonts: FontBook::new(),
            resources: Resources::default(),
            gpu,
            outputs: vec![None, None],
            clocks: vec![GameClock::new()],
            preview: None,
            preview_time: None,
            path: None,
            dirty: false,
            message: "Create your layout, bind live values, then start an output.".into(),
            undo: vec![],
            redo: vec![],
            pending_history: None,
            zoom: 1.,
            pan: Vec2::ZERO,
            snap: true,
            font_search: String::new(),
            remote: None,
            remote_ip: remote::addresses()
                .first()
                .cloned()
                .unwrap_or("127.0.0.1".into()),
            remote_port: 9876,
            qr: None,
            control: None,
            remote_seen: VecDeque::new(),
            new_field: String::new(),
            last_save: Instant::now(),
            recovery,
            confirm_reset: None,
            history_epoch: 0,
            ai_open: false,
            ai_engine: None,
            ai_worker: None,
            ai_status: "Set up once to download the local model.".into(),
            ai_brief: "Arrange these layers into a clear, balanced scoreboard. Keep team names and scores together.".into(),
            ai_proposal: None,
            ai_source: None,
            video_import: None,
        }
    }
    fn remember(&mut self, before: Project) {
        if self.undo.last() != Some(&before) {
            self.undo.push(before);
            if self.undo.len() > 64 {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
        self.dirty = true;
    }
    fn track_ui_edit(&mut self, before: Project, epoch: u64) {
        if self.project != before && self.history_epoch == epoch {
            if self.pending_history.is_none() {
                self.pending_history = Some(before);
            }
            self.dirty = true;
        }
    }
    fn add(&mut self, kind: Kind, pos: Option<Pos2>) {
        let mut e = Element::new(self.project.next_id(), kind);
        if let Some(p) = pos {
            e.x = p.x;
            e.y = p.y;
        }
        if kind == Kind::Text {
            e.fill = [241, 247, 255, 255];
        }
        self.selected.clear();
        self.selected.insert(e.id);
        self.project.designs[self.design].elements.push(e);
    }
    fn undo(&mut self) {
        if let Some(p) = self.pending_history.take() {
            self.remember(p);
        }
        if let Some(p) = self.undo.pop() {
            self.history_epoch += 1;
            self.redo.push(self.project.clone());
            self.project = p;
            self.clocks = (0..self.project.clocks.len())
                .map(|_| GameClock::new())
                .collect();
            self.outputs.resize_with(self.project.routes.len(), || None);
            self.selected.clear();
            self.design = self.design.min(self.project.designs.len() - 1);
            self.dirty = true;
        }
    }
    fn redo(&mut self) {
        if let Some(p) = self.redo.pop() {
            self.history_epoch += 1;
            self.undo.push(self.project.clone());
            self.project = p;
            self.clocks = (0..self.project.clocks.len())
                .map(|_| GameClock::new())
                .collect();
            self.outputs.resize_with(self.project.routes.len(), || None);
            self.selected.clear();
            self.design = self.design.min(self.project.designs.len() - 1);
            self.dirty = true;
        }
    }
    fn save(&mut self, as_new: bool) {
        if let Some(before) = self.pending_history.take() {
            self.remember(before);
        }
        let path = if as_new || self.path.is_none() {
            rfd::FileDialog::new()
                .add_filter("Overlay project", &["overlay"])
                .set_file_name(format!("{}.overlay", self.project.name))
                .save_file()
        } else {
            self.path.clone()
        };
        if let Some(path) = path {
            match self.project.save(&path) {
                Ok(()) => {
                    self.path = Some(path);
                    self.dirty = false;
                    self.message =
                        "Project saved, including controller layout and image assets.".into();
                }
                Err(e) => self.message = e,
            }
        }
    }
    fn load(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Overlay project", &["overlay", "json"])
            .pick_file()
        {
            match Project::load(&path) {
                Ok(p) => {
                    if self.dirty
                        && !matches!(
                            rfd::MessageDialog::new()
                                .set_title("Open project")
                                .set_description("Replace the current unsaved project?")
                                .set_buttons(rfd::MessageButtons::YesNo)
                                .show(),
                            rfd::MessageDialogResult::Yes
                        )
                    {
                        return;
                    }
                    self.install(p);
                    self.path = Some(path);
                    self.dirty = false;
                    self.message = "Project opened. Timer and outputs are stopped.".into();
                }
                Err(e) => self.message = e,
            }
        }
    }
    fn install(&mut self, p: Project) {
        self.history_epoch += 1;
        self.remote = None;
        self.qr = None;
        self.outputs = (0..p.routes.len()).map(|_| None).collect();
        self.project = p;
        self.dirty = false;
        self.design = 0;
        self.selected.clear();
        self.undo.clear();
        self.redo.clear();
        self.pending_history = None;
        self.clocks = (0..self.project.clocks.len())
            .map(|_| GameClock::new())
            .collect();
        self.resources = Resources::default();
        self.preview = None;
        self.preview_time = None;
        self.ai_proposal = None;
        self.ai_source = None;
        self.remote_seen.clear();
    }
    fn export_design(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Overlay design", &["scoredesign"])
            .set_file_name(format!(
                "{}.scoredesign",
                self.project.designs[self.design].name
            ))
            .save_file()
        {
            let design = self.project.designs[self.design].clone();
            let assets = self
                .project
                .assets
                .iter()
                .filter(|a| design.elements.iter().any(|e| e.asset == Some(a.id)))
                .cloned()
                .collect();
            let motions = self
                .project
                .motions
                .iter()
                .filter(|motion| design.elements.iter().any(|e| e.motion == Some(motion.id)))
                .cloned()
                .collect();
            match atomic_json(
                &path,
                &DesignFile {
                    version: VERSION,
                    design,
                    assets,
                    motions,
                },
            ) {
                Ok(()) => self.message = "Reusable design exported.".into(),
                Err(e) => self.message = e,
            }
        }
    }
    fn import_design(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Overlay design", &["scoredesign"])
            .pick_file()
        {
            let result = (|| -> Result<(), String> {
                let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
                if bytes.len() > 512 * 1024 * 1024 {
                    return Err("Design too large".into());
                }
                let mut file: DesignFile =
                    serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                if file.version != VERSION {
                    return Err("Unsupported design version".into());
                }
                validate_design(&file.design)?;
                let mut candidate = self.project.clone();
                for a in &mut file.assets {
                    let old = a.id;
                    a.id = candidate.next_id();
                    for e in &mut file.design.elements {
                        if e.asset == Some(old) {
                            e.asset = Some(a.id);
                        }
                    }
                    candidate.assets.push(a.clone());
                }
                for motion in &mut file.motions {
                    let old = motion.id;
                    motion.id = candidate.next_id();
                    for element in &mut file.design.elements {
                        if element.motion == Some(old) {
                            element.motion = Some(motion.id);
                        }
                    }
                    candidate.motions.push(motion.clone());
                }
                let start = candidate.next_id();
                let groups: std::collections::BTreeSet<_> = file
                    .design
                    .elements
                    .iter()
                    .filter_map(|e| e.group)
                    .collect();
                let groups: std::collections::HashMap<_, _> = groups
                    .into_iter()
                    .enumerate()
                    .map(|(i, g)| (g, start + file.design.elements.len() as u64 + i as u64))
                    .collect();
                for (i, e) in file.design.elements.iter_mut().enumerate() {
                    e.id = start + i as u64;
                    e.group = e.group.and_then(|g| groups.get(&g).copied());
                }
                for e in &file.design.elements {
                    if !e.binding.is_empty() {
                        candidate
                            .values
                            .entry(e.binding.clone())
                            .or_insert("0".into());
                    }
                }
                candidate.designs.push(file.design);
                candidate.validate()?;
                self.project = candidate;
                self.design = self.project.designs.len() - 1;
                self.selected.clear();
                Ok(())
            })();
            self.message = match result {
                Ok(()) => "Design imported.".into(),
                Err(e) => e,
            };
        }
    }
    fn import_image(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Image", &["png", "jpg", "jpeg"])
            .pick_file()
        {
            let result = (|| -> Result<(), String> {
                let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
                if bytes.len() > 32 * 1024 * 1024 {
                    return Err("Image must be under 32 MB".into());
                }
                let image = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
                if image.width() > 8192 || image.height() > 8192 {
                    return Err("Image must be under 8192px per side".into());
                }
                let id = self.project.next_id();
                self.project.assets.push(Asset {
                    id,
                    name: path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into(),
                    bytes: bytes.into(),
                });
                self.add(Kind::Image, None);
                let e = self.project.designs[self.design]
                    .elements
                    .last_mut()
                    .unwrap();
                e.asset = Some(id);
                e.w = 320.;
                e.h = 320. * image.height() as f32 / image.width() as f32;
                Ok(())
            })();
            if let Err(e) = result {
                self.message = e;
            }
        }
    }
    fn import_video(&mut self) {
        if self.video_import.is_some() {
            self.message = "A video import is already running.".into();
            return;
        }
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Video", &["mp4", "mov", "m4v", "webm", "mkv"])
            .pick_file()
        {
            self.video_import = Some(VideoImport {
                epoch: self.history_epoch,
                design: self.design,
                worker: crate::media::import(path, egui::Context::default()),
            });
            self.message = "Importing video in the background…".into();
        }
    }
    fn finish_video_import(&mut self, ctx: &egui::Context) {
        let result = self
            .video_import
            .as_ref()
            .and_then(|job| job.worker.try_recv().ok());
        if let Some(result) = result {
            let job = self.video_import.take().unwrap();
            match result {
                Ok(mut prepared)
                    if job.epoch == self.history_epoch
                        && job.design < self.project.designs.len() =>
                {
                    let before = self.project.clone();
                    prepared.asset.id = self.project.next_id();
                    let mut element = Element::new(prepared.asset.id + 1, Kind::Video);
                    element.name = format!("Video · {}", prepared.asset.name);
                    element.motion = Some(prepared.asset.id);
                    element.w = 640.;
                    let dimensions = prepared.frames[0].size;
                    element.h = 640. * dimensions[1] as f32 / dimensions[0] as f32;
                    self.resources
                        .install_motion(ctx, &prepared.asset, prepared.frames);
                    self.project.motions.push(prepared.asset);
                    self.project.designs[job.design].elements.push(element);
                    self.remember(before);
                    self.preview = Some(Instant::now());
                    self.message =
                        "Video imported. Place it on the canvas or assign its design to a cue."
                            .into();
                }
                Ok(_) => {
                    self.message = "Video import discarded because the project changed.".into()
                }
                Err(error) => self.message = error,
            }
        }
        if self.video_import.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
    fn duplicate(&mut self) {
        self.duplicate_with_offset(24.);
    }
    fn duplicate_with_offset(&mut self, offset: f32) {
        let mut next = self.project.next_id();
        let groups: std::collections::HashMap<_, _> = self.project.designs[self.design]
            .elements
            .iter()
            .filter(|e| self.selected.contains(&e.id) && !e.locked)
            .filter_map(|e| e.group)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .map(|group| {
                let id = next;
                next += 1;
                (group, id)
            })
            .collect();
        let mut copies = vec![];
        for e in &self.project.designs[self.design].elements {
            if self.selected.contains(&e.id) && !e.locked {
                let mut e = e.clone();
                e.id = next;
                next += 1;
                e.x += offset;
                e.y += offset;
                e.group = e.group.and_then(|group| groups.get(&group).copied());
                for frame in &mut e.keyframes {
                    frame.x += offset;
                    frame.y += offset;
                }
                copies.push(e);
            }
        }
        self.selected = copies.iter().map(|e| e.id).collect();
        self.project.designs[self.design].elements.extend(copies);
    }
    fn create_team_board(&mut self) {
        match crate::application::design_templates::team_board(&self.project) {
            Ok(design) => {
                self.project.designs.push(design);
                self.design = self.project.designs.len() - 1;
                self.selected.clear();
                self.message = format!(
                    "Created an adaptive layout for {} teams.",
                    self.project.teams.len()
                );
            }
            Err(error) => self.message = error,
        }
    }
    fn smart_arrange_selection(&mut self) {
        let ids: Vec<_> = self.project.designs[self.design]
            .elements
            .iter()
            .filter(|element| self.selected.contains(&element.id) && !element.locked)
            .map(|element| element.id)
            .collect();
        if ids.len() < 2 {
            self.message = "Select two or more unlocked layers to arrange them.".into();
            return;
        }
        let columns = (ids.len() as f32).sqrt().ceil() as usize;
        let rows = ids.len().div_ceil(columns);
        let design = &mut self.project.designs[self.design];
        let margin = 80.;
        let gap = 24.;
        let cell_w =
            (design.width as f32 - margin * 2. - gap * (columns - 1) as f32) / columns as f32;
        let cell_h = (design.height as f32 - margin * 2. - gap * (rows - 1) as f32) / rows as f32;
        for (index, id) in ids.iter().enumerate() {
            if let Some(element) = design.elements.iter_mut().find(|element| element.id == *id) {
                let column = index % columns;
                let row = index / columns;
                let left = margin + column as f32 * (cell_w + gap);
                let top = margin + row as f32 * (cell_h + gap);
                element.w = element.w.min(cell_w * 0.9);
                element.h = element.h.min(cell_h * 0.9);
                element.x = left + (cell_w - element.w) / 2.;
                element.y = top + (cell_h - element.h) / 2.;
            }
        }
        self.message = format!(
            "Smart-arranged {} selected layers on a balanced grid.",
            ids.len()
        );
    }
    fn take(&mut self, route: usize, design: usize) -> Result<(), String> {
        let d = self
            .project
            .designs
            .get(design)
            .ok_or("Design does not exist")?
            .clone();
        let out = self
            .outputs
            .get_mut(route)
            .and_then(Option::as_mut)
            .ok_or("Start the output first")?;
        if (out.program.width, out.program.height) != (d.width, d.height) {
            return Err("Stop and restart the output to change canvas dimensions".into());
        }
        out.take_design(d, &self.project.assets, &self.project.motions)?;
        out.route.design = design;
        self.project.routes[route].design = design;
        Ok(())
    }
    fn action(&mut self, action: Action) -> Result<(), String> {
        let outcome = crate::application::game_commands::dispatch(
            &mut self.project,
            &mut self.clocks,
            action,
        )
        .map_err(|error| error.to_string())?;
        self.dirty |= outcome.document_changed;
        if outcome.preview_requested {
            self.preview = Some(Instant::now());
        }
        for request in outcome.outputs {
            self.apply_output_request(request)?;
        }
        for event in outcome.events {
            self.fire_event(event);
        }
        Ok(())
    }
    fn apply_output_request(
        &mut self,
        request: crate::application::game_commands::OutputRequest,
    ) -> Result<(), String> {
        use crate::application::game_commands::OutputRequest;
        match request {
            OutputRequest::TakeDesign { route, design } => self.take(route, design),
            OutputRequest::ToggleVisibility { route } => {
                let output = self
                    .outputs
                    .get_mut(route)
                    .and_then(Option::as_mut)
                    .ok_or("Start the output first")?;
                output.visible = !output.visible;
                output.trigger();
                Ok(())
            }
            OutputRequest::ReplayAnimation { route } => {
                let output = self
                    .outputs
                    .get_mut(route)
                    .and_then(Option::as_mut)
                    .ok_or("Start the output first")?;
                output.trigger();
                Ok(())
            }
        }
    }
    fn fire_event(&mut self, event: CueEvent) {
        let targets: Vec<_> = self
            .project
            .cue_rules
            .iter()
            .filter(|rule| rule.enabled && rule.event == event)
            .map(|rule| rule.target.clone())
            .collect();
        for target in targets {
            let result = self.run_cue_target(target);
            if let Err(error) = result {
                self.message = error;
            }
        }
    }
    fn run_cue_target(&mut self, target: CueTarget) -> Result<(), String> {
        let mut outcome = crate::application::game_commands::CommandOutcome::default();
        crate::application::game_commands::apply_cue_target(
            &mut self.project,
            target,
            &mut outcome,
        )
        .map_err(|error| error.to_string())?;
        self.dirty |= outcome.document_changed;
        for request in outcome.outputs {
            self.apply_output_request(request)?;
        }
        Ok(())
    }
    fn handle_remote(&mut self) {
        let commands: Vec<_> = self
            .remote
            .as_ref()
            .map(|r| r.commands.try_iter().collect())
            .unwrap_or_default();
        for command in commands {
            if self.remote_seen.contains(&command.request_id) {
                let _ = command.reply.send(Ok(()));
                continue;
            }
            let before = self.project.clone();
            let action = self
                .project
                .controller
                .controls
                .iter()
                .find(|c| c.id == command.button)
                .map(|c| c.action.clone());
            let result = action
                .ok_or("Button no longer exists".into())
                .and_then(|a| self.action(a));
            if result.is_ok() {
                self.remote_seen.push_back(command.request_id);
                if self.remote_seen.len() > 256 {
                    self.remote_seen.pop_front();
                }
                if self.project != before {
                    self.remember(before);
                }
            }
            let _ = command.reply.send(result);
        }
    }
    fn top(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(24, 30, 41))
                    .inner_margin(16),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Overlay Studio").size(23.).strong());
                    ui.separator();
                    ui.text_edit_singleline(&mut self.project.name);
                    ui.label(if self.dirty { "● Unsaved" } else { "Saved" });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Save project").clicked() {
                            self.save(false);
                        }
                        if ui.button("Open").clicked() {
                            self.load();
                        }
                        ui.menu_button("File", |ui| {
                            if ui.button("Save as…").clicked() {
                                self.save(true);
                                ui.close();
                            }
                            if ui.button("Export design…").clicked() {
                                self.export_design();
                                ui.close();
                            }
                            if ui.button("Import design…").clicked() {
                                self.import_design();
                                ui.close();
                            }
                        });
                    });
                });
                ui.add_space(10.);
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.page, Page::Design, "Canvas Studio");
                    ui.selectable_value(&mut self.page, Page::Live, "Live Desk");
                    ui.selectable_value(&mut self.page, Page::Cues, "Animations & cues");
                    ui.selectable_value(&mut self.page, Page::Remote, "Remote layout");
                    ui.separator();
                    if ui
                        .add_enabled(!self.undo.is_empty(), egui::Button::new("Undo"))
                        .clicked()
                    {
                        self.undo();
                    }
                    if ui
                        .add_enabled(!self.redo.is_empty(), egui::Button::new("Redo"))
                        .clicked()
                    {
                        self.redo();
                    }
                });
            });
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.message);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label("Rust / GPU • Transparent alpha");
                });
            });
        });
    }
    fn left(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("layers")
            .default_size(230.)
            .show(ui, |ui| {
                ui.add_space(10.);
                ui.heading("Designs");
                let mut selected = self.design;
                for (i, d) in self.project.designs.iter().enumerate() {
                    if ui.selectable_label(i == self.design, &d.name).clicked() {
                        selected = i;
                    }
                }
                if selected != self.design {
                    self.design = selected;
                    self.selected.clear();
                    self.pan = Vec2::ZERO;
                }
                ui.horizontal(|ui| {
                    if ui.button("Blank").clicked() {
                        self.project.designs.push(Design {
                            id: 0,
                            name: "Untitled design".into(),
                            width: 1920,
                            height: 1080,
                            elements: vec![],
                        });
                        self.design = self.project.designs.len() - 1;
                        self.selected.clear();
                    }
                    if ui.button("Duplicate").clicked() {
                        let mut d = self.project.designs[self.design].clone();
                        d.name += " copy";
                        self.project.designs.push(d);
                        self.design = self.project.designs.len() - 1;
                        self.selected.clear();
                    }
                });
                ui.separator();
                ui.heading("Create");
                ui.small("Choose a tool, then click or draw on canvas");
                if ui
                    .selectable_label(self.tool.is_none(), "Select / move (V)")
                    .clicked()
                {
                    self.tool = None;
                }
                ui.horizontal_wrapped(|ui| {
                    for (kind, label) in [
                        (Kind::Rectangle, "▭  Rectangle"),
                        (Kind::Ellipse, "○  Ellipse"),
                        (Kind::Triangle, "△  Triangle"),
                        (Kind::Diamond, "◇  Diamond"),
                        (Kind::Line, "╱  Line"),
                        (Kind::Text, "T  Text"),
                    ] {
                        let r = ui.add(egui::Button::new(label).sense(Sense::click_and_drag()));
                        r.dnd_set_drag_payload(kind);
                        if r.clicked() {
                            self.tool = Some(kind);
                            self.message = format!(
                                "{kind:?} tool: click to place, or drag to draw. Escape cancels."
                            );
                        }
                    }
                    if ui.button("Image / logo…").clicked() {
                        self.import_image();
                    }
                    if ui.button("Video clip…").clicked() {
                        self.import_video();
                    }
                });
                ui.separator();
                ui.heading("Layers");
                ui.small("Shift-click selects more · Alt-drag copies");
                let mut clicked = None;
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for e in self.project.designs[self.design].elements.iter_mut().rev() {
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut e.hidden, "").on_hover_text("Hide layer");
                            if ui
                                .selectable_label(
                                    self.selected.contains(&e.id),
                                    format!("{}{}", if e.locked { "🔒 " } else { "" }, e.name),
                                )
                                .clicked()
                            {
                                clicked = Some(e.id);
                            }
                        });
                    }
                });
                if let Some(id) = clicked {
                    if !ui.input(|i| i.modifiers.shift) {
                        self.selected.clear();
                    }
                    if !self.selected.insert(id) {
                        self.selected.remove(&id);
                    }
                }
            });
    }
    fn inspector(&mut self, ui: &mut egui::Ui) {
        egui::Panel::right("properties")
            .default_size(290.)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add_space(10.);
                    ui.heading("Canvas");
                    let d = &mut self.project.designs[self.design];
                    ui.text_edit_singleline(&mut d.name);
                    ui.horizontal(|ui| {
                        ui.label("Width");
                        ui.add(egui::DragValue::new(&mut d.width).range(64..=4096));
                        ui.label("Height");
                        ui.add(egui::DragValue::new(&mut d.height).range(64..=4096));
                    });
                    ui.checkbox(&mut self.snap, "Snap to 10px grid");
                    ui.small("Checkerboard is preview only.");
                    ui.separator();
                    let idx = self.project.designs[self.design]
                        .elements
                        .iter()
                        .position(|e| self.selected.contains(&e.id));
                    let Some(idx) = idx else {
                        ui.label("Select a layer on the canvas or in the layer list.");
                        return;
                    };
                    ui.heading(if self.selected.len() > 1 {
                        "Selection"
                    } else {
                        "Properties"
                    });
                    let mut requested_font = None;
                    {
                        let e = &mut self.project.designs[self.design].elements[idx];
                        ui.text_edit_singleline(&mut e.name);
                        egui::Grid::new("geometry").num_columns(4).show(ui, |ui| {
                            ui.label("X");
                            ui.add(egui::DragValue::new(&mut e.x));
                            ui.label("Y");
                            ui.add(egui::DragValue::new(&mut e.y));
                            ui.end_row();
                            ui.label("W");
                            ui.add(egui::DragValue::new(&mut e.w).range(1. ..=16384.));
                            ui.label("H");
                            ui.add(egui::DragValue::new(&mut e.h).range(1. ..=16384.));
                            ui.end_row();
                        });
                        ui.horizontal(|ui| {
                            ui.label("Rotation");
                            ui.add(egui::DragValue::new(&mut e.rotation).suffix("°"));
                        });
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut e.locked, "Locked");
                            ui.checkbox(&mut e.hidden, "Hidden");
                        });
                        ui.add(egui::Slider::new(&mut e.opacity, 0. ..=1.).text("Opacity"));
                        ui.horizontal(|ui| {
                            ui.label("Fill / text");
                            ui.color_edit_button_srgba_unmultiplied(&mut e.fill);
                        });
                        ui.horizontal(|ui| {
                            ui.label("Stroke");
                            ui.color_edit_button_srgba_unmultiplied(&mut e.stroke);
                            ui.add(egui::DragValue::new(&mut e.stroke_width).range(0. ..=64.));
                        });
                        if e.kind == Kind::Rectangle {
                            ui.add(egui::Slider::new(&mut e.radius, 0. ..=120.).text("Corners"));
                        }
                        if e.kind == Kind::Text {
                            ui.separator();
                            ui.label("Content");
                            egui::ComboBox::from_id_salt("binding")
                                .selected_text(if e.binding.is_empty() {
                                    "Custom text"
                                } else {
                                    &e.binding
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut e.binding,
                                        String::new(),
                                        "Custom text",
                                    );
                                    for key in self.project.values.keys() {
                                        ui.selectable_value(&mut e.binding, key.clone(), key);
                                    }
                                });
                            if e.binding.is_empty() {
                                ui.text_edit_multiline(&mut e.text);
                            } else {
                                ui.small(format!(
                                    "Live preview: {}",
                                    e.content(&self.project.values)
                                ));
                            }
                            ui.add(
                                egui::DragValue::new(&mut e.font_size)
                                    .range(1. ..=2048.)
                                    .prefix("Size "),
                            );
                            ui.label("Font");
                            egui::ComboBox::from_id_salt("font")
                                .selected_text(&e.font)
                                .width(250.)
                                .show_ui(ui, |ui| {
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.font_search)
                                            .hint_text("Search installed fonts"),
                                    );
                                    ui.separator();
                                    if ui
                                        .selectable_label(e.font == "Default", "Default")
                                        .clicked()
                                    {
                                        requested_font = Some("Default".to_string());
                                    }
                                    for name in self.fonts.faces.keys().filter(|name| {
                                        name.to_lowercase()
                                            .contains(&self.font_search.to_lowercase())
                                    }) {
                                        if ui.selectable_label(e.font == *name, name).clicked() {
                                            requested_font = Some(name.clone());
                                        }
                                    }
                                });
                            ui.small("Text automatically fits its box.");
                        }
                        ui.separator();
                        ui.label("Entrance animation");
                        egui::ComboBox::from_id_salt("animation")
                            .selected_text(format!("{:?}", e.animation))
                            .show_ui(ui, |ui| {
                                for a in [
                                    Animation::None,
                                    Animation::Fade,
                                    Animation::Slide,
                                    Animation::Pop,
                                ] {
                                    ui.selectable_value(&mut e.animation, a, format!("{a:?}"));
                                }
                            });
                        ui.add(egui::Slider::new(&mut e.duration, 0.05..=3.).text("Duration (s)"));
                    }
                    if let Some(font) = requested_font {
                        match self.fonts.ensure(&font) {
                            Ok(changed) => {
                                self.project.designs[self.design].elements[idx].font = font;
                                if changed {
                                    ui.ctx().set_fonts(self.fonts.definitions.clone());
                                }
                            }
                            Err(e) => self.message = e,
                        }
                    }
                    ui.separator();
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Duplicate").clicked() {
                            self.duplicate();
                        }
                        if ui.button("Delete").clicked() {
                            self.project.designs[self.design]
                                .elements
                                .retain(|e| !self.selected.contains(&e.id) || e.locked);
                            self.selected.clear();
                        }
                        if ui.button("Group").clicked() {
                            let g = self.project.next_id();
                            for e in &mut self.project.designs[self.design].elements {
                                if self.selected.contains(&e.id) {
                                    e.group = Some(g);
                                }
                            }
                        }
                        if ui.button("Ungroup").clicked() {
                            for e in &mut self.project.designs[self.design].elements {
                                if self.selected.contains(&e.id) {
                                    e.group = None;
                                }
                            }
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Bring forward").clicked() {
                            let es = &mut self.project.designs[self.design].elements;
                            for i in (0..es.len().saturating_sub(1)).rev() {
                                if self.selected.contains(&es[i].id)
                                    && !self.selected.contains(&es[i + 1].id)
                                {
                                    es.swap(i, i + 1);
                                }
                            }
                        }
                        if ui.button("Send backward").clicked() {
                            let es = &mut self.project.designs[self.design].elements;
                            for i in 1..es.len() {
                                if self.selected.contains(&es[i].id)
                                    && !self.selected.contains(&es[i - 1].id)
                                {
                                    es.swap(i, i - 1);
                                }
                            }
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        for (label, axis) in [("Align left", 0), ("Align top", 1), ("Center X", 2)]
                        {
                            if ui.button(label).clicked() {
                                let es = &mut self.project.designs[self.design].elements;
                                let selected: Vec<_> = es
                                    .iter()
                                    .filter(|e| self.selected.contains(&e.id))
                                    .collect();
                                let x = selected.iter().map(|e| e.x).fold(f32::INFINITY, f32::min);
                                let y = selected.iter().map(|e| e.y).fold(f32::INFINITY, f32::min);
                                let center = selected.iter().map(|e| e.x + e.w / 2.).sum::<f32>()
                                    / selected.len().max(1) as f32;
                                for e in es {
                                    if self.selected.contains(&e.id) && !e.locked {
                                        match axis {
                                            0 => e.x = x,
                                            1 => e.y = y,
                                            _ => e.x = center - e.w / 2.,
                                        }
                                    }
                                }
                            }
                        }
                    });
                });
            });
    }
    fn canvas(&mut self, ui: &mut egui::Ui, editable: bool) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(&self.project.designs[self.design].name).strong());
            ui.label(format!(
                "{} × {} · Alpha",
                self.project.designs[self.design].width, self.project.designs[self.design].height
            ));
            if ui
                .button(if self.preview.is_some() {
                    "Stop animation"
                } else {
                    "Preview animation"
                })
                .clicked()
            {
                self.preview = if self.preview.is_some() {
                    None
                } else {
                    Some(Instant::now())
                };
                self.preview_time = None;
            }
            if editable {
                ui.add(egui::Slider::new(&mut self.zoom, 0.25..=2.).text("Zoom"));
                if ui.button("Fit").clicked() {
                    self.zoom = 1.;
                    self.pan = Vec2::ZERO;
                }
                if ui
                    .add_enabled(self.selected.len() > 1, egui::Button::new("Smart arrange"))
                    .on_hover_text("Local layout assistant: balance the selected layers")
                    .clicked()
                {
                    self.smart_arrange_selection();
                }
                if ui.button("AI layout…").clicked() {
                    self.ai_open = true;
                }
            }
        });
        let avail = ui.available_size();
        let (response_rect, response) = ui.allocate_exact_size(avail, Sense::click_and_drag());
        let elapsed = self
            .preview
            .map(|t| t.elapsed().as_secs_f32())
            .or(self.preview_time);
        self.resources.advance_motion(elapsed);
        let d = &self.project.designs[self.design];
        let scale = ((avail.x - 40.) / d.width as f32)
            .min((avail.y - 40.) / d.height as f32)
            .max(0.01)
            * self.zoom;
        let size = Vec2::new(d.width as f32, d.height as f32) * scale;
        let origin = response_rect.center() - size / 2. + self.pan;
        let rect = Rect::from_min_size(origin, size);
        let painter = ui.painter_at(response_rect);
        let cp = painter.with_clip_rect(rect.intersect(response_rect));
        let cell = 18.;
        let cols = (rect.width() / cell).ceil() as usize;
        let rows = (rect.height() / cell).ceil() as usize;
        for y in 0..rows {
            for x in 0..cols {
                cp.rect_filled(
                    Rect::from_min_size(
                        origin + Vec2::new(x as f32 * cell, y as f32 * cell),
                        Vec2::splat(cell),
                    ),
                    0,
                    if (x + y) % 2 == 0 {
                        Color32::from_rgb(43, 49, 60)
                    } else {
                        Color32::from_rgb(54, 61, 72)
                    },
                );
            }
        }
        scene::paint_scene(
            &cp,
            d,
            &self.project.values,
            &self.resources,
            origin,
            scale,
            elapsed,
        );
        painter.rect_stroke(
            rect,
            0,
            Stroke::new(1., Color32::from_gray(100)),
            egui::StrokeKind::Outside,
        );
        if !editable {
            return;
        }
        if let Some(kind) = response.dnd_release_payload::<Kind>() {
            let p = ui
                .input(|i| i.pointer.latest_pos())
                .unwrap_or(rect.center());
            self.add(
                *kind,
                Some(Pos2::new(
                    (p.x - origin.x) / scale,
                    (p.y - origin.y) / scale,
                )),
            );
        }
        for e in &self.project.designs[self.design].elements {
            if self.selected.contains(&e.id) && !e.hidden {
                let r = scene::element_rect(e, origin, scale);
                painter.rect_stroke(
                    r,
                    0,
                    Stroke::new(1.5, Color32::from_rgb(95, 186, 255)),
                    egui::StrokeKind::Outside,
                );
                for p in [
                    r.left_top(),
                    r.right_top(),
                    r.left_bottom(),
                    r.right_bottom(),
                ] {
                    painter.rect_filled(
                        Rect::from_center_size(p, Vec2::splat(8.)),
                        1,
                        Color32::from_rgb(95, 186, 255),
                    );
                }
                painter.line_segment(
                    [r.center_top(), r.center_top() - Vec2::new(0., 20.)],
                    Stroke::new(1., Color32::LIGHT_BLUE),
                );
                painter.circle_filled(r.center_top() - Vec2::new(0., 20.), 5., Color32::LIGHT_BLUE);
            }
        }
        if response.dragged_by(egui::PointerButton::Middle) {
            self.pan += ui.input(|i| i.pointer.delta());
            return;
        }
        if let Some(pos) = response.interact_pointer_pos() {
            let local = Pos2::new((pos.x - origin.x) / scale, (pos.y - origin.y) / scale);
            if response.drag_started() {
                let press = ui.input(|i| i.pointer.press_origin()).unwrap_or(pos);
                let press_local =
                    Pos2::new((press.x - origin.x) / scale, (press.y - origin.y) / scale);
                let mut mode = DragKind::Move;
                let mut handle = None;
                if let Some(kind) = self.tool.take() {
                    self.add(kind, Some(press_local));
                    mode = DragKind::Create;
                    handle = self.selected.iter().next().copied();
                }
                for e in &self.project.designs[self.design].elements {
                    if matches!(mode, DragKind::Create) {
                        break;
                    }
                    if self.selected.contains(&e.id) && !e.locked {
                        let r = scene::element_rect(e, origin, scale);
                        for (corner, left, top) in [
                            (r.left_top(), true, true),
                            (r.right_top(), false, true),
                            (r.left_bottom(), true, false),
                            (r.right_bottom(), false, false),
                        ] {
                            if press.distance(corner) < 13. {
                                handle = Some(e.id);
                                mode = DragKind::Resize { left, top };
                            }
                        }
                        if press.distance(r.center_top() - Vec2::new(0., 20.)) < 13. {
                            handle = Some(e.id);
                            mode = DragKind::Rotate;
                        }
                    }
                }
                if let Some(id) = handle {
                    self.selected = HashSet::from([id]);
                } else {
                    self.select_at(press_local, ui.input(|i| i.modifiers.shift));
                    if ui.input(|i| i.modifiers.alt) && !self.selected.is_empty() {
                        self.duplicate_with_offset(0.);
                        self.message =
                            "Duplicated selection. Keep dragging to place the copy.".into();
                    }
                }
                let elements = self.project.designs[self.design]
                    .elements
                    .iter()
                    .filter(|e| self.selected.contains(&e.id) && !e.locked)
                    .cloned()
                    .collect();
                self.drag = Some(Drag {
                    start: press_local,
                    elements,
                    kind: mode,
                });
            } else if response.clicked() {
                if let Some(kind) = self.tool.take() {
                    self.add(kind, Some(local));
                } else {
                    self.select_at(local, ui.input(|i| i.modifiers.shift));
                }
            }
            if let Some(drag) = &self.drag {
                let delta = local - drag.start;
                for original in &drag.elements {
                    if let Some(e) = self.project.designs[self.design]
                        .elements
                        .iter_mut()
                        .find(|e| e.id == original.id)
                    {
                        match drag.kind {
                            DragKind::Create => {
                                let mut size =
                                    Vec2::new(delta.x.abs().max(10.), delta.y.abs().max(10.));
                                if ui.input(|i| i.modifiers.shift) {
                                    size = Vec2::splat(size.x.max(size.y));
                                }
                                e.w = size.x;
                                e.h = size.y;
                                e.x = if delta.x < 0. {
                                    drag.start.x - size.x
                                } else {
                                    drag.start.x
                                };
                                e.y = if delta.y < 0. {
                                    drag.start.y - size.y
                                } else {
                                    drag.start.y
                                };
                            }
                            DragKind::Move => {
                                e.x = original.x + delta.x;
                                e.y = original.y + delta.y;
                                for (frame, source) in
                                    e.keyframes.iter_mut().zip(&original.keyframes)
                                {
                                    frame.x = source.x + delta.x;
                                    frame.y = source.y + delta.y;
                                }
                            }
                            DragKind::Resize { left, top } => {
                                e.w = (original.w + if left { -delta.x } else { delta.x }).max(10.);
                                e.h = (original.h + if top { -delta.y } else { delta.y }).max(10.);
                                if ui.input(|i| i.modifiers.shift) {
                                    e.h = e.w * original.h / original.w;
                                }
                                e.x = if left {
                                    original.x + original.w - e.w
                                } else {
                                    original.x
                                };
                                e.y = if top {
                                    original.y + original.h - e.h
                                } else {
                                    original.y
                                };
                            }
                            DragKind::Rotate => {
                                let c = Pos2::new(e.x + e.w / 2., e.y + e.h / 2.);
                                let v = local - c;
                                e.rotation = v.y.atan2(v.x).to_degrees() + 90.;
                            }
                        }
                    }
                }
            }
        }
        if response.drag_stopped() {
            self.drag = None;
            if self.snap {
                for e in &mut self.project.designs[self.design].elements {
                    if self.selected.contains(&e.id) && !e.locked {
                        e.x = (e.x / 10.).round() * 10.;
                        e.y = (e.y / 10.).round() * 10.;
                    }
                }
            }
        }
        let keyboard_captured = ui.ctx().egui_wants_keyboard_input();
        ui.input(|i| {
            if !keyboard_captured {
                if i.key_pressed(egui::Key::Escape) || i.key_pressed(egui::Key::V) {
                    self.tool = None;
                }
                if !i.modifiers.command && !i.modifiers.alt {
                    for (key, kind) in [
                        (egui::Key::R, Kind::Rectangle),
                        (egui::Key::O, Kind::Ellipse),
                        (egui::Key::T, Kind::Text),
                        (egui::Key::L, Kind::Line),
                    ] {
                        if i.key_pressed(key) {
                            self.tool = Some(kind);
                        }
                    }
                }
                if i.modifiers.command && i.key_pressed(egui::Key::D) {
                    self.duplicate();
                }
                let delta = if i.key_pressed(egui::Key::ArrowLeft) {
                    Vec2::new(-1., 0.)
                } else if i.key_pressed(egui::Key::ArrowRight) {
                    Vec2::new(1., 0.)
                } else if i.key_pressed(egui::Key::ArrowUp) {
                    Vec2::new(0., -1.)
                } else if i.key_pressed(egui::Key::ArrowDown) {
                    Vec2::new(0., 1.)
                } else {
                    Vec2::ZERO
                } * if i.modifiers.shift { 10. } else { 1. };
                for e in &mut self.project.designs[self.design].elements {
                    if self.selected.contains(&e.id) && !e.locked {
                        e.x += delta.x;
                        e.y += delta.y;
                        for frame in &mut e.keyframes {
                            frame.x += delta.x;
                            frame.y += delta.y;
                        }
                    }
                }
                if i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace) {
                    self.project.designs[self.design]
                        .elements
                        .retain(|e| !self.selected.contains(&e.id) || e.locked);
                }
            }
        });
    }
    fn select_at(&mut self, p: Pos2, shift: bool) {
        let hit = self.project.designs[self.design]
            .elements
            .iter()
            .rev()
            .find(|e| !e.hidden && !e.locked && scene::hit(e, p))
            .map(|e| (e.id, e.group));
        if let Some((id, group)) = hit {
            if self.selected.contains(&id) && !shift {
                return;
            }
            if !shift {
                self.selected.clear();
            }
            if shift && self.selected.contains(&id) {
                self.selected.remove(&id);
            } else {
                self.selected.insert(id);
                if let Some(g) = group {
                    for e in &self.project.designs[self.design].elements {
                        if e.group == Some(g) && !e.locked {
                            self.selected.insert(e.id);
                        }
                    }
                }
            }
        } else if !shift {
            self.selected.clear();
        }
    }
    fn live(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("game controls")
            .default_size(370.)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.heading("Teams & scores");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Create layout").clicked() {
                                self.create_team_board();
                            }
                            if ui.button("+ Team").clicked() && self.project.teams.len() < 32 {
                                self.project.add_team();
                            }
                        });
                    });
                    let mut remove_team = None;
                    for index in 0..self.project.teams.len() {
                        let team = self.project.teams[index].clone();
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.color_edit_button_srgb(&mut self.project.teams[index].color);
                                ui.text_edit_singleline(&mut self.project.teams[index].label);
                                if self.project.teams.len() > 1
                                    && ui.small_button("Remove").clicked()
                                {
                                    remove_team = Some(index);
                                }
                            });
                            if let Some(name) = self.project.values.get_mut(&team.name_key) {
                                ui.add(egui::TextEdit::singleline(name).hint_text("Display name"));
                            }
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new(
                                        self.project
                                            .values
                                            .get(&team.score_key)
                                            .cloned()
                                            .unwrap_or_default(),
                                    )
                                    .size(38.)
                                    .strong(),
                                );
                                for delta in team.steps {
                                    let label = if delta > 0 {
                                        format!("+{delta}")
                                    } else {
                                        delta.to_string()
                                    };
                                    if ui.button(label).clicked() {
                                        let _ = self.action(Action::Adjust {
                                            field: team.score_key.clone(),
                                            step: delta,
                                        });
                                    }
                                }
                            });
                            egui::CollapsingHeader::new("Scoring buttons")
                                .id_salt(("steps", team.id))
                                .show(ui, |ui| {
                                    ui.horizontal_wrapped(|ui| {
                                        let steps = &mut self.project.teams[index].steps;
                                        let mut remove_step = None;
                                        for (step_index, step) in steps.iter_mut().enumerate() {
                                            let previous = *step;
                                            ui.add(egui::DragValue::new(step).speed(1));
                                            if *step == 0 {
                                                *step = if previous < 0 { 1 } else { -1 };
                                            }
                                            if ui.small_button("×").clicked() && step_index > 0 {
                                                remove_step = Some(step_index);
                                            }
                                        }
                                        if let Some(step_index) = remove_step {
                                            steps.remove(step_index);
                                        }
                                        if ui.small_button("+ Step").clicked() && steps.len() < 12 {
                                            steps.push(1);
                                        }
                                    });
                                });
                        });
                        ui.add_space(6.);
                    }
                    if let Some(index) = remove_team {
                        self.project.remove_team(index);
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.heading("Clocks");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("+ Clock").clicked() && self.project.clocks.len() < 16 {
                                self.project.add_clock();
                                self.clocks.push(GameClock::new());
                            }
                        });
                    });
                    let mut remove_clock = None;
                    for index in 0..self.project.clocks.len() {
                        let running = self.clocks.get(index).is_some_and(|c| c.running);
                        let value = clock_text(self.project.clocks[index].seconds);
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.text_edit_singleline(&mut self.project.clocks[index].name);
                                ui.label(RichText::new(value).size(28.).strong());
                            });
                            ui.horizontal_wrapped(|ui| {
                                if ui.button(if running { "Pause" } else { "Start" }).clicked() {
                                    let _ = self.action(Action::ClockToggleAt { clock: index });
                                }
                                if ui.button("Reset…").clicked() {
                                    self.confirm_reset = Some(index);
                                }
                                for seconds in [-10, 10, 60] {
                                    let label = if seconds > 0 {
                                        format!("+{seconds}s")
                                    } else {
                                        format!("{seconds}s")
                                    };
                                    if ui.button(label).clicked() {
                                        let _ = self.action(Action::ClockAdjust {
                                            clock: index,
                                            seconds,
                                        });
                                    }
                                }
                                if self.project.clocks.len() > 1
                                    && ui.small_button("Remove").clicked()
                                {
                                    remove_clock = Some(index);
                                }
                            });
                            ui.checkbox(&mut self.project.clocks[index].countdown, "Count down");
                            ui.add_enabled_ui(!running, |ui| {
                                ui.label("Current time");
                                minutes_seconds(ui, &mut self.project.clocks[index].seconds);
                                ui.label("Reset time");
                                minutes_seconds(ui, &mut self.project.clocks[index].reset_seconds);
                            });
                        });
                        ui.add_space(6.);
                    }
                    if let Some(index) = remove_clock {
                        self.project.remove_clock(index);
                        self.clocks.remove(index);
                        self.confirm_reset = None;
                    }
                    ui.separator();
                    ui.label("Live values");
                    for (key, value) in &mut self.project.values {
                        if !self
                            .project
                            .teams
                            .iter()
                            .any(|t| key == &t.name_key || key == &t.score_key)
                            && !self.project.clocks.iter().any(|c| key == &c.value_key)
                        {
                            ui.horizontal(|ui| {
                                ui.label(key);
                                ui.text_edit_singleline(value);
                            });
                        }
                    }
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut self.new_field);
                        if ui.button("Add field").clicked() && !self.new_field.trim().is_empty() {
                            self.project
                                .values
                                .entry(self.new_field.trim().into())
                                .or_insert("0".into());
                            self.new_field.clear();
                        }
                    });
                });
            });
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Outputs");
            self.route_controls(ui);
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Design preview");
                egui::ComboBox::from_id_salt("live-design")
                    .selected_text(&self.project.designs[self.design].name)
                    .show_ui(ui, |ui| {
                        for (i, d) in self.project.designs.iter().enumerate() {
                            ui.selectable_value(&mut self.design, i, &d.name);
                        }
                    });
            });
            self.canvas(ui, false);
        });
    }
    fn route_controls(&mut self, ui: &mut egui::Ui) {
        for i in 0..self.project.routes.len() {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                let running = self.outputs.get(i).is_some_and(Option::is_some);
                ui.horizontal_wrapped(|ui| {
                    ui.add_enabled_ui(!running, |ui| {
                        let r = &mut self.project.routes[i];
                        ui.add(egui::TextEdit::singleline(&mut r.name).desired_width(230.));
                        egui::ComboBox::from_id_salt(("protocol", i))
                            .selected_text(format!("{:?}", r.protocol))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut r.protocol, Protocol::Syphon, "Syphon");
                                ui.selectable_value(&mut r.protocol, Protocol::Ndi, "NDI");
                            });
                        egui::ComboBox::from_id_salt(("fps", i))
                            .selected_text(format!("{} fps", r.fps))
                            .show_ui(ui, |ui| {
                                for fps in [24, 25, 30, 50, 60] {
                                    ui.selectable_value(&mut r.fps, fps, format!("{fps} fps"));
                                }
                            });
                    });
                    let r = &mut self.project.routes[i];
                    egui::ComboBox::from_id_salt(("route design", i))
                        .selected_text(&self.project.designs[r.design].name)
                        .show_ui(ui, |ui| {
                            for (j, d) in self.project.designs.iter().enumerate() {
                                ui.selectable_value(&mut r.design, j, &d.name);
                            }
                        });
                    if ui
                        .button(if running { "Stop" } else { "Start output" })
                        .clicked()
                    {
                        if running {
                            self.outputs[i] = None;
                        } else if let Some(gpu) = &self.gpu {
                            let route = self.project.routes[i].clone();
                            match Output::start(
                                route.clone(),
                                &self.project.designs[route.design],
                                &self.project.assets,
                                &self.project.motions,
                                &gpu.device,
                                &gpu.queue,
                            ) {
                                Ok(output) => self.outputs[i] = Some(output),
                                Err(e) => self.message = e,
                            }
                        }
                    }
                    if running {
                        if ui.button("Take design").clicked()
                            && let Err(e) = self.take(i, self.project.routes[i].design)
                        {
                            self.message = e;
                        }
                        if let Some(o) = &mut self.outputs[i] {
                            if ui.button(if o.visible { "Hide" } else { "Show" }).clicked() {
                                o.visible = !o.visible;
                                o.trigger();
                            }
                            if ui.button("Play cue").clicked() {
                                o.trigger();
                            }
                            ui.label(&o.status);
                            ui.small(format!("{} frames · {}", o.frames, o.program.name));
                        }
                    }
                });
            });
        }
        if self.project.routes.len() < 8 && ui.button("Add output").clicked() {
            self.project.routes.push(Route {
                id: 0,
                name: format!("Overlay Studio • Output {}", self.project.routes.len() + 1),
                design: self.design,
                design_id: 0,
                protocol: Protocol::Syphon,
                fps: 30,
            });
            self.outputs.push(None);
        }
    }
    fn cues_page(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("cue rules")
            .default_size(390.)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Game action events");
                    if ui.button("+ Rule").clicked() && self.project.cue_rules.len() < 128 {
                        let id = self
                            .project
                            .cue_rules
                            .iter()
                            .map(|r| r.id)
                            .max()
                            .unwrap_or(0)
                            + 1;
                        let field = self
                            .project
                            .teams
                            .first()
                            .map(|t| t.score_key.clone())
                            .unwrap_or_else(|| "home.score".into());
                        self.project.cue_rules.push(CueRule {
                            id,
                            name: "Score celebration".into(),
                            enabled: true,
                            event: CueEvent::ScoreAdded { field },
                            target: CueTarget::ReplayOutput { route: 0 },
                        });
                    }
                });
                ui.small("Run a visual cue automatically when a score or clock changes.");
                let mut remove = None;
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for index in 0..self.project.cue_rules.len() {
                        let mut rule = self.project.cue_rules[index].clone();
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut rule.enabled, "");
                                ui.text_edit_singleline(&mut rule.name);
                                if ui.small_button("Remove").clicked() {
                                    remove = Some(index);
                                }
                            });
                            let mut event_type = match rule.event {
                                CueEvent::ScoreAdded { .. } => 0,
                                CueEvent::ScoreRemoved { .. } => 1,
                                CueEvent::ClockStarted { .. } => 2,
                                CueEvent::ClockReset { .. } => 3,
                                CueEvent::ClockFinished { .. } => 4,
                            };
                            egui::ComboBox::from_id_salt(("cue event", rule.id))
                                .selected_text(
                                    [
                                        "Score added",
                                        "Score removed",
                                        "Clock started",
                                        "Clock reset",
                                        "Clock finished",
                                    ][event_type],
                                )
                                .show_ui(ui, |ui| {
                                    for (i, label) in [
                                        "Score added",
                                        "Score removed",
                                        "Clock started",
                                        "Clock reset",
                                        "Clock finished",
                                    ]
                                    .iter()
                                    .enumerate()
                                    {
                                        ui.selectable_value(&mut event_type, i, *label);
                                    }
                                });
                            let default_field = self
                                .project
                                .teams
                                .first()
                                .map(|t| t.score_key.clone())
                                .unwrap_or_else(|| "home.score".into());
                            let old_type = match rule.event {
                                CueEvent::ScoreAdded { .. } => 0,
                                CueEvent::ScoreRemoved { .. } => 1,
                                CueEvent::ClockStarted { .. } => 2,
                                CueEvent::ClockReset { .. } => 3,
                                CueEvent::ClockFinished { .. } => 4,
                            };
                            if event_type != old_type {
                                rule.event = match event_type {
                                    0 => CueEvent::ScoreAdded {
                                        field: default_field.clone(),
                                    },
                                    1 => CueEvent::ScoreRemoved {
                                        field: default_field,
                                    },
                                    2 => CueEvent::ClockStarted { clock: 0 },
                                    3 => CueEvent::ClockReset { clock: 0 },
                                    _ => CueEvent::ClockFinished { clock: 0 },
                                };
                            }
                            match &mut rule.event {
                                CueEvent::ScoreAdded { field }
                                | CueEvent::ScoreRemoved { field } => {
                                    field_picker(ui, field, &self.project.values)
                                }
                                CueEvent::ClockStarted { clock }
                                | CueEvent::ClockReset { clock }
                                | CueEvent::ClockFinished { clock } => {
                                    clock_picker(ui, clock, &self.project.clocks)
                                }
                            }
                            ui.label("Then");
                            let mut target_type = match rule.target {
                                CueTarget::ReplayOutput { .. } => 0,
                                CueTarget::TakeDesign { .. } => 1,
                                CueTarget::SetValue { .. } => 2,
                            };
                            egui::ComboBox::from_id_salt(("cue target", rule.id))
                                .selected_text(
                                    ["Replay output animation", "Take design", "Set live value"]
                                        [target_type],
                                )
                                .show_ui(ui, |ui| {
                                    for (i, label) in
                                        ["Replay output animation", "Take design", "Set live value"]
                                            .iter()
                                            .enumerate()
                                    {
                                        ui.selectable_value(&mut target_type, i, *label);
                                    }
                                });
                            let old_target = match rule.target {
                                CueTarget::ReplayOutput { .. } => 0,
                                CueTarget::TakeDesign { .. } => 1,
                                CueTarget::SetValue { .. } => 2,
                            };
                            if target_type != old_target {
                                rule.target = match target_type {
                                    0 => CueTarget::ReplayOutput { route: 0 },
                                    1 => CueTarget::TakeDesign {
                                        route: 0,
                                        design: self.design,
                                    },
                                    _ => CueTarget::SetValue {
                                        field: "event".into(),
                                        value: "GOAL!".into(),
                                    },
                                };
                            }
                            match &mut rule.target {
                                CueTarget::ReplayOutput { route } => {
                                    route_picker(ui, route, &self.project.routes)
                                }
                                CueTarget::TakeDesign { route, design } => {
                                    route_picker(ui, route, &self.project.routes);
                                    design_picker(
                                        ui,
                                        design,
                                        &self.project.designs,
                                        ("rule design", rule.id),
                                    );
                                }
                                CueTarget::SetValue { field, value } => {
                                    field_picker(ui, field, &self.project.values);
                                    ui.text_edit_singleline(value);
                                }
                            }
                            if ui.button("Run cue now").clicked() && let Err(error) = self.run_cue_target(rule.target.clone()) {
                                self.message = error;
                            }
                        });
                        self.project.cue_rules[index] = rule;
                        ui.add_space(6.);
                    }
                });
                if let Some(index) = remove {
                    let removed = self.project.cue_rules.remove(index);
                    self.project.controller.controls.retain(|control| !matches!(control.action, Action::RunCue { id } if id == removed.id));
                }
            });
        egui::Panel::bottom("animation timeline").default_size(300.).resizable(true).show(ui, |ui| {
            let mut playhead = self.preview.map(|t| t.elapsed().as_secs_f32()).or(self.preview_time).unwrap_or(0.);
            let end = self.project.designs[self.design].elements.iter()
                .flat_map(|e| e.keyframes.iter().map(|f| f.time).chain(std::iter::once(e.duration)))
                .fold(5.0_f32, f32::max);
            ui.horizontal_wrapped(|ui| {
                ui.heading("Animation timeline");
                if ui.button(if self.preview.is_some() { "Pause" } else { "Play" }).clicked() {
                    if self.preview.is_some() { self.preview = None; self.preview_time = Some(playhead); }
                    else { self.preview = Some(Instant::now() - Duration::from_secs_f32(playhead)); }
                }
                if ui.button("Restart").clicked() { self.preview = Some(Instant::now()); self.preview_time = None; }
                if ui.add(egui::Slider::new(&mut playhead, 0. ..=end).text("Time (s)")).changed() {
                    self.preview = None; self.preview_time = Some(playhead);
                }
                if ui.button("Exit preview").clicked() { self.preview = None; self.preview_time = None; }
            });
            egui::ScrollArea::vertical().show(ui, |ui| {
                for element in &mut self.project.designs[self.design].elements {
                    ui.push_id(element.id, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(&element.name);
                            egui::ComboBox::from_id_salt("preset").selected_text(format!("{:?}", element.animation)).show_ui(ui, |ui| {
                                for preset in [Animation::None, Animation::Fade, Animation::Slide, Animation::Pop] {
                                    ui.selectable_value(&mut element.animation, preset, format!("{preset:?}"));
                                }
                            });
                            ui.add(egui::DragValue::new(&mut element.duration).range(0.05..=10.).suffix(" s"));
                            if ui.button("+ Keyframe").on_hover_text("Capture this layer at the playhead, then edit its transform below").clicked() {
                                let sample = scene::sample_keyframes(element, playhead);
                                let frame = Keyframe::capture(&sample, playhead);
                                if let Some(existing) = element.keyframes.iter_mut().find(|f| (f.time - playhead).abs() < 0.001) { *existing = frame; }
                                else if element.keyframes.len() < 256 { element.keyframes.push(frame); }
                            }
                            ui.checkbox(&mut element.loop_animation, "Loop");
                            let (bar, response) = ui.allocate_exact_size(Vec2::new(180., 20.), Sense::click());
                            ui.painter().rect_filled(bar, 3., Color32::from_gray(35));
                            for frame in &element.keyframes {
                                ui.painter().circle_filled(Pos2::new(bar.left() + frame.time / end * bar.width(), bar.center().y), 4., Color32::LIGHT_BLUE);
                            }
                            let x = bar.left() + (playhead / end).clamp(0., 1.) * bar.width();
                            ui.painter().line_segment([Pos2::new(x, bar.top()), Pos2::new(x, bar.bottom())], Stroke::new(1., Color32::WHITE));
                            if response.clicked() && let Some(pos) = response.interact_pointer_pos() {
                                self.preview = None; self.preview_time = Some(((pos.x - bar.left()) / bar.width()).clamp(0., 1.) * end);
                            }
                        });
                        if !element.keyframes.is_empty() {
                            egui::CollapsingHeader::new(format!("{} keyframes · position, size, rotation, opacity", element.keyframes.len())).id_salt("keyframes").show(ui, |ui| {
                                let mut remove = None;
                                for (index, frame) in element.keyframes.iter_mut().enumerate() {
                                    ui.push_id(index, |ui| {
                                        ui.horizontal_wrapped(|ui| {
                                            ui.add(egui::DragValue::new(&mut frame.time).range(0. ..=300.).speed(0.05).prefix("Time ").suffix(" s"));
                                            ui.add(egui::DragValue::new(&mut frame.x).prefix("X "));
                                            ui.add(egui::DragValue::new(&mut frame.y).prefix("Y "));
                                            ui.add(egui::DragValue::new(&mut frame.w).range(1. ..=16384.).prefix("W "));
                                            ui.add(egui::DragValue::new(&mut frame.h).range(1. ..=16384.).prefix("H "));
                                            ui.add(egui::DragValue::new(&mut frame.rotation).suffix("°"));
                                            ui.add(egui::Slider::new(&mut frame.opacity, 0. ..=1.).text("Opacity"));
                                            egui::ComboBox::from_id_salt("easing").selected_text(format!("{:?}", frame.easing)).show_ui(ui, |ui| {
                                                for easing in [Easing::Linear, Easing::EaseOut, Easing::EaseInOut, Easing::Hold] {
                                                    ui.selectable_value(&mut frame.easing, easing, format!("{easing:?}"));
                                                }
                                            });
                                            if ui.small_button("Remove").clicked() { remove = Some(index); }
                                        });
                                    });
                                }
                                if let Some(index) = remove { element.keyframes.remove(index); }
                            });
                        }
                    });
                }
            });
        });
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Animation preview");
                design_picker(
                    ui,
                    &mut self.design,
                    &self.project.designs,
                    "animation design",
                );
            });
            self.canvas(ui, false);
        });
    }
    fn build_team_controls(&mut self) {
        match remote::ControllerLayout::for_game(&self.project) {
            Ok(layout) => {
                self.project.controller = layout;
                self.control = None;
                self.message = format!(
                    "Built controls for {} teams, including every scoring step.",
                    self.project.teams.len()
                );
            }
            Err(error) => self.message = error,
        }
    }
    fn remote_page(&mut self, ui: &mut egui::Ui) {
        egui::Panel::right("remote properties").default_size(310.).show(ui,|ui|{egui::ScrollArea::vertical().show(ui,|ui|{
            ui.heading("Connect a device");ui.small("Computer and phone/iPad must share the same local network.");if self.remote.is_none(){egui::ComboBox::from_id_salt("interface").selected_text(&self.remote_ip).show_ui(ui,|ui|{for ip in remote::addresses(){ui.selectable_value(&mut self.remote_ip,ip.clone(),ip);}});ui.add(egui::DragValue::new(&mut self.remote_port).range(1024..=65535).prefix("Port "));if ui.button("Start remote control").clicked(){let snapshot=self.snapshot();match Remote::start(&self.remote_ip,self.remote_port,snapshot,ui.ctx().clone()){Ok(remote)=>{if let Ok(image)=remote::qr_image(&remote.url){self.qr=Some(ui.ctx().load_texture("Pairing QR",image,egui::TextureOptions::NEAREST));}self.remote=Some(remote);self.message="Remote control started. Scan the QR with your phone or iPad.".into();},Err(e)=>self.message=e}}}else{if let Some(qr)=&self.qr{ui.add(egui::Image::new(qr).fit_to_exact_size(Vec2::splat(240.)));}
if let Some(remote)=&self.remote{ui.label(format!("http://{}/",remote.address));if ui.button("Copy pairing link").clicked(){ui.ctx().copy_text(remote.url.clone());}}
if ui.button("Stop remote / revoke QR").clicked(){self.remote=None;self.qr=None;self.remote_seen.clear();}}
            ui.separator();ui.heading("Button properties");
            if let Some(id)=self.control&& let Some(b)=self.project.controller.controls.iter_mut().find(|b|b.id==id){ui.text_edit_singleline(&mut b.label);ui.horizontal(|ui|{ui.label("Color");ui.color_edit_button_srgb(&mut b.color);});ui.checkbox(&mut b.confirm,"Confirm before running");egui::Grid::new("control position").show(ui,|ui|{ui.label("Column");ui.add(egui::DragValue::new(&mut b.col).range(0..=11));ui.label("Row");ui.add(egui::DragValue::new(&mut b.row).range(0..=47));ui.end_row();ui.label("Width");ui.add(egui::DragValue::new(&mut b.width).range(1..=12));ui.label("Height");ui.add(egui::DragValue::new(&mut b.height).range(1..=8));});b.col=b.col.min(12-b.width);b.row=b.row.min(48-b.height);
                ui.label("Action");
                let mut action_type=match b.action{Action::Adjust{..}=>0,Action::ClockToggle|Action::ClockToggleAt{..}=>1,Action::ClockReset|Action::ClockResetAt{..}=>2,Action::ClockAdjust{..}=>3,Action::ToggleOverlay{..}=>4,Action::Take{..}=>5,Action::Cue{..}=>6,Action::SetValue{..}=>7,Action::RunCue{..}=>8,Action::SpinWheel|Action::RemoveWheelWinner|Action::RestoreWheel|Action::RollDice|Action::NextQuestion|Action::RevealAnswer|Action::NextParticipant|Action::StartQuestionTimer|Action::AwardCurrentTeam{..}=>7};
                let labels = ["Adjust value","Start / pause clock","Reset clock","Add / remove clock time","Show / hide overlay","Take design","Play animation","Set value","Run named cue"];
                let old=action_type;
                egui::ComboBox::from_id_salt("remote action").selected_text(labels[action_type]).show_ui(ui,|ui|{
                    for(i,label)in labels.iter().enumerate(){if i != 8 || !self.project.cue_rules.is_empty() {ui.selectable_value(&mut action_type,i,*label);}}
                });
                if action_type!=old{b.action=match action_type{0=>Action::Adjust{field:self.project.teams.first().map(|t|t.score_key.clone()).unwrap_or_else(||"round".into()),step:1},1=>Action::ClockToggleAt{clock:0},2=>{b.confirm=true;Action::ClockResetAt{clock:0}},3=>Action::ClockAdjust{clock:0,seconds:10},4=>Action::ToggleOverlay{route:0},5=>Action::Take{route:0,design:0},6=>Action::Cue{route:0},8=>Action::RunCue{id:self.project.cue_rules.first().map(|c|c.id).unwrap_or(0)},_=>Action::SetValue{field:"round".into(),value:"1".into()}};}
                if let Action::RunCue { id } = &mut b.action {
                    egui::ComboBox::from_id_salt("named cue").selected_text(self.project.cue_rules.iter().find(|c|c.id == *id).map(|c|c.name.as_str()).unwrap_or("Choose cue")).show_ui(ui,|ui| {
                        for cue in &self.project.cue_rules { ui.selectable_value(id, cue.id, &cue.name); }
                    });
                }
                match &mut b.action{Action::Adjust{field,step}=>{field_picker(ui,field,&self.project.values);ui.add(egui::DragValue::new(step).prefix("Change "));},Action::SetValue{field,value}=>{field_picker(ui,field,&self.project.values);ui.text_edit_singleline(value);},Action::ClockToggleAt{clock}|Action::ClockResetAt{clock}=>clock_picker(ui,clock,&self.project.clocks),Action::ClockAdjust{clock,seconds}=>{clock_picker(ui,clock,&self.project.clocks);ui.add(egui::DragValue::new(seconds).suffix(" seconds"));},Action::ToggleOverlay{route}|Action::Cue{route}=>route_picker(ui,route,&self.project.routes),Action::Take{route,design}=>{route_picker(ui,route,&self.project.routes);design_picker(ui,design,&self.project.designs,"cue design");},_=>{}}
            }
            if ui.button("Delete selected button").clicked(){self.project.controller.controls.retain(|b|Some(b.id)!=self.control);self.control=None;}
        });});
        egui::CentralPanel::default().show(ui,|ui|{ui.heading("Remote controller layout");ui.small("Drag buttons to arrange the phone/iPad view. Select a button to change its action.");ui.horizontal(|ui|{ui.text_edit_singleline(&mut self.project.controller.name);if ui.button("Add button").clicked(){let id=self.project.controller.controls.iter().map(|c|c.id).max().unwrap_or(0)+1;let row=self.project.controller.controls.iter().map(|c|c.row+c.height).max().unwrap_or(0).min(46);self.project.controller.controls.push(Control{id,label:"New button".into(),col:0,row,width:6,height:2,color:[35,105,146],confirm:false,action:Action::Adjust{field:"home.score".into(),step:1}});self.control=Some(id);}
            if ui.button("Build team controls").clicked(){self.build_team_controls();}});
            let width=ui.available_width().min(760.);let cell=width/12.;let row_h=40.;let rows=self.project.controller.controls.iter().map(|b|b.row+b.height).max().unwrap_or(8).max(8) as f32+2.;egui::ScrollArea::vertical().show(ui,|ui|{let (rect,_)=ui.allocate_exact_size(Vec2::new(width,rows*row_h),Sense::hover());let painter=ui.painter_at(rect);for x in 0..=12{painter.line_segment([rect.min+Vec2::new(x as f32*cell,0.),rect.min+Vec2::new(x as f32*cell,rect.height())],Stroke::new(1.,Color32::from_gray(50)));}for y in 0..=rows as usize{painter.line_segment([rect.min+Vec2::new(0.,y as f32*row_h),rect.min+Vec2::new(width,y as f32*row_h)],Stroke::new(1.,Color32::from_gray(50)));}
                for b in &mut self.project.controller.controls{let r=Rect::from_min_size(rect.min+Vec2::new(b.col as f32*cell,b.row as f32*row_h),Vec2::new(b.width as f32*cell-8.,b.height as f32*row_h-8.));let response=ui.interact(r,ui.id().with(b.id),Sense::click_and_drag());response.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,true,&b.label));painter.rect_filled(r,10,Color32::from_rgb(b.color[0],b.color[1],b.color[2]));painter.text(r.center(),egui::Align2::CENTER_CENTER,&b.label,egui::FontId::proportional(17.),Color32::WHITE);if self.control==Some(b.id){painter.rect_stroke(r,10,Stroke::new(2.,Color32::LIGHT_BLUE),egui::StrokeKind::Outside);}
if response.clicked()||response.drag_started(){self.control=Some(b.id);}
if response.dragged()&& let Some(pos)=response.interact_pointer_pos(){b.col=((pos.x-rect.min.x)/cell-b.width as f32/2.).round().clamp(0.,(12-b.width) as f32) as u8;b.row=((pos.y-rect.min.y)/row_h-b.height as f32/2.).round().clamp(0.,(48-b.height) as f32) as u8;}}
            });
        });
    }
    fn snapshot(&self) -> Snapshot {
        Snapshot::from_project(
            &self.project,
            &self
                .clocks
                .iter()
                .map(|clock| clock.running)
                .collect::<Vec<_>>(),
            self.outputs
                .iter()
                .enumerate()
                .map(|(i, o)| match o {
                    Some(o) => format!("{}: {}", o.route.name, o.status),
                    None => format!(
                        "{}: Stopped",
                        self.project
                            .routes
                            .get(i)
                            .map(|r| r.name.as_str())
                            .unwrap_or("Output")
                    ),
                })
                .collect(),
        )
    }
    fn ai_ui(&mut self, ctx: &egui::Context) {
        let events: Vec<_> = self
            .ai_worker
            .as_ref()
            .map(|rx| rx.try_iter().collect())
            .unwrap_or_default();
        for event in events {
            match event {
                crate::layout_ai::Event::Status(status) => self.ai_status = status,
                crate::layout_ai::Event::Ready(engine) => {
                    self.ai_engine = Some(engine);
                    self.ai_worker = None;
                    self.ai_status =
                        "Local model ready. Select layers or use the whole canvas.".into();
                }
                crate::layout_ai::Event::Proposal {
                    design: proposal, ..
                } => {
                    if let Some((index, source)) = self.ai_source.take() {
                        self.ai_proposal = Some((index, source, proposal));
                    }
                    self.ai_worker = None;
                    self.ai_status = "Suggestion ready. Review the preview before applying.".into();
                }
                crate::layout_ai::Event::Failed(error) => {
                    self.ai_worker = None;
                    self.ai_status = error;
                    self.ai_source = None;
                }
            }
        }
        if self.ai_worker.is_some() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        if !self.ai_open {
            return;
        }
        let mut open = self.ai_open;
        egui::Window::new("Local AI layout assistant").open(&mut open).default_width(680.).show(ctx, |ui| {
            ui.label("Describe the layout you want. The local model organizes the selected layers, or the entire canvas if nothing is selected.");
            ui.small("First setup downloads the runtime and a 523 MB model. Layout generation runs locally.");
            ui.label(&self.ai_status);
            let busy = self.ai_worker.is_some();
            if self.ai_engine.is_none() && ui.add_enabled(!busy, egui::Button::new("Set up local AI")).clicked() {
                self.ai_worker = Some(crate::layout_ai::setup(ctx.clone()));
                self.ai_status = "Setting up local AI…".into();
            }
            ui.add(egui::TextEdit::multiline(&mut self.ai_brief).desired_rows(3).desired_width(f32::INFINITY));
            if ui.add_enabled(!busy && self.ai_engine.is_some(), egui::Button::new("Suggest layout")).clicked() {
                let source = self.project.designs[self.design].clone();
                let ids: Vec<_> = source.elements.iter().filter(|e| !e.locked && !e.hidden && (self.selected.is_empty() || self.selected.contains(&e.id))).map(|e| e.id).collect();
                if ids.is_empty() { self.ai_status = "Add or select an unlocked layer first.".into(); }
                else {
                    self.ai_source = Some((self.design, source.clone())); self.ai_proposal = None;
                    self.ai_worker = Some(crate::layout_ai::propose(source, ids, self.ai_brief.clone(), ctx.clone()));
                    self.ai_status = "Organizing the canvas with the local model…".into();
                }
            }
            if busy { ui.spinner(); }
            if let Some((index, source, proposal)) = &self.ai_proposal {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 280.), Sense::hover());
                let scale = (rect.width() / proposal.width as f32).min(rect.height() / proposal.height as f32);
                let origin = rect.center() - Vec2::new(proposal.width as f32, proposal.height as f32) * scale / 2.;
                ui.painter().rect_filled(rect, 0., Color32::from_gray(40));
                scene::paint_scene(&ui.painter_at(rect), proposal, &self.project.values, &self.resources, origin, scale, None);
                let unchanged = self.project.designs.get(*index) == Some(source);
                if !unchanged { ui.label("The source design changed while the suggestion was generated. Generate a new suggestion."); }
                let apply = ui.add_enabled(unchanged, egui::Button::new("Apply suggested layout")).clicked();
                let discard = ui.button("Discard suggestion").clicked();
                if apply {
                    let before = self.project.clone();
                    self.project.designs[*index] = proposal.clone(); self.remember(before);
                    self.ai_proposal = None; self.ai_status = "Layout applied. Undo restores the previous design.".into();
                } else if discard { self.ai_proposal = None; }
            }
        });
        self.ai_open = open;
    }
}
impl eframe::App for Studio {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.finish_video_import(&ctx);
        self.ai_ui(&ctx);
        self.clocks
            .resize_with(self.project.clocks.len(), GameClock::new);
        let mut finished = vec![];
        for (index, (runtime, spec)) in self
            .clocks
            .iter_mut()
            .zip(&mut self.project.clocks)
            .enumerate()
        {
            if runtime.tick_spec(spec) {
                finished.push(index);
            }
            self.project
                .values
                .insert(spec.value_key.clone(), clock_text(spec.seconds));
            if runtime.running {
                self.dirty = true;
            }
        }
        if let Some(primary) = self.project.clocks.first() {
            self.project.clock_seconds = primary.seconds;
            self.project.countdown = primary.countdown;
        }
        for clock in finished {
            self.fire_event(CueEvent::ClockFinished { clock });
        }
        self.handle_remote();
        let before = self.project.clone();
        let history_epoch = self.history_epoch;
        self.resources
            .sync(&ctx, &self.project.assets, &self.project.motions);
        self.top(ui);
        match self.page {
            Page::Design => {
                self.left(ui);
                self.inspector(ui);
                egui::CentralPanel::default().show(ui, |ui| {
                    self.canvas(ui, true);
                });
            }
            Page::Live => self.live(ui),
            Page::Cues => self.cues_page(ui),
            Page::Remote => self.remote_page(ui),
        }
        self.track_ui_edit(before, history_epoch);
        if !ctx.input(|i| i.pointer.any_down())
            && !ctx.egui_wants_keyboard_input()
            && let Some(before) = self.pending_history.take()
        {
            self.remember(before);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S)) {
            self.save(false);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z)) {
            self.undo();
        }
        if ctx.input_mut(|i| {
            i.consume_key(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::Z,
            )
        }) {
            self.redo();
        }
        let needed: Vec<_> = self
            .project
            .designs
            .iter()
            .flat_map(|d| &d.elements)
            .map(|e| e.font.clone())
            .collect();
        let mut font_changed = false;
        for font in needed {
            match self.fonts.ensure(&font) {
                Ok(changed) => font_changed |= changed,
                Err(e) => {
                    self.message = e;
                }
            }
        }
        if font_changed {
            ctx.set_fonts(self.fonts.definitions.clone());
        }
        if let Some(gpu) = &self.gpu {
            for out in self.outputs.iter_mut().flatten() {
                out.tick(&self.project, &self.fonts, &gpu.device, &gpu.queue);
            }
        }
        if let Some(remote) = &self.remote
            && let Ok(mut s) = remote.snapshot.lock()
        {
            *s = self.snapshot();
        }
        if self.dirty && self.last_save.elapsed() > Duration::from_secs(15) {
            let p = autosave_path();
            if let Some(parent) = p.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Err(e) = self.project.save(&p) {
                self.message = format!("Autosave failed: {e}");
            }
            self.last_save = Instant::now();
        }
        if self.recovery.is_some() {
            egui::Window::new("Recover autosaved project?")
                .collapsible(false)
                .resizable(false)
                .show(&ctx, |ui| {
                    ui.label("A previous autosave is available.");
                    ui.horizontal(|ui| {
                        if ui.button("Recover").clicked() {
                            let p = self.recovery.take().unwrap();
                            self.install(p);
                            self.dirty = true;
                        }
                        if ui.button("Dismiss").clicked() {
                            self.recovery = None;
                        }
                    });
                });
        }
        if let Some(clock_index) = self.confirm_reset {
            egui::Window::new("Reset timer?")
                .collapsible(false)
                .resizable(false)
                .show(&ctx, |ui| {
                    let label = self
                        .project
                        .clocks
                        .get(clock_index)
                        .map(|c| {
                            format!(
                                "Pause and reset {} to {}?",
                                c.name,
                                clock_text(c.reset_seconds)
                            )
                        })
                        .unwrap_or_else(|| "Reset this clock?".into());
                    ui.label(label);
                    ui.horizontal(|ui| {
                        if ui.button("Reset").clicked() {
                            let _ = self.action(Action::ClockResetAt { clock: clock_index });
                            self.confirm_reset = None;
                        }
                        if ui.button("Cancel").clicked() {
                            self.confirm_reset = None;
                        }
                    });
                });
        }
        if self.clocks.iter().any(|clock| clock.running)
            || self.outputs.iter().any(Option::is_some)
            || self.remote.is_some()
            || self.preview.is_some()
        {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }
}
fn field_picker(
    ui: &mut egui::Ui,
    field: &mut String,
    values: &std::collections::BTreeMap<String, String>,
) {
    egui::ComboBox::from_id_salt(ui.next_auto_id())
        .selected_text(&*field)
        .show_ui(ui, |ui| {
            for k in values.keys() {
                ui.selectable_value(field, k.clone(), k);
            }
        });
}
fn route_picker(ui: &mut egui::Ui, route: &mut usize, routes: &[Route]) {
    if routes.is_empty() {
        ui.label("Add an output first");
        return;
    }
    *route = (*route).min(routes.len() - 1);
    egui::ComboBox::from_id_salt(ui.next_auto_id())
        .selected_text(&routes[*route].name)
        .show_ui(ui, |ui| {
            for (i, r) in routes.iter().enumerate() {
                ui.selectable_value(route, i, &r.name);
            }
        });
}
fn clock_picker(ui: &mut egui::Ui, clock: &mut usize, clocks: &[ClockSpec]) {
    if clocks.is_empty() {
        ui.label("Add a clock first");
        return;
    }
    *clock = (*clock).min(clocks.len() - 1);
    egui::ComboBox::from_id_salt(ui.next_auto_id())
        .selected_text(&clocks[*clock].name)
        .show_ui(ui, |ui| {
            for (index, item) in clocks.iter().enumerate() {
                ui.selectable_value(clock, index, &item.name);
            }
        });
}
fn design_picker(
    ui: &mut egui::Ui,
    design: &mut usize,
    designs: &[Design],
    id: impl std::hash::Hash + std::fmt::Debug,
) {
    if designs.is_empty() {
        return;
    }
    *design = (*design).min(designs.len() - 1);
    egui::ComboBox::from_id_salt(id)
        .selected_text(&designs[*design].name)
        .show_ui(ui, |ui| {
            for (index, item) in designs.iter().enumerate() {
                ui.selectable_value(design, index, &item.name);
            }
        });
}
fn minutes_seconds(ui: &mut egui::Ui, total: &mut f64) {
    let rounded = total.max(0.).round() as u64;
    let mut minutes = rounded / 60;
    let mut seconds = rounded % 60;
    ui.horizontal(|ui| {
        let changed_minutes = ui
            .add(
                egui::DragValue::new(&mut minutes)
                    .range(0..=10080)
                    .suffix(" min"),
            )
            .changed();
        let changed_seconds = ui
            .add(
                egui::DragValue::new(&mut seconds)
                    .range(0..=59)
                    .suffix(" sec"),
            )
            .changed();
        if changed_minutes || changed_seconds {
            *total = (minutes * 60 + seconds) as f64;
        }
    });
}
fn autosave_path() -> PathBuf {
    let manual_demo = std::env::current_exe().ok().is_some_and(|path| {
        path.components()
            .any(|component| component.as_os_str() == "Overlay Studio Manual Demo.app")
    });
    let preview = std::env::current_exe().ok().is_some_and(|path| {
        path.components()
            .any(|component| component.as_os_str() == "Overlay Studio Preview.app")
    });
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Library/Application Support/Overlay Studio")
        .join(if manual_demo {
            "manual-demo-recovery.autosave.json"
        } else if preview {
            "preview-recovery.autosave.json"
        } else {
            "recovery.autosave.json"
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn draw(
        studio: &mut Studio,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200., 800.))),
                events,
                ..Default::default()
            },
            |ui| studio.canvas(ui, true),
        );
        output.textures_delta.clear();
        output
    }
    #[test]
    fn fast_drag_uses_press_origin() {
        let ctx = egui::Context::default();
        let mut studio = Studio::with_context(&ctx, None, false);
        studio.snap = false;
        let mut e = Element::new(100, Kind::Rectangle);
        e.fill = [247, 24, 117, 255];
        e.animation = Animation::None;
        studio.project.designs[0].elements = vec![e.clone()];
        let output = draw(&mut studio, &ctx, vec![]);
        let center = output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Path(path) if path.fill == scene::rgba(e.fill, 1.) => {
                    Some(s.shape.visual_bounding_rect().center())
                }
                _ => None,
            })
            .expect("Shape must be drawn");
        let press = egui::Event::PointerButton {
            pos: center,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        };
        draw(
            &mut studio,
            &ctx,
            vec![egui::Event::PointerMoved(center), press],
        );
        draw(
            &mut studio,
            &ctx,
            vec![egui::Event::PointerMoved(center + Vec2::new(80., 40.))],
        );
        assert!(studio.project.designs[0].elements[0].x > e.x + 50.);
        assert!(studio.project.designs[0].elements[0].y > e.y + 20.);
    }
    #[test]
    fn drawing_tool_creates_shape_from_pointer_bounds() {
        let ctx = egui::Context::default();
        let mut studio = Studio::with_context(&ctx, None, false);
        studio.project.designs[0].elements.clear();
        studio.snap = false;
        studio.tool = Some(Kind::Ellipse);
        draw(&mut studio, &ctx, vec![]);
        let start = Pos2::new(300., 300.);
        draw(
            &mut studio,
            &ctx,
            vec![
                egui::Event::PointerMoved(start),
                egui::Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
        );
        draw(
            &mut studio,
            &ctx,
            vec![egui::Event::PointerMoved(start + Vec2::new(160., 100.))],
        );
        let element = &studio.project.designs[0].elements[0];
        assert_eq!(element.kind, Kind::Ellipse);
        assert!(element.w > 200. && element.h > 100.);
        assert!(studio.tool.is_none());
    }
    #[test]
    fn configurable_actions_use_shared_state() {
        let ctx = egui::Context::default();
        let mut studio = Studio::with_context(&ctx, None, false);
        studio
            .action(Action::Adjust {
                field: "home.score".into(),
                step: 3,
            })
            .unwrap();
        assert_eq!(studio.project.values["home.score"], "15");
        studio
            .action(Action::Adjust {
                field: "home.score".into(),
                step: -1,
            })
            .unwrap();
        assert_eq!(studio.project.values["home.score"], "14");
        studio
            .action(Action::SetValue {
                field: "event".into(),
                value: "WINNER: EAGLES".into(),
            })
            .unwrap();
        assert_eq!(studio.project.values["event"], "WINNER: EAGLES");
        studio.action(Action::ClockToggle).unwrap();
        assert!(studio.clocks[0].running);
        studio.action(Action::ClockReset).unwrap();
        assert!(!studio.clocks[0].running);
        assert_eq!(studio.project.clock_seconds, 600.);
        assert!(studio.action(Action::ToggleOverlay { route: 0 }).is_err());
    }
    #[test]
    fn five_team_controls_and_adaptive_design_are_generated() {
        let ctx = egui::Context::default();
        let mut studio = Studio::with_context(&ctx, None, false);
        for n in 3..=5 {
            let name_key = format!("team{n}.name");
            let score_key = format!("team{n}.score");
            studio
                .project
                .values
                .insert(name_key.clone(), format!("TEAM {n}"));
            studio.project.values.insert(score_key.clone(), "0".into());
            studio.project.teams.push(Team {
                id: n as u64,
                label: format!("Team {n}"),
                name_key,
                score_key,
                color: [30 * n as u8, 80, 140],
                steps: vec![-1, 1, 5],
            });
        }
        studio.build_team_controls();
        for team in &studio.project.teams {
            assert!(studio.project.controller.controls.iter().any(|control| {
                matches!(&control.action, Action::Adjust { field, step: 1 } if field == &team.score_key)
            }));
        }
        studio.create_team_board();
        let design = &studio.project.designs[studio.design];
        for team in &studio.project.teams {
            assert!(
                design
                    .elements
                    .iter()
                    .any(|element| element.binding == team.score_key)
            );
        }
        studio.project.validate().unwrap();
    }
    #[test]
    fn independent_clock_actions_and_cue_rules_work() {
        let ctx = egui::Context::default();
        let mut studio = Studio::with_context(&ctx, None, false);
        studio
            .project
            .values
            .insert("clock2".into(), "01:00".into());
        studio.project.clocks.push(ClockSpec {
            id: 2,
            name: "Shot clock".into(),
            value_key: "clock2".into(),
            seconds: 60.,
            reset_seconds: 60.,
            countdown: true,
        });
        studio.clocks.push(GameClock::new());
        studio
            .action(Action::ClockAdjust {
                clock: 1,
                seconds: -10,
            })
            .unwrap();
        assert_eq!(studio.project.clocks[1].seconds, 50.);
        studio.action(Action::ClockToggleAt { clock: 1 }).unwrap();
        assert!(studio.clocks[1].running);
        studio.project.cue_rules.push(CueRule {
            id: 1,
            name: "Celebrate".into(),
            enabled: true,
            event: CueEvent::ScoreAdded {
                field: "home.score".into(),
            },
            target: CueTarget::SetValue {
                field: "event".into(),
                value: "GOAL!".into(),
            },
        });
        studio
            .action(Action::Adjust {
                field: "home.score".into(),
                step: 1,
            })
            .unwrap();
        assert_eq!(studio.project.values["event"], "GOAL!");
    }
    #[test]
    fn toolbar_undo_redo_and_open_do_not_become_new_edits() {
        let ctx = egui::Context::default();
        let mut studio = Studio::with_context(&ctx, None, false);
        let original = studio.project.clone();
        studio.project.adjust_score("home.score", 1);
        studio.remember(original);
        let before = studio.project.clone();
        let epoch = studio.history_epoch;
        studio.undo();
        studio.track_ui_edit(before, epoch);
        assert_eq!(studio.project.values["home.score"], "12");
        assert_eq!(studio.redo.len(), 1);
        assert!(studio.pending_history.is_none());
        let before = studio.project.clone();
        let epoch = studio.history_epoch;
        studio.redo();
        studio.track_ui_edit(before, epoch);
        assert_eq!(studio.project.values["home.score"], "13");
        assert!(studio.redo.is_empty());
        assert!(studio.pending_history.is_none());
        let before = studio.project.clone();
        let epoch = studio.history_epoch;
        let mut loaded = Project::fresh();
        loaded.name = "Another event".into();
        studio.install(loaded);
        studio.track_ui_edit(before, epoch);
        assert!(!studio.dirty);
        assert!(studio.undo.is_empty());
        assert!(studio.redo.is_empty());
        assert!(studio.pending_history.is_none());
    }
}
