use bytemuck::{Pod, Zeroable};
use glam::{Vec3, Vec4, Vec4Swizzles};

use crate::geometry::Mesh;
use crate::scene::bvh::Triangle;
use crate::scene::material::Material;

/// Compact BVH node for GPU - 32 bytes aligned
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Node {
    pub bound_min: [f32; 3],
    pub left_first: u32,  // For internal: left child index. For leaf: triangle start (MSB=1)
    pub bound_max: [f32; 3], 
    pub tri_count: u32,   // For internal: right child index. For leaf: triangle count
}

impl Node {
    const LEAF_FLAG: u32 = 0x80000000;
    
    pub fn make_internal(bound_min: Vec3, bound_max: Vec3, left: u32, right: u32) -> Self {
        Self {
            bound_min: [bound_min.x, bound_min.y, bound_min.z],
            bound_max: [bound_max.x, bound_max.y, bound_max.z],
            left_first: left,
            tri_count: right,
        }
    }
    
    pub fn make_leaf(bound_min: Vec3, bound_max: Vec3, first: u32, count: u32) -> Self {
        Self {
            bound_min: [bound_min.x, bound_min.y, bound_min.z],
            bound_max: [bound_max.x, bound_max.y, bound_max.z],
            left_first: first | Self::LEAF_FLAG,
            tri_count: count,
        }
    }
    
    pub fn is_leaf(&self) -> bool {
        (self.left_first & Self::LEAF_FLAG) != 0
    }
}

/// Build-time node used during BVH construction
struct BuildNode {
    bounds_min: Vec3,
    bounds_max: Vec3,
    left: Option<Box<BuildNode>>,
    right: Option<Box<BuildNode>>,
    first_triangle: u32,
    triangle_count: u32,
}

impl BuildNode {
    fn make_leaf(bounds_min: Vec3, bounds_max: Vec3, first: u32, count: u32) -> Self {
        Self {
            bounds_min,
            bounds_max,
            left: None,
            right: None,
            first_triangle: first,
            triangle_count: count,
        }
    }
    
    fn make_internal(bounds_min: Vec3, bounds_max: Vec3, left: Box<BuildNode>, right: Box<BuildNode>) -> Self {
        Self {
            bounds_min,
            bounds_max,
            left: Some(left),
            right: Some(right),
            first_triangle: 0,
            triangle_count: 0,
        }
    }
}

#[derive(Debug, Default)]
pub struct Tree {
    pub sizes: [u32; 2],  // [node_count, triangle_count] for compatibility
    pub nodes: Vec<Node>,
    pub triangles: Vec<Triangle>,
    pub materials: Vec<Material>,
    pub centroids: Vec<Vec3>,
}

impl From<Mesh> for Tree {
    fn from(mesh: Mesh) -> Self {
        let mut ret = Self::new();
        ret.add_mesh(mesh);
        ret
    }
}

impl Tree {
    pub fn new() -> Self {
        Self {
            triangles: Vec::new(),
            nodes: Vec::new(),
            materials: Vec::new(),
            sizes: [0, 0],
            centroids: Vec::new(),
        }
    }
    
    pub fn build(&mut self) {
        let triangle_count = self.triangles.len();
        if triangle_count == 0 {
            return;
        }
        
        // Create triangle indices for sorting
        let mut indices: Vec<usize> = (0..triangle_count).collect();
        
        // Build the tree recursively
        let root = self.build_recursive(&mut indices, 0, triangle_count);
        
        // Flatten the tree into a compact array
        self.nodes.clear();
        self.flatten_tree(&root, &indices);
        
        // Reorder triangles based on the final ordering
        let mut new_triangles = Vec::with_capacity(triangle_count);
        let mut new_centroids = Vec::with_capacity(triangle_count);
        for &idx in &indices {
            let mut tri = self.triangles[idx];
            // Compute normal
            let normal = (tri.b - tri.a).truncate().cross((tri.c - tri.a).truncate()).normalize();
            tri.custom = normal;
            new_triangles.push(tri);
            new_centroids.push(self.centroids[idx]);
        }
        self.triangles = new_triangles;
        self.centroids = new_centroids;
        
        // Update sizes for shader
        self.sizes = [self.nodes.len() as u32, self.triangles.len() as u32];
    }
    
