//! Comprehensive integration tests for 2D and 3D PAN operations in CADCraft.

use cadcraft_engine::Session;
use cadcraft_geom::{DrawingPlane, Vec3};
use serde_json::json;

#[test]
fn test_pan_step_in_2d_plan_view() {
    let mut s = Session::new();
    s.viewport_px = (1000.0, 1000.0);
    let scale = 10.0; // 10 pixels per drawing unit

    let initial_cam = s.camera();
    assert!(initial_cam.is_plan());

    // Pan by (+100px, -50px)
    s.pan_step(100.0, -50.0, scale).unwrap();

    let cam = s.camera();
    assert!(cam.is_plan(), "Pan must not alter 2D plan view orientation");
    // Mouse moved right (+100px) -> target.x moves by -10 units so geometry moves right (+10 units)
    assert!((cam.target.x - (initial_cam.target.x - 10.0)).abs() < 1e-6);
    // Mouse moved up (-50px) -> target.y moves by -5 units so geometry moves up on screen (-50px)
    assert!((cam.target.y - (initial_cam.target.y - 5.0)).abs() < 1e-6);
    assert_eq!(cam.target.z, initial_cam.target.z, "Pan in 2D must not introduce Z drift");

    // Check that engine view() matches camera target
    let view = s.state().unwrap().view();
    assert!((view.center.x - cam.target.x).abs() < 1e-6);
    assert!((view.center.y - cam.target.y).abs() < 1e-6);
}

#[test]
fn test_pan_step_in_3d_isometric_view() {
    let mut s = Session::new();
    s.viewport_px = (800.0, 600.0);
    let scale = 15.0;

    // Switch to SW ISO standard 3D view
    s.execute("view.swiso", &json!({})).unwrap();
    let cam_before = s.camera();
    assert!(!cam_before.is_plan());
    let az_before = cam_before.azimuth;
    let el_before = cam_before.elevation;

    // Drag mouse by (60px, 45px)
    let dx = 60.0;
    let dy = 45.0;
    s.pan_step(dx, dy, scale).unwrap();

    let cam_after = s.camera();
    assert!((cam_after.azimuth - az_before).abs() < 1e-9, "Pan must not change azimuth");
    assert!((cam_after.elevation - el_before).abs() < 1e-9, "Pan must not change elevation");

    // Translation must be strictly perpendicular to the camera view direction (parallel to screen plane)
    let delta_target = cam_after.target - cam_before.target;
    let view_dir = cam_after.view_dir();
    assert!(delta_target.dot(view_dir).abs() < 1e-6, "Target translation must be perpendicular to camera view vector");

    // Project a world point before and after pan:
    let test_pt = Vec3::new(25.0, 30.0, 15.0);
    let p_before = cam_before.project_point(test_pt);
    let p_after = cam_after.project_point(test_pt);

    // On screen (in drawing units before scale):
    // q_after.x = q_before.x + dx / scale
    // q_after.y = q_before.y - dy / scale
    assert!(((p_after.x - p_before.x) * scale - dx).abs() < 1e-6);
    assert!(((p_before.y - p_after.y) * scale - dy).abs() < 1e-6);
}

#[test]
fn test_pan_consistent_across_zoom_scales() {
    let mut s = Session::new();
    s.viewport_px = (1200.0, 800.0);

    for preset in ["top", "swiso", "front", "neiso"] {
        s.execute(&format!("view.{preset}"), &json!({})).unwrap();

        for scale in [2.5, 10.0, 50.0, 200.0] {
            let cam_start = s.camera();
            let dx = 75.0;
            let dy = -40.0;

            let mut cam = cam_start;
            cam.pan(dx, dy, scale);

            let pt = Vec3::new(12.0, -8.0, 5.0);
            let s0 = cam_start.project_point(pt);
            let s1 = cam.project_point(pt);

            let pixel_dx = (s1.x - s0.x) * scale;
            let pixel_dy = (s0.y - s1.y) * scale;

            assert!((pixel_dx - dx).abs() < 1e-5, "preset {preset} scale {scale}: expected dx {dx}, got {pixel_dx}");
            assert!((pixel_dy - dy).abs() < 1e-5, "preset {preset} scale {scale}: expected dy {dy}, got {pixel_dy}");
        }
    }
}

