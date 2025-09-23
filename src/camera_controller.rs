use glam::{Vec3, Vec4};
use winit::event::{ElementState, MouseButton, MouseScrollDelta};
use winit::dpi::PhysicalPosition;

pub struct OrbitCamera {
    pub position: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fov: f32,
    pub aspect_ratio: f32,
    
    // Orbit controls
    pub radius: f32,
    pub theta: f32, // Horizontal angle
    pub phi: f32,   // Vertical angle
    pub focal_length: f32,
    pub focal_blur_amount: f32,

    // Mouse state
    is_dragging: bool,
    last_mouse_pos: PhysicalPosition<f64>,
    
    // Control sensitivity
    pub zoom_speed: f32,
    pub orbit_speed: f32,
    pub min_radius: f32,
    pub max_radius: f32,
    
    // Track if camera has moved
    pub has_moved: bool,
}

impl OrbitCamera {
    pub fn new(aspect_ratio: f32) -> Self {
        let radius = 5.0;
        let theta = 0.0;
        let phi = std::f32::consts::PI / 4.0;
        
        let mut camera = Self {
            position: Vec3::ZERO,
            target: Vec3::ZERO,
            up: Vec3::Y,
            fov: 45.0_f32.to_radians(),
            aspect_ratio,
            radius,
            theta,
            phi,
            focal_length: 10.0,
            focal_blur_amount: 0.0,
            is_dragging: false,
            last_mouse_pos: PhysicalPosition::new(0.0, 0.0),
            zoom_speed: 0.1,
            orbit_speed: 0.01,
            min_radius: 1.0,
            max_radius: 20.0,
            has_moved: false,
        };
        
        camera.update_position();
        camera
    }
    
    pub fn update_position(&mut self) {
        // Clamp phi to avoid flipping
        self.phi = self.phi.clamp(0.1, std::f32::consts::PI - 0.1);
        
        // Calculate position from spherical coordinates
        let x = self.radius * self.phi.sin() * self.theta.cos();
        let y = self.radius * self.phi.cos();
        let z = self.radius * self.phi.sin() * self.theta.sin();
        
        let new_position = self.target + Vec3::new(x, y, z);
        
        // Only set has_moved if position actually changed
        if (new_position - self.position).length_squared() > 1e-6 {
            self.has_moved = true;
        }
        
        self.position = new_position;
    }
    
    pub fn handle_mouse_input(&mut self, state: ElementState, button: MouseButton) {
        if button == MouseButton::Left {
            self.is_dragging = state == ElementState::Pressed;
        }
    }
    
    pub fn handle_mouse_motion(&mut self, position: PhysicalPosition<f64>) {
        if self.is_dragging {
            let delta_x = position.x - self.last_mouse_pos.x;
            let delta_y = position.y - self.last_mouse_pos.y;
            
            self.theta += delta_x as f32 * self.orbit_speed;
            self.phi -= delta_y as f32 * self.orbit_speed;  // Invert Y-axis
            
            self.update_position();
        }
        
        self.last_mouse_pos = position;
    }
    
    pub fn handle_scroll(&mut self, delta: MouseScrollDelta) {
        let scroll_amount = match delta {
            MouseScrollDelta::LineDelta(_, y) => y,
            MouseScrollDelta::PixelDelta(pos) => pos.y as f32 * 0.01,
        };
        
        self.radius -= scroll_amount * self.zoom_speed * self.radius;
        self.radius = self.radius.clamp(self.min_radius, self.max_radius);
        
        self.update_position();
    }
    
    pub fn resize(&mut self, width: u32, height: u32) {
        self.aspect_ratio = width as f32 / height as f32;
    }
    
    pub fn build_view_matrix(&self) -> (Vec3, Vec3, Vec3) {
        let forward = (self.target - self.position).normalize();
        let right = forward.cross(self.up).normalize();
        let up = right.cross(forward).normalize();
        
        (forward, up, right)
    }
    
    pub fn to_uniform(&self) -> CameraUniform {
        let (forward, up, right) = self.build_view_matrix();
        
        CameraUniform {
            eye: self.position.extend(1.0),
            direction: forward.extend(0.0),
            up: up.extend(0.0),
            right: right.extend(0.0),
            focal_length: self.focal_length,
            focal_blur_amount: self.focal_blur_amount,
            fov: self.fov,
            _padding: 0.0,
        }
    }
    
    pub fn reset_movement_flag(&mut self) {
        self.has_moved = false;
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    pub eye: Vec4,
    pub direction: Vec4,
    pub up: Vec4,
    pub right: Vec4,
    pub focal_length: f32,
    pub focal_blur_amount: f32,
    pub fov: f32,
    pub _padding: f32,
}