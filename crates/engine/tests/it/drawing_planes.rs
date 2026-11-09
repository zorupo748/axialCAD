//! Integration tests for drawing planes (workplanes) in 3D wireframe drafting.

use cadcraft_doc::{EntityKind, Line as DocLine};
use cadcraft_engine::{Input, Session};
use cadcraft_geom::{DrawingPlane, OrbitCamera, Vec2, Vec3};
use serde_json::json;

fn last_line(s: &Session) -> DocLine {
    let e = s.doc().unwrap().model.iter().last().unwrap();
    let EntityKind::Line(l) = &e.kind else { panic!("expected Line entity") };
    l.clone()
}

#[test]
fn test_plane_command_and_aliases() {
    let mut s = Session::new();
    assert_eq!(s.drawing_plane(), DrawingPlane::XY);

    // plane JSON execution
    let v = s.execute("plane", &json!({"plane": "xz"})).unwrap();
    assert_eq!(v["plane"], "XZ");
    assert_eq!(s.drawing_plane(), DrawingPlane::XZ);

    // cplane alias
    let v = s.execute("cplane", &json!({"plane": "xy"})).unwrap();
    assert_eq!(v["plane"], "XY");
    assert_eq!(s.drawing_plane(), DrawingPlane::XY);

    // workplane alias
    let v = s.execute("workplane", &json!({"plane": "yz"})).unwrap();
    assert_eq!(v["plane"], "YZ");
    assert_eq!(s.drawing_plane(), DrawingPlane::YZ);

    let v = s.execute("workplane", &json!({"plane": "xz"})).unwrap();
    assert_eq!(v["plane"], "XZ");
    assert_eq!(s.drawing_plane(), DrawingPlane::XZ);

    // subcommands
    s.execute("plane.yz", &json!({})).unwrap();
    assert_eq!(s.drawing_plane(), DrawingPlane::YZ);

    s.execute("plane.xy", &json!({})).unwrap();
    assert_eq!(s.drawing_plane(), DrawingPlane::XY);

    s.execute("plane.xz", &json!({})).unwrap();
    assert_eq!(s.drawing_plane(), DrawingPlane::XZ);

    // Real UCS command contract (independent of PLANE)
    let ucs_v = s
        .execute(
            "ucs",
            &json!({
                "option": "3point",
                "origin": [10.0, 20.0, 30.0],
                "x_axis": [11.0, 20.0, 30.0],
                "y_pt": [10.0, 20.0, 31.0]
            }),
        )
        .unwrap();
    assert_eq!(ucs_v["status"], "ok");
    assert_eq!(s.ucs().origin, Vec3::new(10.0, 20.0, 30.0));
    assert_eq!(s.ucs().z_axis, Vec3::new(0.0, -1.0, 0.0));

    let world_v = s.execute("ucs", &json!({"option": "world"})).unwrap();
    assert_eq!(world_v["ucs"], "world");
    assert!(s.ucs().is_world());
}

#[test]
fn test_plane_interactive_prompt_machine() {
    let mut s = Session::new();
    s.start("plane").unwrap();
    assert!(s.running.is_some());

    // Switch to YZ via keyword
    s.input(Input::Keyword("YZ".into())).unwrap();
    assert!(s.running.is_none());
    assert_eq!(s.drawing_plane(), DrawingPlane::YZ);

    // Prompt again, hit Enter to keep current
    s.start("plane").unwrap();
    s.input(Input::Enter).unwrap();
    assert!(s.running.is_none());
    assert_eq!(s.drawing_plane(), DrawingPlane::YZ);

    // Switch to XZ via text
    s.start("plane").unwrap();
    s.input(Input::Text("xz".into())).unwrap();
    assert!(s.running.is_none());
    assert_eq!(s.drawing_plane(), DrawingPlane::XZ);
}

