//! Toolkit-neutral local layout job state and Slint panel bindings.
use crate::{Desktop, StudioWindow, refresh, report};
use overlay_studio::{layout_ai, model::Design};
use slint::ComponentHandle;
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::Receiver,
    },
};

enum Job {
    Setup,
    Suggest,
}

struct Source {
    generation: u64,
    design: usize,
    value: Design,
}

pub struct Session {
    engine: Option<layout_ai::Engine>,
    worker: Option<Receiver<layout_ai::Event>>,
    job: Option<Job>,
    cancel: Option<Arc<AtomicBool>>,
    source: Option<Source>,
    proposal: Option<Design>,
    status: String,
    cancelling: bool,
    preview_revision: u64,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            engine: None,
            worker: None,
            job: None,
            cancel: None,
            source: None,
            proposal: None,
            status: "Set up the private local model, then describe a layout.".into(),
            cancelling: false,
            preview_revision: 1,
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl Session {
    fn start_setup(&mut self) -> Result<(), String> {
        if self.worker.is_some() {
            return Err("A local AI job is already running.".into());
        }
        if self.engine.is_some() {
            self.status = "Local model is ready.".into();
            return Ok(());
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.worker = Some(layout_ai::setup_with_notify(cancel.clone(), || {}));
        self.job = Some(Job::Setup);
        self.cancel = Some(cancel);
        self.cancelling = false;
        self.status = "Preparing the local AI runtime…".into();
        Ok(())
    }

    fn start_suggest(
        &mut self,
        document: &overlay_studio::application::document_session::DocumentSession,
        brief: &str,
    ) -> Result<(), String> {
        if self.worker.is_some() {
            return Err("A local AI job is already running.".into());
        }
        if self.engine.is_none() {
            return Err("Set up local AI first.".into());
        }
        let brief = brief.trim();
        if brief.is_empty() {
            return Err("Describe the layout you want.".into());
        }
        if brief.chars().count() > 1_000 {
            return Err("Keep the layout request under 1,000 characters.".into());
        }
        let source = document.active().clone();
        let ids = source
            .elements
            .iter()
            .filter(|element| {
                !element.locked
                    && !element.hidden
                    && (document.selected.is_empty() || document.selected.contains(&element.id))
            })
            .map(|element| element.id)
            .collect::<Vec<_>>();
        if ids.is_empty() {
            return Err("Add or select an unlocked, visible layer first.".into());
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.worker = Some(layout_ai::propose_with_notify(
            source.clone(),
            ids,
            brief.into(),
            cancel.clone(),
            || {},
        ));
        self.job = Some(Job::Suggest);
        self.cancel = Some(cancel);
        self.source = Some(Source {
            generation: document.generation,
            design: document.design,
            value: source,
        });
        self.proposal = None;
        self.preview_revision = self.preview_revision.wrapping_add(1);
        self.cancelling = false;
        self.status = "Organizing the selected layers on this computer…".into();
        Ok(())
    }

    pub fn cancel(&mut self) {
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::Relaxed);
            self.cancelling = self.worker.is_some();
            if self.cancelling {
                self.status = "Cancelling the local AI job…".into();
            }
        }
    }

    pub fn reset_document(&mut self) {
        self.cancel();
        self.source = None;
        self.proposal = None;
        self.preview_revision = self.preview_revision.wrapping_add(1);
    }

    pub fn preview<'a>(
        &'a self,
        document: &'a overlay_studio::application::document_session::DocumentSession,
    ) -> Option<&'a Design> {
        let source = self.source.as_ref()?;
        let proposal = self.proposal.as_ref()?;
        (source.generation == document.generation
            && source.design == document.design
            && document.project.designs.get(source.design) == Some(&source.value))
        .then_some(proposal)
    }

    pub fn preview_revision(&self) -> u64 {
        self.preview_revision
    }

    fn discard(&mut self) {
        self.proposal = None;
        self.source = None;
        self.preview_revision = self.preview_revision.wrapping_add(1);
        self.status = if self.engine.is_some() {
            "Suggestion discarded. Describe another layout when ready.".into()
        } else {
            "Set up the private local model, then describe a layout.".into()
        };
    }

    fn apply(
        &mut self,
        document: &mut overlay_studio::application::document_session::DocumentSession,
    ) -> Result<(), String> {
        let source = self.source.as_ref().ok_or("No layout suggestion to apply.")?;
        let proposal = self
            .proposal
            .clone()
            .ok_or("No layout suggestion to apply.")?;
        document.apply_layout_proposal(
            source.generation,
            source.design,
            &source.value,
            proposal,
        )?;
        self.proposal = None;
        self.source = None;
        self.preview_revision = self.preview_revision.wrapping_add(1);
        self.status = "Layout applied. Undo restores the previous positions.".into();
        Ok(())
    }

    pub fn poll(&mut self) -> bool {
        let events = self
            .worker
            .as_ref()
            .map(|worker| worker.try_iter().collect::<Vec<_>>())
            .unwrap_or_default();
        if events.is_empty() {
            return false;
        }
        let mut finished = false;
        for event in events {
            match event {
                layout_ai::Event::Status(status) => self.status = status,
                layout_ai::Event::Ready(engine) => {
                    self.engine = Some(engine);
                    self.status =
                        "Local model ready. Select layers or use the whole canvas.".into();
                    finished = true;
                }
                layout_ai::Event::Proposal { design, fallback } => {
                    self.proposal = Some(design);
                    self.preview_revision = self.preview_revision.wrapping_add(1);
                    self.status = if fallback {
                        "The model returned invalid geometry, so a safe balanced layout is ready for review. Try again for another AI result.".into()
                    } else {
                        "Suggestion ready. Review the canvas before applying it.".into()
                    };
                    finished = true;
                }
                layout_ai::Event::Failed(error) => {
                    self.status = if error.contains("cancelled") {
                        "Local AI job cancelled.".into()
                    } else {
                        error
                    };
                    if matches!(self.job, Some(Job::Suggest)) {
                        self.source = None;
                        self.proposal = None;
                        self.preview_revision = self.preview_revision.wrapping_add(1);
                    }
                    finished = true;
                }
            }
        }
        if finished {
            self.worker = None;
            self.job = None;
            self.cancel = None;
            self.cancelling = false;
        }
        true
    }
}

