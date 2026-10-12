use crate::model::{Design, Element};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    time::{Duration, Instant},
};

const HOST: &str = "127.0.0.1:11439";
const MODEL: &str = "qwen3:0.6b";

pub enum Event {
    Status(String),
    Ready(Engine),
    Proposal { design: Design, fallback: bool },
    Failed(String),
}
pub struct Engine {
    child: Option<Child>,
}
impl Drop for Engine {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub fn support_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Library/Application Support/Overlay Studio/local-ai")
}

fn run_command(
    command: &mut Command,
    input: Option<Vec<u8>>,
    cancel: &AtomicBool,
    timeout: Duration,
    output_limit: usize,
) -> Result<Output, String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("Local AI job cancelled".into());
    }
    command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    if let Some(input) = input {
        if let Err(error) = child
            .stdin
            .take()
            .ok_or("Missing request pipe")?
            .write_all(&input)
        {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error.to_string());
        }
    }
    let stdout = child.stdout.take().ok_or("Missing command output pipe")?;
    let stderr = child.stderr.take().ok_or("Missing command error pipe")?;
    let output = std::thread::spawn(move || {
        let mut bytes = vec![];
        stdout
            .take(output_limit as u64 + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let errors = std::thread::spawn(move || {
        let mut bytes = vec![];
        stderr
            .take(1_048_577)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let started = Instant::now();
    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            break Err("Local AI job cancelled".to_string());
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            break Err("Local AI job timed out".to_string());
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(error.to_string());
            }
        }
    };
    let stdout = output
        .join()
        .map_err(|_| "Local AI output reader stopped")?
        .map_err(|e| e.to_string())?;
    let stderr = errors
        .join()
        .map_err(|_| "Local AI error reader stopped")?
        .map_err(|e| e.to_string())?;
    let status = status?;
    if stdout.len() > output_limit {
        return Err("Local AI response was too large".into());
    }
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

fn curl_cancellable(
    path: &str,
    body: Option<serde_json::Value>,
    timeout: u64,
    cancel: &AtomicBool,
) -> Result<serde_json::Value, String> {
    let mut command = Command::new("/usr/bin/curl");
    command.args([
        "--fail-with-body",
        "--silent",
        "--show-error",
        "--noproxy",
        "*",
        "--max-time",
        &timeout.to_string(),
    ]);
    command.arg(format!("http://{HOST}{path}"));
    if body.is_some() {
        command.args([
            "-H",
            "Content-Type: application/json",
            "--data-binary",
            "@-",
        ]);
    }
    let input = body.map(|body| body.to_string().into_bytes());
    let output = run_command(
        &mut command,
        input,
        cancel,
        Duration::from_secs(timeout + 5),
        16 * 1024 * 1024,
    )?;
    if !output.status.success() {
        return Err(format!(
            "Local AI request failed: {} {}",
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        ));
    }
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;
    if let Some(error) = value.get("error").and_then(|v| v.as_str()) {
        return Err(error.into());
    }
    Ok(value)
}

fn command_ok(
    command: &mut Command,
    cancel: &AtomicBool,
    timeout: Duration,
) -> Result<Output, String> {
    let output = run_command(command, None, cancel, timeout, 16 * 1024 * 1024)?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(String::from_utf8_lossy(&output.stderr).into_owned())
    }
}

fn install_binary(dir: &Path, tx: &Sender<Event>, cancel: &AtomicBool) -> Result<PathBuf, String> {
    let binary = dir.join("runtime/ollama");
    if binary.is_file() {
        return Ok(binary);
    }
    for existing in [
        "/opt/homebrew/bin/ollama",
        "/usr/local/bin/ollama",
        "/Applications/Ollama.app/Contents/Resources/ollama",
    ] {
        if Path::new(existing).is_file() {
            return Ok(PathBuf::from(existing));
        }
    }
    if !cfg!(target_os = "macos") {
        return Err("Automatic AI setup currently requires macOS.".into());
    }
    let _ = tx.send(Event::Status(
        "Downloading local AI runtime from Ollama…".into(),
    ));
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let archive = dir.join("ollama-darwin.tgz.partial");
    command_ok(
        Command::new("/usr/bin/curl")
            .args([
                "--fail",
                "--location",
                "--silent",
                "--show-error",
                "--max-time",
                "900",
                "--proto",
                "=https",
                "--proto-redir",
                "=https",
                "-o",
            ])
            .arg(&archive)
            .arg("https://github.com/ollama/ollama/releases/download/v0.40.2/ollama-darwin.tgz"),
        cancel,
        Duration::from_secs(905),
    )?;
    let listing = command_ok(
        Command::new("/usr/bin/tar").arg("-tzf").arg(&archive),
        cancel,
        Duration::from_secs(60),
    )?;
    for item in String::from_utf8_lossy(&listing.stdout).lines() {
        if Path::new(item).is_absolute()
            || Path::new(item)
                .components()
                .any(|c| c == std::path::Component::ParentDir)
        {
            return Err("AI archive contains an invalid path".into());
        }
    }
    let runtime = dir.join("runtime");
    std::fs::create_dir_all(&runtime).map_err(|e| e.to_string())?;
    command_ok(
        Command::new("/usr/bin/tar")
            .arg("-xzf")
            .arg(&archive)
            .arg("-C")
            .arg(&runtime),
        cancel,
        Duration::from_secs(120),
    )?;
    if !binary.is_file() {
        return Err("AI runtime archive did not include Ollama".into());
    }
    let _ = std::fs::remove_file(archive);
    Ok(binary)
}

pub fn setup_with_notify(
    cancel: Arc<AtomicBool>,
    notify: impl Fn() + Send + 'static,
) -> Receiver<Event> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = (|| -> Result<Engine, String> {
            let dir = support_dir();
            let binary = install_binary(&dir, &tx, &cancel)?;
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let mut engine = Engine { child: None };
            if curl_cancellable("/api/version", None, 2, &cancel).is_err() {
                if cancel.load(Ordering::Relaxed) {
                    return Err("Local AI job cancelled".into());
                }
                let log =
                    std::fs::File::create(dir.join("runtime.log")).map_err(|e| e.to_string())?;
                engine.child = Some(
                    Command::new(binary)
                        .arg("serve")
                        .env("OLLAMA_HOST", HOST)
                        .env("OLLAMA_MODELS", dir.join("models"))
                        .env("OLLAMA_NO_CLOUD", "1")
                        .env("OLLAMA_MAX_LOADED_MODELS", "1")
                        .stdin(Stdio::null())
                        .stdout(Stdio::from(log.try_clone().map_err(|e| e.to_string())?))
                        .stderr(Stdio::from(log))
                        .spawn()
                        .map_err(|e| e.to_string())?,
                );
                let mut ready = false;
                for _ in 0..60 {
                    if curl_cancellable("/api/version", None, 1, &cancel).is_ok() {
                        ready = true;
                        break;
                    }
                    if cancel.load(Ordering::Relaxed) {
                        return Err("Local AI job cancelled".into());
                    }
                    if engine
                        .child
                        .as_mut()
                        .and_then(|c| c.try_wait().ok().flatten())
                        .is_some()
                    {
                        return Err("Local AI could not start. See local-ai/runtime.log.".into());
                    }
                    std::thread::sleep(Duration::from_millis(500));
                }
                if !ready {
                    return Err("Timed out starting local AI".into());
                }
            }
            let _ = tx.send(Event::Status(
                "Installing Qwen3 local layout model (523 MB)…".into(),
            ));
            curl_cancellable(
                "/api/pull",
                Some(json!({"model":MODEL,"stream":false})),
                1800,
                &cancel,
            )?;
            Ok(engine)
        })();
        let _ = tx.send(match result {
            Ok(engine) => Event::Ready(engine),
            Err(error) => Event::Failed(error),
        });
        notify();
    });
    rx
}