#[test]
fn test_draw_vertical_and_spatial_lines_via_plane_clicks() {
    let mut s = Session::new();

    // 1. Draw 2D line on XY plane
    s.set_drawing_plane(DrawingPlane::XY);
    s.start("line").unwrap();
    s.input(Input::Point3(Vec3::new(10.0, 20.0, 0.0))).unwrap();
    s.input(Input::Point3(Vec3::new(30.0, 40.0, 0.0))).unwrap();
    s.input(Input::Enter).unwrap();

    let l_xy = last_line(&s);
    assert_eq!(l_xy.a, Vec3::new(10.0, 20.0, 0.0));
    assert_eq!(l_xy.b, Vec3::new(30.0, 40.0, 0.0));

    // 2. Switch to XZ plane and draw a purely vertical wireframe line (changing Z only)
    s.set_drawing_plane(DrawingPlane::XZ);
    s.start("line").unwrap();
    s.input(Input::Point3(Vec3::new(50.0, 0.0, 0.0))).unwrap();
    s.input(Input::Point3(Vec3::new(50.0, 0.0, 100.0))).unwrap(); // Vertical delta Z = 100
    s.input(Input::Enter).unwrap();

    let l_vert = last_line(&s);
    assert_eq!(l_vert.a, Vec3::new(50.0, 0.0, 0.0));
    assert_eq!(l_vert.b, Vec3::new(50.0, 0.0, 100.0));
    // Verify it is vertical
    assert_eq!(l_vert.a.x, l_vert.b.x);
    assert_eq!(l_vert.a.y, 0.0);
    assert_eq!(l_vert.b.y, 0.0);
    assert_eq!(l_vert.b.z - l_vert.a.z, 100.0);

    // 3. Draw diagonal wireframe line on XZ plane (X and Z)
    s.start("line").unwrap();
    s.input(Input::Point3(Vec3::new(0.0, 0.0, 20.0))).unwrap();
    s.input(Input::Point3(Vec3::new(60.0, 0.0, 80.0))).unwrap();
    s.input(Input::Enter).unwrap();

    let l_xz = last_line(&s);
    assert_eq!(l_xz.a, Vec3::new(0.0, 0.0, 20.0));
    assert_eq!(l_xz.b, Vec3::new(60.0, 0.0, 80.0));

    // 4. Switch to YZ plane and draw spatial wireframe line (Y and Z)
    s.set_drawing_plane(DrawingPlane::YZ);
    s.start("line").unwrap();
    s.input(Input::Point3(Vec3::new(0.0, 15.0, 25.0))).unwrap();
    s.input(Input::Point3(Vec3::new(0.0, 75.0, 125.0))).unwrap();
    s.input(Input::Enter).unwrap();

    let l_yz = last_line(&s);
    assert_eq!(l_yz.a, Vec3::new(0.0, 15.0, 25.0));
    assert_eq!(l_yz.b, Vec3::new(0.0, 75.0, 125.0));
    assert_eq!(l_yz.a.x, 0.0);
    assert_eq!(l_yz.b.x, 0.0);
}

#[test]
fn test_switching_plane_does_not_mutate_existing_geometry() {
    let mut s = Session::new();

    s.set_drawing_plane(DrawingPlane::XY);
    s.start("line").unwrap();
    s.input(Input::Point3(Vec3::new(1.0, 2.0, 0.0))).unwrap();
    s.input(Input::Point3(Vec3::new(3.0, 4.0, 0.0))).unwrap();
    s.input(Input::Enter).unwrap();

    s.set_drawing_plane(DrawingPlane::XZ);
    s.start("line").unwrap();
    s.input(Input::Point3(Vec3::new(5.0, 0.0, 10.0))).unwrap();
    s.input(Input::Point3(Vec3::new(5.0, 0.0, 50.0))).unwrap();
    s.input(Input::Enter).unwrap();

    s.set_drawing_plane(DrawingPlane::YZ);

    // Verify both lines remain completely unchanged
    let model: Vec<_> = s.doc().unwrap().model.iter().collect();
    assert_eq!(model.len(), 2);

    let EntityKind::Line(l1) = &model[0].kind else { panic!("line 1") };
    assert_eq!(l1.a, Vec3::new(1.0, 2.0, 0.0));
    assert_eq!(l1.b, Vec3::new(3.0, 4.0, 0.0));

    let EntityKind::Line(l2) = &model[1].kind else { panic!("line 2") };
    assert_eq!(l2.a, Vec3::new(5.0, 0.0, 10.0));
    assert_eq!(l2.b, Vec3::new(5.0, 0.0, 50.0));
}

