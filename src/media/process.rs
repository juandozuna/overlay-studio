//! Bounded worker subprocess execution with cancellation and a wall-clock deadline.
use std::{
    io::Read,
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

pub(super) fn run(
    command: &mut Command,
    cancel: &AtomicBool,
    timeout: Duration,
    limit: usize,
) -> Result<Output, String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("Media import cancelled".into());
    }
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Could not start FFmpeg: {e}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or("Missing conversion output pipe")?;
    let stderr = child.stderr.take().ok_or("Missing conversion error pipe")?;
    let output = std::thread::spawn(move || {
        let mut bytes = vec![];
        stdout
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let errors = std::thread::spawn(move || {
        let mut bytes = vec![];
        stderr.take(65537).read_to_end(&mut bytes).map(|_| bytes)
    });
    let started = Instant::now();
    let result = loop {
        if cancel.load(Ordering::Relaxed) {
            break Err("Media import cancelled".to_string());
        }
        if started.elapsed() >= timeout {
            break Err("Video conversion timed out; try a shorter source clip".into());
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(error) => break Err(error.to_string()),
        }
    };
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    let stdout = output
        .join()
        .map_err(|_| "Conversion output reader stopped")?
        .map_err(|e| e.to_string())?;
    let stderr = errors
        .join()
        .map_err(|_| "Conversion error reader stopped")?
        .map_err(|e| e.to_string())?;
    let status = result?;
    if stdout.len() > limit {
        return Err("Converted video exceeds 64 MB".into());
    }
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[test]
    fn deadline_kills_and_reaps_a_stalled_converter() {
        let started = Instant::now();
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "exec sleep 10"]);
        let result = run(
            &mut command,
            &AtomicBool::new(false),
            Duration::from_millis(80),
            1024,
        );
        assert!(result.unwrap_err().contains("timed out"));
        assert!(started.elapsed() < Duration::from_secs(2));
    }
    #[test]
    fn cancellation_terminates_an_active_converter() {
        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let signal = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(80));
            flag.store(true, Ordering::Relaxed);
        });
        let started = Instant::now();
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "exec sleep 10"]);
        let result = run(&mut command, &cancel, Duration::from_secs(5), 1024);
        signal.join().unwrap();
        assert!(result.unwrap_err().contains("cancelled"));
        assert!(started.elapsed() < Duration::from_secs(2));
    }
    #[test]
    fn oversized_pipe_output_is_bounded() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "printf abcdefghijklmnopqrstuvwxyz"]);
        assert!(
            run(
                &mut command,
                &AtomicBool::new(false),
                Duration::from_secs(2),
                8
            )
            .unwrap_err()
            .contains("exceeds")
        );
    }
}
