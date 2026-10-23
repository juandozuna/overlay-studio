//! Imports sound files into the project (FX-3).
use crate::model::SoundAsset;
use std::{path::Path, sync::Arc};

pub const EXTENSIONS: [&str; 4] = ["wav", "mp3", "ogg", "flac"];
const MAX_BYTES: u64 = 16 * 1024 * 1024;

pub fn import_sound(path: &Path, next_id: u64) -> Result<SoundAsset, String> {
    let extension = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    check_extension(&extension)?;
    let size = std::fs::metadata(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?.len();
    check_size(size)?;
    let bytes = std::fs::read(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    check_size(bytes.len() as u64)?;
    let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Sound").to_string();
    Ok(SoundAsset { id: next_id, name, bytes: Arc::new(bytes) })
}

fn check_extension(extension: &str) -> Result<(), String> {
    if EXTENSIONS.contains(&extension) {
        Ok(())
    } else {
        Err(format!("Unsupported sound format .{extension}; use wav, mp3, ogg or flac"))
    }
}

fn check_size(size: u64) -> Result<(), String> {
    match size {
        0 => Err("The sound file is empty".into()),
        s if s > MAX_BYTES => Err("The sound file is larger than 16 MB".into()),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_extension_and_size() {
        assert!(check_extension("wav").is_ok());
        assert!(check_extension("txt").is_err());
        assert!(check_size(0).is_err());
        assert!(check_size(MAX_BYTES).is_ok());
        assert!(check_size(MAX_BYTES + 1).is_err());
    }

    #[test]
    fn imports_a_file_with_its_stem_as_name() {
        let dir = std::env::temp_dir().join(format!("sound-import-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("Big Hit.WAV");
        std::fs::write(&file, [1u8, 2, 3]).unwrap();
        let asset = import_sound(&file, 4).unwrap();
        assert_eq!((asset.id, asset.name.as_str(), asset.bytes.len()), (4, "Big Hit", 3));
        std::fs::write(dir.join("empty.mp3"), []).unwrap();
        assert!(import_sound(&dir.join("empty.mp3"), 5).is_err());
        assert!(import_sound(&dir.join("x.txt"), 6).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
