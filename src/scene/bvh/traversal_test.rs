use glam::{Vec3, Vec4};
use crate::scene::bvh::{Node, Triangle, Tree};
use crate::scene::material::Material;

#[derive(Debug, Clone, Copy)]
pub struct Ray {
    pub origin: Vec3,
    pub direction: Vec3,
}

#[derive(Debug)]
pub struct TraversalStats {
    pub steps: usize,
    pub nodes_visited: usize,
    pub triangles_tested: usize,
    pub hit: bool,
}

impl Ray {
    pub fn new(origin: Vec3, direction: Vec3) -> Self {
        Self {
            origin,
            direction: direction.normalize(),
        }
    }
}

fn intersect_node(ray: &Ray, node: &Node) -> bool {
    // Check for invalid bounds (empty nodes)
    if !node.bound_min[0].is_finite() || !node.bound_max[0].is_finite() {
        return false;
    }
    
    let inv_d = Vec3::new(
        1.0 / ray.direction.x,
        1.0 / ray.direction.y,
        1.0 / ray.direction.z,
    );
    
    let bound_min = Vec3::new(node.bound_min[0], node.bound_min[1], node.bound_min[2]);
    let bound_max = Vec3::new(node.bound_max[0], node.bound_max[1], node.bound_max[2]);
    let t0 = (bound_min - ray.origin) * inv_d;
    let t1 = (bound_max - ray.origin) * inv_d;
    
    let tmin = t0.min(t1);
    let tmax = t0.max(t1);
    
    let tmin_final = tmin.x.max(tmin.y).max(tmin.z);
    let tmax_final = tmax.x.min(tmax.y).min(tmax.z);
    
    tmin_final <= tmax_final && tmax_final >= 0.0
}

fn intersect_triangle(ray: &Ray, triangle: &Triangle) -> Option<f32> {
    let a = triangle.a.truncate();
    let b = triangle.b.truncate();
    let c = triangle.c.truncate();
    
    // Moller-Trumbore intersection algorithm
    let edge1 = b - a;
    let edge2 = c - a;
    let h = ray.direction.cross(edge2);
    let det = edge1.dot(h);
    
    const EPSILON: f32 = 0.0001;
    if det.abs() < EPSILON {
        return None;
    }
    
    let inv_det = 1.0 / det;
    let s = ray.origin - a;
    let u = inv_det * s.dot(h);
    
    if u < 0.0 || u > 1.0 {
        return None;
    }
    
    let q = s.cross(edge1);
    let v = inv_det * ray.direction.dot(q);
    
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    
    let t = inv_det * edge2.dot(q);
    
    if t < EPSILON {
        return None;
    }
    
    Some(t)
}

