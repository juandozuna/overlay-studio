use crate::{
    model::*,
    scene::{self, Resources},
};
mod pacing;
use egui_wgpu::{Renderer, RendererOptions, ScreenDescriptor, wgpu};
use pacing::FramePacer;
use std::{
    ffi::{CString, c_char, c_void},
    sync::mpsc::{self, Receiver, SyncSender},
    time::{Duration, Instant},
};

pub struct Output {
    pub route: Route,
    pub status: String,
    pub visible: bool,
    pub frames: u64,
    pub program: Design,
    program_assets: Vec<Asset>,
    program_motions: Vec<MotionAsset>,
    texture: wgpu::Texture,
    renderer: Renderer,
    context: egui::Context,
    resources: Resources,
    font_revision: u64,
    pacer: FramePacer,
    started: Instant,
    #[cfg(target_os = "macos")]
    syphon: Option<syphon_wgpu::SyphonWgpuOutput>,
    ndi: Option<NdiWorker>,
    readback: Option<wgpu::Buffer>,
    mapped: Option<Receiver<Result<(), wgpu::BufferAsyncError>>>,
    width: u32,
    height: u32,
    stride: u32,
}
impl Output {
    pub fn start(
        route: Route,
        d: &Design,
        assets: &[Asset],
        motions: &[MotionAsset],
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<Self, String> {
        validate_design(d)?;
        if !(1..=120).contains(&route.fps) {
            return Err("Frame rate must be between 1 and 120 fps".into());
        }
        let pacer = FramePacer::new(route.fps, Instant::now());
        let width = d.width;
        let height = d.height;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Scoreboard alpha output"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let renderer = Renderer::new(
            device,
            wgpu::TextureFormat::Bgra8Unorm,
            RendererOptions {
                dithering: false,
                ..Default::default()
            },
        );
        #[cfg(target_os = "macos")]
        let syphon = if route.protocol == Protocol::Syphon {
            Some(
                syphon_wgpu::SyphonWgpuOutput::new(&route.name, device, queue, width, height)
                    .map_err(|e| e.to_string())?,
            )
        } else {
            None
        };
        #[cfg(not(target_os = "macos"))]
        if route.protocol == Protocol::Syphon {
            return Err("Syphon requires macOS".into());
        }
        let ndi = if route.protocol == Protocol::Ndi {
            Some(NdiWorker::new(
                route.name.clone(),
                width,
                height,
                route.fps,
            )?)
        } else {
            None
        };
        let stride = (width * 4).div_ceil(256) * 256;
        let readback = ndi.as_ref().map(|_| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("NDI readback"),
                size: stride as u64 * height as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            })
        });
        Ok(Self {
            route,
            program: d.clone(),
            program_assets: assets
                .iter()
                .filter(|a| d.elements.iter().any(|e| e.asset == Some(a.id)))
                .cloned()
                .collect(),
            program_motions: motions
                .iter()
                .filter(|motion| d.elements.iter().any(|e| e.motion == Some(motion.id)))
                .cloned()
                .collect(),
            status: "Starting".into(),
            visible: true,
            frames: 0,
            texture,
            renderer,
            context: egui::Context::default(),
            resources: Resources::default(),
            font_revision: u64::MAX,
            pacer,
            started: Instant::now(),
            #[cfg(target_os = "macos")]
            syphon,
            ndi,
            readback,
            mapped: None,
            width,
            height,
            stride,
        })
    }
    pub fn trigger(&mut self) {
        self.started = Instant::now();
    }
    pub fn program_assets(&self) -> &[Asset] {
        &self.program_assets
    }
    pub fn program_motions(&self) -> &[MotionAsset] {
        &self.program_motions
    }
    pub fn elapsed_seconds(&self) -> f32 {
        self.started.elapsed().as_secs_f32()
    }
    pub fn take_design(
        &mut self,
        design: Design,
        assets: &[Asset],
        motions: &[MotionAsset],
    ) -> Result<(), String> {
        if (design.width, design.height) != (self.width, self.height) {
            return Err("Stop and restart the output to change canvas dimensions".into());
        }
        self.program_assets = assets
            .iter()
            .filter(|a| design.elements.iter().any(|e| e.asset == Some(a.id)))
            .cloned()
            .collect();
        self.program_motions = motions
            .iter()
            .filter(|motion| design.elements.iter().any(|e| e.motion == Some(motion.id)))
            .cloned()
            .collect();
        self.program = design;
        self.resources = Resources::default();
        self.trigger();
        Ok(())
    }
    pub fn tick(
        &mut self,
        p: &Project,
        fonts: &scene::FontBook,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        let _ = device.poll(wgpu::PollType::Poll);
        if let Some(rx) = &self.mapped {
            match rx.try_recv() {
                Ok(result) => {
                    self.mapped = None;
                    if result.is_ok() {
                        let buffer = self.readback.as_ref().unwrap();
                        let data = match buffer.slice(..).get_mapped_range() {
                            Ok(data) => data,
                            Err(e) => {
                                self.status = format!("GPU readback: {e}");
                                buffer.unmap();
                                return;
                            }
                        };
                        let mut frame = vec![0u8; (self.width * self.height * 4) as usize];
                        for y in 0..self.height as usize {
                            frame[y * self.width as usize * 4..(y + 1) * self.width as usize * 4]
                                .copy_from_slice(
                                    &data[y * self.stride as usize
                                        ..y * self.stride as usize + self.width as usize * 4],
                                );
                        }
                        drop(data);
                        buffer.unmap();
                        if let Some(ndi) = &self.ndi {
                            match ndi.frames.try_send(frame) {
                                Ok(()) => {
                                    self.frames += 1;
                                    self.status = "Sending NDI · alpha".into();
                                }
                                Err(mpsc::TrySendError::Full(_)) => {
                                    self.status = "NDI busy · frame skipped".into()
                                }
                                Err(_) => self.status = "NDI sender stopped".into(),
                            }
                        }
                    } else {
                        self.status = "GPU readback failed".into();
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.mapped = None;
                    self.status = "GPU readback disconnected".into();
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.mapped.is_some() || !self.pacer.due(Instant::now()) {
            return;
        }
        let d = &self.program;
        if (d.width, d.height) != (self.width, self.height) {
            self.status = "Canvas size changed · restart output".into();
            return;
        }
        if self.font_revision != fonts.revision {
            self.context.set_fonts(fonts.definitions.clone());
            self.font_revision = fonts.revision;
        }
        self.resources
            .sync(&self.context, &self.program_assets, &self.program_motions);
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(self.width as f32, self.height as f32),
            )),
            ..Default::default()
        };
        let elapsed = self.started.elapsed().as_secs_f32();
        self.resources.advance_motion(Some(elapsed));
        let mut full = self.context.run_ui(input, |ui| {
            if self.visible {
                scene::paint_scene(
                    ui.painter(),
                    d,
                    &p.values,
                    &self.resources,
                    egui::Pos2::ZERO,
                    1.,
                    Some(elapsed),
                );
            }
        });
        let jobs = self.context.tessellate(full.shapes, 1.);
        for (id, deltas) in &full.textures_delta.set {
            for delta in deltas {
                self.renderer.update_texture(device, queue, *id, delta);
            }
        }
        let screen = ScreenDescriptor {
            size_in_pixels: [self.width, self.height],
            pixels_per_point: 1.,
        };
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Scoreboard frame"),
        });
        let commands = self
            .renderer
            .update_buffers(device, queue, &mut encoder, &jobs, &screen);
        let view = self.texture.create_view(&Default::default());
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Transparent scoreboard"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            self.renderer
                .render(&mut pass.forget_lifetime(), &jobs, &screen);
        }
        if let Some(buffer) = &self.readback {
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(self.stride),
                        rows_per_image: Some(self.height),
                    },
                },
                wgpu::Extent3d {
                    width: self.width,
                    height: self.height,
                    depth_or_array_layers: 1,
                },
            );
        }
        queue.submit(commands.into_iter().chain([encoder.finish()]));
        for id in &full.textures_delta.free {
            self.renderer.free_texture(id);
        }
        // egui 0.36 checks that texture deltas have been explicitly consumed.
        // They have all been uploaded/freed above; acknowledge that before drop.
        full.textures_delta.clear();
        #[cfg(target_os = "macos")]
        if let Some(s) = &mut self.syphon {
            let status = s.publish(&self.texture, device, queue);
            self.status = match status {
                syphon_wgpu::PublishStatus::ZeroCopy => {
                    self.frames += 1;
                    "Syphon · GPU sharing"
                }
                syphon_wgpu::PublishStatus::NoClients => "Syphon · waiting for receiver",
                syphon_wgpu::PublishStatus::CpuFallback => {
                    self.frames += 1;
                    "Syphon · CPU fallback"
                }
                syphon_wgpu::PublishStatus::PoolExhausted => "Syphon · GPU busy",
            }
            .into();
        }
        if let Some(buffer) = &self.readback {
            let (tx, rx) = mpsc::channel();
            buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                let _ = tx.send(r);
            });
            self.mapped = Some(rx);
        }
    }
}
pub fn unpremultiply(data: &mut [u8]) {
    for p in data.chunks_exact_mut(4) {
        let a = p[3] as u32;
        if a == 0 {
            p[0] = 0;
            p[1] = 0;
            p[2] = 0;
        } else if a < 255 {
            for c in &mut p[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
}

#[cfg(target_os = "macos")]
pub fn verify_syphon() -> Result<(), String> {
    verify_syphon_named("Overlay Studio")
}
#[cfg(target_os = "macos")]
pub fn verify_syphon_named(filter: &str) -> Result<(), String> {
    unsafe extern "C" {
        static kCFRunLoopDefaultMode: *const c_void;
        fn CFRunLoopRunInMode(mode: *const c_void, seconds: f64, return_after_source: bool) -> i32;
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    let info = loop {
        if let Some(info) = syphon_core::SyphonServerDirectory::servers()
            .into_iter()
            .find(|s| s.name.contains(filter))
        {
            break info;
        }
        if Instant::now() > deadline {
            return Err("No Overlay Studio Syphon output found. Start one in Live Desk.".into());
        }
        unsafe {
            CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.05, false);
        }
    };
    println!("Discovered Syphon: {}", info.name);
    let client = syphon_core::SyphonClient::connect_by_info(&info).map_err(|e| e.to_string())?;
    loop {
        if let Some(mut frame) = client.try_receive().map_err(|e| e.to_string())? {
            let bytes = frame.to_vec().map_err(|e| e.to_string())?;
            let min = bytes.chunks_exact(4).map(|p| p[3]).min().unwrap_or(255);
            let max = bytes.chunks_exact(4).map(|p| p[3]).max().unwrap_or(0);
            if min != 0 || max < 200 {
                return Err(format!("Unexpected alpha range {min}–{max}"));
            }
            println!(
                "Received {} × {} BGRA; alpha range {min}–{max}",
                frame.width, frame.height
            );
            return Ok(());
        }
        if Instant::now() > deadline {
            return Err("Syphon connected but no frame received.".into());
        }
        unsafe {
            CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.05, false);
        }
    }
}

#[repr(C)]
struct SendCreate {
    name: *const c_char,
    groups: *const c_char,
    clock_video: bool,
    clock_audio: bool,
}
#[repr(C)]
struct VideoFrame {
    x: i32,
    y: i32,
    fourcc: u32,
    fps_n: i32,
    fps_d: i32,
    aspect: f32,
    format: i32,
    timecode: i64,
    data: *mut u8,
    stride: i32,
    metadata: *const c_char,
    timestamp: i64,
}
type Create = unsafe extern "C" fn(*const SendCreate) -> *mut c_void;
type SendVideo = unsafe extern "C" fn(*mut c_void, *const VideoFrame);
type Destroy = unsafe extern "C" fn(*mut c_void);
pub struct NdiWorker {
    frames: SyncSender<Vec<u8>>,
}
impl NdiWorker {
    fn new(name: String, width: u32, height: u32, fps: u32) -> Result<Self, String> {
        let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(2);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let result =
                (|| -> Result<(libloading::Library, *mut c_void, SendVideo, Destroy), String> {
                    let mut paths = vec![];
                    if let Ok(path) = std::env::var("OVERLAY_NDI_LIBRARY") {
                        paths.push(path);
                    }
                    paths.extend(
                        [
                            "/usr/local/lib/libndi.dylib",
                            "/Library/NDI SDK for Apple/lib/macOS/libndi.dylib",
                            "/Applications/Resolume Arena/libndi.dylib",
                            "libndi.dylib",
                            "libndi.so.6",
                            "Processing.NDI.Lib.x64.dll",
                        ]
                        .into_iter()
                        .map(str::to_string),
                    );
                    let library=paths.iter().find_map(|p|unsafe{libloading::Library::new(p).ok()}).ok_or("NDI runtime not found. Install NDI Tools or set OVERLAY_NDI_LIBRARY to its library path.")?;
                    // Symbols and C layouts follow NDI's public SDK. The library stays alive through sender destruction.
                    unsafe {
                        let initialize = library
                            .get::<unsafe extern "C" fn() -> bool>(b"NDIlib_initialize\0")
                            .map_err(|e| e.to_string())?;
                        if !initialize() {
                            return Err("NDI initialization failed".into());
                        }
                        let create = *library
                            .get::<Create>(b"NDIlib_send_create\0")
                            .map_err(|e| e.to_string())?;
                        let send = *library
                            .get::<SendVideo>(b"NDIlib_send_send_video_v2\0")
                            .map_err(|e| e.to_string())?;
                        let destroy = *library
                            .get::<Destroy>(b"NDIlib_send_destroy\0")
                            .map_err(|e| e.to_string())?;
                        let name = CString::new(name).map_err(|e| e.to_string())?;
                        let instance = create(&SendCreate {
                            name: name.as_ptr(),
                            groups: c"Public".as_ptr(),
                            clock_video: false,
                            clock_audio: false,
                        });
                        if instance.is_null() {
                            return Err("NDI sender could not be created".into());
                        }
                        Ok((library, instance, send, destroy))
                    }
                })();
            match result {
                Ok((_library, instance, send, destroy)) => {
                    let _ = ready_tx.send(Ok(()));
                    while let Ok(mut bytes) = rx.recv() {
                        // CPU alpha conversion belongs with the network worker,
                        // never on the main-thread GPU/UI scheduler.
                        unpremultiply(&mut bytes);
                        let frame = VideoFrame {
                            x: width as i32,
                            y: height as i32,
                            fourcc: u32::from_le_bytes(*b"BGRA"),
                            fps_n: fps as i32,
                            fps_d: 1,
                            aspect: width as f32 / height as f32,
                            format: 1,
                            timecode: i64::MAX,
                            data: bytes.as_mut_ptr(),
                            stride: (width * 4) as i32,
                            metadata: std::ptr::null(),
                            timestamp: 0,
                        };
                        unsafe {
                            send(instance, &frame);
                        }
                    }
                    unsafe {
                        destroy(instance);
                    }
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                }
            }
        });
        ready_rx
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| format!("NDI startup: {e}"))??;
        Ok(Self { frames: tx })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ndi_alpha_conversion() {
        let mut data = [64, 32, 16, 128, 12, 12, 12, 0, 1, 2, 3, 255];
        unpremultiply(&mut data);
        assert_eq!(data, [128, 64, 32, 128, 0, 0, 0, 0, 1, 2, 3, 255]);
    }
    #[test]
    fn ffi_layout() {
        assert_eq!(std::mem::size_of::<VideoFrame>(), 72);
        assert_eq!(std::mem::size_of::<SendCreate>(), 24);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn gpu_output_preserves_transparency() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::METAL,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        let (device, queue) =
            pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let mut p = Project::fresh();
        let d = &mut p.designs[0];
        d.width = 320;
        d.height = 180;
        for e in &mut d.elements {
            e.x /= 6.;
            e.y /= 6.;
            e.w /= 6.;
            e.h /= 6.;
            e.font_size /= 6.;
            e.radius /= 6.;
        }
        let mut route = p.routes[0].clone();
        route.name = format!("Overlay Studio regression {}", std::process::id());
        let mut output =
            Output::start(route, &p.designs[0], &p.assets, &p.motions, &device, &queue).unwrap();
        output.started = Instant::now() - Duration::from_secs(2);
        output.tick(&p, &scene::FontBook::new(), &device, &queue);
        let stride = 1280;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Regression readback"),
            size: stride * 180,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &output.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride as u32),
                    rows_per_image: Some(180),
                },
            },
            wgpu::Extent3d {
                width: 320,
                height: 180,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);
        let (tx, rx) = mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(5)),
            })
            .unwrap();
        rx.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
        let bytes = buffer.slice(..).get_mapped_range().unwrap();
        assert_eq!(
            &bytes[..4],
            &[0, 0, 0, 0],
            "Outside artwork must be transparent"
        );
        assert!(
            bytes.chunks_exact(4).filter(|p| p[3] > 128).count() > 1000,
            "Artwork must render opaque pixels"
        );
        assert!(
            bytes.chunks_exact(4).filter(|p| p[3] == 0).count() > 40000,
            "Most of this overlay must remain transparent"
        );
        drop(bytes);
        buffer.unmap();
        output.pacer = FramePacer::new(output.route.fps, Instant::now());
        output.visible = false;
        output.tick(&p, &scene::FontBook::new(), &device, &queue);
    }
}
