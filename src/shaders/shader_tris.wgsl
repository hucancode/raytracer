const PI = 3.141592653589793;
const PI2 = PI*2;
const EPSILON = 0.0001;
const FLT_MAX = 3.40282e+38;
const MAT_LAMBERTIAN = 1u;
const MAT_METAL = 2u;
const MAT_DIELECTRIC = 3u;
const SKY = vec3f(0.54, 0.86, 0.92);
const BLUE = vec3f(0.54, 0.7, 0.98);
const RED = vec3f(0.98, 0.2, 0.2);
const SAMPLE_FRAME = 1000;
const SAMPLE_PER_FRAME = 1;
const BOUNCE_MAX = 5;

@group(0) @binding(0)
var<uniform> resolution: vec2u;
@group(0) @binding(1)
var<uniform> frame_count: u32;
@group(0) @binding(2)
var<uniform> time: u32;
@group(0) @binding(3)
var<storage, read_write> image: array<f32>;
@group(0) @binding(4)
var<uniform> camera: Camera;
@group(1) @binding(0)
var<uniform> bvh_info: vec2u; // x: node count, y: triangle count
@group(1) @binding(1)
var<storage> nodes: array<Node>;
@group(1) @binding(2)
var<storage> triangles: array<Triangle>;
@group(1) @binding(3)
var<storage> materials: array<Material>;

struct Camera {
  eye: vec4f,
  direction: vec4f,
  up: vec4f,
  right: vec4f,
  focal_length: f32,
  focal_blur_amount: f32,
  fov: f32,
}

struct Ray {
  origin: vec3f,
  direction: vec3f,
}

struct Node {
  bound_min: vec3f,
  left_first: u32,  // MSB=1 for leaf, contains triangle start index
  bound_max: vec3f,
  tri_count: u32,   // For leaf: triangle count, for internal: right child index
}

struct Triangle {
  a: vec4f,
  b: vec4f,
  c: vec4f,
  normal: vec3f,
  material: u32,
}

struct Material {
    albedo: vec4f,
    params: vec3f,
    id: u32,
}

struct HitRecord {
  point: vec3f,
  normal: vec3f,
  t: f32,
  material: Material,
  front_face: bool,
}

const DEFAULT_MATERIAL = Material(vec4f(0.0,0.4,0.0,1.0), vec3f(), MAT_LAMBERTIAN);
const EMPTY_HIT_RECORD = HitRecord(vec3f(), vec3f(), FLT_MAX, DEFAULT_MATERIAL, false);

@vertex
fn vs_main(@builtin(vertex_index) vertexIndex: u32) -> @builtin(position) vec4f {
  // 2-triangles screen space
  let a = vec4f(-1.0, -1.0, 0.0, 1.0);
  let b = vec4f(1.0, -1.0, 0.0, 1.0);
  let c = vec4f(1.0, 1.0, 0.0, 1.0);
  let d = vec4f(-1.0, 1.0, 0.0, 1.0);
  switch (vertexIndex) {
    case 0u, 3u: { return a; }
    case 1u: { return b; }
    case 2u, 4u: { return c; }
    case 5u, default: { return d; }
  }
  return vec4f(0.0, 0.0, 0.0, 1.0);
}

fn rng_int(state: ptr<function, u32>) {
    // PCG random number generator
    // Based on https://www.shadertoy.com/view/XlGcRh
    let oldState = *state + 747796405u + 2891336453u;
    let word = ((oldState >> ((oldState >> 28u) + 4u)) ^ oldState) * 277803737u;
    *state = (word >> 22u) ^ word;
}

fn rng_float(state: ptr<function, u32>) -> f32 {
    rng_int(state);
    return f32(*state) / f32(0xffffffffu);
}

fn rng_vec2(state: ptr<function, u32>) -> vec2f {
    return vec2f(rng_float(state), rng_float(state));
}

fn rng_vec3(state: ptr<function, u32>) -> vec3f {
    return vec3f(rng_float(state), rng_float(state), rng_float(state));
}

fn point_on_ray(ray: Ray, t: f32) -> vec3f {
  return ray.origin + t * ray.direction;
}

fn random_on_hemisphere(state: ptr<function, u32>, normal: vec3f) -> vec3f {
  var n = normal;
  let len = length(n);
  if len < EPSILON {
    return vec3f(0.0, 1.0, 0.0);
  }
  n = normalize(n);

  let u = rng_float(state);
  let v = rng_float(state);
  let r = sqrt(u);
  let phi = PI2 * v;
  let local = vec3f(r * cos(phi), r * sin(phi), sqrt(max(0.0, 1.0 - u)));

  var tangent = vec3f();
  if abs(n.x) > abs(n.z) {
    tangent = normalize(vec3f(-n.y, n.x, 0.0));
  } else {
    tangent = normalize(vec3f(0.0, -n.z, n.y));
  }
  let bitangent = cross(n, tangent);

  let world = local.x * tangent + local.y * bitangent + local.z * n;
  return normalize(world);
}