pub fn refresh_panel(ui: &StudioWindow, state: &Desktop) {
    let preview = state.ai.preview(&state.document).is_some();
    let status = if state.ai.proposal.is_some() && !preview {
        "The source design changed. Generate a new suggestion.".into()
    } else {
        state.ai.status.clone()
    };
    ui.set_ai_status(status.into());
    ui.set_ai_ready(state.ai.engine.is_some());
    ui.set_ai_busy(state.ai.worker.is_some());
    ui.set_ai_cancelling(state.ai.cancelling);
    ui.set_ai_has_proposal(preview);
}

pub fn connect(ui: &StudioWindow, state: Rc<RefCell<Desktop>>) {
    let weak = ui.as_weak();
    ui.on_ai_command(move |command| {
        let Some(ui) = weak.upgrade() else {
            return;
        };
        let mut state = state.borrow_mut();
        let result = match command.as_str() {
            "setup" => state.ai.start_setup(),
            "suggest" => {
                let brief = ui.get_ai_brief().to_string();
                let Desktop { document, ai, .. } = &mut *state;
                ai.start_suggest(document, &brief)
            }
            "cancel" => {
                state.ai.cancel();
                Ok(())
            }
            "discard" => {
                state.ai.discard();
                Ok(())
            }
            "apply" => {
                let Desktop { document, ai, .. } = &mut *state;
                ai.apply(document)
            }
            _ => Err("Unknown local AI action.".into()),
        };
        report(&ui, result);
        refresh_panel(&ui, &state);
        refresh(&ui, &state);
    });
}
