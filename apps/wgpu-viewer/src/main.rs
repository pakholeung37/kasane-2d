//! Small window host for viewing a real model with the same WGPU pipeline.
use std::error::Error;
use std::future::Future;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use kasane_render_wgpu::{WgpuEncodeTarget, WgpuOutputMode, WgpuRenderer, WgpuTargetConfig};
use kasane_wgpu_validate::{checked, Case, LoadedModel, TextureSet};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        if let std::task::Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

struct ViewerState {
    window: Arc<Window>,
    instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    device: wgpu::Device,
    queue: wgpu::Queue,
    case: Case,
    model: LoadedModel,
    textures: TextureSet,
    renderer: WgpuRenderer,
}

impl ViewerState {
    fn fit(&self, width: u32, height: u32) -> f32 {
        self.case.fit_long_side
            * (width as f32 / self.case.width as f32).min(height as f32 / self.case.height as f32)
    }

    fn new(window: Arc<Window>, case: Case) -> Result<Self, Box<dyn Error>> {
        let size = window.inner_size();
        let width = size.width.max(1);
        let height = size.height.max(1);
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone())?;
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))?;
        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))?;
        let mut config = surface
            .get_default_config(&adapter, width, height)
            .ok_or("No compatible surface configuration")?;
        let capabilities = surface.get_capabilities(&adapter);
        if let Some(format) = capabilities
            .formats
            .iter()
            .copied()
            .find(|format| format.is_srgb())
        {
            config.format = format;
        }
        let model = LoadedModel::new(&case)?;
        let textures = TextureSet::upload(
            &device,
            &queue,
            &model,
            case.texture_profile == "linear_mipmap",
        )?;
        let catalog = textures.catalog();
        let mut renderer = checked(WgpuRenderer::new(
            &device,
            WgpuTargetConfig {
                width,
                height,
                format: config.format,
            },
        ))?;
        checked(renderer.sync_model(&device, &model.frame, &catalog))?;
        let fit = case.fit_long_side
            * (width as f32 / case.width as f32).min(height as f32 / case.height as f32);
        checked(renderer.update_view(&device, model.view(width, height, fit).0))?;
        surface.configure(&device, &config);
        window.set_title(&format!(
            "Kasane WGPU Viewer — {}",
            case.model3
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        ));
        Ok(Self {
            window,
            instance,
            surface,
            config,
            device,
            queue,
            case,
            model,
            textures,
            renderer,
        })
    }

    fn resize(&mut self, width: u32, height: u32) -> Result<(), Box<dyn Error>> {
        if width == 0 || height == 0 || (width == self.config.width && height == self.config.height)
        {
            return Ok(());
        }
        let mut config = self.config.clone();
        config.width = width;
        config.height = height;
        // Prepare the replacement before changing the surface, so a rejected
        // layout leaves the last displayed scene intact.
        let mut renderer = checked(WgpuRenderer::new(
            &self.device,
            WgpuTargetConfig {
                width,
                height,
                format: config.format,
            },
        ))?;
        let catalog = self.textures.catalog();
        checked(renderer.sync_model(&self.device, &self.model.frame, &catalog))?;
        checked(renderer.update_view(
            &self.device,
            self.model.view(width, height, self.fit(width, height)).0,
        ))?;
        self.surface.configure(&self.device, &config);
        self.config = config;
        self.renderer = renderer;
        self.window.request_redraw();
        Ok(())
    }

    fn render(&mut self) -> Result<bool, Box<dyn Error>> {
        let (frame, suboptimal) = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => (frame, false),
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => (frame, true),
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(false)
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                self.window.request_redraw();
                return Ok(false);
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.surface = self.instance.create_surface(self.window.clone())?;
                self.surface.configure(&self.device, &self.config);
                self.window.request_redraw();
                return Ok(false);
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err("Surface validation failed".into())
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("kasane.viewer.encoder"),
            });
        let catalog = self.textures.catalog();
        checked(self.renderer.encode(
            WgpuEncodeTarget {
                device: &self.device,
                queue: &self.queue,
                encoder: &mut encoder,
                output: &view,
                output_mode: WgpuOutputMode::Replace,
            },
            &catalog,
        ))?;
        self.queue.submit([encoder.finish()]);
        frame.present();
        if suboptimal {
            self.surface.configure(&self.device, &self.config);
        }
        Ok(true)
    }
}

