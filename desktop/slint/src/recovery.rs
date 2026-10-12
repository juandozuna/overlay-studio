//! Background recovery discovery and autosave scheduling.
use crate::{Desktop, StudioWindow};
use overlay_studio::{application::document_session::DocumentSession, recovery::Snapshot};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(15);

type LoadResult = Result<Option<Snapshot>, String>;
type SaveResult = (u64, u64, Result<(), String>);

pub struct Manager {
    path: PathBuf,
    load: Option<mpsc::Receiver<LoadResult>>,
    save: Option<mpsc::Receiver<SaveResult>>,
    candidate: Option<Snapshot>,
    hidden: bool,
    last_attempt: Instant,
    last_saved: Option<(u64, u64)>,
    epoch: Arc<AtomicU64>,
}

impl Manager {
    pub fn new() -> Self {
        Self::with_path(overlay_studio::recovery::default_path())
    }

    fn with_path(path: PathBuf) -> Self {
        let (tx, rx) = mpsc::channel();
        let worker_path = path.clone();
        std::thread::spawn(move || {
            let result = if worker_path.is_file() {
                Snapshot::load(&worker_path).map(Some)
            } else {
                Ok(None)
            };
            let _ = tx.send(result);
        });
        Self {
            path,
            load: Some(rx),
            save: None,
            candidate: None,
            hidden: false,
            last_attempt: Instant::now(),
            last_saved: None,
            epoch: Arc::new(AtomicU64::new(1)),
        }
    }

    pub fn poll(&mut self, ui: &StudioWindow, document: &DocumentSession) {
        if let Some(rx) = &self.load {
            match rx.try_recv() {
                Ok(Ok(candidate)) => {
                    self.candidate = candidate;
                    self.load = None;
                    refresh(ui, self);
                }
                Ok(Err(error)) => {
                    self.load = None;
                    let invalid = self.path.with_extension("invalid.json");
                    let _ = std::fs::rename(&self.path, invalid);
                    ui.set_status(format!("Recovery file was invalid and set aside: {error}").into());
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.load = None;
                    ui.set_status("Recovery reader stopped unexpectedly".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(rx) = &self.save {
            match rx.try_recv() {
                Ok((generation, revision, result)) => {
                    self.save = None;
                    match result {
                        Ok(()) => self.last_saved = Some((generation, revision)),
                        Err(error) => ui.set_status(format!("Recovery save failed: {error}").into()),
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.save = None;
                    ui.set_status("Recovery writer stopped unexpectedly".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        let marker = (document.generation, document.revision);
        if document.dirty
            && self.save.is_none()
            && self.last_saved != Some(marker)
            && self.last_attempt.elapsed() >= AUTOSAVE_INTERVAL
        {
            self.last_attempt = Instant::now();
            let snapshot = Snapshot::new(document.project.clone(), document.path.clone());
            let path = self.path.clone();
            let epoch = self.epoch.clone();
            let token = epoch.load(Ordering::Acquire);
            let (tx, rx) = mpsc::channel();
            self.save = Some(rx);
            std::thread::spawn(move || {
                let result = if epoch.load(Ordering::Acquire) == token {
                    snapshot.save(&path)
                } else {
                    Ok(())
                };
                if epoch.load(Ordering::Acquire) != token {
                    let _ = std::fs::remove_file(&path);
                }
                let _ = tx.send((marker.0, marker.1, result));
            });
        }
    }

    pub fn take_candidate(&mut self) -> Option<Snapshot> {
        self.candidate.take()
    }

    pub fn later(&mut self) {
        self.hidden = true;
    }

    pub fn clear(&mut self) {
        self.candidate = None;
        self.hidden = false;
        self.last_saved = None;
        self.epoch.fetch_add(1, Ordering::AcqRel);
        let _ = std::fs::remove_file(&self.path);
    }
}

pub fn refresh(ui: &StudioWindow, manager: &Manager) {
    ui.set_recovery_visible(manager.candidate.is_some() && !manager.hidden);
    if let Some(candidate) = &manager.candidate {
        ui.set_recovery_project(candidate.project.name.clone().into());
        let source = candidate
            .source_path
            .as_ref()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "an unsaved project".into());
        ui.set_recovery_detail(
            format!(
                "Restore {} from the last automatic recovery snapshot. Clocks, outputs, and remote control will start stopped.",
                source
            )
            .into(),
        );
    }
}

pub fn recover(state: &mut Desktop) -> Result<(), String> {
    let candidate = state
        .recovery
        .take_candidate()
        .ok_or("Recovery snapshot is no longer available")?;
    let source_path = candidate.source_path.clone();
    state.install_project(candidate.project, source_path, true)?;
    state.recovery.clear();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use overlay_studio::model::Project;

    fn temp_path() -> PathBuf {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        std::env::temp_dir().join(format!(
            "overlay-studio-slint-recovery-{}-{}.json",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ))
    }

    #[test]
    fn manager_discovers_valid_snapshot() {
        let path = temp_path();
        Snapshot::new(Project::fresh(), None).save(&path).unwrap();
        let mut manager = Manager::with_path(path.clone());
        let result = manager
            .load
            .take()
            .unwrap()
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        manager.candidate = result.unwrap();
        assert!(manager.candidate.is_some());
        manager.clear();
        assert!(!path.exists());
    }

    #[test]
    fn recovery_installs_document_dirty_with_runtime_stopped() {
        let path = temp_path();
        let mut state = Desktop::new();
        state.recovery.path = path.clone();
        state.game.clocks[0].running = true;
        let mut recovered = Project::fresh();
        recovered.name = "Recovered broadcast".into();
        let source = PathBuf::from("recovered.overlay");
        state.recovery.candidate = Some(Snapshot::new(recovered, Some(source.clone())));
        recover(&mut state).unwrap();
        assert_eq!(state.document.project.name, "Recovered broadcast");
        assert_eq!(state.document.path, Some(source));
        assert!(state.document.dirty);
        assert!(state.game.clocks.iter().all(|clock| !clock.running));
        assert!(state.outputs.senders.iter().all(Option::is_none));
        assert!(state.remote.server.is_none());
        let _ = std::fs::remove_file(path);
    }
}
