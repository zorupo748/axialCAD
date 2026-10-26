//! Tests for 3D coordinates in LINE, 2D tool compatibility and DXF persistence.

use cadcraft_doc::EntityKind;
use cadcraft_engine::Session;
use cadcraft_geom::{Vec2, Vec3};
use serde_json::json;

fn last_entity(s: &Session) -> EntityKind {
    s.doc().unwrap().model.iter().last().map(|e| e.kind.clone()).unwrap()
}

#[test]
fn line_interactive_accepts_3d_coordinates_and_preserves_z() {
    let mut s = Session::new();
    s.cmdline("line 0,0,0 100,50,75").unwrap();
    s.cmdline("").unwrap(); // Finish LINE command
    assert!(s.running.is_none());

    let EntityKind::Line(l) = last_entity(&s) else { panic!("expected Line entity") };
    assert_eq!(l.a, Vec3::new(0.0, 0.0, 0.0));
    assert_eq!(l.b, Vec3::new(100.0, 50.0, 75.0));
}

#[test]
fn line_interactive_2d_coordinates_default_z_zero() {
    let mut s = Session::new();
    s.cmdline("line 10,20 30,40").unwrap();
    s.cmdline("").unwrap();
    assert!(s.running.is_none());

    let EntityKind::Line(l) = last_entity(&s) else { panic!("expected Line entity") };
    assert_eq!(l.a, Vec3::new(10.0, 20.0, 0.0));
    assert_eq!(l.b, Vec3::new(30.0, 40.0, 0.0));
}

#[test]
fn line_interactive_relative_coordinates_preserve_elevation() {
    let mut s = Session::new();
    // First line starts at 0,0,10 and goes to 10,20,30
    s.cmdline("line 0,0,10 10,20,30").unwrap();
    // Next point relative 2D (@5,10) should preserve Z = 30
    s.cmdline("@5,10").unwrap();
    // Next point relative 3D (@5,10,15) should add to Z (30 + 15 = 45)
    s.cmdline("@5,10,15").unwrap();
    s.cmdline("").unwrap();

    let model: Vec<_> = s.doc().unwrap().model.iter().collect();
    assert_eq!(model.len(), 3);

    let EntityKind::Line(l1) = &model[0].kind else { panic!("line 1") };
    assert_eq!(l1.a, Vec3::new(0.0, 0.0, 10.0));
    assert_eq!(l1.b, Vec3::new(10.0, 20.0, 30.0));

    let EntityKind::Line(l2) = &model[1].kind else { panic!("line 2") };
    assert_eq!(l2.a, Vec3::new(10.0, 20.0, 30.0));
    assert_eq!(l2.b, Vec3::new(15.0, 30.0, 30.0));

    let EntityKind::Line(l3) = &model[2].kind else { panic!("line 3") };
    assert_eq!(l3.a, Vec3::new(15.0, 30.0, 30.0));
    assert_eq!(l3.b, Vec3::new(20.0, 40.0, 45.0));
}

#[test]
fn line_json_form_accepts_3d_and_2d_points() {
    let mut s = Session::new();
    s.execute(
        "line",
        &json!({
            "points": [
                [0.0, 0.0, 0.0],
                [100.0, 50.0, 75.0],
                [150.0, 80.0, 120.0]
            ]
        }),
    )
    .unwrap();

    let model: Vec<_> = s.doc().unwrap().model.iter().collect();
    assert_eq!(model.len(), 2);

    let EntityKind::Line(l1) = &model[0].kind else { panic!("line 1") };
    assert_eq!(l1.a, Vec3::new(0.0, 0.0, 0.0));
    assert_eq!(l1.b, Vec3::new(100.0, 50.0, 75.0));

    let EntityKind::Line(l2) = &model[1].kind else { panic!("line 2") };
    assert_eq!(l2.a, Vec3::new(100.0, 50.0, 75.0));
    assert_eq!(l2.b, Vec3::new(150.0, 80.0, 120.0));

    // Also test 2D points in JSON
    s.execute(
        "line",
        &json!({
            "points": [
                [10.0, 20.0],
                [30.0, 40.0]
            ]
        }),
    )
    .unwrap();

    let model2: Vec<_> = s.doc().unwrap().model.iter().collect();
    let EntityKind::Line(l3) = &model2[2].kind else { panic!("line 3") };
    assert_eq!(l3.a, Vec3::new(10.0, 20.0, 0.0));
    assert_eq!(l3.b, Vec3::new(30.0, 40.0, 0.0));
}

#[test]
fn tools_2d_continue_working_with_2d_and_3d_inputs() {
    let mut s = Session::new();
    // CIRCLE with 2D center and radius
    s.cmdline("circle 10,20 5").unwrap();
    assert!(s.running.is_none());
    let EntityKind::Circle(c1) = last_entity(&s) else { panic!("circle 1") };
    assert_eq!(c1.center, Vec3::new(10.0, 20.0, 0.0));
    assert_eq!(c1.radius, 5.0);

    // CIRCLE with typed 3D point (fallback to 2D center)
    s.cmdline("circle 15,25,30 8").unwrap();
    assert!(s.running.is_none());
    let EntityKind::Circle(c2) = last_entity(&s) else { panic!("circle 2") };
    assert_eq!(c2.center.xy(), Vec2::new(15.0, 25.0));
    assert_eq!(c2.radius, 8.0);

    // RECTANG with 2D corners
    s.cmdline("rectang 0,0 20,10").unwrap();
    assert!(s.running.is_none());
    let EntityKind::LwPolyline(p) = last_entity(&s) else { panic!("rectang") };
    assert_eq!(p.vertices.len(), 4);
}

#[test]
fn line_3d_preserves_coordinates_through_dxf_roundtrip() {
    let mut s = Session::new();
    s.execute(
        "line",
        &json!({
            "points": [
                [0.0, 0.0, 0.0],
                [100.0, 50.0, 75.0]
            ]
        }),
    )
    .unwrap();

    let doc = s.doc().unwrap();
    let dxf_text = cadcraft_io::write_dxf(doc);
    assert!(!dxf_text.is_empty());

    let restored_doc = cadcraft_io::read_dxf(dxf_text.as_bytes()).unwrap();
    let lines: Vec<_> = restored_doc.model.iter().filter_map(|e| if let EntityKind::Line(l) = &e.kind { Some(l.clone()) } else { None }).collect();

    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].a, Vec3::new(0.0, 0.0, 0.0));
    assert_eq!(lines[0].b, Vec3::new(100.0, 50.0, 75.0));
}
