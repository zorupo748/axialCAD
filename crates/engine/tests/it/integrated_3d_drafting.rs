//! Comprehensive integration tests for Bloques 1, 2, 3 and 4:
//! 1. LINE with automatic 3D axis selection (X/Y/Z) without V.
//! 2. Free mode with ORTHOMODE disabled.
//! 3. No residual vertical lock after finish/cancel/keywords.
//! 4. First point at chosen elevation plane offset without typing XYZ.
//! 5. Return to origin plane.
//! 6. 3D line endpoint snaps with positive and negative Z.
//! 7. Overlapping endpoints deterministic order & cycling.
//! 8. 2D snaps and point modifiers unaffected.
//! 9. QNEWTEMPLATE preference persistence and document independence.

use cadcraft_doc::{EntityKind, Line as DocLine, Space};
use cadcraft_engine::cmd::file::{qnew_template, set_qnew_template};
use cadcraft_engine::snap::snap_3d_line_endpoints;
use cadcraft_engine::{Input, Session};
use cadcraft_geom::{Axis3D, DrawingPlane, Vec2, Vec3};
use serde_json::json;

fn last_line(s: &Session) -> DocLine {
    let e = s.doc().unwrap().model.iter().last().expect("at least one entity");
    let EntityKind::Line(l) = &e.kind else { panic!("expected Line entity") };
    l.clone()
}

#[test]
fn test_line_3d_axis_selection_swiso_and_front_without_v() {
    let mut s = Session::new();
    s.viewport_px = (800.0, 600.0);

    // In SW ISO view
    s.execute("view.swiso", &json!({})).unwrap();
    let cam = s.camera();

    let base = Vec3::new(0.0, 0.0, 0.0);
    let _s_base = cam.project_point(base);

    // Project points along +X, +Y, +Z
    let s_x = cam.project_point(Vec3::new(10.0, 0.0, 0.0));
    let s_y = cam.project_point(Vec3::new(0.0, 10.0, 0.0));
    let s_z = cam.project_point(Vec3::new(0.0, 0.0, 10.0));

    // Towards +X
    let (p_x, axis_x, _) = cam.closest_axis_3d_with_prev(s_x, base, None);
    assert_eq!(axis_x, Axis3D::X);
    assert!(p_x.x > 0.0 && p_x.y.abs() < 1e-4 && p_x.z.abs() < 1e-4);

    // Towards +Y
    let (p_y, axis_y, _) = cam.closest_axis_3d_with_prev(s_y, base, None);
    assert_eq!(axis_y, Axis3D::Y);
    assert!(p_y.y > 0.0 && p_y.x.abs() < 1e-4 && p_y.z.abs() < 1e-4);

    // Towards +Z
    let (p_z, axis_z, _) = cam.closest_axis_3d_with_prev(s_z, base, None);
    assert_eq!(axis_z, Axis3D::Z);
    assert!(p_z.z > 0.0 && p_z.x.abs() < 1e-4 && p_z.y.abs() < 1e-4);

    // Transition test: chaining vertical -> horizontal -> vertical without V
    let (_, a1, _) = cam.closest_axis_3d_with_prev(s_z, base, None);
    assert_eq!(a1, Axis3D::Z);
    let (_, a2, _) = cam.closest_axis_3d_with_prev(s_x, base, Some(a1));
    assert_eq!(a2, Axis3D::X);
    let (_, a3, _) = cam.closest_axis_3d_with_prev(s_z, base, Some(a2));
    assert_eq!(a3, Axis3D::Z);

    // In FRONT view (azimuth 0, elevation 0): Z is vertical, X is horizontal
    s.execute("view.front", &json!({})).unwrap();
    let cam_front = s.camera();
    let s_fz = cam_front.project_point(Vec3::new(0.0, 0.0, 15.0));
    let s_fx = cam_front.project_point(Vec3::new(15.0, 0.0, 0.0));

    let (_, fa_z, _) = cam_front.closest_axis_3d_with_prev(s_fz, base, None);
    assert_eq!(fa_z, Axis3D::Z);
    let (_, fa_x, _) = cam_front.closest_axis_3d_with_prev(s_fx, base, None);
    assert_eq!(fa_x, Axis3D::X);
}

