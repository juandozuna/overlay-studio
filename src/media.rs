use crate::{model::MotionAsset, scene};
mod process;
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

pub struct PreparedVideo {
    pub asset: MotionAsset,
    pub frames: Arc<Vec<egui::ColorImage>>,
}
pub fn find_ffmpeg() -> Option<PathBuf> {
    let mut candidates = vec![
        PathBuf::from("/usr/local/bin/ffmpeg"),
        PathBuf::from("/opt/homebrew/bin/ffmpeg"),
    ];
    if let Some(path) = std::env::var_os("PATH") {
        candidates.extend(std::env::split_paths(&path).map(|directory| directory.join("ffmpeg")));
    }
    candidates.into_iter().find(|path| path.is_file())
}
pub fn convert_video(path: &Path) -> Result<PreparedVideo, String> {
    convert_video_cancellable(path, &AtomicBool::new(false))
}
pub fn convert_video_cancellable(
    path: &Path,
    cancel: &AtomicBool,
) -> Result<PreparedVideo, String> {
    check_cancel(cancel)?;
    let source = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if !source.is_file() {
        return Err("Choose a regular video file".into());
    }
    if source.len() > 512 * 1024 * 1024 {
        return Err("Video source must be under 512 MB".into());
    }
    let ffmpeg = find_ffmpeg().ok_or("Install FFmpeg to import video clips.")?;
    let mut command = Command::new(ffmpeg);
    command
        .args(["-nostdin", "-v", "error", "-i"])
        .arg(path)
        .args([
            "-t",
            "15",
            "-vf",
            "fps=15,scale=640:360:force_original_aspect_ratio=decrease",
            "-f",
            "gif",
            "pipe:1",
        ]);
    let output = process::run(
        &mut command,
        cancel,
        std::time::Duration::from_secs(120),
        64 * 1024 * 1024,
    )?;
    if !output.status.success() {
        return Err(format!(
            "Video conversion failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    if output.stdout.is_empty() || output.stdout.len() > 64 * 1024 * 1024 {
        return Err("Converted video is empty or exceeds 64 MB".into());
    }
    let frames = scene::decode_motion(&output.stdout)?;
    check_cancel(cancel)?;
    Ok(PreparedVideo {
        asset: MotionAsset {
            id: 0,
            name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into(),
            gif: output.stdout.into(),
            fps: 15.,
            looped: false,
        },
        frames,
    })
}
pub(crate) fn check_cancel(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err("Media import cancelled".into())
    } else {
        Ok(())
    }
}
pub fn import(path: PathBuf, ctx: egui::Context) -> mpsc::Receiver<Result<PreparedVideo, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(convert_video(&path));
        ctx.request_repaint();
    });
    rx
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_video_import_is_portable_and_frames_are_shared() {
        let Some(ffmpeg) = find_ffmpeg() else {
            return;
        };
        let path =
            std::env::temp_dir().join(format!("overlay-motion-{}.mp4", rand::random::<u64>()));
        let status = Command::new(ffmpeg)
            .args([
                "-nostdin",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=80x48:rate=15",
                "-frames:v",
                "225",
                "-pix_fmt",
                "yuv420p",
                "-y",
            ])
            .arg(&path)
            .status()
            .unwrap();
        assert!(status.success());
        let rx = import(path.clone(), egui::Context::default());
        let mut prepared = rx
            .recv_timeout(std::time::Duration::from_secs(30))
            .unwrap()
            .unwrap();
        assert!(
            prepared.frames.len() > 150,
            "Exercise a clip longer than the entrance-duration limit"
        );
        assert!(Arc::ptr_eq(
            &prepared.frames,
            &scene::decode_motion(&prepared.asset.gif).unwrap()
        ));
        prepared.asset.id = 100;
        let mut project = crate::model::Project::fresh();
        project.motions.push(prepared.asset);
        let mut video = crate::model::Element::new(101, crate::model::Kind::Video);
        video.motion = Some(100);
        project.designs[0].elements.push(video);
        let saved = path.with_extension("overlay");
        project.save(&saved).unwrap();
        let loaded = crate::model::Project::load(&saved).unwrap();
        assert_eq!(
            scene::decode_motion(&loaded.motions[0].gif).unwrap().len(),
            prepared.frames.len()
        );
        let imported =
            crate::infrastructure::media_import::PreparedMedia::read(&path, true).unwrap();
        let mut document = crate::application::document_session::DocumentSession::default();
        let (id, _pixels) = document.import_media(imported, 0).unwrap();
        assert_eq!(document.selected_layer().unwrap().motion, Some(id));
        assert!(document.selected_layer().unwrap().duration <= 10.);
        document.undo().unwrap();
        assert!(
            !document
                .active()
                .elements
                .iter()
                .any(|e| e.motion == Some(id))
        );
        document.redo();
        document.project.validate().unwrap();
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(saved);
    }
}
