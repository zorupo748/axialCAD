//! Tests for 3D coordinates in LINE, 2D tool compatibility and DXF persistence.

use cadcraft_doc::EntityKind;
use cadcraft_engine::{Input, Session};
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

#[test]
fn line_interactive_2d_coordinates_after_3d_line_resets_z_zero() {
    let mut s = Session::new();
    // First line with elevated Z = 50.0
    s.cmdline("line 0,0,50 10,0,50").unwrap();
    s.cmdline("").unwrap();
    assert!(s.running.is_none());

    // Subsequent 2D line must use Z = 0.0, not carry over previous line elevation
    s.cmdline("line 10,20 30,40").unwrap();
    s.cmdline("").unwrap();
    assert!(s.running.is_none());

    let EntityKind::Line(l) = last_entity(&s) else { panic!("expected Line entity") };
    assert_eq!(l.a, Vec3::new(10.0, 20.0, 0.0));
    assert_eq!(l.b, Vec3::new(30.0, 40.0, 0.0));
}

#[test]
fn line_with_endpoints_at_different_heights_and_roundtrip() {
    let mut s = Session::new();
    s.cmdline("line 10,20,30 40,50,90").unwrap();
    s.cmdline("").unwrap();
    assert!(s.running.is_none());

    let EntityKind::Line(l) = last_entity(&s) else { panic!("expected Line entity") };
    assert_eq!(l.a, Vec3::new(10.0, 20.0, 30.0));
    assert_eq!(l.b, Vec3::new(40.0, 50.0, 90.0));

    // Verify DXF persistence preserves both distinct heights
    let dxf_bytes = cadcraft_io::write_dxf(s.doc().unwrap());
    let restored = cadcraft_io::read_dxf(dxf_bytes.as_bytes()).unwrap();
    let l_restored = restored.model.iter().find_map(|e| if let EntityKind::Line(line) = &e.kind { Some(line) } else { None }).unwrap();
    assert_eq!(l_restored.a, Vec3::new(10.0, 20.0, 30.0));
    assert_eq!(l_restored.b, Vec3::new(40.0, 50.0, 90.0));
}

#[test]
fn line_mixed_2d_and_3d_endpoints() {
    let mut s = Session::new();
    // Start at 2D coordinate (Z = 0) and go to 3D coordinate (Z = 45)
    s.cmdline("line 5,15 25,35,45").unwrap();
    s.cmdline("").unwrap();
    assert!(s.running.is_none());

    let EntityKind::Line(l) = last_entity(&s) else { panic!("expected Line entity") };
    assert_eq!(l.a, Vec3::new(5.0, 15.0, 0.0));
    assert_eq!(l.b, Vec3::new(25.0, 35.0, 45.0));
}

#[test]
fn line_global_z0_option_is_explicit_and_applies_to_one_point() {
    let mut s = Session::new();
    s.cmdline("line 4,6,100").unwrap();
    let prompt = s.current_prompt().unwrap();
    assert_eq!(prompt.match_keyword("z0").as_deref(), Some("Z=0"));
    assert_eq!(prompt.match_keyword("v").as_deref(), Some("Vertical"));
    assert_eq!(prompt.match_keyword("u").as_deref(), Some("Undo"));
    assert!(!prompt.global_xy_zero);

    s.cmdline("Z0").unwrap();
    let active = s.current_prompt().unwrap();
    assert!(active.global_xy_zero);
    assert_eq!(active.message, "Specify next point on global Z=0");
    assert_eq!(active.match_keyword("f").as_deref(), Some("Free"));

    s.settings.zlock = true;
    s.settings.orthomode = true;
    s.input(Input::Point3(Vec3::new(30.0, 20.0, 0.0))).unwrap();
    assert!(!s.current_prompt().unwrap().global_xy_zero, "a confirmed Z=0 point consumes the option");
    assert!(s.settings.zlock && s.settings.orthomode, "the option does not change global constraints");

    s.settings.zlock = false;
    s.settings.orthomode = false;
    s.input(Input::Point3(Vec3::new(45.0, 35.0, 12.0))).unwrap();
    assert!(!s.current_prompt().unwrap().global_xy_zero, "the next segment uses the normal LINE prompt");
    s.cmdline("").unwrap();
    assert!(s.running.is_none(), "empty Enter finishes LINE and does not add a point");

    let lines: Vec<_> = s
        .doc()
        .unwrap()
        .model
        .iter()
        .filter_map(|entity| match &entity.kind {
            EntityKind::Line(line) => Some((line.a, line.b)),
            _ => None,
        })
        .collect();
    assert_eq!(lines, vec![(Vec3::new(4.0, 6.0, 100.0), Vec3::new(30.0, 20.0, 0.0)), (Vec3::new(30.0, 20.0, 0.0), Vec3::new(45.0, 35.0, 12.0)),]);
}