#[test]
fn test_pan_preserves_geometry_and_drawing_plane() {
    let mut s = Session::new();
    s.viewport_px = (1000.0, 1000.0);

    // Set drawing plane to XZ
    s.set_drawing_plane(DrawingPlane::XZ);

    // Draw 3D lines
    s.execute("line", &json!({ "points": [[0.0, 0.0, 0.0], [10.0, 20.0, 30.0]] })).unwrap();
    s.execute("line", &json!({ "points": [[10.0, 20.0, 30.0], [50.0, 50.0, 100.0]] })).unwrap();

    // Rotate to 3D view and perform multiple pans
    s.execute("view.seiso", &json!({})).unwrap();
    s.pan_step(120.0, 80.0, 12.0).unwrap();
    s.pan_step(-40.0, 30.0, 12.0).unwrap();

    // Verify drawing plane is still XZ
    assert_eq!(s.drawing_plane(), DrawingPlane::XZ);

    // Verify stored entity geometry is strictly unchanged
    let doc = s.doc().unwrap();
    let space = s.space();
    let entities: Vec<_> = doc.space(&space).unwrap().iter().collect();
    assert_eq!(entities.len(), 2);

    match &entities[0].kind {
        cadcraft_doc::EntityKind::Line(l) => {
            assert_eq!(l.a, Vec3::new(0.0, 0.0, 0.0));
            assert_eq!(l.b, Vec3::new(10.0, 20.0, 30.0));
        }
        _ => panic!("Expected line entity"),
    }

    match &entities[1].kind {
        cadcraft_doc::EntityKind::Line(l) => {
            assert_eq!(l.a, Vec3::new(10.0, 20.0, 30.0));
            assert_eq!(l.b, Vec3::new(50.0, 50.0, 100.0));
        }
        _ => panic!("Expected line entity"),
    }
}

#[test]
fn test_pan_orbit_zoom_consecutive_interoperability() {
    let mut s = Session::new();
    s.viewport_px = (1000.0, 1000.0);

    // 1. Start in SW ISO
    s.execute("view.swiso", &json!({})).unwrap();
    let initial_cam = s.camera();

    // 2. Pan 3D view
    s.pan_step(50.0, -30.0, 10.0).unwrap();
    let panned_cam = s.camera();
    assert_ne!(panned_cam.target, initial_cam.target);
    assert_eq!(panned_cam.azimuth, initial_cam.azimuth);
    assert_eq!(panned_cam.elevation, initial_cam.elevation);

    // 3. Orbit after pan
    s.orbit_step(40.0, 20.0).unwrap();
    let orbited_cam = s.camera();
    assert_ne!(orbited_cam.azimuth, panned_cam.azimuth);
    assert_ne!(orbited_cam.elevation, panned_cam.elevation);
    // Target remained the one established by the pan
    assert!((orbited_cam.target.x - panned_cam.target.x).abs() < 1e-6);
    assert!((orbited_cam.target.y - panned_cam.target.y).abs() < 1e-6);
    assert!((orbited_cam.target.z - panned_cam.target.z).abs() < 1e-6);

    // 4. Zoom about screen center
    s.zoom_about(1.5, cadcraft_geom::Vec2::new(orbited_cam.target.x, orbited_cam.target.y)).unwrap();
    let zoomed_cam = s.camera();
    assert!(zoomed_cam.height < orbited_cam.height);

    // 5. Restore PLAN view
    s.execute("plan.world", &json!({})).unwrap();
    let plan_cam = s.camera();
    assert!(plan_cam.is_plan());
}
