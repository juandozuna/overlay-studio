mod ai_panel;
mod animation_panel;
mod audio;
#[cfg(test)]
mod configuration_tests;
mod events_panel;
mod files;
mod fx_panel;
mod live_values;
mod outputs;
mod remote_panel;
mod recovery;
mod renderer;
mod value_labels;
mod view_models;
mod wheel_panel;
use overlay_studio::{
    application::{
        design_templates,
        document_session::{Alignment, DocumentSession, LayerEdit},
        game_session::GameSession,
        game_setup, style_values,
    },
    domain::controls::Action,
    model::{Kind, Protocol, TextAlign, clock_text},
    scene::FontBook,
};
use renderer::CanvasRenderer;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::{cell::RefCell, rc::Rc};
slint::include_modules!();

struct Desktop {
    document: DocumentSession,
    game: GameSession,
    outputs: outputs::OutputManager,
    remote: remote_panel::Session,
    animation: animation_panel::Session,
    events: events_panel::Session,
    wheel: wheel_panel::Session,
    ai: ai_panel::Session,
    recovery: recovery::Manager,
    media_pixels:
        std::collections::BTreeMap<u64, overlay_studio::infrastructure::media_import::Pixels>,
    media_cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    close_after_media: bool,
    gesture_origin: Option<(f32, f32)>,
    handle_origin: Option<(f32, f32, f32, f32, f32)>,
    palette_origin: Option<(f32, f32)>,
    fonts: Vec<String>,
}
impl Desktop {
    fn guard_removed_fields(&self, keys: &[String]) -> Result<(), String> {
        for (index, sender) in self.outputs.senders.iter().enumerate() {
            if let Some(sender) = sender
                && sender
                    .program
                    .elements
                    .iter()
                    .any(|e| keys.contains(&e.binding))
            {
                let name = self
                    .document
                    .project
                    .routes
                    .get(index)
                    .map(|r| r.name.as_str())
                    .unwrap_or("this output");
                return Err(format!(
                    "Stop {name}, or Take a design without this field, before removing it"
                ));
            }
        }
        Ok(())
    }
    fn configure(
        &mut self,
        kind: &str,
        index: usize,
        field: &str,
        value: &str,
    ) -> Result<(), String> {
        if kind == "output" && field != "design" && field != "add" && self.outputs.is_running(index)
        {
            return Err("Stop the output before changing its stream settings".into());
        }
        let keys = if field == "remove" {
            match kind {
                "value" => vec![value.trim().into()],
                "team" => self
                    .document
                    .project
                    .teams
                    .get(index)
                    .map(|t| vec![t.name_key.clone(), t.score_key.clone()])
                    .unwrap_or_default(),
                "clock" => self
                    .document
                    .project
                    .clocks
                    .get(index)
                    .map(|c| vec![c.value_key.clone()])
                    .unwrap_or_default(),
                _ => vec![],
            }
        } else {
            vec![]
        };
        self.guard_removed_fields(&keys)?;
        self.document
            .configure(&mut self.game, kind, index, field, value)
    }
    fn history(&mut self, redo: bool) -> Result<(), String> {
        self.guard_removed_fields(&self.document.history_removed_fields(redo))?;
        if self
            .document
            .history_stream_changes(redo)
            .into_iter()
            .any(|index| self.outputs.is_running(index))
        {
            return Err("Stop the affected output before undoing its stream settings".into());
        }
        let clocks = self.document.project.clocks.clone();
        if redo {
            self.document.redo();
        } else {
            self.document.undo()?;
        }
        game_setup::reconcile_clocks(&mut self.game, &clocks, &self.document.project.clocks);
        self.outputs
            .senders
            .resize_with(self.document.project.routes.len(), || None);
        self.animation.reset();
        Ok(())
    }
    fn new() -> Self {
        Self::from_document(DocumentSession::default())
    }
    #[cfg(test)]
    fn two_team_demo() -> Self {
        Self::from_document(DocumentSession::new(overlay_studio::model::Project::fresh()))
    }
    fn from_document(document: DocumentSession) -> Self {
        let game = GameSession::new(&document.project);
        let outputs = outputs::OutputManager::new(document.project.routes.len());
        Self {
            document,
            game,
            outputs,
            remote: remote_panel::Session::default(),
            animation: animation_panel::Session::default(),
            events: events_panel::Session::default(),
            wheel: wheel_panel::Session::default(),
            ai: ai_panel::Session::default(),
            recovery: recovery::Manager::new(),
            media_pixels: Default::default(),
            media_cancel: None,
            close_after_media: false,
            gesture_origin: None,
            handle_origin: None,
            palette_origin: None,
            fonts: FontBook::new().faces.keys().cloned().collect(),
        }
    }
    fn execute(&mut self, action: Action) -> Result<(), String> {
        let outcome = self
            .game
            .execute(&mut self.document.project, action)
            .map_err(|e| e.to_string())?;
        self.document.dirty |= outcome.document_changed;
        if outcome.document_changed || outcome.preview_requested {
            self.document.revision += 1;
        }
        for request in outcome.outputs {
            self.outputs.apply(&mut self.document.project, request)?;
        }
        Ok(())
    }

