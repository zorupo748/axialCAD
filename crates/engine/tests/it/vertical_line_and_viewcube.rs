//! Tests for ViewCube preset views, camera persistence across pan/zoom,
//! and 3D vertical line constraints (Z-Lock and Ortho 3D).

use cadcraft_engine::Session;
use cadcraft_geom::Vec3;
use serde_json::json;

#[test]
fn test_viewcube_views_and_camera_persistence() {
    let mut s = Session::new();
    s.viewport_px = (1000.0, 1000.0);

    // 1. Check all standard orthogonal and isometric views
    let views = [
        ("view.top", "TOP"),
        ("view.bottom", "BOTTOM"),
        ("view.front", "FRONT"),
        ("view.back", "BACK"),
        ("view.left", "LEFT"),
        ("view.right", "RIGHT"),
        ("view.swiso", "SW ISO"),
        ("view.seiso", "SE ISO"),
        ("view.neiso", "NE ISO"),
        ("view.nwiso", "NW ISO"),
    ];

    for (cmd, expected_name) in views {
        s.execute(cmd, &json!({})).unwrap();
        let cam = s.camera();
        assert_eq!(cam.view_name(), expected_name, "View {cmd} must set camera to {expected_name}");

        // Pan and zoom after setting view
        let orig_az = cam.azimuth;
        let orig_el = cam.elevation;
        s.pan_step(50.0, -25.0, 10.0).unwrap();
        s.zoom_about(1.2, cadcraft_geom::Vec2::ZERO).unwrap();

        let cam_after = s.camera();
        assert_eq!(cam_after.view_name(), expected_name, "Pan and zoom must not alter orientation of {expected_name}");
        assert!((cam_after.azimuth - orig_az).abs() < 1e-9);
        assert!((cam_after.elevation - orig_el).abs() < 1e-9);
    }
}

#[test]
fn test_vertical_line_zlock_keeps_xy_constant() {
    let mut s = Session::new();
    s.viewport_px = (1000.0, 1000.0);

    // Orient camera to SW Isometric
    s.execute("view.swiso", &json!({})).unwrap();

    // Start LINE command and specify first point P = (10, 20, 0)
    s.cmdline("line").unwrap();
    s.cmdline("10,20,0").unwrap();
    assert_eq!(s.last_point3, Vec3::new(10.0, 20.0, 0.0));

    // Enable Z-Lock via keyword "Vertical"
    s.cmdline("Vertical").unwrap();
    assert!(s.settings.zlock, "Vertical keyword must activate zlock");

    // Use Direct Distance Entry along Z: type 100
    s.cmdline("100").unwrap();

    // Verify the line drawn connects (10, 20, 0) to (10, 20, 100)
    let doc = s.doc().unwrap();
    let space = s.space();
    let entities: Vec<_> = doc.space(&space).unwrap().iter().collect();
    assert_eq!(entities.len(), 1);

    match &entities[0].kind {
        cadcraft_doc::EntityKind::Line(l) => {
            assert_eq!(l.a, Vec3::new(10.0, 20.0, 0.0), "Base point must be preserved");
            assert_eq!(l.b, Vec3::new(10.0, 20.0, 100.0), "End point must maintain X and Y and change only Z");
        }
        _ => panic!("Expected line entity"),
    }
}

#[test]
fn test_vertical_z_calculation_across_different_orientations() {
    let mut s = Session::new();
    s.viewport_px = (1000.0, 1000.0);
    let base = Vec3::new(15.0, 25.0, 10.0);

    // Test across SW ISO, Front, Right, and arbitrary orbit
    let presets = ["swiso", "seiso", "front", "right"];
    for preset in presets {
        s.execute(&format!("view.{preset}"), &json!({})).unwrap();
        let cam = s.camera();

        // Project a target point directly above base at height Z = 85.0
        let target_pt = Vec3::new(15.0, 25.0, 85.0);
        let q = cam.project_point(target_pt);

        // Calculate vertical Z from that screen coordinate
        let z = cam.unproject_vertical_z(q, base);
        assert!(
            (z - 85.0).abs() < 1e-4,
            "Orientation {preset}: expected Z = 85.0, got {z}"
        );
    }
}

