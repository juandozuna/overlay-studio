mod app;
use overlay_studio::{application, layout_ai, media, model, output, remote, scene};

fn main() -> eframe::Result {
    if std::env::args().any(|arg| arg == "--verify-local-ai") {
        if let Err(error) = layout_ai::verify() {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return Ok(());
    }
    std::panic::set_hook(Box::new(|info| {
        let log = format!("{info}\n{}\n", std::backtrace::Backtrace::force_capture());
        eprintln!("{log}");
        if let Some(home) = std::env::var_os("HOME") {
            let dir =
                std::path::PathBuf::from(home).join("Library/Application Support/Overlay Studio");
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::write(dir.join("crash.log"), log);
        }
    }));
    #[cfg(target_os = "macos")]
    if std::env::args().any(|arg| arg == "--verify-syphon") {
        let args: Vec<_> = std::env::args().collect();
        let source = args
            .windows(2)
            .find(|pair| pair[0] == "--source")
            .map(|pair| pair[1].as_str())
            .unwrap_or("Overlay Studio");
        match output::verify_syphon_named(source) {
            Ok(()) => return Ok(()),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Overlay Studio")
            .with_inner_size([1440.0, 920.0])
            .with_min_inner_size([1000.0, 700.0]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Overlay Studio",
        options,
        Box::new(|cc| Ok(Box::new(app::Studio::new(cc)))),
    )
}