#[test]
fn test_explicit_xyz_typing_overrides_active_plane() {
    let mut s = Session::new();
    // Active plane is XZ (where Y is typically 0)
    s.set_drawing_plane(DrawingPlane::XZ);

    // Explicit XYZ coordinates with non-zero Y must be respected
    s.cmdline("line 10,25,30 40,55,60").unwrap();
    s.cmdline("").unwrap();

    let l = last_line(&s);
    assert_eq!(l.a, Vec3::new(10.0, 25.0, 30.0));
    assert_eq!(l.b, Vec3::new(40.0, 55.0, 60.0));
}

#[test]
fn test_2d_commands_compatible_with_active_3d_planes() {
    let mut s = Session::new();
    s.execute("layer.new", &json!({"name": "Circles3D"})).unwrap();
    s.execute("setvar", &json!({"name": "CLAYER", "value": "Circles3D"})).unwrap();
    s.set_drawing_plane(DrawingPlane::XZ);

    // CIRCLE preserves the 3D WCS center, active layer/properties, and active UCS extrusion normal
    s.start("circle").unwrap();
    s.input(Input::Point3(Vec3::new(10.0, 20.0, 30.0))).unwrap();
    s.input(Input::Text("5".into())).unwrap();

    let e = s.doc().unwrap().model.iter().last().unwrap();
    let EntityKind::Circle(c) = &e.kind else { panic!("expected Circle") };
    assert_eq!(c.center, Vec3::new(10.0, 20.0, 30.0));
    assert_eq!(c.radius, 5.0);
    assert_eq!(e.common.extrusion, Vec3::Z);
    assert_eq!(e.common.layer, "Circles3D");

    // On an inclined/rotated UCS (XZ plane at Y=15), CIRCLE records WCS center and UCS normal
    s.execute(
        "ucs",
        &json!({
            "option": "3point",
            "origin": [0.0, 15.0, 0.0],
            "x_axis": [1.0, 15.0, 0.0],
            "y_pt": [0.0, 15.0, 1.0]
        }),
    )
    .unwrap();
    s.cmdline("circle 10,25 4").unwrap();
    let e2 = s.doc().unwrap().model.iter().last().unwrap();
    let EntityKind::Circle(c2) = &e2.kind else { panic!("expected Circle") };
    assert_eq!(c2.center, Vec3::new(10.0, 15.0, 25.0));
    assert_eq!(c2.radius, 4.0);
    assert_eq!(e2.common.extrusion, Vec3::new(0.0, -1.0, 0.0));
    assert_eq!(e2.common.layer, "Circles3D");
}

#[test]
fn test_camera_ray_plane_intersection_produces_3d_points_from_screen_coords() {
    let mut cam = OrbitCamera::default();
    cam.set_preset("swiso");

    // In isometric view, moving mouse vertically (changing q.y) on XZ plane
    // changes height Z and coordinate X
    let q1 = Vec2::new(0.0, 0.0);
    let q2 = Vec2::new(0.0, 10.0); // Mouse dragged upwards on screen

    let p1_xz = cam.unproject_to_drawing_plane(q1, DrawingPlane::XZ);
    let p2_xz = cam.unproject_to_drawing_plane(q2, DrawingPlane::XZ);

    assert_eq!(p1_xz.y, 0.0);
    assert_eq!(p2_xz.y, 0.0);
    // Height Z must have increased
    assert!(p2_xz.z > p1_xz.z);

    // On YZ plane, moving mouse vertically changes height Z and coordinate Y
    let p1_yz = cam.unproject_to_drawing_plane(q1, DrawingPlane::YZ);
    let p2_yz = cam.unproject_to_drawing_plane(q2, DrawingPlane::YZ);

    assert_eq!(p1_yz.x, 0.0);
    assert_eq!(p2_yz.x, 0.0);
    assert!(p2_yz.z > p1_yz.z);
}
