use crate::camera_controller::OrbitCamera;
use crate::gui::GuiState;
use crate::renderer::RenderOutput;
use crate::scene::{Scene, SceneSphere, SceneTris};
use rand::Rng;
use std::sync::Arc;
use std::{i8, time::Instant};
use winit::application::ApplicationHandler;
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

pub struct App {
    scene: Option<Box<dyn Scene>>,
    window: Option<Arc<Window>>,
    scene_id: i8,
    start_time_stamp: Instant,
    camera: Option<OrbitCamera>,
    gui: Option<GuiState>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            start_time_stamp: Instant::now(),
            scene_id: 0,
            scene: None,
            window: None,
            camera: None,
            gui: None,
        }
    }
}

impl App {
    pub fn parse_args(&mut self, args: Vec<String>) {
        let mut rng = rand::thread_rng();
        let j = rng.gen_range(1..=7);
        let i = args.get(1).map_or(j, |s| s.parse::<i8>().unwrap_or(j));
        self.scene_id = i;
    }
    async fn build_scene(&mut self) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        let render_output = RenderOutput::Window(window.clone());
        let mut scene: Box<dyn Scene> = match self.scene_id {
            2 => Box::new(SceneSphere::new(render_output).await),
            3 => Box::new(SceneTris::new_quad(render_output).await),
            4 => Box::new(SceneTris::new_cube(render_output).await),
            5 => Box::new(SceneTris::new_suzane(render_output).await),
            6 => Box::new(SceneTris::new_lucy(render_output).await),
            7 => Box::new(SceneTris::new_dragon(render_output).await),
            _ => Box::new(SceneSphere::new_simple(render_output).await),
        };
        scene.init();
        self.scene = Some(scene);
        let size = window.inner_size();
        let aspect_ratio = size.width as f32 / size.height as f32;
        self.camera = Some(OrbitCamera::new(aspect_ratio));
        
        // Initialize GUI with camera values
        if let (Some(scene), Some(camera)) = (&self.scene, &self.camera) {
            let mut gui = GuiState::new(
                scene.get_device(),
                scene.get_format(),
                window,
            );
            
            // Sync GUI values with camera
            gui.camera_radius = camera.radius;
            gui.camera_theta = camera.theta.to_degrees();
            gui.camera_phi = camera.phi.to_degrees();
            gui.fov = camera.fov.to_degrees();
            gui.blur_amount = camera.focal_blur_amount;
            gui.focal_length = camera.focal_length;

            self.gui = Some(gui);
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(Window::default_attributes())
                .unwrap(),
        );
        self.window = Some(window);
        pollster::block_on(self.build_scene());
    }
    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        if cause == StartCause::Poll {
            let time = self.start_time_stamp.elapsed().as_millis() as u32;
            if let Some(scene) = self.scene.as_mut() {
                scene.set_time(time);
            }
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
        }
    }
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        if event == WindowEvent::CloseRequested {
            event_loop.exit();
            return;
        }
        
        // Handle GUI events first
        let mut gui_consumed = false;
        if let (Some(gui), Some(window)) = (&mut self.gui, &self.window) {
            gui_consumed = gui.handle_event(window, &event);
        }
        
        // If GUI didn't consume the event, handle it normally
        if !gui_consumed {
            if let Some(scene) = self.scene.as_mut() {
                match event {
                    WindowEvent::RedrawRequested => {
                        // Update camera from GUI values
                        if let (Some(camera), Some(gui)) = (&mut self.camera, &self.gui) {
                            // Check if GUI values actually changed
                            let new_radius = gui.camera_radius;
                            let new_theta = gui.camera_theta.to_radians();
                            let new_phi = gui.camera_phi.to_radians();
                            let new_fov = gui.fov.to_radians();
                            let new_blur = gui.blur_amount;
                            let new_focal_length = gui.focal_length;

                            let epsilon = 1e-6;
                            let changed = (camera.radius - new_radius).abs() > epsilon
                                || (camera.theta - new_theta).abs() > epsilon
                                || (camera.phi - new_phi).abs() > epsilon
                                || (camera.fov - new_fov).abs() > epsilon
                                || (camera.focal_blur_amount - new_blur).abs() > epsilon
                                || (camera.focal_length - new_focal_length).abs() > epsilon;

                            if changed {
                                camera.radius = new_radius;
                                camera.theta = new_theta;
                                camera.phi = new_phi;
                                camera.fov = new_fov;
                                camera.focal_blur_amount = new_blur;
                                camera.focal_length = new_focal_length;
                                camera.update_position();
                                scene.reset_frame_count();
                                camera.reset_movement_flag();
                            } else if camera.has_moved {
                                scene.reset_frame_count();
                                camera.reset_movement_flag();
                            }
                            
                            scene.update_camera(camera.to_uniform());
                        }
                        
                        // Draw scene with or without GUI
                        if let (Some(gui), Some(window)) = (&mut self.gui, &self.window) {
                            scene.draw_with_gui(gui, window);
                        } else {
                            scene.draw();
                        }
                    }
                    WindowEvent::Resized(size) => {
                        scene.resize(size.width, size.height);
                        if let Some(camera) = self.camera.as_mut() {
                            camera.resize(size.width, size.height);
                        }
                    }
                    WindowEvent::MouseInput { state, button, .. } => {
                        if let Some(camera) = self.camera.as_mut() {
                            camera.handle_mouse_input(state, button);
                            // Update GUI values when camera moves
                            if let Some(gui) = self.gui.as_mut() {
                                gui.camera_radius = camera.radius;
                                gui.camera_theta = camera.theta.to_degrees();
                                gui.camera_phi = camera.phi.to_degrees();
                                gui.blur_amount = camera.focal_blur_amount;
                                gui.focal_length = camera.focal_length;
                            }
                        }
                    }
                    WindowEvent::CursorMoved { position, .. } => {
                        if let Some(camera) = self.camera.as_mut() {
                            camera.handle_mouse_motion(position);
                            // Update GUI values when camera moves
                            if let Some(gui) = self.gui.as_mut() {
                                gui.camera_radius = camera.radius;
                                gui.camera_theta = camera.theta.to_degrees();
                                gui.camera_phi = camera.phi.to_degrees();
                                gui.blur_amount = camera.focal_blur_amount;
                                gui.focal_length = camera.focal_length;
                            }
                        }
                    }
                    WindowEvent::MouseWheel { delta, .. } => {
                        if let Some(camera) = self.camera.as_mut() {
                            camera.handle_scroll(delta);
                            // Update GUI values when camera moves
                            if let Some(gui) = self.gui.as_mut() {
                                gui.camera_radius = camera.radius;
                                gui.camera_theta = camera.theta.to_degrees();
                                gui.camera_phi = camera.phi.to_degrees();
                                gui.blur_amount = camera.focal_blur_amount;
                                gui.focal_length = camera.focal_length;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}
