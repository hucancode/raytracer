use crate::renderer::Renderer;
use std::fmt::Write;
use std::mem::size_of;
use std::sync::mpsc::channel;
use wgpu::{BufferDescriptor, BufferUsages};

fn copy_image_buffer(renderer: &mut Renderer) -> Vec<u8> {
    let width = renderer.config.width;
    let height = renderer.config.height;
    let size = (width * height * 3 * size_of::<f32>() as u32) as u64;
    let device = &renderer.device;
    let output_buffer = device.create_buffer(&BufferDescriptor {
        usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
        size,
        mapped_at_creation: false,
        label: None,
    });
    let input_buffer = &renderer.buffers[0].buffers[3];
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Copy Buffer Encoder"),
    });
    encoder.copy_buffer_to_buffer(input_buffer, 0, &output_buffer, 0, size);
    renderer.queue.submit(Some(encoder.finish()));
    let buffer_slice = output_buffer.slice(..);
    let (tx, rx) = channel();
    buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
        tx.send(result.is_ok()).unwrap()
    });
    let _ = device.poll(wgpu::MaintainBase::Wait);
    let ret = if rx.recv().unwrap_or(false) {
        buffer_slice.get_mapped_range().to_vec()
    } else {
        Vec::new()
    };
    output_buffer.unmap();
    ret
}
pub fn render_ppm(renderer: &mut Renderer) -> String {
    let width = renderer.config.width;
    let height = renderer.config.height;
    // Don't draw an extra frame here - assume frames have already been rendered
    let data = copy_image_buffer(renderer);
    let mut ret = String::new();
    let data = bytemuck::cast_slice::<u8, f32>(data.as_ref());
    let data = data
        .chunks_exact(3)
        .map(|a| [a[0], a[1], a[2]])
        .map(|[r, g, b]| [r * 255.0, g * 255.0, b * 255.0])
        .map(|[r, g, b]| [r as u8, g as u8, b as u8])
        .collect::<Vec<[u8; 3]>>();
    writeln!(ret, "P3").unwrap();
    writeln!(ret, "{width} {height} 255").unwrap();
    for [r, g, b] in data.iter() {
        write!(ret, "{r} {g} {b} ").unwrap();
    }
    ret
}