#[test]
fn test_ortho_3d_and_free_drawing_modes() {
    let mut s = Session::new();
    s.viewport_px = (1000.0, 1000.0);
    s.execute("view.swiso", &json!({})).unwrap();

    // 1. Free drawing mode: draw a slanted line in 3D
    s.cmdline("line").unwrap();
    s.cmdline("0,0,0").unwrap();
    s.cmdline("30,40,50").unwrap();
    s.cmdline("").unwrap(); // Finish line

    // Verify slanted line stored exactly
    let doc = s.doc().unwrap();
    let space = s.space();
    let entities: Vec<_> = doc.space(&space).unwrap().iter().collect();
    assert_eq!(entities.len(), 1);
    match &entities[0].kind {
        cadcraft_doc::EntityKind::Line(l) => {
            assert_eq!(l.a, Vec3::new(0.0, 0.0, 0.0));
            assert_eq!(l.b, Vec3::new(30.0, 40.0, 50.0));
        }
        _ => panic!("Expected line entity"),
    }

    // 2. Ortho 3D mode enabled:
    s.settings.orthomode = true;
    s.cmdline("line").unwrap();
    s.cmdline("10,10,0").unwrap();

    // Simulate cursor moved straight up on screen from base projection
    let cam = s.camera();
    let base = Vec3::new(10.0, 10.0, 0.0);
    let p_screen = cam.project_point(base);
    let q_up = cadcraft_geom::Vec2::new(p_screen.x, p_screen.y + 40.0);
    let (snapped_pt, _) = cam.closest_axis_3d(q_up, base);

    assert_eq!(snapped_pt.x, 10.0);
    assert_eq!(snapped_pt.y, 10.0);
    assert!(snapped_pt.z > 0.0, "Ortho 3D must select Z axis when mouse moves vertically");

    // Click input with snapped point
    s.input(cadcraft_engine::Input::Point3(snapped_pt)).unwrap();
    s.cmdline("").unwrap();

    // Verify second line is strictly vertical
    let doc2 = s.doc().unwrap();
    let entities2: Vec<_> = doc2.space(&space).unwrap().iter().collect();
    assert_eq!(entities2.len(), 2);
    match &entities2[1].kind {
        cadcraft_doc::EntityKind::Line(l) => {
            assert_eq!(l.a.x, 10.0);
            assert_eq!(l.a.y, 10.0);
            assert_eq!(l.b.x, 10.0);
            assert_eq!(l.b.y, 10.0);
            assert!(l.b.z > l.a.z);
        }
        _ => panic!("Expected line entity"),
    }
}

#[test]
fn test_vertical_line_with_v_keyword_and_point_input_defense() {
    let mut s = Session::new();
    s.viewport_px = (1000.0, 1000.0);
    s.execute("view.swiso", &json!({})).unwrap();

    // Start LINE command
    s.cmdline("line").unwrap();
    // Pick first point at (20.0, 30.0, 5.0)
    s.input(cadcraft_engine::Input::Point3(Vec3::new(20.0, 30.0, 5.0))).unwrap();

    // Type "V" keyword at the prompt
    s.cmdline("V").unwrap();
    assert!(s.settings.zlock, "Typing 'V' must enable zlock");

    // Set cursor3 to represent vertical mouse motion to Z = 75.0
    s.cursor3 = Vec3::new(20.0, 30.0, 75.0);

    // Simulate an input: even if an Input::Point(99.0, 99.0) arrives from a 2D snap or mouse release,
    // LineM must defend against it and preserve X=20.0, Y=30.0, Z=75.0
    s.input(cadcraft_engine::Input::Point(cadcraft_geom::Vec2::new(99.0, 99.0))).unwrap();
    s.cmdline("").unwrap(); // Finish LINE

    let doc = s.doc().unwrap();
    let space = s.space();
    let entities: Vec<_> = doc.space(&space).unwrap().iter().collect();
    assert_eq!(entities.len(), 1);
    match &entities[0].kind {
        cadcraft_doc::EntityKind::Line(l) => {
            assert_eq!(l.a, Vec3::new(20.0, 30.0, 5.0));
            assert_eq!(l.b, Vec3::new(20.0, 30.0, 75.0));
        }
        _ => panic!("Expected line entity"),
    }
}

#[test]
fn test_zlock_command_toggle() {
    let mut s = Session::new();
    assert!(!s.settings.zlock);

    // Toggle on via command
    s.execute("zlock", &json!({})).unwrap();
    assert!(s.settings.zlock);

    // Toggle off via command
    s.execute("zlock", &json!({})).unwrap();
    assert!(!s.settings.zlock);

    // Explicit on/off
    s.execute("zlock", &json!({ "on": true })).unwrap();
    assert!(s.settings.zlock);
    s.execute("zlock", &json!({ "on": false })).unwrap();
    assert!(!s.settings.zlock);
}