    fn build_recursive(&self, indices: &mut [usize], start: usize, end: usize) -> BuildNode {
        // Compute bounds for this node
        let mut bounds_min = Vec3::splat(f32::INFINITY);
        let mut bounds_max = Vec3::splat(f32::NEG_INFINITY);
        
        for i in start..end {
            let tri = &self.triangles[indices[i]];
            bounds_min = bounds_min.min(tri.a.truncate()).min(tri.b.truncate()).min(tri.c.truncate());
            bounds_max = bounds_max.max(tri.a.truncate()).max(tri.b.truncate()).max(tri.c.truncate());
        }
        
        let count = end - start;
        
        // Create leaf if few enough triangles
        const MAX_LEAF_SIZE: usize = 4;
        if count <= MAX_LEAF_SIZE {
            return BuildNode::make_leaf(bounds_min, bounds_max, start as u32, count as u32);
        }
        
        // Find best split axis and position using SAH
        let (split_axis, split_pos) = self.find_best_split(indices, start, end, &bounds_min, &bounds_max);
        
        // Partition triangles
        let mid = self.partition(indices, start, end, split_axis, split_pos);
        
        // Handle degenerate case
        if mid == start || mid == end {
            return BuildNode::make_leaf(bounds_min, bounds_max, start as u32, count as u32);
        }
        
        // Recursively build children
        let left = self.build_recursive(indices, start, mid);
        let right = self.build_recursive(indices, mid, end);
        
        BuildNode::make_internal(bounds_min, bounds_max, Box::new(left), Box::new(right))
    }
    
    fn find_best_split(&self, indices: &[usize], start: usize, end: usize, 
                       bounds_min: &Vec3, bounds_max: &Vec3) -> (usize, f32) {
        let mut best_axis = 0;
        let mut best_pos = 0.0;
        let mut best_cost = f32::INFINITY;
        
        const NUM_BINS: usize = 32;

        for axis in 0..3 {
            let min_val = bounds_min[axis];
            let max_val = bounds_max[axis];

            if (max_val - min_val).abs() < 0.001 {
                continue;
            }

            let bin_size = (max_val - min_val) / NUM_BINS as f32;
            if bin_size <= 0.0 {
                continue;
            }

            let mut bin_counts = [0u32; NUM_BINS];
            let mut bin_bounds_min = vec![Vec3::splat(f32::INFINITY); NUM_BINS];
            let mut bin_bounds_max = vec![Vec3::splat(f32::NEG_INFINITY); NUM_BINS];

            for j in start..end {
                let tri_idx = indices[j];
                let center = self.centroids[tri_idx];
                let mut bin = ((center[axis] - min_val) / bin_size) as isize;
                if bin < 0 {
                    bin = 0;
                }
                if bin as usize >= NUM_BINS {
                    bin = (NUM_BINS - 1) as isize;
                }
                let bin = bin as usize;
                bin_counts[bin] += 1;

                let tri = &self.triangles[tri_idx];
                let ta = tri.a.truncate();
                let tb = tri.b.truncate();
                let tc = tri.c.truncate();
                let tri_min = ta.min(tb).min(tc);
                let tri_max = ta.max(tb).max(tc);
                bin_bounds_min[bin] = bin_bounds_min[bin].min(tri_min);
                bin_bounds_max[bin] = bin_bounds_max[bin].max(tri_max);
            }

            let mut left_counts = [0u32; NUM_BINS];
            let mut left_bounds_min = vec![Vec3::splat(f32::INFINITY); NUM_BINS];
            let mut left_bounds_max = vec![Vec3::splat(f32::NEG_INFINITY); NUM_BINS];
            let mut running_count = 0u32;
            let mut running_min = Vec3::splat(f32::INFINITY);
            let mut running_max = Vec3::splat(f32::NEG_INFINITY);
            for i in 0..NUM_BINS {
                running_count += bin_counts[i];
                running_min = running_min.min(bin_bounds_min[i]);
                running_max = running_max.max(bin_bounds_max[i]);
                left_counts[i] = running_count;
                left_bounds_min[i] = running_min;
                left_bounds_max[i] = running_max;
            }

            let mut right_counts = [0u32; NUM_BINS];
            let mut right_bounds_min = vec![Vec3::splat(f32::INFINITY); NUM_BINS];
            let mut right_bounds_max = vec![Vec3::splat(f32::NEG_INFINITY); NUM_BINS];
            running_count = 0;
            running_min = Vec3::splat(f32::INFINITY);
            running_max = Vec3::splat(f32::NEG_INFINITY);
            for i in (0..NUM_BINS).rev() {
                running_count += bin_counts[i];
                running_min = running_min.min(bin_bounds_min[i]);
                running_max = running_max.max(bin_bounds_max[i]);
                right_counts[i] = running_count;
                right_bounds_min[i] = running_min;
                right_bounds_max[i] = running_max;
            }

            let parent_area = surface_area(bounds_min, bounds_max);
            const TRAVERSAL_COST: f32 = 1.0;
            const INTERSECTION_COST: f32 = 1.0;

            for i in 0..(NUM_BINS - 1) {
                let left_count = left_counts[i];
                let right_count = right_counts[i + 1];
                if left_count == 0 || right_count == 0 {
                    continue;
                }

                let left_area = surface_area(&left_bounds_min[i], &left_bounds_max[i]);
                let right_area = surface_area(&right_bounds_min[i + 1], &right_bounds_max[i + 1]);
                let cost = TRAVERSAL_COST
                    + INTERSECTION_COST
                        * (left_count as f32 * left_area + right_count as f32 * right_area)
                        / parent_area;

                if cost < best_cost {
                    best_cost = cost;
                    best_axis = axis;
                    best_pos = min_val + bin_size * (i + 1) as f32;
                }
            }
        }
        
        (best_axis, best_pos)
    }
    
