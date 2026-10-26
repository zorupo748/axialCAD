//! Tests for AutoCAD's PLAN, VPOINT, and 3D View commands.

use cadcraft_engine::Session;
use cadcraft_engine::geom::Vec2;
use serde_json::json;

#[test]
fn test_plan_command_interactive_and_json() {
    let mut s = Session::new();
    s.viewport_px = (1200.0, 800.0);
    // Draw some geometry: line from (0,0) to (200, 100)
    s.execute("line", &json!({ "points": [[0.0, 0.0], [200.0, 100.0]] })).unwrap();

    // Pan away to an offset view
    s.execute("view.set", &json!({ "center": [1000.0, 1000.0], "height": 50.0 })).unwrap();
    let away_view = s.state().unwrap().view();
    assert_eq!(away_view.center, Vec2::new(1000.0, 1000.0));

    // 1. Run PLAN via command line with default Enter (Current UCS)
    s.cmdline("plan").unwrap();
    assert!(s.prompt_text().contains("Enter an option [Current ucs/Ucs/World] <Current>:"));
    s.cmdline("").unwrap();
    assert!(s.running.is_none());

    // Camera has returned to plan view centered on the geometry
    let restored_view = s.state().unwrap().view();
    assert!((restored_view.center.x - 100.0).abs() < 1e-4);
    assert!((restored_view.center.y - 50.0).abs() < 1e-4);
    assert!(restored_view.height >= 100.0);

    // 2. Pan away again and run PLAN with World option
    s.execute("view.set", &json!({ "center": [-500.0, -500.0], "height": 20.0 })).unwrap();
    s.cmdline("plan").unwrap();
    s.cmdline("w").unwrap();
    assert!(s.running.is_none());

    let world_view = s.state().unwrap().view();
    assert!((world_view.center.x - 100.0).abs() < 1e-4);
    assert!((world_view.center.y - 50.0).abs() < 1e-4);

    // 3. JSON commands: plan.world, plan.current, view.top
    let res = s.execute("plan.world", &json!({})).unwrap();
    assert_eq!(res["status"], "ok");
    assert_eq!(res["view"], "world_plan");

    let res_curr = s.execute("plan.current", &json!({})).unwrap();
    assert_eq!(res_curr["status"], "ok");
    assert_eq!(res_curr["view"], "current_plan");

    let res_top = s.execute("view.top", &json!({})).unwrap();
    assert_eq!(res_top["status"], "ok");
}

#[test]
fn test_vpoint_and_view_presets() {
    let mut s = Session::new();
    s.viewport_px = (1000.0, 1000.0);
    s.execute("line", &json!({ "points": [[-50.0, -50.0], [50.0, 50.0]] })).unwrap();

    // VPOINT with direction
    let res = s.execute("vpoint", &json!({ "direction": [0.0, 0.0, 1.0] })).unwrap();
    assert_eq!(res["status"], "ok");
    assert_eq!(res["direction"], json!([0.0, 0.0, 1.0]));

    let v = s.state().unwrap().view();
    assert!((v.center.x - 0.0).abs() < 1e-4);
    assert!((v.center.y - 0.0).abs() < 1e-4);

    // VPOINT interactive
    s.cmdline("vpoint").unwrap();
    assert!(s.prompt_text().contains("Current view direction"));
    s.cmdline("0,0,1").unwrap();
    assert!(s.running.is_none());

    // Standard orthographic/isometric presets
    let presets = [
        ("view.bottom", "bottom"),
        ("view.left", "left"),
        ("view.right", "right"),
        ("view.front", "front"),
        ("view.back", "back"),
        ("view.swiso", "swiso"),
        ("view.seiso", "seiso"),
        ("view.neiso", "neiso"),
        ("view.nwiso", "nwiso"),
    ];
    for (cmd, name) in presets {
        let r = s.execute(cmd, &json!({})).unwrap();
        assert_eq!(r["status"], "ok");
        assert_eq!(r["preset"], name);
    }
}

#[test]
fn test_3dorbit_and_interactive_step() {
    let mut s = Session::new();
    s.viewport_px = (800.0, 600.0);
    s.execute("line", &json!({ "points": [[0.0, 0.0], [10.0, 10.0]] })).unwrap();

    // 1. Initial camera is in Plan view
    assert!(s.camera().is_plan());
    assert_eq!(s.camera().view_name(), "TOP");

    // 2. Execute 3dorbit JSON
    let res = s.execute("3dorbit", &json!({ "azimuth": -45.0, "elevation": 35.264 })).unwrap();
    assert_eq!(res["status"], "ok");
    assert!(!s.camera().is_plan());
    assert_eq!(s.camera().view_name(), "SE ISO");

    // 3. Interactive 3DORBIT
    s.cmdline("3dorbit").unwrap();
    assert!(s.orbit_active());
    assert!(s.prompt_text().contains("Press ESC or ENTER to exit"));
    s.cmdline("").unwrap();
    assert!(!s.orbit_active());

    // 4. Orbit step drag updates angles
    let az0 = s.camera().azimuth;
    let el0 = s.camera().elevation;
    s.orbit_step(50.0, -30.0).unwrap();
    assert_ne!(s.camera().azimuth, az0);
    assert_ne!(s.camera().elevation, el0);

    // 5. Returning to plan via PLAN
    s.execute("plan", &json!({ "option": "world" })).unwrap();
    assert!(s.camera().is_plan());
    assert_eq!(s.camera().view_name(), "TOP");
}

#[test]
fn test_camera_orbit_does_not_alter_drawing_geometry() {
    let mut s = Session::new();
    s.viewport_px = (1000.0, 800.0);

    // Create a 2D line and a circle
    s.execute("line", &json!({ "points": [[10.0, 20.0], [50.0, 80.0]] })).unwrap();
    s.execute("circle", &json!({ "center": [30.0, 40.0], "radius": 15.0 })).unwrap();

    // Snapshot entity data before orbiting
    let doc_before = s.doc().unwrap().clone();
    let space = s.space();
    let entities_before: Vec<_> = doc_before.space(&space).unwrap().iter().map(|e| e.kind.clone()).collect();
    assert_eq!(entities_before.len(), 2);

    // Perform extensive 3D orbits and view transitions
    s.orbit_step(120.0, 45.0).unwrap();
    s.orbit_step(-80.0, -90.0).unwrap();
    s.execute("view.swiso", &json!({})).unwrap();
    s.orbit_step(50.0, 20.0).unwrap();
    s.execute("vpoint", &json!({ "direction": [1.0, 2.0, 3.0] })).unwrap();
    s.execute("view.front", &json!({})).unwrap();

    // Verify drawing geometry remains completely untouched
    let doc_after = s.doc().unwrap();
    let entities_after: Vec<_> = doc_after.space(&space).unwrap().iter().map(|e| e.kind.clone()).collect();
    assert_eq!(entities_before, entities_after, "3D camera orbit must NEVER alter drawing geometry or coordinates");
}