fn random_on_disk(state: ptr<function, u32>, radius: f32) -> vec3f {
  let v = normalize(rng_vec2(state));
  let r = rng_float(state)*radius;
  return vec3f(v, 0.0)*r;
}

fn make_ray(uv: vec2f, state: ptr<function, u32>) -> Ray {
  let k = tan(camera.fov*0.5);
  let x = camera.right*uv.x*k;
  let y = camera.up*uv.y*k;
  let z = camera.direction;
  var direction = normalize(x+y+z);
  var origin = camera.eye;
  // focus blur
  let focus_point = origin + direction*camera.focal_length;
  origin += vec4f(random_on_disk(state, camera.focal_blur_amount), 1.0);
  direction = normalize(focus_point - origin);
  return Ray(origin.xyz, direction.xyz);
}

fn intersect_node(ray: Ray, inv_dir: vec3f, node: Node) -> vec2f {
  let t0 = (node.bound_min - ray.origin) * inv_dir;
  let t1 = (node.bound_max - ray.origin) * inv_dir;
  let tmin = min(t0, t1);
  let tmax = max(t0, t1);
  let tmin_final = max(max(tmin.x, tmin.y), tmin.z);
  let tmax_final = min(min(tmax.x, tmax.y), tmax.z);
  return vec2f(tmin_final, tmax_final);
}

fn intersect_triangle(ray: Ray, i: u32, ret: ptr<function, HitRecord>) {
  let a = triangles[i].a.xyz;
  let b = triangles[i].b.xyz;
  let c = triangles[i].c.xyz;
  let normal = triangles[i].normal;
  let material = triangles[i].material;

  // Moller-Trumbore intersection algorithm
  let edge1 = b - a;
  let edge2 = c - a;
  let h = cross(ray.direction, edge2);
  let det = dot(edge1, h);

  if abs(det) < EPSILON {
    return;
  }

  let inv_det = 1.0 / det;
  let s = ray.origin - a;
  let u = inv_det * dot(s, h);

  if u < 0.0 || u > 1.0 {
    return;
  }

  let q = cross(s, edge1);
  let v = inv_det * dot(ray.direction, q);

  if v < 0.0 || u + v > 1.0 {
    return;
  }

  let t = inv_det * dot(edge2, q);

  if t < EPSILON || t >= (*ret).t {
    return;
  }

  (*ret).point = point_on_ray(ray, t);
  (*ret).normal = normal;
  (*ret).t = t;
  (*ret).material = materials[material];
  (*ret).front_face = dot((*ret).normal, ray.direction) > 0;
}

fn reflect(v: vec3f, n: vec3f) -> vec3f {
    return v - 2*dot(v,n)*n;
}

fn refract(uv: vec3f, n: vec3f, etai_over_etat: f32) -> vec3f {
    let cos_theta = min(dot(-uv, n), 1.0);
    let r_out_perp =  etai_over_etat * (uv + cos_theta*n);
    let len = length(r_out_perp);
    let r_out_parallel = -sqrt(abs(1.0 - len*len)) * n;
    return r_out_perp + r_out_parallel;
}

fn reflectance(cosine: f32, ref_idx: f32) -> f32 {
    var r0 = (1-ref_idx) / (1+ref_idx);
    r0 = r0*r0;
    return r0 + (1-r0)*pow((1 - cosine), 5.0);
}

fn scatter(state: ptr<function, u32>, ray: Ray, hit: HitRecord) -> Ray {
  switch hit.material.id {
    case MAT_LAMBERTIAN: {
      let direction = random_on_hemisphere(state, hit.normal);
      return Ray(hit.point, normalize(direction));
    }
    case MAT_METAL: {
      let fuzziness = hit.material.params.x;
      let direction = reflect(ray.direction, hit.normal) + fuzziness * random_on_hemisphere(state, hit.normal);
      return Ray(hit.point, normalize(direction));
    }
    case MAT_DIELECTRIC: {
      var ir = hit.material.params.x;
      if hit.front_face {
        ir = 1.0/ir;
      }
      let cos_theta = min(dot(-ray.direction, hit.normal), 1.0);
      let sin_theta = sqrt(1.0 - cos_theta*cos_theta);
      let cannot_refract = ir * sin_theta > 1.0;
      if (cannot_refract || reflectance(cos_theta, ir) > fract(rng_float(state))) {
          let direction = reflect(ray.direction, hit.normal);
          return Ray(hit.point, normalize(direction));
      } else {
          let direction = refract(ray.direction, hit.normal, ir);
          return Ray(hit.point, normalize(direction));
      }
    }
    default: {
      return Ray(vec3f(), vec3f(0));
    }
  }
}