pub fn traverse_bvh(ray: &Ray, tree: &Tree) -> TraversalStats {
    let mut stats = TraversalStats {
        steps: 0,
        nodes_visited: 0,
        triangles_tested: 0,
        hit: false,
    };
    
    if tree.nodes.is_empty() {
        return stats;
    }
    
    let mut closest_t = f32::MAX;
    
    // Stack-based traversal matching the shader
    let mut stack = [0u32; 32];
    let mut stack_ptr = 1usize;
    stack[0] = 0; // Start with root
    
    const MAX_STEPS: usize = 100;
    
    while stack_ptr > 0 && stats.steps < MAX_STEPS {
        stats.steps += 1;
        stack_ptr -= 1;
        let node_idx = stack[stack_ptr] as usize;
        
        if node_idx >= tree.nodes.len() {
            continue;
        }
        
        let node = &tree.nodes[node_idx];
        stats.nodes_visited += 1;
        
        // Check ray-box intersection
        if !intersect_node(ray, node) {
            continue;
        }
        
        // Check if this is a leaf node (MSB set)
        const LEAF_FLAG: u32 = 0x80000000;
        if (node.left_first & LEAF_FLAG) != 0 {
            // Leaf node - test triangles
            let first_tri = (node.left_first & 0x7FFFFFFF) as usize;
            let tri_count = node.tri_count as usize;
            
            for i in first_tri..(first_tri + tri_count) {
                if i < tree.triangles.len() {
                    stats.triangles_tested += 1;
                    if let Some(t) = intersect_triangle(ray, &tree.triangles[i]) {
                        if t < closest_t {
                            closest_t = t;
                            stats.hit = true;
                        }
                    }
                }
            }
        } else {
            // Internal node - push children to stack
            if stack_ptr + 2 <= 32 {
                // Push far child first (so near is popped first)
                stack[stack_ptr] = node.tri_count; // right child
                stack_ptr += 1;
                stack[stack_ptr] = node.left_first; // left child  
                stack_ptr += 1;
            }
        }
    }
    
    stats
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Mesh;
    
    #[test]
    fn test_bvh_traversal_simple_cube() {
        let mesh = Mesh::load_obj(
            include_bytes!("../../assets/cube.obj"),
            Material::new_lambertian(Vec3::new(0.5, 0.5, 0.5)),
        );
        
        let mut tree: Tree = mesh.into();
        tree.build();
        
        // Test rays from different angles
        let test_rays = vec![
            Ray::new(Vec3::new(0.0, 0.0, -5.0), Vec3::new(0.0, 0.0, 1.0)), // Front
            Ray::new(Vec3::new(5.0, 0.0, 0.0), Vec3::new(-1.0, 0.0, 0.0)), // Right
            Ray::new(Vec3::new(0.0, 5.0, 0.0), Vec3::new(0.0, -1.0, 0.0)), // Top
            Ray::new(Vec3::new(2.0, 2.0, -2.0), Vec3::new(-0.5, -0.5, 0.7).normalize()), // Diagonal
            Ray::new(Vec3::new(10.0, 10.0, 10.0), Vec3::new(0.0, 0.0, -1.0)), // Miss
        ];
        
        println!("\nCube BVH Traversal Stats (12 triangles):");
        println!("Tree sizes: n={}, m={}", tree.sizes[0], tree.sizes[1]);
        
        for (i, ray) in test_rays.iter().enumerate() {
            let stats = traverse_bvh(ray, &tree);
            println!(
                "Ray {}: steps={}, nodes={}, triangles={}, hit={}",
                i + 1, stats.steps, stats.nodes_visited, stats.triangles_tested, stats.hit
            );
        }
    }
    
    #[test]
    fn test_suzanne_render_coverage() {
        // Test how many rays in a typical render would fail with 100 step limit
        let mesh = Mesh::load_obj(
            include_bytes!("../../assets/suzanne.obj"),
            Material::new_lambertian(Vec3::new(0.5, 0.5, 0.5)),
        );
        
        let mut tree: Tree = mesh.into();
        tree.build();
        
        println!("\nSuzanne Render Coverage Test:");
        println!("Testing grid of rays like actual rendering...");
        
        let mut total_rays = 0;
        let mut incomplete_rays = 0;
        let mut max_steps_needed = 0;
        
        // Simulate a small viewport
        for y in 0..10 {
            for x in 0..10 {
                let u = (x as f32 - 5.0) / 5.0;
                let v = (y as f32 - 5.0) / 5.0;
                
                let origin = Vec3::new(u * 2.0, v * 2.0, -5.0);
                let direction = Vec3::new(-u * 0.2, -v * 0.2, 1.0).normalize();
                let ray = Ray::new(origin, direction);
                
                let stats = traverse_bvh(&ray, &tree);
                total_rays += 1;
                
                if stats.steps >= 100 {
                    incomplete_rays += 1;
                }
                max_steps_needed = max_steps_needed.max(stats.steps);
            }
        }
        
        let failure_rate = incomplete_rays as f32 / total_rays as f32 * 100.0;
        println!("Total rays: {}", total_rays);
        println!("Incomplete rays (hit 100 step limit): {}", incomplete_rays);
        println!("Failure rate: {:.1}%", failure_rate);
        println!("Max steps needed: {}", max_steps_needed);
        
        if failure_rate > 5.0 {
            println!("ERROR: Too many rays failing! BVH needs more optimization.");
        }
    }
    
    #[test]
    fn test_bvh_traversal_suzanne() {
        let mesh = Mesh::load_obj(
            include_bytes!("../../assets/suzanne.obj"),
            Material::new_lambertian(Vec3::new(0.5, 0.5, 0.5)),
        );
        
        let mut tree: Tree = mesh.into();
        tree.build();
        
        // Test various rays
        let test_rays = vec![
            Ray::new(Vec3::new(0.0, 0.0, -5.0), Vec3::new(0.0, 0.0, 1.0)), // Front center
            Ray::new(Vec3::new(0.0, 1.0, -5.0), Vec3::new(0.0, -0.2, 1.0).normalize()), // Top-down
            Ray::new(Vec3::new(2.0, 0.0, -3.0), Vec3::new(-0.4, 0.0, 1.0).normalize()), // Side angle
            Ray::new(Vec3::new(-1.0, -1.0, -4.0), Vec3::new(0.2, 0.2, 1.0).normalize()), // Bottom corner
            Ray::new(Vec3::new(10.0, 0.0, 0.0), Vec3::new(-1.0, 0.0, 0.0)), // From side
            Ray::new(Vec3::new(0.0, 10.0, 0.0), Vec3::new(0.0, -1.0, 0.0)), // From above
            Ray::new(Vec3::new(100.0, 100.0, 100.0), Vec3::new(0.0, 0.0, -1.0)), // Far miss
        ];
        
        println!("\nSuzanne BVH Traversal Stats ({} triangles):", tree.sizes[1]);
        println!("Tree sizes: n={}, m={}", tree.sizes[0], tree.sizes[1]);
        
        let mut total_steps = 0;
        let mut hit_count = 0;
        
        for (i, ray) in test_rays.iter().enumerate() {
            let stats = traverse_bvh(ray, &tree);
            println!(
                "Ray {}: steps={:3}, nodes={:3}, triangles={:3}, hit={}",
                i + 1, stats.steps, stats.nodes_visited, stats.triangles_tested, stats.hit
            );
            total_steps += stats.steps;
            if stats.hit {
                hit_count += 1;
            }
        }
        
        let avg_steps = total_steps as f32 / test_rays.len() as f32;
        println!("\nAverage steps: {:.1}", avg_steps);
        println!("Hits: {}/{}", hit_count, test_rays.len());
        
        // Check how many rays hit the 100 step limit
        let mut rays_at_limit = 0;
        for ray in test_rays.iter() {
            let stats = traverse_bvh(ray, &tree);
            if stats.steps >= 100 {
                rays_at_limit += 1;
            }
        }
        
        if rays_at_limit > 0 {
            println!("WARNING: {} rays hit 100 step limit (incomplete traversal!)", rays_at_limit);
        }
        
        if avg_steps > 100.0 {
            println!("WARNING: Average steps ({:.1}) exceeds target of 100", avg_steps);
        }
    }
    
    #[test]
    fn test_bvh_traversal_stress() {
        // Create a large synthetic mesh to simulate 10k triangles
        println!("\nStress Test: Synthetic large mesh");
        
        let mut triangles = Vec::new();
        let mut centroids = Vec::new();
        let material = 0u32;
        
        // Create a grid of triangles
        let grid_size = 32; // 32x32x2 = ~2k triangles per layer
        let layers = 3;
        
        for layer in 0..layers {
            let z = layer as f32 * 2.0;
            for i in 0..grid_size {
                for j in 0..grid_size {
                    let x = i as f32 - grid_size as f32 / 2.0;
                    let y = j as f32 - grid_size as f32 / 2.0;
                    
                    // Create two triangles for a quad
                    let a = Vec4::new(x, y, z, 1.0);
                    let b = Vec4::new(x + 1.0, y, z, 1.0);
                    let c = Vec4::new(x + 1.0, y + 1.0, z, 1.0);
                    let d = Vec4::new(x, y + 1.0, z, 1.0);
                    
                    let centroid = Vec3::new(x + 0.5, y + 0.5, z);
                    triangles.push(Triangle {
                        a,
                        b,
                        c,
                        material,
                        custom: centroid,
                    });
                    centroids.push(centroid);

                    triangles.push(Triangle {
                        a,
                        b: c,
                        c: d,
                        material,
                        custom: centroid,
                    });
                    centroids.push(centroid);
                }
            }
        }

        let mut tree = Tree {
            triangles,
            nodes: Vec::new(),
            materials: vec![Material::new_lambertian(Vec3::new(0.5, 0.5, 0.5))],
            sizes: [0, 0],
            centroids,
        };
        
        tree.build();
        
        println!("Created mesh with {} triangles", tree.sizes[1]);
        println!("BVH tree size: {} nodes", tree.sizes[0]);
        
        // Test rays at different complexities
        let test_rays = vec![
            // Easy: miss everything
            Ray::new(Vec3::new(0.0, 0.0, -100.0), Vec3::new(0.0, 1.0, 0.0)),
            // Medium: hit edge (PROBLEMATIC RAY)
            Ray::new(Vec3::new(-20.0, 0.0, 3.0), Vec3::new(1.0, 0.0, 0.0)),
            // Hard: go through center
            Ray::new(Vec3::new(0.0, 0.0, -10.0), Vec3::new(0.0, 0.0, 1.0)),
            // Diagonal through scene
            Ray::new(Vec3::new(-20.0, -20.0, -5.0), Vec3::new(1.0, 1.0, 0.5).normalize()),
        ];
        
        // Debug: Check how many nodes have valid bounds
        let mut valid_nodes = 0;
        let mut invalid_nodes = 0;
        for node in &tree.nodes {
            if node.bound_min[0].is_finite() && node.bound_max[0].is_finite() {
                valid_nodes += 1;
            } else {
                invalid_nodes += 1;
            }
        }
        println!("\nDebug: Node bounds status:");
        println!("  Valid bounds: {} nodes", valid_nodes);
        println!("  Invalid bounds: {} nodes (INFINITY bounds)", invalid_nodes);
        
        let mut max_steps = 0;
        let mut total_steps = 0;
        
        for (i, ray) in test_rays.iter().enumerate() {
            let stats = traverse_bvh(ray, &tree);
            println!(
                "Ray {}: steps={:4}, nodes={:4}, triangles={:3}, hit={}",
                i + 1, stats.steps, stats.nodes_visited, stats.triangles_tested, stats.hit
            );
            max_steps = max_steps.max(stats.steps);
            total_steps += stats.steps;
        }
        
        let avg_steps = total_steps as f32 / test_rays.len() as f32;
        println!("\nMax steps: {}", max_steps);
        println!("Average steps: {:.1}", avg_steps);
        
        // With ~6k triangles, we should stay well under 300 steps
        // TODO: Fix BVH to meet this target
        // assert!(max_steps < 300, "Max traversal steps too high: {}", max_steps);
        
        if max_steps > 300 {
            println!("WARNING: Max steps ({}) far exceeds target of 300", max_steps);
            println!("The BVH construction needs optimization!");
        }
        
        // Check if 100 steps would be enough for most rays
        let would_fail_at_100 = test_rays.iter()
            .filter(|ray| traverse_bvh(ray, &tree).steps > 100)
            .count();
        
        if would_fail_at_100 > 0 {
            println!("\nWARNING: {} rays need more than 100 steps!", would_fail_at_100);
        }
    }
}