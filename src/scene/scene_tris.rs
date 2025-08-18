use glam::Vec3;
use std::cmp::min;
use std::{f32::consts::PI, mem::size_of};
use wgpu::BufferBindingType;

use crate::renderer::RenderOutput;
use crate::{geometry::Mesh, renderer::Renderer};

use super::{bvh::{Tree, Triangle}, material::Material, Camera};
use super::bvh::tree::Node;

const MAX_TRIS: usize = 1000000;
const MAX_MATS: usize = 1000;

pub struct SceneTris {
    pub renderer: Renderer,
    pub camera: Camera,
    pub tris_bvh: Tree,
}

impl SceneTris {
    pub fn write_tree_data(&mut self) {
        let data = [
            (
                bytemuck::cast_slice(&self.tris_bvh.sizes),
                2 * size_of::<u32>(),
            ),
            (
                bytemuck::cast_slice(&self.tris_bvh.nodes),
                MAX_TRIS * size_of::<Node>(),
            ),
            (
                bytemuck::cast_slice(&self.tris_bvh.triangles),
                MAX_TRIS * size_of::<Triangle>(),
            ),
            (
                bytemuck::cast_slice(&self.tris_bvh.materials),
                MAX_MATS * size_of::<Material>(),
            ),
        ];
        for (i, (data, size)) in data.into_iter().enumerate() {
            let n = min(data.len(), size);
            self.renderer.write_buffer(&data[0..n], i);
        }
    }
    async fn make_renderer(output: RenderOutput) -> Renderer {
        Renderer::new(
            output,
            vec![
                (BufferBindingType::Uniform, 2 * size_of::<u32>() as u64), // bvh tree size
                (
                    BufferBindingType::Storage { read_only: true },
                    (MAX_TRIS * size_of::<Node>()) as u64,
                ), // nodes
                (
                    BufferBindingType::Storage { read_only: true },
                    (MAX_TRIS * size_of::<Triangle>()) as u64,
                ), // triangles
                (
                    BufferBindingType::Storage { read_only: true },
                    (MAX_MATS * size_of::<Material>()) as u64,
                ), // materials
            ],
            include_str!("../shaders/shader_tris.wgsl"),
        )
        .await
    }
    pub async fn new_dragon(output: RenderOutput) -> Self {
        let mesh = Mesh::load_obj(
            include_bytes!("../assets/xyzrgb_dragon_lp_20.obj"),
            Material::new_lambertian(Vec3::new(0.7, 0.7, 0.2)),
        );
        let mut tree: Tree = mesh.into();
        let mesh = Mesh::load_obj(
            include_bytes!("../assets/floor.obj"),
            Material::new_lambertian(Vec3::new(0.5, 0.5, 0.6)),
        );
        tree.add_mesh(mesh);
        tree.build();
        let renderer = Self::make_renderer(output).await;
        let camera = Camera::new(
            Vec3::new(0.0, 2.0, 8.0),
            Vec3::new(0.0, 0.0, -8.0),
            5.6,
            0.0,
            PI * 0.3,
        );
        Self {
            renderer,
            camera,
            tris_bvh: tree,
        }
    }
    pub async fn new_lucy(output: RenderOutput) -> Self {
        let mesh = Mesh::load_obj(
            include_bytes!("../assets/lucy_lp_20.obj"),
            Material::new_lambertian(Vec3::new(0.4, 0.3, 0.6)),
        );
        let mut tree: Tree = mesh.into();
        let mesh = Mesh::load_obj(
            include_bytes!("../assets/floor.obj"),
            Material::new_lambertian(Vec3::new(0.5, 0.5, 0.6)),
        );
        tree.add_mesh(mesh);
        tree.build();
        let renderer = Self::make_renderer(output).await;
        let camera = Camera::new(
            Vec3::new(0.0, 5.0, 6.0),
            Vec3::new(0.0, 0.0, -8.0),
            5.6,
            0.0,
            PI * 0.3,
        );
        Self {
            renderer,
            camera,
            tris_bvh: tree,
        }
    }
    pub async fn new_suzane(output: RenderOutput) -> Self {
        let mesh = Mesh::load_obj(
            include_bytes!("../assets/suzanne.obj"),
            Material::new_lambertian(Vec3::new(0.3, 0.4, 0.6)),
        );
        let mut tree: Tree = mesh.into();
        let mesh = Mesh::load_obj(
            include_bytes!("../assets/ico_sphere.obj"),
            Material::new_dielectric(0.2),
        );
        tree.add_mesh(mesh);
        let mesh = Mesh::load_obj(
            include_bytes!("../assets/cube_s.obj"),
            Material::new_metal(Vec3::new(0.5, 0.5, 0.6), 0.2),
        );
        tree.add_mesh(mesh);
        let mesh = Mesh::load_obj(
            include_bytes!("../assets/cube_m.obj"),
            Material::new_dielectric(0.1),
        );
        tree.add_mesh(mesh);
        let mesh = Mesh::load_obj(
            include_bytes!("../assets/cube_l.obj"),
            Material::new_lambertian(Vec3::new(0.5, 0.5, 0.6)),
        );
        tree.add_mesh(mesh);
        tree.build();
        let camera = Camera::new(
            Vec3::new(0.0, 2.2, 4.5),
            Vec3::new(0.0, 0.0, -4.5),
            5.6,
            0.0,
            PI * 0.3,
        );
        let renderer = Self::make_renderer(output).await;
        Self {
            renderer,
            camera,
            tris_bvh: tree,
        }
    }
    pub async fn new_cube(output: RenderOutput) -> Self {
        let mesh = Mesh::load_obj(
            include_bytes!("../assets/cube2.obj"),
            Material::new_lambertian(Vec3::new(0.5, 0.5, 0.6)),
        );
        let mut tree: Tree = mesh.into();
        tree.build();
        let camera = Camera::new(
            Vec3::new(0.0, 2.2, 6.5),
            Vec3::new(0.0, 0.1, -3.0),
            2.2,
            0.0,
            PI * 0.3,
        );
        let renderer = Self::make_renderer(output).await;
        Self {
            renderer,
            camera,
            tris_bvh: tree,
        }
    }
    pub async fn new_quad(output: RenderOutput) -> Self {
        let mesh = Mesh::load_obj(
            include_bytes!("../assets/quad.obj"),
            Material::new_lambertian(Vec3::new(0.5, 0.5, 0.6)),
        );
        let mut tree: Tree = mesh.into();
        tree.build();
        let camera = Camera::new(
            Vec3::new(0.0, 0.2, 3.5),
            Vec3::new(0.0, 0.1, -3.0),
            2.2,
            0.0,
            PI * 0.3,
        );
        let renderer = Self::make_renderer(output).await;
        Self {
            renderer,
            camera,
            tris_bvh: tree,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::render_ppm;
    use crate::scene::Scene;
    use std::io::Write;

    #[test]
    fn suzanne() {
        let width = 1024;
        let height = 768;
        let mut scene =
            pollster::block_on(SceneTris::new_suzane(RenderOutput::Headless(width, height)));
        scene.init();
        let content = render_ppm(&mut scene.renderer);
        let mut file = std::fs::File::create("suzanne.ppm").unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }
    #[test]
    fn simple_quad() {
        let width = 1024;
        let height = 768;
        let mut scene =
            pollster::block_on(SceneTris::new_quad(RenderOutput::Headless(width, height)));
        scene.init();
        let content = render_ppm(&mut scene.renderer);
        let mut file = std::fs::File::create("quad.ppm").unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }
    #[test]
    fn simple_cube() {
        let width = 1024;
        let height = 768;
        let mut scene =
            pollster::block_on(SceneTris::new_cube(RenderOutput::Headless(width, height)));
        scene.init();
        let content = render_ppm(&mut scene.renderer);
        let mut file = std::fs::File::create("cube.ppm").unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }
}
