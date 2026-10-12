use crate::{
    media,
    model::{Asset, MotionAsset},
    scene,
};
use std::{
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

pub enum Pixels {
    Image(Arc<egui::ColorImage>),
    Video(Arc<Vec<egui::ColorImage>>),
}
pub struct PreparedMedia {
    pub(crate) image: Option<Asset>,
    pub(crate) motion: Option<MotionAsset>,
    pub(crate) pixels: Pixels,
}
impl PreparedMedia {
    pub fn read(path: &Path, video: bool) -> Result<Self, String> {
        Self::read_cancellable(path, video, &AtomicBool::new(false))
    }
    pub fn read_cancellable(path: &Path, video: bool, cancel: &AtomicBool) -> Result<Self, String> {
        media::check_cancel(cancel)?;
        if video {
            let prepared = media::convert_video_cancellable(path, cancel)?;
            Ok(Self {
                image: None,
                motion: Some(prepared.asset),
                pixels: Pixels::Video(prepared.frames),
            })
        } else {
            let source = std::fs::metadata(path).map_err(|e| e.to_string())?;
            if !source.is_file() {
                return Err("Choose a regular image file".into());
            }
            if source.len() > 32 * 1024 * 1024 {
                return Err("Image must be under 32 MB".into());
            }
            let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
            if bytes.len() > 32 * 1024 * 1024 {
                return Err("Image must be under 32 MB".into());
            }
            let pixels = scene::decode_image(&bytes)?;
            media::check_cancel(cancel)?;
            Ok(Self {
                image: Some(Asset {
                    id: 0,
                    name: path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into(),
                    bytes: bytes.into(),
                }),
                motion: None,
                pixels: Pixels::Image(pixels),
            })
        }
    }
    pub fn dimensions(&self) -> [usize; 2] {
        match &self.pixels {
            Pixels::Image(image) => image.size,
            Pixels::Video(frames) => frames[0].size,
        }
    }
}
