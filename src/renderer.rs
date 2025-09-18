use std::{borrow::Cow, cmp::max, mem::size_of, sync::Arc};
use wgpu::{
    BindGroupDescriptor, BindGroupEntry, BindGroupLayoutDescriptor, BindGroupLayoutEntry,
    BindingType, BufferBindingType, BufferDescriptor, BufferUsages, Color,
    CommandEncoderDescriptor, Device, DeviceDescriptor, FragmentState, Instance, Limits, LoadOp,
    MultisampleState, Operations, PipelineCompilationOptions, PipelineLayoutDescriptor,
    PrimitiveState, Queue, RenderPassColorAttachment, RenderPassDescriptor, RenderPipeline,
    RenderPipelineDescriptor, RequestAdapterOptions, ShaderModuleDescriptor, ShaderSource,
    ShaderStages, StoreOp, Surface, SurfaceConfiguration, Texture, TextureDescriptor,
    TextureFormat, TextureViewDescriptor, VertexState,
};
use winit::window::Window;

use crate::camera_controller::CameraUniform;
use crate::scene::Camera;

const MAX_IMAGE_BUFFER_SIZE: usize = 4096 * 2048;

pub struct Buffers {
    pub buffers: Vec<wgpu::Buffer>,
    pub group: wgpu::BindGroup,
}

pub enum RenderOutput {
    Window(Arc<Window>),
    Headless(u32, u32),
}