// stack-based BVH traversal
fn traverse_bvh(ray: Ray) -> HitRecord {
    var ret = EMPTY_HIT_RECORD;

    // Fixed-size stack (32 is enough for any reasonable BVH)
    var stack: array<u32, 32>;
    var stack_ptr = 0u;

    // Start with root node
    stack[0] = 0u;
    stack_ptr = 1u;

    var steps = 0u;
    let max_steps = 600u;
    let inv_dir = 1.0 / ray.direction;

    while stack_ptr > 0u && steps < max_steps {
        steps++;
        stack_ptr--;
        let node_idx = stack[stack_ptr];
        if node_idx >= bvh_info.x {
            continue;
        }
        let node = nodes[node_idx];

        // Check ray-box intersection
        let node_hit = intersect_node(ray, inv_dir, node);
        let node_tmin = node_hit.x;
        let node_tmax = node_hit.y;
        if node_tmin > node_tmax || node_tmax < 0.0 || node_tmin > ret.t {
            continue;
        }

        // Check if this is a leaf node (MSB set)
        if (node.left_first & 0x80000000u) != 0u {
            // Leaf node - test triangles
            let first_tri = node.left_first & 0x7FFFFFFFu;
            let tri_count = node.tri_count;

            for (var i = first_tri; i < first_tri + tri_count; i++) {
                if i < bvh_info.y {
                    intersect_triangle(ray, i, &ret);
                }
            }
        } else {
            // Internal node - push children to stack
            let left_idx = node.left_first;
            let right_idx = node.tri_count;

            var left_tmin = FLT_MAX;
            var right_tmin = FLT_MAX;
            var left_valid = false;
            var right_valid = false;

            if left_idx < bvh_info.x {
                let left_node = nodes[left_idx];
                let left_hit = intersect_node(ray, inv_dir, left_node);
                let left_tmax = left_hit.y;
                left_tmin = left_hit.x;
                left_valid = left_tmin <= left_tmax && left_tmax >= 0.0 && left_tmin <= ret.t;
            }

            if right_idx < bvh_info.x {
                let right_node = nodes[right_idx];
                let right_hit = intersect_node(ray, inv_dir, right_node);
                let right_tmax = right_hit.y;
                right_tmin = right_hit.x;
                right_valid = right_tmin <= right_tmax && right_tmax >= 0.0 && right_tmin <= ret.t;
            }

            if left_valid && right_valid {
                if left_tmin <= right_tmin {
                    if stack_ptr + 2u <= 32u {
                        stack[stack_ptr] = right_idx;
                        stack_ptr++;
                        stack[stack_ptr] = left_idx;
                        stack_ptr++;
                    }
                } else {
                    if stack_ptr + 2u <= 32u {
                        stack[stack_ptr] = left_idx;
                        stack_ptr++;
                        stack[stack_ptr] = right_idx;
                        stack_ptr++;
                    }
                }
            } else if left_valid {
                if stack_ptr + 1u <= 32u {
                    stack[stack_ptr] = left_idx;
                    stack_ptr++;
                }
            } else if right_valid {
                if stack_ptr + 1u <= 32u {
                    stack[stack_ptr] = right_idx;
                    stack_ptr++;
                }
            }
        }
    }

    return ret;
}

fn trace(ray: Ray, state: ptr<function, u32>) -> vec3f {
  var attenuation = vec3f(1);
  var current_ray = ray;

  for(var b = 0; b < BOUNCE_MAX; b++) {
    var hit = traverse_bvh(current_ray);
    if abs(hit.t - FLT_MAX) < EPSILON {
      break;
    }
    current_ray = scatter(state, current_ray, hit);
    attenuation *= hit.material.albedo.rgb * 0.7;
  }

  let sky = mix(SKY, BLUE, ray.direction.y*0.5 + 0.5);
  return attenuation * sky;
}

@fragment
fn fs_main(@builtin(position) position: vec4f) -> @location(0) vec4f {
  var rng_state = (u32(position.x)*resolution.y + u32(position.y)) * time;
  let aspect_ratio = f32(resolution.x) / f32(resolution.y);
  let position_aa = position.xy + normalize(rng_vec2(&rng_state));
  var uv = position_aa / (vec2f(resolution) - vec2f(1));
  uv = (2 * uv - vec2(1)) * vec2(aspect_ratio, -1);
  let ray = make_ray(uv, &rng_state);

  var color = vec3f(0);
  for (var i = 0; i < SAMPLE_PER_FRAME; i += 1) {
    color += trace(ray, &rng_state);
  }
  color /= f32(SAMPLE_PER_FRAME);

  let x = u32(position.x);
  let y = u32(position.y);
  let i = (y * resolution.x + x)*3;
  let oldColor = vec3f(image[i], image[i+1], image[i+2]);
  let newColor = mix(oldColor, color, 1.0/(min(f32(frame_count), f32(SAMPLE_FRAME))+1));
  image[i] = newColor.r;
  image[i+1] = newColor.g;
  image[i+2] = newColor.b;
  return vec4f(newColor, 1.0);
}