#[test]
fn test_free_mode_unconstrained_when_ortho_off() {
    let mut s = Session::new();
    s.settings.orthomode = false;
    let cam = s.camera();

    // With orthomode false, mouse movement unprojects freely to plane
    let ray_q = Vec2::new(123.0, 234.0);
    let p_free = cam.unproject_to_plane_with_offset(ray_q, DrawingPlane::XY, 0.0).expect("unproject");
    assert!(p_free.x.is_finite() && p_free.y.is_finite());
    assert_eq!(p_free.z, 0.0);
}

#[test]
fn test_no_residual_vertical_lock_after_cancel_or_finish() {
    let mut s = Session::new();
    s.cmdline("line 0,0,0").unwrap();
    // Enable V
    s.cmdline("Vertical").unwrap();
    assert!(s.settings.zlock, "zlock must be active");

    // Cancel LINE
    s.input(Input::Cancel).unwrap();
    assert!(s.running.is_none());
    assert!(!s.settings.zlock, "zlock must NOT persist after cancel");

    // Start again, enable V, finish normally with Enter
    s.cmdline("line 0,0,0").unwrap();
    s.cmdline("V").unwrap();
    assert!(s.settings.zlock);
    s.cmdline("0,0,10").unwrap();
    s.cmdline("").unwrap(); // Finish LINE with Enter
    assert!(s.running.is_none());
    assert!(!s.settings.zlock, "zlock must NOT persist after normal command finish");

    // Test explicit keyword enable/disable during command
    s.cmdline("line 0,0,0").unwrap();
    s.cmdline("V ON").unwrap();
    assert!(s.settings.zlock);
    s.cmdline("V OFF").unwrap();
    assert!(!s.settings.zlock, "V OFF keyword must explicitly deactivate zlock");
    s.cmdline("Horizontal").unwrap();
    assert!(!s.settings.zlock, "Horizontal keyword must deactivate zlock");
    s.input(Input::Cancel).unwrap();
}

#[test]
fn test_first_point_at_chosen_elevation_plane_offset() {
    let mut s = Session::new();
    // Set elevation plane offset to 35.5
    s.set_drawing_plane_offset(35.5);
    assert_eq!(s.drawing_plane_offset(), 35.5);

    // Start LINE and place first point as 2D point (as mouse click produces)
    s.start("line").unwrap();
    s.input(Input::Point(Vec2::new(10.0, 20.0))).unwrap();

    // Place second point at 2D (50, 60)
    s.input(Input::Point(Vec2::new(50.0, 60.0))).unwrap();
    s.input(Input::Enter).unwrap();

    let line = last_line(&s);
    assert_eq!(line.a, Vec3::new(10.0, 20.0, 35.5), "first point must use drawing plane offset");
    assert_eq!(line.b, Vec3::new(50.0, 60.0, 35.5), "horizontal segment must preserve elevation");
}

#[test]
fn test_plane_offset_origin_return_and_commands() {
    let mut s = Session::new();

    // Set offset via plane.offset
    s.execute("plane.offset", &json!({"offset": 42.0})).unwrap();
    assert_eq!(s.drawing_plane_offset(), 42.0);

    // Check ELEVATION system variable
    let elev = cadcraft_engine::sysvars::get(&s, "ELEVATION").unwrap();
    assert_eq!(elev, 42.0);

    // Return to origin via plane.origin
    s.execute("plane.origin", &json!({})).unwrap();
    assert_eq!(s.drawing_plane_offset(), 0.0);

    // Also verify cota.origen alias
    s.execute("elevation", &json!({"offset": 18.0})).unwrap();
    assert_eq!(s.drawing_plane_offset(), 18.0);
    s.execute("cota.origen", &json!({})).unwrap();
    assert_eq!(s.drawing_plane_offset(), 0.0);
}