struct App {
    case: Case,
    state: Option<ViewerState>,
    frames_remaining: Option<u32>,
    failure: Option<String>,
    smoke_resize: bool,
    resize_requested: bool,
    resize_observed: bool,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        eprintln!("Viewer: creating window and GPU resources");
        let attributes = Window::default_attributes()
            .with_title("Kasane WGPU Viewer")
            .with_inner_size(LogicalSize::new(800.0, 800.0));
        let result = event_loop
            .create_window(attributes)
            .map(Arc::new)
            .map_err(|error| -> Box<dyn Error> { Box::new(error) })
            .and_then(|window| ViewerState::new(window, self.case.clone()));
        match result {
            Ok(state) => {
                state.window.request_redraw();
                self.state = Some(state);
                eprintln!("Viewer: ready for first redraw");
            }
            Err(error) => {
                eprintln!("Viewer initialization failed: {error}");
                self.failure = Some(error.to_string());
                event_loop.exit();
            }
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if self.frames_remaining.is_some() && (!self.resize_requested || self.resize_observed) {
            if let Some(state) = &self.state {
                state.window.request_redraw();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        if id != state.window.id() {
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput { event, .. }
                if event.logical_key == Key::Named(NamedKey::Escape) =>
            {
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                let changed =
                    size.width != state.config.width || size.height != state.config.height;
                if let Err(error) = state.resize(size.width, size.height) {
                    eprintln!("Viewer resize rejected: {error}");
                } else if self.resize_requested && changed {
                    self.resize_observed = true;
                }
            }
            WindowEvent::RedrawRequested => match state.render() {
                Ok(true) => {
                    if let Some(remaining) = self.frames_remaining.as_mut() {
                        if *remaining == 0 {
                            return;
                        }
                        *remaining -= 1;
                        if self.smoke_resize && !self.resize_requested {
                            self.resize_requested = true;
                            let _ = state.window.request_inner_size(PhysicalSize::new(
                                state.config.width + 64,
                                state.config.height + 64,
                            ));
                        }
                        if *remaining == 0 {
                            println!("presented frame");
                            event_loop.exit();
                        }
                    }
                }
                Ok(false) => {}
                Err(error) => {
                    eprintln!("Viewer rendering failed: {error}");
                    self.failure = Some(error.to_string());
                    event_loop.exit();
                }
            },
            _ => {}
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let path = args
        .next()
        .ok_or("usage: viewer CASE_JSON [--frames N] [--smoke-resize]")?;
    let mut frames_remaining = None;
    let mut smoke_resize = false;
    while let Some(arg) = args.next() {
        if arg == "--frames" && frames_remaining.is_none() {
            let count = args.next().ok_or("--frames requires a positive count")?;
            let count = count.to_string_lossy().parse::<u32>()?;
            if count == 0 {
                return Err("--frames requires a positive count".into());
            }
            frames_remaining = Some(count);
        } else if arg == "--smoke-resize" && !smoke_resize {
            smoke_resize = true;
        } else {
            return Err("usage: viewer CASE_JSON [--frames N] [--smoke-resize]".into());
        }
    }
    if smoke_resize && frames_remaining != Some(2) {
        return Err("--smoke-resize requires --frames 2".into());
    }
    let case = Case::read(Path::new(&path))?;
    let mut app = App {
        case,
        state: None,
        frames_remaining,
        failure: None,
        smoke_resize,
        resize_requested: false,
        resize_observed: false,
    };
    let event_loop = EventLoop::new()?;
    if app.frames_remaining.is_some() {
        event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
    }
    event_loop.run_app(&mut app)?;
    if let Some(error) = app.failure {
        return Err(error.into());
    }
    if let Some(remaining) = app.frames_remaining.filter(|remaining| *remaining > 0) {
        return Err(format!("Viewer exited with {remaining} unpresented smoke frames").into());
    }
    if app.smoke_resize && !app.resize_observed {
        return Err("Viewer resize smoke did not observe a resized surface".into());
    }
    Ok(())
}
