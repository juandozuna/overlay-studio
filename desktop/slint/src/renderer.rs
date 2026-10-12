use egui_wgpu::{Renderer, RendererOptions, ScreenDescriptor, wgpu};
use overlay_studio::{
    application::document_session::DocumentSession,
    model::{Asset, Design, MotionAsset},
    scene::{self, FontBook, Resources},
};
use std::collections::BTreeMap;

pub struct SceneFrame<'a> {
    pub design: &'a Design,
    pub values: &'a BTreeMap<String, String>,
    pub assets: &'a [Asset],
    pub motions: &'a [MotionAsset],
    pub revision: u64,
    pub generation: u64,
    pub view_key: u64,
    pub elapsed: Option<f32>,
    pub visible: bool,
}

/// Native scene painter on Slint's device. Preview pixels stay on the GPU.
pub struct CanvasRenderer {
    texture: wgpu::Texture,
    renderer: Renderer,
    context: egui::Context,
    resources: Resources,
    fonts: FontBook,
    size: [u32; 2],
    revision: u64,
    generation: u64,
    view_key: u64,
    animated: bool,
    pub warning: Option<String>,
}
fn target(device: &wgpu::Device, size: [u32; 2]) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Slint canvas"),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}
impl CanvasRenderer {
    pub fn new(device: &wgpu::Device) -> Self {
        let size = [1920, 1080];
        Self {
            texture: target(device, size),
            renderer: Renderer::new(
                device,
                wgpu::TextureFormat::Rgba8Unorm,
                RendererOptions {
                    dithering: false,
                    ..Default::default()
                },
            ),
            context: egui::Context::default(),
            resources: Resources::default(),
            fonts: FontBook::new(),
            size,
            revision: 0,
            generation: 0,
            view_key: 0,
            animated: false,
            warning: None,
        }
    }
    pub fn image(&self) -> Result<slint::Image, String> {
        slint::Image::try_from(self.texture.clone()).map_err(|e| e.to_string())
    }
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        session: &DocumentSession,
    ) -> Result<bool, String> {
        self.render_scene(
            device,
            queue,
            SceneFrame {
                design: session.active(),
                values: &session.project.values,
                assets: &session.project.assets,
                motions: &session.project.motions,
                revision: session.revision,
                generation: session.generation,
                view_key: session.design as u64,
                elapsed: None,
                visible: true,
            },
        )
    }
    pub fn render_scene(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: SceneFrame<'_>,
    ) -> Result<bool, String> {
        if self.revision == scene.revision
            && self.generation == scene.generation
            && self.view_key == scene.view_key
            && scene.elapsed.is_none()
            && !self.animated
        {
            return Ok(false);
        }
        if self.generation != scene.generation {
            self.resources = Resources::default();
            self.generation = scene.generation;
        }
        let design = scene.design;
        let size = [design.width, design.height];
        let resized = size != self.size;
        if resized {
            self.texture = target(device, size);
            self.size = size;
        }
        let mut fonts_changed = false;
        self.warning = None;
        for e in &design.elements {
            match self.fonts.ensure(&e.font) {
                Ok(changed) => fonts_changed |= changed,
                Err(error) => self.warning = Some(error),
            };
        }
        if fonts_changed {
            self.context.set_fonts(self.fonts.definitions.clone());
        }
        self.resources
            .sync(&self.context, scene.assets, scene.motions);
        self.resources.advance_motion(scene.elapsed);
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(size[0] as f32, size[1] as f32),
            )),
            ..Default::default()
        };
        let mut frame = self.context.run_ui(input, |ui| {
            if !scene.visible {
                return;
            }
            scene::paint_scene(
                ui.painter(),
                design,
                scene.values,
                &self.resources,
                egui::Pos2::ZERO,
                1.,
                scene.elapsed,
            )
        });
        let jobs = self.context.tessellate(frame.shapes, 1.);
        for (id, deltas) in &frame.textures_delta.set {
            for delta in deltas {
                self.renderer.update_texture(device, queue, *id, delta);
            }
        }
        let screen = ScreenDescriptor {
            size_in_pixels: size,
            pixels_per_point: 1.,
        };
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Slint canvas scene"),
        });
        let commands = self
            .renderer
            .update_buffers(device, queue, &mut encoder, &jobs, &screen);
        let view = self.texture.create_view(&Default::default());
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Transparent canvas"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.renderer
                .render(&mut pass.forget_lifetime(), &jobs, &screen);
        }
        queue.submit(commands.into_iter().chain([encoder.finish()]));
        for id in &frame.textures_delta.free {
            self.renderer.free_texture(id);
        }
        frame.textures_delta.clear();
        self.revision = scene.revision;
        self.view_key = scene.view_key;
        self.animated = scene.elapsed.is_some();
        Ok(resized)
    }
}