    fn partition(&self, indices: &mut [usize], start: usize, end: usize, axis: usize, split_pos: f32) -> usize {
        let mut left = start;
        let mut right = end - 1;
        
        while left <= right {
            let center = self.centroids[indices[left]];

            if center[axis] < split_pos {
                left += 1;
            } else {
                indices.swap(left, right);
                if right == 0 { break; }
                right -= 1;
            }
        }
        
        left
    }
    
    fn flatten_tree(&mut self, node: &BuildNode, indices: &[usize]) -> u32 {
        let node_index = self.nodes.len() as u32;
        
        if node.left.is_none() {
            // Leaf node
            self.nodes.push(Node::make_leaf(
                node.bounds_min,
                node.bounds_max,
                node.first_triangle,
                node.triangle_count,
            ));
        } else {
            // Internal node - reserve space
            self.nodes.push(Node::make_internal(
                node.bounds_min,
                node.bounds_max,
                0, // Will be filled
                0, // Will be filled
            ));
            
            // Flatten children
            let left_index = self.flatten_tree(node.left.as_ref().unwrap(), indices);
            let right_index = self.flatten_tree(node.right.as_ref().unwrap(), indices);
            
            // Update the node with child indices
            self.nodes[node_index as usize].left_first = left_index;
            self.nodes[node_index as usize].tri_count = right_index;
        }
        
        node_index
    }
    
    pub fn add_mesh(&mut self, mesh: Mesh) {
        let material = self.materials.len() as u32;
        self.materials.push(mesh.material);
        for t in mesh.indices.chunks_exact(3) {
            let a = Vec4::from_array(mesh.vertices[t[0] as usize].position);
            let b = Vec4::from_array(mesh.vertices[t[1] as usize].position);
            let c = Vec4::from_array(mesh.vertices[t[2] as usize].position);
            let center3x = (a + b + c).xyz();
            self.centroids.push(center3x);
            self.triangles.push(Triangle {
                a,
                b,
                c,
                material,
                custom: center3x,
            });
        }
    }
}

fn surface_area(min: &Vec3, max: &Vec3) -> f32 {
    let d = *max - *min;
    2.0 * (d.x * d.y + d.x * d.z + d.y * d.z)
}

#[cfg(test)]
mod tests {
    use glam::Vec3;

    use super::*;

    #[test]
    fn simple_cube() {
        let mesh = Mesh::load_obj(
            include_bytes!("../../assets/cube.obj"),
            Material::new_lambertian(Vec3::new(0.5, 0.5, 0.5)),
        );
        let mut tree: Tree = mesh.into();
        tree.build();
        assert_eq!(tree.triangles.len(), 12);
        assert_eq!(tree.materials.len(), 1);
        // Should have much fewer nodes than old system
        assert!(tree.nodes.len() < 16);
    }

    #[test]
    fn suzanne() {
        let mesh = Mesh::load_obj(
            include_bytes!("../../assets/suzanne.obj"),
            Material::new_lambertian(Vec3::new(0.5, 0.5, 0.5)),
        );
        let mut tree: Tree = mesh.into();
        tree.build();
        assert_eq!(tree.triangles.len(), 979);
        assert_eq!(tree.materials.len(), 1);
        // Should have much fewer nodes than old system (was 1024)
        assert!(tree.nodes.len() < 700);
    }
}