#[test]
fn line_global_z0_option_rejects_nonzero_z_and_can_be_disabled_or_cancelled() {
    let mut s = Session::new();
    s.cmdline("line 4,6,100").unwrap();
    s.cmdline("Z=0").unwrap();
    s.cmdline("30,20,50").unwrap();
    assert!(s.current_prompt().unwrap().global_xy_zero, "a conflicting explicit 3D point is rejected");
    assert!(s.log.last().is_some_and(|message| message.contains("not on global Z=0")), "the conflict is reported");
    s.cmdline("30,20,0.000000001").unwrap();
    assert!(s.current_prompt().unwrap().global_xy_zero, "even a small explicit nonzero Z is not silently projected");
    assert_eq!(s.doc().unwrap().model.len(), 0, "a rejected endpoint adds no line");

    s.cmdline("F").unwrap();
    assert!(!s.current_prompt().unwrap().global_xy_zero, "Free disables the option without placing a point");
    s.cmdline("30,20,50").unwrap();
    assert_eq!(s.doc().unwrap().model.len(), 1);
    s.cancel();
    assert!(s.running.is_none() && s.current_prompt().is_none(), "cancel clears LINE's temporary state");

    s.cmdline("line 1,2,75").unwrap();
    assert!(!s.current_prompt().unwrap().global_xy_zero, "a new LINE starts with the option disabled");
    s.cancel();
    assert!(s.running.is_none());

    s.cmdline("line 1,2,75").unwrap();
    s.cmdline("Z0").unwrap();
    s.cancel();
    assert!(s.running.is_none());
    s.cmdline("line 1,2,75").unwrap();
    assert!(!s.current_prompt().unwrap().global_xy_zero, "canceling an active mode leaves no state in the next LINE");
    s.cmdline("Z0").unwrap();
    s.cmdline("").unwrap();
    assert!(s.running.is_none(), "Enter ends LINE while Z=0 mode is active");
    s.cmdline("line 1,2,75").unwrap();
    assert!(!s.current_prompt().unwrap().global_xy_zero, "finishing an active mode leaves no state in the next LINE");
    s.cancel();
}

#[test]
fn line_global_z0_option_preserves_explicit_2d_coordinate_semantics() {
    let mut s = Session::new();
    s.cmdline("line 4,6,100").unwrap();
    s.cmdline("Z0").unwrap();
    s.cmdline("@3,4").unwrap();
    assert!(!s.current_prompt().unwrap().global_xy_zero, "typed 2D input explicitly exits the one-shot mode");
    s.cmdline("").unwrap();

    let lines: Vec<_> = s
        .doc()
        .unwrap()
        .model
        .iter()
        .filter_map(|entity| match &entity.kind {
            EntityKind::Line(line) => Some((line.a, line.b)),
            _ => None,
        })
        .collect();
    assert_eq!(lines, vec![(Vec3::new(4.0, 6.0, 100.0), Vec3::new(7.0, 10.0, 100.0))]);
}
