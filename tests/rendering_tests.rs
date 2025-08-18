use wgsl_toy::render_ppm;
use wgsl_toy::scene::{Camera, Scene, SceneSphere, Sphere};
use wgsl_toy::renderer::RenderOutput;
use glam::Vec3;
use std::fs;
use std::path::Path;

const TEST_WIDTH: u32 = 512;
const TEST_HEIGHT: u32 = 512;
const TEST_FRAMES: u32 = 10;
const TOLERANCE_PERCENT: f32 = 2.0;

/// Helper function to render a scene and compare with golden reference
fn test_scene_against_golden(
    scene: &mut SceneSphere,
    golden_name: &str,
) -> Result<(), String> {
    // Initialize scene
    scene.init();

    // Render frames
    for i in 0..TEST_FRAMES {
        scene.set_time(1000 + i * 10);
        scene.draw();
    }

    // Get rendered output
    let ppm_data = render_ppm(&mut scene.renderer);

    // Save test output for debugging
    let output_path = format!("tests/output/{}.ppm", golden_name);
    fs::create_dir_all("tests/output").ok();
    fs::write(&output_path, &ppm_data).map_err(|e| format!("Failed to write output: {}", e))?;

    // Load golden reference
    let golden_path = format!("tests/golden/{}.ppm", golden_name);
    if !Path::new(&golden_path).exists() {
        return Err(format!("Golden reference {} not found. Run the corresponding _snapshot test to generate it.", golden_path));
    }

    let golden_data = fs::read_to_string(&golden_path)
        .map_err(|e| format!("Failed to read golden reference: {}", e))?;

    // Compare images
    compare_ppm_images(&ppm_data, &golden_data, TOLERANCE_PERCENT)
        .map_err(|e| format!("Image comparison failed: {:?}", e))?;

    Ok(())
}

/// Helper function to generate golden reference image
fn generate_golden_image(scene: &mut SceneSphere, golden_name: &str) {
    // Initialize scene
    scene.init();

    // Render frames
    println!("Generating golden image for {}", golden_name);
    for i in 0..TEST_FRAMES {
        scene.set_time(1000 + i * 10);
        scene.draw();
        if (i + 1) % 25 == 0 {
            println!("  Progress: {}/{}", i + 1, TEST_FRAMES);
        }
    }

    // Get rendered output
    let ppm_data = render_ppm(&mut scene.renderer);

    // Save golden reference
    fs::create_dir_all("tests/golden").expect("Failed to create golden directory");
    let golden_path = format!("tests/golden/{}.ppm", golden_name);
    fs::write(&golden_path, &ppm_data).expect("Failed to write golden reference");

    println!("✓ Golden image saved to {}", golden_path);
}

#[derive(Debug)]
enum ComparisonError {
    DifferentDimensions,
    PixelCountMismatch,
    ExcessiveDifference { avg_diff: f32 },
}

fn compare_ppm_images(
    img1: &str,
    img2: &str,
    tolerance_percent: f32,
) -> Result<(), ComparisonError> {
    let lines1: Vec<&str> = img1.lines().collect();
    let lines2: Vec<&str> = img2.lines().collect();

    // Check headers match
    if lines1.len() < 2 || lines2.len() < 2 || lines1[1] != lines2[1] {
        return Err(ComparisonError::DifferentDimensions);
    }

    // Parse pixels
    let pixels1: Vec<u8> = lines1[2..]
        .join(" ")
        .split_whitespace()
        .filter_map(|s| s.parse::<u8>().ok())
        .collect();

    let pixels2: Vec<u8> = lines2[2..]
        .join(" ")
        .split_whitespace()
        .filter_map(|s| s.parse::<u8>().ok())
        .collect();

    if pixels1.len() != pixels2.len() {
        return Err(ComparisonError::PixelCountMismatch);
    }

    // Calculate differences
    let total_diff: f32 = pixels1
        .iter()
        .zip(pixels2.iter())
        .map(|(a, b)| (*a as f32 - *b as f32).abs())
        .sum();

    let avg_diff = total_diff / pixels1.len() as f32;
    let avg_diff_percent = (avg_diff / 255.0) * 100.0;

    if avg_diff_percent > tolerance_percent {
        return Err(ComparisonError::ExcessiveDifference {
            avg_diff: avg_diff_percent,
        });
    }

    Ok(())
}

