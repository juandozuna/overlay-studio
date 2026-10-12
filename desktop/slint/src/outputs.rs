//! Native sender lifecycle. Kept on the main thread for the macOS Syphon adapter.
use egui_wgpu::wgpu;
use overlay_studio::{
    application::game_commands::OutputRequest, model::Project, output::Output, scene::FontBook,
};

pub struct OutputManager {
    pub senders: Vec<Option<Output>>,
    gpu: Option<(wgpu::Device, wgpu::Queue)>,
    fonts: FontBook,
}
impl OutputManager {
    pub fn new(count: usize) -> Self {
        Self {
            senders: (0..count).map(|_| None).collect(),
            gpu: None,
            fonts: FontBook::new(),
        }
    }
    pub fn install_gpu(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.gpu = Some((device.clone(), queue.clone()));
    }
    pub fn stop_all(&mut self) {
        for sender in &mut self.senders {
            *sender = None;
        }
    }
    pub fn reset(&mut self, count: usize) {
        self.stop_all();
        self.senders = (0..count).map(|_| None).collect();
    }
    pub fn is_running(&self, index: usize) -> bool {
        self.senders.get(index).is_some_and(Option::is_some)
    }
    pub fn toggle(&mut self, project: &Project, index: usize) -> Result<(), String> {
        let route = project.routes.get(index).ok_or("Output does not exist")?;
        self.senders.resize_with(project.routes.len(), || None);
        if self.is_running(index) {
            self.senders[index] = None;
            return Ok(());
        }
        let (device, queue) = self.gpu.as_ref().ok_or("GPU is still initializing")?;
        let design = project
            .designs
            .get(route.design)
            .ok_or("Design does not exist")?;
        self.senders[index] = Some(Output::start(
            route.clone(),
            design,
            &project.assets,
            &project.motions,
            device,
            queue,
        )?);
        Ok(())
    }
    pub fn apply(&mut self, project: &mut Project, request: OutputRequest) -> Result<(), String> {
        let index = match request {
            OutputRequest::ToggleVisibility { route }
            | OutputRequest::TakeDesign { route, .. }
            | OutputRequest::ReplayAnimation { route } => route,
        };
        let sender = self
            .senders
            .get_mut(index)
            .and_then(Option::as_mut)
            .ok_or("Start the output first")?;
        match request {
            OutputRequest::ToggleVisibility { .. } => {
                sender.visible = !sender.visible;
                sender.trigger();
            }
            OutputRequest::ReplayAnimation { .. } => sender.trigger(),
            OutputRequest::TakeDesign { design, .. } => {
                let d = project
                    .designs
                    .get(design)
                    .ok_or("Design does not exist")?
                    .clone();
                sender.take_design(d, &project.assets, &project.motions)?;
                sender.route.design = design;
                project.routes[index].design = design;
            }
        }
        Ok(())
    }
    pub fn tick(&mut self, project: &Project) -> Vec<String> {
        let Some((device, queue)) = &self.gpu else {
            return vec![];
        };
        let mut errors = vec![];
        for sender in self.senders.iter_mut().flatten() {
            for layer in &sender.program.elements {
                if let Err(error) = self.fonts.ensure(&layer.font) {
                    errors.push(error);
                }
            }
            sender.tick(project, &self.fonts, device, queue);
        }
        errors
    }
}
