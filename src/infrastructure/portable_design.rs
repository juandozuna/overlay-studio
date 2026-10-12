//! Portable design file boundary. Validate media on a worker before UI commit.
use crate::model::{DesignFile, Project, VERSION, atomic_json};
use std::{io::Read, path::Path};

pub struct VerifiedDesign {
    file: DesignFile,
}
impl VerifiedDesign {
    pub(crate) fn into_file(self) -> DesignFile {
        self.file
    }
}
pub fn read(path: &Path) -> Result<VerifiedDesign, String> {
    let mut bytes = vec![];
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(512 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 512 * 1024 * 1024 {
        return Err("Design exceeds 512 MB".into());
    }
    verify(serde_json::from_slice(&bytes).map_err(|e| e.to_string())?)
}
pub fn verify(file: DesignFile) -> Result<VerifiedDesign, String> {
    if file.version != VERSION {
        return Err("Unsupported design version".into());
    }
    let mut p = Project::fresh();
    p.designs = vec![file.design];
    p.assets = file.assets;
    p.motions = file.motions;
    p.routes.clear();
    p.cue_rules.clear();
    p.controller.controls.clear();
    for e in &p.designs[0].elements {
        if !e.binding.is_empty() {
            p.values.entry(e.binding.clone()).or_insert("0".into());
        }
    }
    p.validate()?;
    Ok(VerifiedDesign {
        file: DesignFile {
            version: VERSION,
            design: p.designs.remove(0),
            assets: p.assets,
            motions: p.motions,
        },
    })
}
pub fn export(project: &Project, index: usize) -> Result<DesignFile, String> {
    let design = project
        .designs
        .get(index)
        .ok_or("Design does not exist")?
        .clone();
    let assets = project
        .assets
        .iter()
        .filter(|a| design.elements.iter().any(|e| e.asset == Some(a.id)))
        .cloned()
        .collect();
    let motions = project
        .motions
        .iter()
        .filter(|m| design.elements.iter().any(|e| e.motion == Some(m.id)))
        .cloned()
        .collect();
    Ok(DesignFile {
        version: VERSION,
        design,
        assets,
        motions,
    })
}
pub fn write(path: &Path, file: &DesignFile) -> Result<(), String> {
    atomic_json(path, file)
}