// Helper function to create lambertian test scene
fn create_lambertian_scene() -> SceneSphere {
    let mut scene = pollster::block_on(SceneSphere::new(
        RenderOutput::Headless(TEST_WIDTH, TEST_HEIGHT),
    ));

    scene.objects.clear();

    // Red lambertian sphere
    scene.objects.push(Sphere::new_lambertian(
        Vec3::new(-2.0, 0.0, -5.0),
        1.0,
        Vec3::new(0.8, 0.2, 0.2),
    ));

    // Green lambertian sphere
    scene.objects.push(Sphere::new_lambertian(
        Vec3::new(0.0, 0.0, -5.0),
        1.0,
        Vec3::new(0.2, 0.8, 0.2),
    ));

    // Blue lambertian sphere
    scene.objects.push(Sphere::new_lambertian(
        Vec3::new(2.0, 0.0, -5.0),
        1.0,
        Vec3::new(0.2, 0.2, 0.8),
    ));

    // Ground
    scene.objects.push(Sphere::new_lambertian(
        Vec3::new(0.0, -101.0, -5.0),
        100.0,
        Vec3::new(0.5, 0.5, 0.5),
    ));

    scene
}

// Test 1: Lambertian (Diffuse) Materials
#[test]
fn test_lambertian_materials() {
    let mut scene = create_lambertian_scene();
    test_scene_against_golden(&mut scene, "lambertian_materials")
        .expect("Lambertian materials test failed");
}

#[test]
#[ignore] // Run with --ignored to generate golden images
fn test_lambertian_materials_snapshot() {
    let mut scene = create_lambertian_scene();
    generate_golden_image(&mut scene, "lambertian_materials");
}

// Helper function to create metal test scene
fn create_metal_scene() -> SceneSphere {
    let mut scene = pollster::block_on(SceneSphere::new(
        RenderOutput::Headless(TEST_WIDTH, TEST_HEIGHT),
    ));

    scene.objects.clear();

    // Perfect mirror (no fuzz)
    scene.objects.push(Sphere::new_metal(
        Vec3::new(-2.0, 0.0, -5.0),
        1.0,
        Vec3::new(0.8, 0.8, 0.8),
        0.0,
    ));

    // Slightly rough metal
    scene.objects.push(Sphere::new_metal(
        Vec3::new(0.0, 0.0, -5.0),
        1.0,
        Vec3::new(0.8, 0.6, 0.2),
        0.2,
    ));

    // Very rough metal
    scene.objects.push(Sphere::new_metal(
        Vec3::new(2.0, 0.0, -5.0),
        1.0,
        Vec3::new(0.6, 0.2, 0.8),
        0.5,
    ));

    // Ground
    scene.objects.push(Sphere::new_lambertian(
        Vec3::new(0.0, -101.0, -5.0),
        100.0,
        Vec3::new(0.5, 0.5, 0.5),
    ));

    scene
}

// Test 2: Metal Materials
#[test]
fn test_metal_materials() {
    let mut scene = create_metal_scene();
    test_scene_against_golden(&mut scene, "metal_materials")
        .expect("Metal materials test failed");
}

#[test]
#[ignore]
fn test_metal_materials_snapshot() {
    let mut scene = create_metal_scene();
    generate_golden_image(&mut scene, "metal_materials");
}

// Helper function to create dielectric test scene
fn create_dielectric_scene() -> SceneSphere {
    let mut scene = pollster::block_on(SceneSphere::new(
        RenderOutput::Headless(TEST_WIDTH, TEST_HEIGHT),
    ));

    scene.objects.clear();

    // Glass sphere (IOR 1.5)
    scene.objects.push(Sphere::new_dielectric(
        Vec3::new(0.0, 0.0, -5.0),
        1.5,
        1.5,
    ));

    // Smaller glass spheres around it
    scene.objects.push(Sphere::new_dielectric(
        Vec3::new(-2.0, 0.0, -4.0),
        0.5,
        1.33, // Water
    ));

    scene.objects.push(Sphere::new_dielectric(
        Vec3::new(2.0, 0.0, -4.0),
        0.5,
        2.4, // Diamond
    ));

    // Add some colored spheres behind for refraction
    scene.objects.push(Sphere::new_lambertian(
        Vec3::new(0.0, 0.0, -8.0),
        1.0,
        Vec3::new(1.0, 0.0, 0.0),
    ));

    // Ground
    scene.objects.push(Sphere::new_lambertian(
        Vec3::new(0.0, -101.5, -5.0),
        100.0,
        Vec3::new(0.5, 0.5, 0.5),
    ));

    scene
}