pub fn setup(ctx: egui::Context) -> Receiver<Event> {
    setup_with_notify(Arc::new(AtomicBool::new(false)), move || {
        ctx.request_repaint()
    })
}

#[derive(Serialize, Deserialize)]
pub struct Placement {
    pub id: u64,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}
#[derive(Deserialize)]
struct Plan {
    placements: Vec<Placement>,
}

pub fn apply_plan(
    design: &Design,
    selected: &[u64],
    placements: &[Placement],
) -> Result<Design, String> {
    let mut result = design.clone();
    let mut seen = std::collections::HashSet::new();
    if placements.len() != selected.len() {
        return Err("AI did not place every selected layer. Try a smaller selection.".into());
    }
    for placement in placements {
        if !selected.contains(&placement.id)
            || !seen.insert(placement.id)
            || [placement.x, placement.y, placement.w, placement.h]
                .iter()
                .any(|v| !v.is_finite())
            || placement.x < 0.
            || placement.y < 0.
            || placement.w < 1.
            || placement.h < 1.
            || placement.x + placement.w > design.width as f32 + 0.5
            || placement.y + placement.h > design.height as f32 + 0.5
        {
            return Err("AI returned an invalid layout. Your design was preserved.".into());
        }
        let element = result
            .elements
            .iter_mut()
            .find(|e| e.id == placement.id && !e.locked)
            .ok_or("AI tried to move an unavailable layer")?;
        let dx = placement.x - element.x;
        let dy = placement.y - element.y;
        let sx = placement.w / element.w;
        let sy = placement.h / element.h;
        for frame in &mut element.keyframes {
            frame.x = placement.x + (frame.x - element.x) * sx;
            frame.y = placement.y + (frame.y - element.y) * sy;
            frame.w *= sx;
            frame.h *= sy;
        }
        let _ = (dx, dy);
        element.x = placement.x;
        element.y = placement.y;
        element.w = placement.w;
        element.h = placement.h;
    }
    crate::model::validate_design(&result)?;
    Ok(result)
}