pub enum RenderTarget {
    Surface(Surface<'static>),
    Texture(Texture),
}

pub struct Renderer {
    pub device: Device,
    pub queue: Queue,
    pub target: RenderTarget,
    pub config: SurfaceConfiguration,
    pub buffers: Vec<Buffers>,
    pub frame_count: u32,
    render_pipeline: RenderPipeline,
    current_frame: Option<wgpu::SurfaceTexture>,
}
impl Renderer {
    pub async fn new(
        output: RenderOutput,
        custom_buffers: Vec<(BufferBindingType, u64)>,
        shader_source: &str,
    ) -> Self {
        let instance = Instance::default();
        let (device, queue, config, target) = match output {
            RenderOutput::Window(window) => {
                let mut size = window.inner_size();
                size.width = size.width.max(1);
                size.height = size.height.max(1);
                let surface = instance.create_surface(window).unwrap();
                let adapter = instance
                    .request_adapter(&RequestAdapterOptions {
                        compatible_surface: Some(&surface),
                        ..Default::default()
                    })
                    .await
                    .expect("Failed to find an appropriate adapter");
                let config = surface
                    .get_default_config(&adapter, size.width, size.height)
                    .unwrap();
                let mut limits =
                    Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits());
                let max_storage_buffer_size = 256 << 20;
                limits.max_buffer_size =
                    max(limits.max_buffer_size, max_storage_buffer_size as u64);
                limits.max_storage_buffer_binding_size = max_storage_buffer_size;
                limits.max_storage_buffers_per_shader_stage = 4;
                let (device, queue) = adapter
                    .request_device(&DeviceDescriptor {
                        required_limits: limits,
                        ..Default::default()
                    })
                    .await
                    .expect("Failed to create device");
                surface.configure(&device, &config);
                (device, queue, config, RenderTarget::Surface(surface))
            }
            RenderOutput::Headless(width, height) => {
                let adapter = instance
                    .request_adapter(&RequestAdapterOptions::default())
                    .await
                    .expect("Failed to find an appropriate adapter");
                let config = SurfaceConfiguration {
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    format: TextureFormat::Rgba8UnormSrgb,
                    width,
                    height,
                    desired_maximum_frame_latency: 1,
                    alpha_mode: wgpu::CompositeAlphaMode::Auto,
                    view_formats: vec![TextureFormat::Rgba8UnormSrgb],
                    present_mode: wgpu::PresentMode::Mailbox,
                };
                let mut limits =
                    Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits());
                let max_storage_buffer_size = 256 << 20;
                limits.max_buffer_size =
                    max(limits.max_buffer_size, max_storage_buffer_size as u64);
                limits.max_storage_buffer_binding_size = max_storage_buffer_size;
                limits.max_storage_buffers_per_shader_stage = 4;
                let (device, queue) = adapter
                    .request_device(&DeviceDescriptor {
                        required_limits: limits,
                        ..Default::default()
                    })
                    .await
                    .expect("Failed to create device");
                let texture = device.create_texture(&TextureDescriptor {
                    label: None,
                    size: wgpu::Extent3d {
                        width: config.width,
                        height: config.height,
                        depth_or_array_layers: 1,
                    },
                    view_formats: &config.view_formats,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: config.format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                });
                (device, queue, config, RenderTarget::Texture(texture))
            }
        };
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: None,
            source: ShaderSource::Wgsl(Cow::Borrowed(shader_source)),
        });
        let builtin_buffer = vec![
            (BufferBindingType::Uniform, 2 * size_of::<u32>() as u64), // resolution
            (BufferBindingType::Uniform, size_of::<u32>() as u64),     // frame count
            (BufferBindingType::Uniform, size_of::<u32>() as u64),     // time
            (
                BufferBindingType::Storage { read_only: false },
                (MAX_IMAGE_BUFFER_SIZE * size_of::<u32>()) as u64,
            ), // image data
            (BufferBindingType::Uniform, size_of::<Camera>() as u64),  // camera
        ];
        let buffers = [builtin_buffer, custom_buffers];
        let bind_group_layouts: Vec<wgpu::BindGroupLayout> = buffers
            .iter()
            .map(|group| {
                let entries: Vec<_> = group
                    .iter()
                    .enumerate()
                    .map(|(binding, &(ty, _))| BindGroupLayoutEntry {
                        binding: binding as u32,
                        visibility: ShaderStages::FRAGMENT,
                        ty: BindingType::Buffer {
                            ty,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    })
                    .collect();
                device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                    label: None,
                    entries: entries.as_slice(),
                })
            })
            .collect();
        let bind_group_layouts: Vec<_> = bind_group_layouts.iter().collect();
        println!("creating pipeline layout");
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            bind_group_layouts: &bind_group_layouts,
            ..Default::default()
        });
        let render_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: PipelineCompilationOptions::default(),
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(config.format.into())],
                compilation_options: PipelineCompilationOptions::default(),
            }),
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let buffers: Vec<_> = buffers
            .iter()
            .enumerate()
            .map(|(i, group)| {
                let buffers: Vec<_> = group
                    .iter()
                    .map(|&(ty, size)| {
                        let usage = match ty {
                            BufferBindingType::Storage { read_only: _ } => {
                                BufferUsages::STORAGE
                                    | BufferUsages::COPY_DST
                                    | BufferUsages::COPY_SRC
                            }
                            BufferBindingType::Uniform => {
                                BufferUsages::UNIFORM | BufferUsages::COPY_DST
                            }
                        };
                        device.create_buffer(&BufferDescriptor {
                            usage,
                            size,
                            mapped_at_creation: false,
                            label: None,
                        })
                    })
                    .collect();
                let entries: Vec<_> = group
                    .iter()
                    .enumerate()
                    .map(|(binding, _)| BindGroupEntry {
                        binding: binding as u32,
                        resource: buffers[binding].as_entire_binding(),
                    })
                    .collect();
                let group = device.create_bind_group(&BindGroupDescriptor {
                    layout: bind_group_layouts[i],
                    entries: entries.as_slice(),
                    label: None,
                });
                Buffers { buffers, group }
            })
            .collect();
        println!("created pipeline");
        let buffer = &buffers[0].buffers[0];
        queue.write_buffer(
            buffer,
            0,
            bytemuck::bytes_of(&[config.width, config.height]),
        );

        let mut renderer = Self {
            device,
            config,
            target,
            queue,
            render_pipeline,
            buffers,
            frame_count: 0,
            current_frame: None,
        };

        renderer.clear_image_buffer();

        renderer
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        match self.target {
            RenderTarget::Surface(ref surface) => {
                self.config.width = max(1, width);
                self.config.height = max(1, height);
                surface.configure(&self.device, &self.config);
            }
            RenderTarget::Texture(_) => {
                self.config.width = max(1, width);
                self.config.height = max(1, height);
                let texture = self.device.create_texture(&TextureDescriptor {
                    label: None,
                    size: wgpu::Extent3d {
                        width: self.config.width,
                        height: self.config.height,
                        depth_or_array_layers: 1,
                    },
                    view_formats: &self.config.view_formats,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: self.config.format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                });
                self.target = RenderTarget::Texture(texture);
            }
        }
        let buffer = &self.buffers[0].buffers[0];
        self.queue
            .write_buffer(buffer, 0, bytemuck::bytes_of(&[width, height]));

        self.clear_image_buffer();

        self.frame_count = 0;
    }

    pub fn set_time(&mut self, time: u32) {
        let buffer = &self.buffers[0].buffers[2];
        self.queue
            .write_buffer(buffer, 0, bytemuck::bytes_of(&[time]));
    }
    pub fn set_frame_count(&mut self, n: u32) {
        let buffer = &self.buffers[0].buffers[1];
        self.queue.write_buffer(buffer, 0, bytemuck::bytes_of(&[n]));
    }
    pub fn set_camera(&mut self, camera: &Camera) {
        let buffer = &self.buffers[0].buffers[4];
        self.queue
            .write_buffer(buffer, 0, bytemuck::bytes_of(camera))
    }

    pub fn update_camera_uniform(&mut self, camera: CameraUniform) {
        let buffer = &self.buffers[0].buffers[4];
        self.queue
            .write_buffer(buffer, 0, bytemuck::bytes_of(&camera))
    }

    pub fn reset_frame_count(&mut self) {
        self.frame_count = 0;

        self.clear_image_buffer();
    }

    fn clear_image_buffer(&mut self) {
        let image_buffer = &self.buffers[0].buffers[3];
        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("clear image buffer"),
            });
        encoder.clear_buffer(image_buffer, 0, None);
        self.queue.submit(Some(encoder.finish()));
    }

    pub fn write_buffer(&mut self, data: &[u8], buffer: usize) {
        let buffer = &self.buffers[1].buffers[buffer];
        self.queue.write_buffer(buffer, 0, data)
    }

    pub fn draw(&mut self) {
        let (mut encoder, view) = self.begin_frame();
        self.render_scene(&mut encoder, &view);
        self.end_frame(encoder, view);
    }

    pub fn begin_frame(&mut self) -> (wgpu::CommandEncoder, wgpu::TextureView) {
        self.set_frame_count(self.frame_count);
        let encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor::default());

        let view = match &self.target {
            RenderTarget::Surface(surface) => {
                let frame = surface.get_current_texture().unwrap();
                let view = frame.texture.create_view(&TextureViewDescriptor::default());
                self.current_frame = Some(frame);
                view
            }
            RenderTarget::Texture(texture) => {
                texture.create_view(&TextureViewDescriptor::default())
            }
        };

        (encoder, view)
    }

    pub fn render_scene(&mut self, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
        let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
            color_attachments: &[Some(RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLACK),
                    store: StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        rpass.set_pipeline(&self.render_pipeline);
        for (i, group) in self.buffers.iter().enumerate() {
            rpass.set_bind_group(i as u32, &group.group, &[]);
        }
        rpass.draw(0..6, 0..1);
    }

    pub fn end_frame(&mut self, encoder: wgpu::CommandEncoder, _view: wgpu::TextureView) {
        self.queue.submit(Some(encoder.finish()));

        // Present the frame we got in begin_frame
        if let Some(frame) = self.current_frame.take() {
            frame.present();
        }

        self.frame_count += 1;
    }
}