// Test 3: Dielectric (Glass) Materials
#[test]
fn test_dielectric_materials() {
    let mut scene = create_dielectric_scene();
    test_scene_against_golden(&mut scene, "dielectric_materials")
        .expect("Dielectric materials test failed");
}

#[test]
#[ignore]
fn test_dielectric_materials_snapshot() {
    let mut scene = create_dielectric_scene();
    generate_golden_image(&mut scene, "dielectric_materials");
}

// Helper function to create camera position test scene
fn create_camera_position_scene() -> SceneSphere {
    let mut scene = pollster::block_on(SceneSphere::new(
        RenderOutput::Headless(TEST_WIDTH, TEST_HEIGHT),
    ));

    scene.objects.clear();

    // Create a simple test scene
    for i in -2i32..=2 {
        scene.objects.push(Sphere::new_lambertian(
            Vec3::new(i as f32 * 1.5, 0.0, -5.0 - i.abs() as f32),
            0.5,
            Vec3::new(0.5 + i as f32 * 0.1, 0.5, 0.5 - i as f32 * 0.1),
        ));
    }

    // Ground
    scene.objects.push(Sphere::new_lambertian(
        Vec3::new(0.0, -100.5, -5.0),
        100.0,
        Vec3::new(0.5, 0.5, 0.5),
    ));

    // Set custom camera position
    scene.camera = Camera::new(
        Vec3::new(3.0, 1.5, -2.0),
        Vec3::new(0.0, 0.0, -5.0),
        5.0,
        0.1,
        0.8,
    );

    scene
}

// Test 4: Camera Position and Angle
#[test]
fn test_camera_position() {
    let mut scene = create_camera_position_scene();
    test_scene_against_golden(&mut scene, "camera_position")
        .expect("Camera position test failed");
}

#[test]
#[ignore]
fn test_camera_position_snapshot() {
    let mut scene = create_camera_position_scene();
    generate_golden_image(&mut scene, "camera_position");
}

// Helper function to create depth of field test scene
fn create_depth_of_field_scene() -> SceneSphere {
    let mut scene = pollster::block_on(SceneSphere::new(
        RenderOutput::Headless(TEST_WIDTH, TEST_HEIGHT),
    ));

    scene.objects.clear();

    // Spheres at different depths
    for i in -3i32..=3 {
        let z = -3.0 - i.abs() as f32 * 2.0;
        scene.objects.push(Sphere::new_lambertian(
            Vec3::new(i as f32, 0.0, z),
            0.4,
            Vec3::new(
                1.0 - (i + 3) as f32 / 6.0,
                0.5,
                (i + 3) as f32 / 6.0,
            ),
        ));
    }

    // Ground
    scene.objects.push(Sphere::new_lambertian(
        Vec3::new(0.0, -100.4, -5.0),
        100.0,
        Vec3::new(0.5, 0.5, 0.5),
    ));

    // Camera with strong depth of field
    scene.camera = Camera::new(
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, 0.0, -5.0),
        5.0,   // focal_length (focus on middle sphere)
        0.3,   // large aperture for strong DOF
        0.8,
    );

    scene
}

// Test 5: Depth of Field
#[test]
fn test_depth_of_field() {
    let mut scene = create_depth_of_field_scene();
    test_scene_against_golden(&mut scene, "depth_of_field")
        .expect("Depth of field test failed");
}

#[test]
#[ignore]
fn test_depth_of_field_snapshot() {
    let mut scene = create_depth_of_field_scene();
    generate_golden_image(&mut scene, "depth_of_field");
}

