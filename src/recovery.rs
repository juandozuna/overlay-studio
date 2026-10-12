//! Versioned, validated recovery snapshots shared by desktop front ends.
use crate::model::{Project, atomic_json};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const VERSION: u32 = 1;
const MAX_RECOVERY_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct Snapshot {
    pub version: u32,
    pub saved_unix_ms: u64,
    pub source_path: Option<PathBuf>,
    pub project: Project,
}

impl Snapshot {
    pub fn new(mut project: Project, source_path: Option<PathBuf>) -> Self {
        let _ = project.sync_stable_references();
        let saved_unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .min(u64::MAX as u128) as u64;
        Self {
            version: VERSION,
            saved_unix_ms,
            source_path,
            project,
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if self.version != VERSION {
            return Err("Unsupported recovery snapshot version".into());
        }
        self.project.validate()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        atomic_json(path, self)
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let metadata = std::fs::metadata(path).map_err(|error| error.to_string())?;
        if metadata.len() > MAX_RECOVERY_BYTES {
            return Err("Recovery snapshot exceeds 512 MB".into());
        }
        let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
        let snapshot: Self = serde_json::from_slice(&bytes)
            .map_err(|error| format!("Invalid recovery snapshot: {error}"))?;
        if snapshot.version != VERSION {
            return Err("Unsupported recovery snapshot version".into());
        }
        snapshot.project.validate()?;
        Ok(snapshot)
    }
}

pub fn default_path() -> PathBuf {
    if let Some(path) = std::env::var_os("OVERLAY_STUDIO_RECOVERY_PATH") {
        return path.into();
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Library/Application Support/Overlay Studio")
        .join("slint-recovery-v1.autosave.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Asset;
    use std::sync::Arc;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "overlay-studio-{name}-{}-{}.json",
            std::process::id(),
            rand::random::<u64>()
        ))
    }

    #[test]
    fn snapshot_round_trip_validates_and_preserves_source() {
        let path = temp_path("recovery-round-trip");
        let source = PathBuf::from("broadcast.overlay");
        let snapshot = Snapshot::new(Project::fresh(), Some(source.clone()));
        snapshot.save(&path).unwrap();
        let loaded = Snapshot::load(&path).unwrap();
        assert_eq!(loaded.version, VERSION);
        assert_eq!(loaded.source_path, Some(source));
        assert!(loaded.project == snapshot.project);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn malformed_snapshot_is_rejected() {
        let path = temp_path("recovery-malformed");
        std::fs::write(&path, b"{not-json").unwrap();
        let error = match Snapshot::load(&path) {
            Ok(_) => panic!("malformed recovery snapshot loaded"),
            Err(error) => error,
        };
        assert!(error.contains("Invalid recovery"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn cloning_project_shares_embedded_payloads() {
        let mut project = Project::fresh();
        project.assets.push(Asset {
            id: project.next_id(),
            name: "Logo".into(),
            bytes: Arc::new(vec![1, 2, 3]),
        });
        let cloned = project.clone();
        assert!(Arc::ptr_eq(
            &project.assets[0].bytes,
            &cloned.assets[0].bytes
        ));
        let json = serde_json::to_value(&project).unwrap();
        assert_eq!(json["assets"][0]["bytes"], serde_json::json!([1, 2, 3]));
    }
}
