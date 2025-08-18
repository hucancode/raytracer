use egui::Context;
use egui_wgpu::ScreenDescriptor;
use egui_winit::State as EguiState;
use wgpu::{CommandEncoder, Device, Queue, TextureFormat, TextureView};
use winit::event::WindowEvent;
use winit::window::Window;

pub struct GuiState {
    state: EguiState,
    renderer: egui_wgpu::Renderer,
    pub show_debug: bool,
    pub camera_radius: f32,
    pub camera_theta: f32,
    pub camera_phi: f32,
    pub fov: f32,
}

impl GuiState {
    pub fn new(
        device: &Device,
        format: TextureFormat,
        window: &Window,
        samples: u32,
    ) -> Self {
        let viewport_id = egui::ViewportId::ROOT;
        let state = EguiState::new(Context::default(), viewport_id, window, None, None, None);
        
        let renderer = egui_wgpu::Renderer::new(device, format, None, samples, false);
        
        Self {
            state,
            renderer,
            show_debug: true,
            camera_radius: 10.0,
            camera_theta: 0.0,
            camera_phi: 0.0,
            fov: 60.0,
        }
    }

    pub fn handle_event(&mut self, window: &Window, event: &WindowEvent) -> bool {
        let response = self.state.on_window_event(window, event);
        response.consumed
    }

    pub fn update(&mut self, window: &Window) -> egui::FullOutput {
        let input = self.state.take_egui_input(window);
        let mut show_debug = self.show_debug;
        let mut camera_radius = self.camera_radius;
        let mut camera_theta = self.camera_theta;
        let mut camera_phi = self.camera_phi;
        let mut fov = self.fov;
        
        let output = self.state.egui_ctx().run(input, |ctx| {
            egui::Window::new("Debug Panel")
                .open(&mut show_debug)
                .show(ctx, |ui| {
                    ui.heading("Camera Controls");
                    
                    ui.add(egui::Slider::new(&mut camera_radius, 1.0..=50.0)
                        .text("Radius"));
                    
                    ui.add(egui::Slider::new(&mut camera_theta, -180.0..=180.0)
                        .text("Theta (deg)")
                        .suffix("°"));
                    
                    ui.add(egui::Slider::new(&mut camera_phi, -89.0..=89.0)
                        .text("Phi (deg)")
                        .suffix("°"));
                    
                    ui.add(egui::Slider::new(&mut fov, 30.0..=120.0)
                        .text("FOV")
                        .suffix("°"));
                    
                    ui.separator();
                    
                    ui.label(egui::RichText::new("Controls:").strong());
                    ui.label("Left Mouse: Orbit camera");
                    ui.label("Scroll: Zoom in/out");
                });
        });
        
        // Update self with the modified values if they changed
        if self.camera_radius != camera_radius || 
           self.camera_theta != camera_theta || 
           self.camera_phi != camera_phi || 
           self.fov != fov {
            println!("GUI values changed: radius={}, theta={}, phi={}, fov={}", 
                     camera_radius, camera_theta, camera_phi, fov);
        }
        
        self.show_debug = show_debug;
        self.camera_radius = camera_radius;
        self.camera_theta = camera_theta;
        self.camera_phi = camera_phi;
        self.fov = fov;
        
        output
    }

    pub fn render(
        &mut self,
        device: &Device,
        queue: &Queue,
        encoder: &mut CommandEncoder,
        window: &Window,
        surface_view: &TextureView,
        output: egui::FullOutput,
    ) {
        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [window.inner_size().width, window.inner_size().height],
            pixels_per_point: window.scale_factor() as f32,
        };

        self.state.handle_platform_output(window, output.platform_output);

        let paint_jobs = self.state.egui_ctx().tessellate(output.shapes, output.pixels_per_point);

        for (id, image_delta) in &output.textures_delta.set {
            self.renderer.update_texture(device, queue, *id, &image_delta);
        }

        self.renderer.update_buffers(device, queue, encoder, &paint_jobs, &screen_descriptor);

        {
            let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: surface_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            let mut render_pass = render_pass.forget_lifetime();
            self.renderer.render(&mut render_pass, &paint_jobs, &screen_descriptor);
        }

        for id in &output.textures_delta.free {
            self.renderer.free_texture(id);
        }
    }
}