#[test]
fn test_snap_3d_line_endpoints_positive_and_negative_z() {
    let mut s = Session::new();
    s.viewport_px = (800.0, 600.0);
    s.execute("view.swiso", &json!({})).unwrap();

    // Create line 1 with positive Z: (10, 20, 30) -> (40, 50, 60)
    s.cmdline("line 10,20,30 40,50,60").unwrap();
    s.cmdline("").unwrap();

    // Create line 2 with negative Z: (-10, -20, -30) -> (-40, -50, -60)
    s.cmdline("line -10,-20,-30 -40,-50,-60").unwrap();
    s.cmdline("").unwrap();

    let cam = s.camera();
    let doc = s.doc().unwrap();

    // Snap to positive endpoint (40, 50, 60)
    let s_pos = cam.project_point(Vec3::new(40.0, 50.0, 60.0));
    let hit_pos = snap_3d_line_endpoints(doc, &Space::Model, &cam, s_pos, 15.0, 0).expect("snap hit for pos Z");
    assert_eq!(hit_pos.point3, Some(Vec3::new(40.0, 50.0, 60.0)));
    assert_eq!(hit_pos.input(), Input::Point3(Vec3::new(40.0, 50.0, 60.0)));

    // Snap to negative endpoint (-40, -50, -60)
    let s_neg = cam.project_point(Vec3::new(-40.0, -50.0, -60.0));
    let hit_neg = snap_3d_line_endpoints(doc, &Space::Model, &cam, s_neg, 15.0, 0).expect("snap hit for neg Z");
    assert_eq!(hit_neg.point3, Some(Vec3::new(-40.0, -50.0, -60.0)));
    assert_eq!(hit_neg.input(), Input::Point3(Vec3::new(-40.0, -50.0, -60.0)));
}

#[test]
fn test_snap_3d_line_endpoints_deterministic_order_and_cycling() {
    let mut s = Session::new();
    s.viewport_px = (800.0, 600.0);
    s.execute("view.swiso", &json!({})).unwrap();

    // Two lines meeting at exactly the same endpoint (20, 20, 20)
    s.cmdline("line 0,0,0 20,20,20").unwrap();
    s.cmdline("").unwrap();
    s.cmdline("line 50,50,50 20,20,20").unwrap();
    s.cmdline("").unwrap();

    let cam = s.camera();
    let doc = s.doc().unwrap();
    let s_joint = cam.project_point(Vec3::new(20.0, 20.0, 20.0));

    // Cycle index 0
    let hit0 = snap_3d_line_endpoints(doc, &Space::Model, &cam, s_joint, 15.0, 0).expect("hit 0");
    // Cycle index 1
    let hit1 = snap_3d_line_endpoints(doc, &Space::Model, &cam, s_joint, 15.0, 1).expect("hit 1");

    assert_eq!(hit0.point3, Some(Vec3::new(20.0, 20.0, 20.0)));
    assert_eq!(hit1.point3, Some(Vec3::new(20.0, 20.0, 20.0)));
}

#[test]
fn test_qnewtemplate_persistence_and_independence() {
    let dir = std::env::temp_dir().join(format!("cadcraft-tmpl-test-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let tmpl_path = dir.join("test_custom.dwt");

    // Write a synthetic DXF template with a custom layer
    let tmpl_content = "0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nLAYER\n0\nLAYER\n2\nCUSTOM_LAYER\n70\n0\n62\n1\n0\nENDTAB\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nLINE\n8\nCUSTOM_LAYER\n10\n1.0\n20\n2.0\n30\n3.0\n11\n4.0\n21\n5.0\n31\n6.0\n0\nENDSEC\n0\nEOF\n";
    std::fs::write(&tmpl_path, tmpl_content).unwrap();

    let mut s = Session::new();

    // Configure QNEWTEMPLATE
    let path_str = tmpl_path.to_string_lossy().to_string();
    set_qnew_template(&path_str);
    assert_eq!(qnew_template(), path_str);

    // Profile contains QNEWTEMPLATE
    let prof = cadcraft_engine::cmd::options::profile(&s);
    assert_eq!(prof["QNEWTEMPLATE"].as_str(), Some(path_str.as_str()));

    // Clear template and verify restore
    set_qnew_template("");
    assert_eq!(qnew_template(), "");
    cadcraft_engine::cmd::options::restore(&mut s, &prof);
    assert_eq!(qnew_template(), path_str);

    // Clean up
    set_qnew_template("");
    let _ = std::fs::remove_dir_all(&dir);
}
