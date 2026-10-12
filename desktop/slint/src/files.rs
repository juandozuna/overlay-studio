//! Nonblocking native dialogs and file jobs; live scheduling stays in the event loop.
use crate::{Desktop, StudioWindow, refresh, report};
use overlay_studio::infrastructure::portable_design;
use overlay_studio::model::Project;
use slint::ComponentHandle;
use std::{
    cell::RefCell,
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

pub fn cancel_media(ui: &StudioWindow, state: &Desktop) {
    if let Some(cancel) = &state.media_cancel {
        cancel.store(true, Ordering::Relaxed);
        ui.set_media_import_cancelling(true);
        ui.set_status("Cancelling media import…".into());
    }
}
pub fn request_close(
    ui: &StudioWindow,
    state: &Rc<RefCell<Desktop>>,
) -> slint::CloseRequestResponse {
    if ui.get_io_busy() {
        if ui.get_media_import_busy() {
            cancel_media(ui, &state.borrow());
            state.borrow_mut().close_after_media = true;
        } else {
            ui.set_status("Finish or cancel the file dialog before closing".into());
        }
        return slint::CloseRequestResponse::KeepWindowShown;
    }
    if !state.borrow().document.dirty {
        state.borrow_mut().recovery.clear();
        return slint::CloseRequestResponse::HideWindow;
    }
    let weak = ui.as_weak();
    if let Err(error) = slint::spawn_local(async move {
        if matches!(
            rfd::AsyncMessageDialog::new()
                .set_title("Close Overlay Studio")
                .set_description(
                    "Close without saving the current project? Outputs and remote access will stop."
                )
                .set_buttons(rfd::MessageButtons::YesNo)
                .show()
                .await,
            rfd::MessageDialogResult::Yes
        ) && let Some(ui) = weak.upgrade()
        {
            let _ = ui.hide();
        }
    }) {
        ui.set_status(error.to_string().into());
    }
    slint::CloseRequestResponse::KeepWindowShown
}

async fn load(path: PathBuf) -> Result<Project, String> {
    let (tx, rx) = futures_channel::oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(Project::load(&path));
    });
    rx.await.map_err(|_| "Project loader stopped".to_string())?
}
async fn save(project: Project, path: PathBuf) -> Result<(), String> {
    let (tx, rx) = futures_channel::oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(project.save(&path));
    });
    rx.await.map_err(|_| "Project writer stopped".to_string())?
}
pub fn start(ui: &StudioWindow, state: Rc<RefCell<Desktop>>, command: String) {
    if ui.get_io_busy() {
        return;
    }
    ui.set_io_busy(true);
    let cleanup_state = state.clone();
    if matches!(command.as_str(), "import-image" | "import-video") {
        state.borrow_mut().media_cancel = Some(Arc::new(AtomicBool::new(false)));
        ui.set_status("Choose media to import · Preparation runs in the background…".into());
    }
    let weak = ui.as_weak();
    let result = slint::spawn_local(async move {
        let result = perform(&state, &command, &weak).await;
        state.borrow_mut().media_cancel = None;
        let close = std::mem::take(&mut state.borrow_mut().close_after_media);
        if let Some(ui) = weak.upgrade() {
            ui.set_io_busy(false);
            ui.set_media_import_busy(false);
            ui.set_media_import_cancelling(false);
            report(&ui, result);
            refresh(&ui, &state.borrow());
            if close
                && matches!(
                    request_close(&ui, &state),
                    slint::CloseRequestResponse::HideWindow
                )
            {
                let _ = ui.hide();
            }
        }
    });
    if let Err(error) = result {
        if let Some(cancel) = cleanup_state.borrow_mut().media_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        ui.set_io_busy(false);
        ui.set_status(error.to_string().into());
    }
}
async fn perform(
    state: &Rc<RefCell<Desktop>>,
    command: &str,
    ui: &slint::Weak<StudioWindow>,
) -> Result<(), String> {
    if matches!(command, "import-image" | "import-video") {
        let (generation, catalog_revision, design) = {
            let s = state.borrow();
            (
                s.document.generation,
                s.document.catalog_revision,
                s.document.design,
            )
        };
        let video = command == "import-video";
        let dialog = rfd::AsyncFileDialog::new();
        let dialog = if video {
            dialog.add_filter("Video clip", &["mp4", "mov", "m4v", "webm", "mkv"])
        } else {
            dialog.add_filter("Image or logo", &["png", "jpg", "jpeg"])
        };
        let Some(file) = dialog.pick_file().await else {
            return Ok(());
        };
        let path = file.path().to_path_buf();
        if let Some(ui) = ui.upgrade() {
            ui.set_media_import_busy(true);
            ui.set_status("Preparing media in the background…".into());
        }
        let cancel = state
            .borrow()
            .media_cancel
            .clone()
            .ok_or("Media job was cancelled")?;
        let worker_cancel = cancel.clone();
        let (tx, rx) = futures_channel::oneshot::channel();
        std::thread::spawn(move || {
            let _ = tx.send(
                overlay_studio::infrastructure::media_import::PreparedMedia::read_cancellable(
                    &path,
                    video,
                    &worker_cancel,
                ),
            );
        });
        let media = rx.await.map_err(|_| "Media importer stopped")??;
        if cancel.load(Ordering::Relaxed) {
            return Err("Media import cancelled".into());
        }
        let mut s = state.borrow_mut();
        if s.document.generation != generation || s.document.catalog_revision != catalog_revision {
            return Err("Canvas collection changed; import again on the intended design".into());
        }
        let (id, pixels) = s.document.import_media(media, design)?;
        s.media_pixels.insert(id, pixels);
        s.animation.reset();
        return Ok(());
    }
    if command == "new-project" {
        let dirty = state.borrow().document.dirty;
        if dirty
            && !matches!(
                rfd::AsyncMessageDialog::new()
                    .set_title("New project")
                    .set_description("Replace current unsaved work? Clocks and outputs will stop.")
                    .set_buttons(rfd::MessageButtons::YesNo)
                    .show()
                    .await,
                rfd::MessageDialogResult::Yes
            )
        {
            return Ok(());
        }
        let mut state = state.borrow_mut();
        state.install_project(Project::fresh(), None, false)?;
        state.recovery.clear();
        if let Some(ui) = ui.upgrade() {
            ui.set_new_field_name("".into());
            ui.set_ai_open(false);
        }
        return Ok(());
    }
    if command == "import-design" {
        let Some(file) = rfd::AsyncFileDialog::new()
            .add_filter("Overlay design", &["scoredesign"])
            .pick_file()
            .await
        else {
            return Ok(());
        };
        let path = file.path().to_path_buf();
        let (tx, rx) = futures_channel::oneshot::channel();
        std::thread::spawn(move || {
            let _ = tx.send(portable_design::read(&path));
        });
        let design = rx.await.map_err(|_| "Design importer stopped")??;
        state.borrow_mut().document.import_design(design)?;
        return Ok(());
    }
    if command == "export-design" {
        let name = state.borrow().document.active().name.clone();
        let Some(file) = rfd::AsyncFileDialog::new()
            .add_filter("Overlay design", &["scoredesign"])
            .set_file_name(format!("{name}.scoredesign"))
            .save_file()
            .await
        else {
            return Ok(());
        };
        let payload = {
            let state = state.borrow();
            portable_design::export(&state.document.project, state.document.design)?
        };
        let path = file.path().to_path_buf();
        let (tx, rx) = futures_channel::oneshot::channel();
        std::thread::spawn(move || {
            let _ = tx.send(portable_design::write(&path, &payload));
        });
        return rx.await.map_err(|_| "Design exporter stopped")?;
    }
    if command == "open" {
        let Some(file) = rfd::AsyncFileDialog::new()
            .add_filter("Overlay project", &["overlay", "json"])
            .pick_file()
            .await
        else {
            return Ok(());
        };
        let project = load(file.path().into()).await?;
        let dirty = state.borrow().document.dirty;
        if dirty
            && !matches!(
                rfd::AsyncMessageDialog::new()
                    .set_title("Open project")
                    .set_description(
                        "Replace the current unsaved project? Clocks and outputs will stop."
                    )
                    .set_buttons(rfd::MessageButtons::YesNo)
                    .show()
                    .await,
                rfd::MessageDialogResult::Yes
            )
        {
            return Ok(());
        }
        let mut state = state.borrow_mut();
        state.install_project(project, Some(file.path().into()), false)?;
        state.recovery.clear();
        if let Some(ui) = ui.upgrade() {
            ui.set_new_field_name("".into());
            ui.set_ai_open(false);
        }
    } else {
        let existing = if command == "save" {
            state.borrow().document.path.clone()
        } else {
            None
        };
        let path = if let Some(path) = existing {
            path
        } else {
            let name = state.borrow().document.project.name.clone();
            let Some(file) = rfd::AsyncFileDialog::new()
                .add_filter("Overlay project", &["overlay"])
                .set_file_name(format!("{name}.overlay"))
                .save_file()
                .await
            else {
                return Ok(());
            };
            file.path().to_path_buf()
        };
        let (snapshot, revision) = {
            let mut state = state.borrow_mut();
            state.document.end_gesture()?;
            (state.document.project.clone(), state.document.revision)
        };
        save(snapshot, path.clone()).await?;
        let mut state = state.borrow_mut();
        state.document.path = Some(path);
        if state.document.revision == revision {
            state.document.dirty = false;
            state.recovery.clear();
        }
    }
    Ok(())
}