/// A bounded, deterministic proposal used only after the local model returns
/// malformed or out-of-canvas geometry on every attempt.
pub fn balanced_fallback(design: &Design, selected: &[u64]) -> Result<Design, String> {
    if selected.is_empty() {
        return Err("Select at least one layer to arrange.".into());
    }
    let columns = (selected.len() as f32).sqrt().ceil() as usize;
    let rows = selected.len().div_ceil(columns);
    let shortest = design.width.min(design.height) as f32;
    let margin = (shortest * 0.05).clamp(8., 80.);
    let gap = (shortest * 0.02).clamp(4., 28.);
    let cell_w = (design.width as f32 - margin * 2. - gap * (columns - 1) as f32) / columns as f32;
    let cell_h = (design.height as f32 - margin * 2. - gap * (rows - 1) as f32) / rows as f32;
    if cell_w < 1. || cell_h < 1. {
        return Err("The canvas is too small to arrange the selected layers.".into());
    }
    let placements = selected
        .iter()
        .enumerate()
        .map(|(index, id)| {
            let element = design
                .elements
                .iter()
                .find(|element| element.id == *id && !element.locked)
                .ok_or("A selected layer is no longer available")?;
            let scale = (cell_w * 0.88 / element.w)
                .min(cell_h * 0.78 / element.h)
                .min(1.);
            let w = (element.w * scale).max(1.);
            let h = (element.h * scale).max(1.);
            let column = index % columns;
            let row = index / columns;
            let left = margin + column as f32 * (cell_w + gap);
            let top = margin + row as f32 * (cell_h + gap);
            Ok(Placement {
                id: *id,
                x: left + (cell_w - w) / 2.,
                y: top + (cell_h - h) / 2.,
                w,
                h,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    apply_plan(design, selected, &placements)
}

pub fn propose_with_notify(
    design: Design,
    selected: Vec<u64>,
    brief: String,
    cancel: Arc<AtomicBool>,
    notify: impl Fn() + Send + 'static,
) -> Receiver<Event> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = (|| -> Result<(Design, bool), String> {
            let elements: Vec<_> = design.elements.iter().filter(|e| selected.contains(&e.id) && !e.locked)
                .map(|e: &Element| json!({"id":e.id,"name":e.name,"kind":format!("{:?}",e.kind),"binding":e.binding,"text":e.text,"group":e.group,"preferred_width":e.w,"preferred_height":e.h})).collect();
            let schema = json!({"type":"object","properties":{"placements":{"type":"array","items":{"type":"object","properties":{"id":{"type":"integer"},"x":{"type":"number"},"y":{"type":"number"},"w":{"type":"number"},"h":{"type":"number"}},"required":["id","x","y","w","h"]}}},"required":["placements"]});
            let mut feedback = String::new();
            for attempt in 0..3 {
                if cancel.load(Ordering::Relaxed) {
                    return Err("Local AI job cancelled".into());
                }
                let prompt = format!(
                    "Design NEW positions on a transparent scoreboard canvas of {} by {} pixels. User request: {}. Layers: {}. Return JSON placements for EACH supplied layer ID exactly once. Every x and y must be zero or greater. Every width and height must be at least 1. Every x + width must be at most {} and every y + height must be at most {}. Pick meaningful positions so independent text layers do not overlap. Keep each team name near its score and place the clock between teams. Decorative panels must sit behind related text. You may resize layers to fit. Center the composition with generous margins. Never alter IDs. {} /no_think",
                    design.width,
                    design.height,
                    brief,
                    serde_json::to_string(&elements).map_err(|e| e.to_string())?,
                    design.width,
                    design.height,
                    feedback
                );
                let generated = curl_cancellable(
                    "/api/generate",
                    Some(
                        json!({"model":MODEL,"prompt":prompt,"format":schema,"think":false,"stream":false,"keep_alive":"2m","options":{"temperature":0.05 + attempt as f32 * 0.02,"num_ctx":8192,"num_predict":4096}}),
                    ),
                    240,
                    &cancel,
                )
                .and_then(|value| {
                    let response = value["response"].as_str().ok_or("Empty model response")?;
                    let plan: Plan = serde_json::from_str(response).map_err(|e| e.to_string())?;
                    apply_plan(&design, &selected, &plan.placements)
                });
                match generated {
                    Ok(proposal) => return Ok((proposal, false)),
                    Err(error) if error.contains("cancelled") => return Err(error),
                    Err(error) => {
                        feedback = format!(
                            "The previous attempt was rejected: {error}. Correct every placement."
                        );
                    }
                }
            }
            balanced_fallback(&design, &selected).map(|proposal| (proposal, true))
        })();
        let _ = tx.send(match result {
            Ok((design, fallback)) => Event::Proposal { design, fallback },
            Err(error) => Event::Failed(error),
        });
        notify();
    });
    rx
}

pub fn propose(
    design: Design,
    selected: Vec<u64>,
    brief: String,
    ctx: egui::Context,
) -> Receiver<Event> {
    propose_with_notify(
        design,
        selected,
        brief,
        Arc::new(AtomicBool::new(false)),
        move || ctx.request_repaint(),
    )
}

pub fn verify() -> Result<(), String> {
    let ctx = egui::Context::default();
    let rx = setup(ctx.clone());
    let engine = loop {
        match rx.recv().map_err(|e| e.to_string())? {
            Event::Ready(engine) => break engine,
            Event::Failed(error) => return Err(error),
            Event::Status(status) => println!("{status}"),
            _ => {}
        }
    };
    let mut design = crate::model::template("AI diagnostic", 0);
    design
        .elements
        .retain(|e| e.kind == crate::model::Kind::Text);
    for element in &mut design.elements {
        element.x = 20.;
        element.y = 20.;
    }
    let ids = design.elements.iter().map(|e| e.id).collect();
    let rx = propose(
        design,
        ids,
        "Create a centered two-team scoreboard with name, score, and clock.".into(),
        ctx,
    );
    match rx.recv().map_err(|e| e.to_string())? {
        Event::Proposal { design, .. } => {
            let moved = design
                .elements
                .iter()
                .filter(|e| e.x != 20. || e.y != 20.)
                .count();
            if moved == 0 {
                return Err("AI left the deliberately overlapping layers unchanged".into());
            }
            println!(
                "Local AI generated a validated layout for {} layers; moved {} overlapping layers",
                design.elements.len(),
                moved
            );
        }
        Event::Failed(error) => return Err(error),
        _ => return Err("Unexpected AI response".into()),
    }
    drop(engine);
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn cancellation_terminates_and_reaps_local_ai_subprocess() {
        let cancel = Arc::new(AtomicBool::new(false));
        let signal = cancel.clone();
        let thread = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(80));
            signal.store(true, Ordering::Relaxed);
        });
        let started = Instant::now();
        let result = run_command(
            Command::new("/bin/sh").args(["-c", "exec sleep 10"]),
            None,
            &cancel,
            Duration::from_secs(5),
            1_024,
        );
        thread.join().unwrap();
        assert!(result.unwrap_err().contains("cancelled"));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn model_plan_requires_complete_valid_ids_and_keeps_animation_attached() {
        let mut design = crate::model::template("Test", 0);
        let id = design.elements[0].id;
        let frame = crate::model::Keyframe::capture(&design.elements[0], 0.);
        design.elements[0].keyframes.push(frame);
        let source = design.clone();
        let placement = Placement {
            id,
            x: 100.,
            y: 100.,
            w: 300.,
            h: 100.,
        };
        let result = apply_plan(&design, &[id], &[placement]).unwrap();
        assert_eq!(result.elements[0].keyframes[0].x, 100.);
        assert!(design == source);
        assert!(apply_plan(&design, &[id], &[]).is_err());
        assert!(
            apply_plan(
                &design,
                &[id],
                &[Placement {
                    id,
                    x: -10.,
                    y: 0.,
                    w: 100.,
                    h: 100.
                }]
            )
            .is_err()
        );
        assert!(
            apply_plan(
                &design,
                &[id],
                &[Placement {
                    id: 999,
                    x: 0.,
                    y: 0.,
                    w: 100.,
                    h: 100.
                }]
            )
            .is_err()
        );
    }

    #[test]
    fn balanced_fallback_stays_in_bounds_and_moves_every_selected_layer() {
        let mut design = crate::model::template("Fallback", 0);
        let selected = design
            .elements
            .iter()
            .map(|element| element.id)
            .collect::<Vec<_>>();
        for element in &mut design.elements {
            element.x = 0.;
            element.y = 0.;
        }
        let proposal = balanced_fallback(&design, &selected).unwrap();
        crate::model::validate_design(&proposal).unwrap();
        assert!(
            proposal
                .elements
                .iter()
                .all(|element| element.x > 0. && element.y > 0.)
        );
        for (index, element) in proposal.elements.iter().enumerate() {
            assert!(
                proposal.elements[..index]
                    .iter()
                    .all(|other| other.x != element.x || other.y != element.y)
            );
        }
    }
}