    fn install_project(
        &mut self,
        project: overlay_studio::model::Project,
        path: Option<std::path::PathBuf>,
        dirty: bool,
    ) -> Result<(), String> {
        self.document.replace(project, path)?;
        self.document.dirty = dirty;
        self.remote.stop();
        self.remote.selected = None;
        self.animation.reset();
        self.events.selected = None;
        self.ai.reset_document();
        self.media_pixels.clear();
        self.game = GameSession::new(&self.document.project);
        self.outputs.reset(self.document.project.routes.len());
        Ok(())
    }
}
fn strings(values: impl IntoIterator<Item = String>) -> ModelRc<SharedString> {
    Rc::new(VecModel::from(
        values.into_iter().map(Into::into).collect::<Vec<_>>(),
    ))
    .into()
}
fn refresh(ui: &StudioWindow, state: &Desktop) {
    recovery::refresh(ui, &state.recovery);
    ai_panel::refresh_panel(ui, state);
    remote_panel::refresh_panel(ui, state);
    animation_panel::refresh_panel(ui, state);
    events_panel::refresh_panel(ui, state);
    wheel_panel::refresh_panel(ui, state);
    let s = &state.document;
    let d = s.active();
    ui.set_project_name(s.project.name.clone().into());
    ui.set_dirty(s.dirty);
    ui.set_project_is_new(s.path.is_none());
    ui.set_can_undo(s.can_undo());
    ui.set_can_redo(s.can_redo());
    ui.set_active_design(s.design as i32);
    ui.set_designs(view_models::sync(
        ui.get_designs(),
        s.project
            .designs
            .iter()
            .map(|d| d.name.clone().into())
            .collect(),
    ));
    ui.set_design_name(d.name.clone().into());
    ui.set_canvas_width(d.width as f32);
    ui.set_canvas_height(d.height as f32);
    ui.set_canvas_name_edit(d.name.clone().into());
    ui.set_canvas_width_edit(d.width.to_string().into());
    ui.set_canvas_height_edit(d.height.to_string().into());
    ui.set_layers(
        Rc::new(VecModel::from(
            d.elements
                .iter()
                .rev()
                .map(|e| LayerRow {
                    id: e.id.to_string().into(),
                    name: e.name.clone().into(),
                    selected: s.selected.contains(&e.id),
                    hidden: e.hidden,
                    locked: e.locked,
                })
                .collect::<Vec<_>>(),
        ))
        .into(),
    );
    ui.set_fields(view_models::sync(
        ui.get_fields(),
        std::iter::once("Literal text".into())
            .chain(
                s.project
                    .values
                    .keys()
                    .map(|key| format!("Live · {}", value_labels::label(&s.project, key)).into()),
            )
            .collect(),
    ));
    refresh_live(ui, state);
    ui.set_selection_visible(s.selected_layer().is_some());
    if let Some(e) = s.selected_layer() {
        ui.set_layer_name(e.name.clone().into());
        ui.set_text_content(e.text.clone().into());
        ui.set_binding(if e.binding.is_empty() {
            "Custom text".into()
        } else {
            e.binding.clone().into()
        });
        ui.set_binding_index(if e.binding.is_empty() {
            0
        } else {
            s.project
                .values
                .keys()
                .position(|key| key == &e.binding)
                .map(|index| index as i32 + 1)
                .unwrap_or(-1)
        });
        ui.set_font(e.font.clone().into());
        ui.set_font_index(
            ui.get_fonts()
                .iter()
                .position(|name| name.as_str() == e.font)
                .map(|index| index as i32)
                .unwrap_or(-1),
        );
        ui.set_font_size(e.font_size);
        ui.set_layer_x(e.x);
        ui.set_layer_y(e.y);
        ui.set_layer_w(e.w);
        ui.set_layer_h(e.h);
        ui.set_layer_rotation(e.rotation);
        ui.set_layer_opacity(e.opacity);
        ui.set_layer_hidden(e.hidden);
        ui.set_layer_locked(e.locked);
        ui.set_text_layer(e.kind == Kind::Text);
        ui.set_rectangle_layer(e.kind == Kind::Rectangle);
        ui.set_fill_color(slint::Color::from_argb_u8(
            e.fill[3], e.fill[0], e.fill[1], e.fill[2],
        ));
        ui.set_stroke_color(slint::Color::from_argb_u8(
            e.stroke[3],
            e.stroke[0],
            e.stroke[1],
            e.stroke[2],
        ));
        ui.set_stroke_width_edit(e.stroke_width.to_string().into());
        ui.set_corners_edit(e.radius.to_string().into());
        ui.set_text_align_index(match e.text_align {
            TextAlign::Left => 0,
            TextAlign::Center => 1,
            TextAlign::Right => 2,
        });
        if let Some(b) = s.bounds() {
            ui.set_selection_x(b[0]);
            ui.set_selection_y(b[1]);
            ui.set_selection_w(b[2]);
            ui.set_selection_h(b[3]);
            ui.set_selection_rotation(if s.selected.len() == 1 {
                e.rotation
            } else {
                0.
            });
        }
        ui.set_draft_x(format!("{:.2}", e.x).into());
        ui.set_draft_y(format!("{:.2}", e.y).into());
        ui.set_draft_w(format!("{:.2}", e.w).into());
        ui.set_draft_h(format!("{:.2}", e.h).into());
        ui.set_draft_rotation(format!("{:.2}", e.rotation).into());
        ui.set_draft_opacity(format!("{:.2}", e.opacity).into());
        ui.set_draft_font_size(format!("{:.2}", e.font_size).into());
    }
    ui.window().request_redraw();
}
fn refresh_live(ui: &StudioWindow, state: &Desktop) {
    live_values::refresh_panel(ui, state);
    let s = &state.document;
    ui.set_dirty(s.dirty);
    ui.set_outputs_names(view_models::sync(
        ui.get_outputs_names(),
        s.project
            .routes
            .iter()
            .map(|r| r.name.clone().into())
            .collect(),
    ));
    let old_teams = ui.get_teams();
    ui.set_teams(view_models::sync(
        ui.get_teams(),
        s.project
            .teams
            .iter()
            .enumerate()
            .map(|(index, t)| TeamRow {
                index: index as i32,
                label: t.label.clone().into(),
                color_text: format!("#{:02X}{:02X}{:02X}", t.color[0], t.color[1], t.color[2])
                    .into(),
                steps: view_models::sync(
                    old_teams
                        .row_data(index)
                        .map(|t| t.steps)
                        .unwrap_or_default(),
                    t.steps
                        .iter()
                        .map(|step| format!("{step:+}").into())
                        .collect(),
                ),
                steps_text: t
                    .steps
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
                    .into(),
                name: s
                    .project
                    .values
                    .get(&t.name_key)
                    .cloned()
                    .unwrap_or_default()
                    .into(),
                score: s
                    .project
                    .values
                    .get(&t.score_key)
                    .cloned()
                    .unwrap_or_default()
                    .into(),
                field: t.score_key.clone().into(),
                color: slint::Color::from_rgb_u8(t.color[0], t.color[1], t.color[2]),
            })
            .collect::<Vec<_>>(),
    ));
    ui.set_clocks(view_models::sync(
        ui.get_clocks(),
        s.project
            .clocks
            .iter()
            .enumerate()
            .map(|(index, c)| ClockRow {
                index: index as i32,
                name: c.name.clone().into(),
                time: clock_text(c.seconds).into(),
                reset: clock_text(c.reset_seconds).into(),
                running: state.game.clocks.get(index).is_some_and(|c| c.running),
                countdown: c.countdown,
            })
            .collect::<Vec<_>>(),
    ));
    ui.set_outputs(view_models::sync(
        ui.get_outputs(),
        s.project
            .routes
            .iter()
            .enumerate()
            .map(|(index, r)| {
                let sender = state.outputs.senders.get(index).and_then(Option::as_ref);
                OutputRow {
                    index: index as i32,
                    name: r.name.clone().into(),
                    protocol_index: if r.protocol == Protocol::Syphon { 0 } else { 1 },
                    fps_index: [24, 25, 30, 50, 60]
                        .iter()
                        .position(|fps| *fps == r.fps)
                        .unwrap_or(2) as i32,
                    design: r.design as i32,
                    program_name: sender
                        .map(|s| s.program.name.clone())
                        .unwrap_or_default()
                        .into(),
                    running: sender.is_some(),
                    visible: sender.is_some_and(|s| s.visible),
                    status: sender.map(|s| s.status.clone()).unwrap_or_default().into(),
                }
            })
            .collect::<Vec<_>>(),
    ));
}
fn report(ui: &StudioWindow, result: Result<(), String>) {
    match result {
        Ok(()) => ui.set_status("Ready · Slint migration in progress".into()),
        Err(error) => ui.set_status(error.into()),
    }
}
fn refresh_runtime(ui: &StudioWindow, state: &Desktop) {
    wheel_panel::refresh_panel(ui, state);
    live_values::refresh_panel(ui, state);
    ui.set_dirty(state.document.dirty);
    let p = &state.document.project;
    let teams = ui.get_teams();
    for (index, team) in p.teams.iter().enumerate() {
        if let Some(row) = teams.row_data(index) {
            let row = view_models::team_runtime(
                row,
                p.values
                    .get(&team.name_key)
                    .map(String::as_str)
                    .unwrap_or(""),
                p.values
                    .get(&team.score_key)
                    .map(String::as_str)
                    .unwrap_or(""),
                ui.get_setup(),
            );
            if teams.row_data(index) != Some(row.clone()) {
                teams.set_row_data(index, row);
            }
        }
    }
    ui.set_fields(view_models::sync(
        ui.get_fields(),
        std::iter::once("Literal text".into())
            .chain(
                p.values
                    .keys()
                    .map(|key| format!("Live · {}", value_labels::label(p, key)).into()),
            )
            .collect(),
    ));
    let labels = p
        .values
        .keys()
        .map(|key| value_labels::label(p, key).into())
        .collect::<Vec<_>>();
    ui.set_remote_fields(view_models::sync(ui.get_remote_fields(), labels.clone()));
    ui.set_event_fields(view_models::sync(ui.get_event_fields(), labels));
    let clocks = ui.get_clocks();
    for (index, spec) in state.document.project.clocks.iter().enumerate() {
        if let Some(row) = clocks.row_data(index)
            && let Some(runtime) = state.game.clocks.get(index)
        {
            let row = view_models::clock_runtime(row, spec, runtime);
            if clocks.row_data(index) != Some(row.clone()) {
                clocks.set_row_data(index, row);
            }
        }
    }
    let outputs = ui.get_outputs();
    for index in 0..outputs.row_count() {
        if let Some(mut row) = outputs.row_data(index) {
            let sender = state.outputs.senders.get(index).and_then(Option::as_ref);
            row.running = sender.is_some();
            row.visible = sender.is_some_and(|s| s.visible);
            row.program_name = sender
                .map(|s| s.program.name.clone())
                .unwrap_or_default()
                .into();
            row.status = sender.map(|s| s.status.clone()).unwrap_or_default().into();
            if outputs.row_data(index) != Some(row.clone()) {
                outputs.set_row_data(index, row);
            }
        }
    }
}
fn parse_launch_args() -> (Option<std::path::PathBuf>, i32, bool, bool, bool) {
    let mut open = None;
    let mut workspace = 0;
    let mut events = false;
    let mut setup = false;
    let mut skip_recovery = false;
    let mut args = std::env::args().skip(1).peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--open" => open = args.next().map(std::path::PathBuf::from),
            "--workspace" => {
                workspace = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0)
                    .clamp(0, 3);
            }
            "--events" => events = true,
            "--setup" => setup = true,
            "--skip-recovery" => skip_recovery = true,
            path if path.ends_with(".overlay") => open = Some(std::path::PathBuf::from(path)),
            _ => {}
        }
    }
    (open, workspace, events, setup, skip_recovery)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (open_path, workspace, events, setup, skip_recovery) = parse_launch_args();
    slint::BackendSelector::new()
        .require_wgpu_30(slint::wgpu_30::WGPUConfiguration::default())
        .select()?;
    let ui = StudioWindow::new()?;
    ui.on_color_to_text(|c, alpha| {
        if alpha {
            style_values::color_text([c.red(), c.green(), c.blue(), c.alpha()]).into()
        } else {
            format!("#{:02X}{:02X}{:02X}", c.red(), c.green(), c.blue()).into()
        }
    });
    let state = Rc::new(RefCell::new(Desktop::new()));
    if skip_recovery {
        let recovery = &mut state.borrow_mut().recovery;
        recovery.clear();
        recovery.later();
    }
    if let Some(path) = open_path {
        match overlay_studio::model::Project::load(&path) {
            Ok(project) => {
                let result = state.borrow_mut().install_project(project, Some(path), false);
                report(&ui, result);
            }
            Err(error) => report(&ui, Err(error)),
        }
    }
    ui.set_workspace(workspace);
    ui.set_event_editor(events);
    ui.set_setup(setup);
    let recovery_state = state.clone();
    let recovery_window = ui.as_weak();
    ui.on_recovery_command(move |command| {
        let Some(ui) = recovery_window.upgrade() else {
            return;
        };
        let mut state = recovery_state.borrow_mut();
        let result = match command.as_str() {
            "recover" => recovery::recover(&mut state),
            "discard" => {
                state.recovery.clear();
                Ok(())
            }
            "later" => {
                state.recovery.later();
                Ok(())
            }
            _ => Err("Unknown recovery action".into()),
        };
        report(&ui, result);
        refresh(&ui, &state);
    });
    let close_state = state.clone();
    let close_window = ui.as_weak();
    ui.window().on_close_requested(move || {
        close_window
            .upgrade()
            .map(|ui| files::request_close(&ui, &close_state))
            .unwrap_or(slint::CloseRequestResponse::HideWindow)
    });
    ui.set_fonts(strings(
        std::iter::once("Default".into()).chain(state.borrow().fonts.clone()),
    ));
    refresh(&ui, &state.borrow());
    let renderer = Rc::new(RefCell::new(None::<CanvasRenderer>));
    let program_renderer = Rc::new(RefCell::new(None::<CanvasRenderer>));
    let weak = ui.as_weak();
    let render_state = state.clone();
    ui.window().set_rendering_notifier(move |phase, api| {
        let (Some(ui), slint::GraphicsAPI::WGPU30 { device, queue, .. }) = (weak.upgrade(), api)
        else {
            return;
        };
        if matches!(phase, slint::RenderingState::RenderingSetup) {
            render_state.borrow_mut().outputs.install_gpu(device, queue);
            let native = CanvasRenderer::new(device);
            match native.image() {
                Ok(image) => ui.set_canvas_texture(image),
                Err(e) => ui.set_status(e.into()),
            };
            *renderer.borrow_mut() = Some(native);
            let native = CanvasRenderer::new(device);
            match native.image() {
                Ok(image) => ui.set_program_texture(image),
                Err(error) => ui.set_status(error.into()),
            };
            *program_renderer.borrow_mut() = Some(native);
        }
        if matches!(phase, slint::RenderingState::RenderingTeardown) {
            render_state.borrow_mut().outputs.stop_all();
        }
        if matches!(phase, slint::RenderingState::BeforeRendering)
            && let Some(native) = renderer.borrow_mut().as_mut()
        {
            let state = render_state.borrow();
            let result = if ui.get_workspace() == 1 {
                let index = state
                    .document
                    .project
                    .routes
                    .get(ui.get_monitor_output() as usize)
                    .map(|r| r.design)
                    .unwrap_or(state.document.design);
                let design = state
                    .document
                    .project
                    .designs
                    .get(index)
                    .unwrap_or(state.document.active());
                ui.set_design_name(design.name.clone().into());
                ui.set_canvas_width(design.width as f32);
                ui.set_canvas_height(design.height as f32);
                ui.set_active_design(index as i32);
                native.render_scene(
                    device,
                    queue,
                    renderer::SceneFrame {
                        design,
                        values: &state.document.project.values,
                        assets: &state.document.project.assets,
                        motions: &state.document.project.motions,
                        revision: state.document.revision,
                        generation: state.document.generation,
                        view_key: index as u64,
                        elapsed: None,
                        visible: true,
                    },
                )
            } else {
                let design = state.document.active();
                ui.set_design_name(design.name.clone().into());
                ui.set_canvas_width(design.width as f32);
                ui.set_canvas_height(design.height as f32);
                ui.set_active_design(state.document.design as i32);
                if ui.get_workspace() == 0
                    && let Some(proposal) = state.ai.preview(&state.document)
                {
                    native.render_scene(
                        device,
                        queue,
                        renderer::SceneFrame {
                            design: proposal,
                            values: &state.document.project.values,
                            assets: &state.document.project.assets,
                            motions: &state.document.project.motions,
                            revision: state.document.revision
                                ^ state.ai.preview_revision().rotate_left(32),
                            generation: state.document.generation,
                            view_key: state.document.design as u64 ^ (1_u64 << 63),
                            elapsed: None,
                            visible: true,
                        },
                    )
                } else if ui.get_workspace() == 3 && state.animation.preview {
                    native.render_scene(
                        device,
                        queue,
                        renderer::SceneFrame {
                            design,
                            values: &state.document.project.values,
                            assets: &state.document.project.assets,
                            motions: &state.document.project.motions,
                            revision: state.document.revision,
                            generation: state.document.generation,
                            view_key: state.document.design as u64,
                            elapsed: Some(state.animation.time),
                            visible: true,
                        },
                    )
                } else {
                    native.render(device, queue, &state.document)
                }
            };
            match result {
                Ok(resized) => {
                    if resized {
                        match native.image() {
                            Ok(image) => ui.set_canvas_texture(image),
                            Err(e) => ui.set_status(e.into()),
                        };
                    }
                }
                Err(error) => ui.set_status(error.into()),
            }
            if let Some(warning) = &native.warning {
                ui.set_status(warning.clone().into());
            }
            if ui.get_workspace() == 1 {
                let state = render_state.borrow();
                let sender = state
                    .outputs
                    .senders
                    .get(ui.get_monitor_output() as usize)
                    .and_then(Option::as_ref);
                ui.set_program_available(sender.is_some());
                if let Some(sender) = sender {
                    ui.set_program_label(
                        format!(
                            "{} · {}",
                            sender.program.name,
                            if sender.visible { "Live" } else { "Hidden" }
                        )
                        .into(),
                    );
                    if let Some(program) = program_renderer.borrow_mut().as_mut() {
                        match program.render_scene(
                            device,
                            queue,
                            renderer::SceneFrame {
                                design: &sender.program,
                                values: &state.document.project.values,
                                assets: sender.program_assets(),
                                motions: sender.program_motions(),
                                revision: 0,
                                generation: state.document.generation,
                                view_key: ui.get_monitor_output() as u64,
                                elapsed: Some(sender.elapsed_seconds()),
                                visible: sender.visible,
                            },
                        ) {
                            Ok(resized) => {
                                if resized {
                                    match program.image() {
                                        Ok(image) => ui.set_program_texture(image),
                                        Err(error) => ui.set_status(error.into()),
                                    };
                                }
                            }
                            Err(error) => ui.set_status(error.into()),
                        }
                    }
                } else {
                    ui.set_program_label("Stopped".into());
                }
            }
        }
    })?;
    let weak = ui.as_weak();
    let shared = state.clone();
    ui.on_choose_design(move |index| {
        if let Some(ui) = weak.upgrade() {
            let mut state = shared.borrow_mut();
            if ui.get_workspace() == 1 {
                let route = ui.get_monitor_output() as usize;
                if state
                    .document
                    .project
                    .routes
                    .get(route)
                    .is_some_and(|r| r.design != index as usize)
                {
                    let Desktop { document, game, .. } = &mut *state;
                    let result =
                        document.configure(game, "output", route, "design", &index.to_string());
                    report(&ui, result);
                }
            }
            if state.document.design != index as usize {
                state.animation.reset();
            }
            report(&ui, state.document.select_design(index as usize));
            refresh(&ui, &state);
        }
    });
    let weak = ui.as_weak();
    let shared = state.clone();
    ui.on_choose_monitor(move |index| {
        if let Some(ui) = weak.upgrade()
            && shared
                .borrow()
                .document
                .project
                .routes
                .get(index as usize)
                .is_some()
        {
            ui.set_monitor_output(index);
            refresh(&ui, &shared.borrow());
        }
    });
    let weak = ui.as_weak();
    let shared = state.clone();
    ui.on_choose_layer(move |id| {
        if let Some(ui) = weak.upgrade() {
            let mut state = shared.borrow_mut();
            state
                .document
                .select(id.parse::<u64>().ok(), ui.get_extend_selection());
            refresh(&ui, &state);
        }
    });
    let weak = ui.as_weak();
    let shared = state.clone();
    ui.on_filter_fonts(move |query| {
        if let Some(ui) = weak.upgrade() {
            let query = query.to_lowercase();
            ui.set_fonts(strings(
                std::iter::once("Default".into()).chain(
                    shared
                        .borrow()
                        .fonts
                        .iter()
                        .filter(|name| name.to_lowercase().contains(&query))
                        .cloned(),
                ),
            ));
            ui.set_font_index(
                ui.get_fonts()
                    .iter()
                    .position(|name| name == ui.get_font())
                    .map(|index| index as i32)
                    .unwrap_or(-1),
            );
        }
    });
    let weak = ui.as_weak();
    let shared = state.clone();
    ui.on_edit_property(move |property, value| {
        let Some(ui) = weak.upgrade() else {
            return;
        };
        let mut state = shared.borrow_mut();
        let Some(e) = state.document.selected_layer().cloned() else {
            return;
        };
        let result = (|| {
            let number = || {
                value
                    .parse::<f32>()
                    .map_err(|_| "Enter a valid number.".to_string())
            };
            let edit = match property.as_str() {
                "name" => LayerEdit::Name(value.to_string()),
                "text" => LayerEdit::Text(value.to_string()),
                "binding" => LayerEdit::Binding(value.to_string()),
                "binding-index" => {
                    let index = value.parse::<usize>().map_err(|_| "Choose a live field")?;
                    LayerEdit::Binding(if index == 0 {
                        String::new()
                    } else {
                        state
                            .document
                            .project
                            .values
                            .keys()
                            .nth(index - 1)
                            .cloned()
                            .ok_or("Field no longer exists")?
                    })
                }
                "font" => LayerEdit::Font(value.to_string()),
                "font-size" => LayerEdit::FontSize(number()?),
                "hidden" => LayerEdit::Hidden(value == "true"),
                "locked" => LayerEdit::Locked(value == "true"),
                "opacity" => LayerEdit::Opacity(number()?),
                "fill" => LayerEdit::Fill(style_values::parse_color(&value)?),
                "stroke" => LayerEdit::Stroke(style_values::parse_color(&value)?),
                "stroke-width" => {
                    let v = number()?;
                    if !(0. ..=64.).contains(&v) {
                        return Err("Stroke width must be 0–64".into());
                    }
                    LayerEdit::StrokeWidth(v)
                }
                "corners" => {
                    let v = number()?;
                    if !(0. ..=120.).contains(&v) {
                        return Err("Corners must be 0–120".into());
                    }
                    LayerEdit::Corners(v)
                }
                "text-align" => LayerEdit::TextAlign(match value.as_str() {
                    "Left" => TextAlign::Left,
                    "Right" => TextAlign::Right,
                    _ => TextAlign::Center,
                }),
                field => {
                    let v = number()?;
                    LayerEdit::Geometry {
                        x: if field == "x" { v } else { e.x },
                        y: if field == "y" { v } else { e.y },
                        width: if field == "w" { v } else { e.w },
                        height: if field == "h" { v } else { e.h },
                        rotation: if field == "rotation" { v } else { e.rotation },
                    }
                }
            };
            state.document.apply(e.id, edit)
        })();
        report(&ui, result);
        refresh(&ui, &state);
    });
    let weak = ui.as_weak();
    let shared = state.clone();
    ui.on_score(move |field, delta| {
        let Some(ui) = weak.upgrade() else {
            return;
        };
        let mut state = shared.borrow_mut();
        report(
            &ui,
            state.execute(Action::Adjust {
                field: field.to_string(),
                step: delta as i64,
            }),
        );
        refresh(&ui, &state);
    });
    let weak = ui.as_weak();
    let shared = state.clone();
    ui.on_score_step(move |field, step| {
        let Some(ui) = weak.upgrade() else {
            return;
        };
        let mut state = shared.borrow_mut();
        let result = step
            .parse::<i64>()
            .map_err(|_| "Invalid scoring step".to_string())
            .and_then(|step| {
                state.execute(Action::Adjust {
                    field: field.to_string(),
                    step,
                })
            });
        report(&ui, result);
        refresh(&ui, &state);
    });
    let weak = ui.as_weak();
    let shared = state.clone();
    ui.on_configure(move |kind, index, field, value| {
        let Some(ui) = weak.upgrade() else {
            return;
        };
        let mut state = shared.borrow_mut();
        let result = state.configure(&kind, index as usize, &field, &value);
        report(&ui, result);
        refresh(&ui, &state);
    });
    let weak = ui.as_weak();
    let shared = state.clone();
    ui.on_clock_action(move |index, command| {
        let Some(ui) = weak.upgrade() else {
            return;
        };
        if command == "reset" {
            let shared = shared.clone();
            let weak = ui.as_weak();
            let label = shared
                .borrow()
                .document
                .project
                .clocks
                .get(index as usize)
                .map(|c| format!("Reset {} to {}?", c.name, clock_text(c.reset_seconds)))
                .unwrap_or_else(|| "Clock no longer exists".into());
            let _ = slint::spawn_local(async move {
                if !matches!(
                    rfd::AsyncMessageDialog::new()
                        .set_title("Reset clock")
                        .set_description(label)
                        .set_buttons(rfd::MessageButtons::YesNo)
                        .show()
                        .await,
                    rfd::MessageDialogResult::Yes
                ) {
                    return;
                }
                if let Some(ui) = weak.upgrade() {
                    let mut state = shared.borrow_mut();
                    report(
                        &ui,
                        state.execute(Action::ClockResetAt {
                            clock: index as usize,
                        }),
                    );
                    refresh(&ui, &state);
                }
            });
            return;
        }
        let mut state = shared.borrow_mut();
        let action = if command == "toggle" {
            Ok(Action::ClockToggleAt {
                clock: index as usize,
            })
        } else {
            command
                .parse::<i64>()
                .map(|seconds| Action::ClockAdjust {
                    clock: index as usize,
                    seconds,
                })
                .map_err(|_| "Invalid clock adjustment".to_string())
        };
        let result = action.and_then(|action| state.execute(action));
        report(&ui, result);
        refresh(&ui, &state);
    });
    let weak = ui.as_weak();
    let shared = state.clone();
    ui.on_output_action(move |index, command| {
        let Some(ui) = weak.upgrade() else {
            return;
        };
        let mut state = shared.borrow_mut();
        let index = index as usize;
        let result = if command == "toggle" {
            let Desktop {
                document, outputs, ..
            } = &mut *state;
            outputs.toggle(&document.project, index)
        } else {
            let action = match command.as_str() {
                "take" => state
                    .document
                    .project
                    .routes
                    .get(index)
                    .map(|r| Action::Take {
                        route: index,
                        design: r.design,
                    }),
                "visibility" => Some(Action::ToggleOverlay { route: index }),
                "cue" => Some(Action::Cue { route: index }),
                _ => None,
            };
            action
                .ok_or_else(|| "Unknown output action".to_string())
                .and_then(|a| state.execute(a))
        };
        report(&ui, result);
        refresh(&ui, &state);
    });
    let weak = ui.as_weak();
    let shared = state.clone();
    ui.on_canvas_pointer(move |phase, x, y, shift, alt| {
        let Some(ui) = weak.upgrade() else {
            return;
        };
        let mut state = shared.borrow_mut();
        let result = (|| {
            if phase == 0 {
                state.gesture_origin = Some((x, y));
                if ui.get_tool() == 0 {
                    let id = state.document.hit_test(x, y);
                    if !id.is_some_and(|id| state.document.selected.contains(&id)) {
                        state
                            .document
                            .select(id, shift || ui.get_extend_selection());
                    }
                    state.document.begin_gesture()?;
                    if alt && id.is_some() {
                        state.document.duplicate_offset(0., 0.)?;
                        state.document.reset_gesture_origin();
                    }
                }
            } else if phase == 1 && ui.get_tool() == 0 {
                if let Some((ox, oy)) = state.gesture_origin
                    && (x - ox).hypot(y - oy) >= 1.
                {
                    state
                        .document
                        .translate_gesture(x - ox, y - oy, ui.get_snap())?;
                }
            } else if phase == 2 {
                if ui.get_tool() == 0 {
                    state.document.end_gesture()?;
                } else if let Some((ox, oy)) = state.gesture_origin {
                    let kind = match ui.get_tool() {
                        1 => Kind::Rectangle,
                        2 => Kind::Ellipse,
                        3 => Kind::Triangle,
                        4 => Kind::Diamond,
                        5 => Kind::Line,
                        _ => Kind::Text,
                    };
                    let (w, h) = if (x - ox).abs() < 4. && (y - oy).abs() < 4. {
                        (320., 140.)
                    } else {
                        ((x - ox).abs().max(1.), (y - oy).abs().max(1.))
                    };
                    state.document.add(kind, x.min(ox), y.min(oy), w, h)?;
                    ui.set_tool(0);
                }
                state.gesture_origin = None;
            }
            Ok(())
        })();
        report(&ui, result);
        refresh(&ui, &state);
    });
    let weak = ui.as_weak();
    let shared = state.clone();
    ui.on_canvas_handle(move |kind, phase, x, y, shift, radius, scale| {
        let Some(ui) = weak.upgrade() else {
            return;
        };
        let mut state = shared.borrow_mut();
        let result = (|| {
            if phase == 0 {
                state.document.begin_gesture()?;
                state.handle_origin = Some((x, y, ui.get_selection_rotation(), radius, scale));
            } else {
                let Some((ox, oy, rotation, radius, scale)) = state.handle_origin else {
                    return Ok(());
                };
                let angle = -rotation.to_radians();
                let (s, c) = angle.sin_cos();
                let px = (x - ox) / scale;
                let py = (y - oy) / scale;
                let dx = px * c - py * s;
                let dy = px * s + py * c;
                if kind == 4 {
                    let mut degrees = (dy - radius).atan2(dx).to_degrees() + 90.;
                    if shift {
                        degrees = (degrees / 15.).round() * 15.;
                    }
                    state.document.rotate_gesture(degrees)?;
                } else {
                    state.document.resize_gesture(kind as u8, dx, dy, shift)?;
                }
                if phase == 2 {
                    state.document.end_gesture()?;
                    state.handle_origin = None;
                }
            }
            Ok(())
        })();
        report(&ui, result);
        refresh(&ui, &state);
    });
    let weak = ui.as_weak();
    let shared = state.clone();
    ui.on_palette_drop(move |tool, phase, x, y| {
        let Some(ui) = weak.upgrade() else {
            return;
        };
        let mut state = shared.borrow_mut();
        if phase == 0 {
            state.palette_origin = Some((x, y));
            return;
        }
        let Some((ox, oy)) = state.palette_origin.take() else {
            return;
        };
        if (x - ox).hypot(y - oy) < 6. {
            return;
        }
        let dx = (x - ui.get_canvas_origin_x()) / ui.get_canvas_scale();
        let dy = (y - ui.get_canvas_origin_y()) / ui.get_canvas_scale();
        if dx < 0. || dy < 0. || dx > ui.get_canvas_width() || dy > ui.get_canvas_height() {
            return;
        }
        let kind = match tool {
            1 => Kind::Rectangle,
            2 => Kind::Ellipse,
            3 => Kind::Triangle,
            4 => Kind::Diamond,
            5 => Kind::Line,
            _ => Kind::Text,
        };
        let result = state
            .document
            .add(kind, (dx - 160.).max(0.), (dy - 70.).max(0.), 320., 140.)
            .map(|_| ());
        ui.set_tool(0);
        report(&ui, result);
        refresh(&ui, &state);
    });
    let weak = ui.as_weak();
    ui.on_shortcut(move |key, control, shift| {
        let Some(ui) = weak.upgrade() else {
            return false;
        };
        let text = key.to_ascii_lowercase();
        if control {
            if let Some(workspace) = match text.as_str() {
                "1" => Some(0),
                "2" => Some(1),
                "3" => Some(2),
                "4" => Some(3),
                _ => None,
            } {
                ui.set_workspace(workspace);
                return true;
            }
            let command = match text.as_str() {
                "s" => "save",
                "z" => {
                    if shift {
                        "redo"
                    } else {
                        "undo"
                    }
                }
                "d" => "duplicate",
                _ => return false,
            };
            ui.invoke_command(command.into());
            return true;
        }
        for (keycode, name) in [
            (slint::platform::Key::LeftArrow, "left"),
            (slint::platform::Key::RightArrow, "right"),
            (slint::platform::Key::UpArrow, "up"),
            (slint::platform::Key::DownArrow, "down"),
        ] {
            let encoded: SharedString = keycode.into();
            if key == encoded {
                ui.invoke_command(format!("nudge-{name}{}", if shift { "-10" } else { "" }).into());
                return true;
            }
        }
        let delete: SharedString = slint::platform::Key::Delete.into();
        let backspace: SharedString = slint::platform::Key::Backspace.into();
        if key == delete || key == backspace {
            ui.invoke_command("delete".into());
            return true;
        }
        let escape: SharedString = slint::platform::Key::Escape.into();
        if key == escape {
            ui.set_tool(0);
            return true;
        }
        let tool = match text.as_str() {
            "v" => 0,
            "r" => 1,
            "o" => 2,
            "l" => 5,
            "t" => 6,
            _ => return false,
        };
        ui.set_tool(tool);
        true
    });
    let weak = ui.as_weak();
    let shared = state.clone();
    ui.on_command(move |command| {
        let Some(ui) = weak.upgrade() else {
            return;
        };
        if command == "cancel-media" {
            files::cancel_media(&ui, &shared.borrow());
            return;
        }
        if matches!(
            command.as_str(),
            "save"
                | "save-as"
                | "open"
                | "new-project"
                | "import-design"
                | "export-design"
                | "import-image"
                | "import-video"
        ) {
            files::start(&ui, shared.clone(), command.to_string());
            return;
        }
        let mut state = shared.borrow_mut();
        let result = match command.as_str() {
            "refresh" => Ok(()),
            "rename-project" => state
                .document
                .rename_project(ui.get_project_name().to_string()),
            "undo" => state.history(false),
            "redo" => state.history(true),
            "duplicate" => state.document.duplicate(),
            "delete" => state.document.delete_selected(),
            "blank-design" => {
                ui.set_workspace(0);
                state.document.blank_design()
            }
            "copy-design" => {
                ui.set_workspace(0);
                state.document.duplicate_design()
            }
            "team-board" => {
                ui.set_workspace(0);
                design_templates::team_board(&state.document.project)
                    .and_then(|d| state.document.append_design(d))
            }
            "canvas-apply" => (|| {
                let w = ui
                    .get_canvas_width_edit()
                    .parse::<u32>()
                    .map_err(|_| "Enter a whole pixel width")?;
                let h = ui
                    .get_canvas_height_edit()
                    .parse::<u32>()
                    .map_err(|_| "Enter a whole pixel height")?;
                state
                    .document
                    .canvas_setup(ui.get_canvas_name_edit().to_string(), w, h)
            })(),
            "group" => state.document.group(),
            "ungroup" => state.document.ungroup(),
            "forward" => state.document.reorder(true),
            "backward" => state.document.reorder(false),
            "align-left" => state.document.align(Alignment::Left),
            "align-top" => state.document.align(Alignment::Top),
            "align-center" => state.document.align(Alignment::CenterX),
            key if key.starts_with("nudge-") => {
                let amount = if key.ends_with("-10") { 10. } else { 1. };
                let dx = if key.starts_with("nudge-left") {
                    -amount
                } else if key.starts_with("nudge-right") {
                    amount
                } else {
                    0.
                };
                let dy = if key.starts_with("nudge-up") {
                    -amount
                } else if key.starts_with("nudge-down") {
                    amount
                } else {
                    0.
                };
                state.document.nudge(dx, dy)
            }
            _ => Err("Unknown action.".into()),
        };
        report(&ui, result);
        refresh(&ui, &state);
    });
    ui.set_status(
        "Canvas editing and live score commands connected · Migration in progress".into(),
    );
    let timer = slint::Timer::default();
    ai_panel::connect(&ui, state.clone());
    remote_panel::connect(&ui, state.clone());
    animation_panel::connect(&ui, state.clone());
    events_panel::connect(&ui, state.clone());
    wheel_panel::connect(&ui, state.clone());
    live_values::connect(&ui, state.clone());
    let weak = ui.as_weak();
    let shared = state.clone();
    let mut last_refresh = std::time::Instant::now();
    let mut last_monitor_frame = std::time::Instant::now();
    let mut last_animation_frame = std::time::Instant::now();
    let mut last_wheel_frame = std::time::Instant::now();
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(8),
        move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let Ok(mut state) = shared.try_borrow_mut() else {
                return;
            };
            let Desktop {
                recovery,
                document,
                ..
            } = &mut *state;
            recovery.poll(&ui, document);
            if state.ai.poll() {
                ai_panel::refresh_panel(&ui, &state);
                ui.window().request_redraw();
            }
            if remote_panel::poll(&mut state) {
                refresh_live(&ui, &state);
                ui.window().request_redraw();
            }
            if state.animation.playing
                && last_animation_frame.elapsed() >= std::time::Duration::from_millis(33)
            {
                if animation_panel::tick(&ui, &mut state) {
                    ui.window().request_redraw();
                }
                last_animation_frame = std::time::Instant::now();
            }
            let revealed = {
                let Desktop { document, game, .. } = &mut *state;
                let outcome = game.reveal(&mut document.project);
                document.dirty |= outcome.document_changed;
                if outcome.preview_requested {
                    document.revision += 1;
                }
                outcome.preview_requested
            };
            if revealed {
                wheel_panel::refresh_panel(&ui, &state);
                ui.window().request_redraw();
            }
            if state.game.animation_busy()
                && last_wheel_frame.elapsed() >= std::time::Duration::from_millis(16)
            {
                ui.window().request_redraw();
                last_wheel_frame = std::time::Instant::now();
            }
            let active = state.game.clocks.iter().any(|c| c.running)
                || state.outputs.senders.iter().any(Option::is_some)
                || state.remote.server.is_some();
            if !active {
                return;
            }
            let were_running = state.game.clocks.iter().any(|c| c.running);
            let Desktop {
                document,
                game,
                outputs,
                ..
            } = &mut *state;
            match game.tick(&mut document.project) {
                Ok(outcome) => {
                    document.dirty |= outcome.document_changed;
                    if outcome.preview_requested {
                        document.revision += 1;
                        ui.window().request_redraw();
                    }
                    for request in outcome.outputs {
                        if let Err(error) = outputs.apply(&mut document.project, request) {
                            ui.set_status(error.into());
                        }
                    }
                }
                Err(error) => ui.set_status(error.to_string().into()),
            }
            for error in outputs.tick(&document.project) {
                ui.set_status(error.into());
            }
            if ui.get_workspace() == 1
                && outputs.senders.iter().any(Option::is_some)
                && last_monitor_frame.elapsed() >= std::time::Duration::from_millis(33)
            {
                ui.window().request_redraw();
                last_monitor_frame = std::time::Instant::now();
            }
            if last_refresh.elapsed() >= std::time::Duration::from_millis(150)
                || (were_running && !state.game.clocks.iter().any(|c| c.running))
            {
                refresh_runtime(&ui, &state);
                last_refresh = std::time::Instant::now();
            }
        },
    );
    ui.run()?;
    state.borrow_mut().ai.cancel();
    state.borrow_mut().remote.stop();
    state.borrow_mut().outputs.stop_all();
    Ok(())
}