// Helper function to create complex scene
fn create_complex_scene() -> SceneSphere {
    let mut scene = pollster::block_on(SceneSphere::new(
        RenderOutput::Headless(TEST_WIDTH, TEST_HEIGHT),
    ));

    scene.objects.clear();

    // Create the same complex scene as our golden
    for i in -2i32..=2 {
        for j in -2i32..=2 {
            if i == 0 && j == 0 {
                // Central glass sphere
                scene.objects.push(Sphere::new_dielectric(
                    Vec3::new(0.0, 0.0, -5.0),
                    0.8,
                    1.5,
                ));
            } else {
                let x = i as f32 * 1.2;
                let z = -5.0 + j as f32 * 1.2;
                let material_type = (i + j).abs() as i32 % 3;

                let sphere = match material_type {
                    0 => Sphere::new_lambertian(
                        Vec3::new(x, 0.0, z),
                        0.3,
                        Vec3::new(0.7, 0.3, 0.3),
                    ),
                    1 => Sphere::new_metal(
                        Vec3::new(x, 0.0, z),
                        0.3,
                        Vec3::new(0.7, 0.7, 0.7),
                        0.1,
                    ),
                    _ => Sphere::new_dielectric(Vec3::new(x, 0.0, z), 0.3, 1.33),
                };

                scene.objects.push(sphere);
            }
        }
    }

    // Ground
    scene.objects.push(Sphere::new_lambertian(
        Vec3::new(0.0, -100.3, -5.0),
        100.0,
        Vec3::new(0.5, 0.5, 0.5),
    ));

    scene
}

// Test 6: Complex Scene
#[test]
fn test_complex_scene() {
    let mut scene = create_complex_scene();
    test_scene_against_golden(&mut scene, "complex_scene")
        .expect("Complex scene test failed");
}

#[test]
#[ignore]
fn test_complex_scene_snapshot() {
    let mut scene = create_complex_scene();
    generate_golden_image(&mut scene, "complex_scene");
}

// Helper function to create shadow test scene
fn create_shadow_scene() -> SceneSphere {
    let mut scene = pollster::block_on(SceneSphere::new(
        RenderOutput::Headless(TEST_WIDTH, TEST_HEIGHT),
    ));

    scene.objects.clear();

    // Large sphere casting shadow
    scene.objects.push(Sphere::new_lambertian(
        Vec3::new(0.0, 2.0, -5.0),
        2.0,
        Vec3::new(0.7, 0.3, 0.3),
    ));

    // Small sphere in shadow area
    scene.objects.push(Sphere::new_lambertian(
        Vec3::new(0.0, -0.5, -5.0),
        0.5,
        Vec3::new(0.3, 0.7, 0.3),
    ));

    // Ground to receive shadows
    scene.objects.push(Sphere::new_lambertian(
        Vec3::new(0.0, -101.0, -5.0),
        100.0,
        Vec3::new(0.8, 0.8, 0.8),
    ));

    scene
}

// Test 7: Shadow rendering
#[test]
fn test_shadow_rendering() {
    let mut scene = create_shadow_scene();
    test_scene_against_golden(&mut scene, "shadow_rendering")
        .expect("Shadow rendering test failed");
}

#[test]
#[ignore]
fn test_shadow_rendering_snapshot() {
    let mut scene = create_shadow_scene();
    generate_golden_image(&mut scene, "shadow_rendering");
}

// Test 8: Performance test - ensure rendering completes in reasonable time
#[test]
fn test_rendering_performance() {
    use std::time::Instant;

    let mut scene = pollster::block_on(SceneSphere::new(
        RenderOutput::Headless(TEST_WIDTH, TEST_HEIGHT),
    ));

    // Create a moderately complex scene
    scene.objects.clear();
    for i in 0..20 {
        let angle = i as f32 * std::f32::consts::PI * 2.0 / 20.0;
        let x = angle.cos() * 3.0;
        let z = -5.0 + angle.sin() * 3.0;

        scene.objects.push(Sphere::new_lambertian(
            Vec3::new(x, 0.0, z),
            0.4,
            Vec3::new(
                (i as f32 / 20.0),
                0.5,
                1.0 - (i as f32 / 20.0),
            ),
        ));
    }

    scene.objects.push(Sphere::new_lambertian(
        Vec3::new(0.0, -100.4, -5.0),
        100.0,
        Vec3::new(0.5, 0.5, 0.5),
    ));

    scene.init();

    let start = Instant::now();

    for i in 0..TEST_FRAMES {
        scene.set_time(1000 + i * 10);
        scene.draw();
    }

    let elapsed = start.elapsed();

    // Ensure 100 frames complete in under 5 seconds
    assert!(
        elapsed.as_secs() < 5,
        "Rendering took too long: {:?}",
        elapsed
    );

    println!("Performance: {} frames in {:?}", TEST_FRAMES, elapsed);
}
