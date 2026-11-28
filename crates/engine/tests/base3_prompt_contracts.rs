//! Existing planar prompt contracts for commands outside the spatial edit migration.

use cadcraft_engine::{Input, Prompt, Session};
use cadcraft_geom::{Vec2, Vec3};
use serde_json::json;

const POINT: Vec3 = Vec3::new(2.0, 3.0, 30.0);

fn offset_prompt() -> Prompt {
    let mut s = Session::new();
    s.start("offset").unwrap();
    s.input(Input::Point3(POINT)).unwrap();
    s.current_prompt().unwrap()
}

fn array_prompt() -> Prompt {
    let mut s = Session::new();
    s.execute("line", &json!({"points": [[0,0,30],[4,6,30]]})).unwrap();
    s.script("ARRAYRECT L\n\n").unwrap();
    s.input(Input::Keyword("Spacing".into())).unwrap();
    s.current_prompt().unwrap()
}

fn drawing_prompt() -> Prompt {
    let mut s = Session::new();
    s.start("pline").unwrap();
    s.input(Input::Point3(POINT)).unwrap();
    s.current_prompt().unwrap()
}

fn dimension_prompt() -> Prompt {
    let mut s = Session::new();
    s.start("dimlinear").unwrap();
    s.input(Input::Point3(POINT)).unwrap();
    s.current_prompt().unwrap()
}

fn pedit_prompt() -> Prompt {
    let mut s = Session::new();
    s.script("PLINE 2,3 5,4\n\n").unwrap();
    let handle = s.doc().unwrap().model.last().unwrap().handle;
    s.doc_mut().unwrap().modify_entity(handle, |e| {
        let cadcraft_doc::EntityKind::LwPolyline(p) = &mut e.kind else { panic!("expected polyline") };
        p.elevation = POINT.z;
    }).unwrap();
    s.start("pedit").unwrap();
    s.input(Input::Point(POINT.xy())).unwrap();
    s.input(Input::Keyword("Edit vertex".into())).unwrap();
    s.input(Input::Keyword("Tangent".into())).unwrap();
    s.current_prompt().unwrap()
}

fn lisp_prompt() -> Prompt {
    let mut s = Session::new();
    s.cmdline("(getpoint '(2 3 30))").unwrap();
    s.current_prompt().unwrap()
}

#[test]
fn base3_excluded_prompts_keep_their_planar_contracts() {
    for (name, prompt) in [
        ("OFFSET", offset_prompt()), ("ARRAYRECT", array_prompt()), ("PLINE", drawing_prompt()),
        ("DIMLINEAR", dimension_prompt()), ("PEDIT", pedit_prompt()), ("getpoint", lisp_prompt()),
    ] {
        assert!(!prompt.spatial_point, "{name}");
        assert_eq!(prompt.base, Some(Vec2::new(2.0, 3.0)), "{name}: {}", prompt.message);
        assert_eq!(prompt.base3, None, "{name}");
    }
}

#[test]
#[ignore = "OFFSET's two-point distance prompt still stores the first point in XY"]
fn base3_offset_distance_should_keep_the_first_point() {
    assert_eq!(offset_prompt().base3, Some(POINT));
}

#[test]
#[ignore = "ARRAYRECT spacing still uses the centroid of planar selection bounds"]
fn base3_arrayrect_spacing_should_keep_the_elevated_centroid() {
    assert_eq!(array_prompt().base3, Some(POINT));
}

#[test]
#[ignore = "PLINE still stores its continuation vertex in XY"]
fn base3_pline_should_keep_the_continuation_vertex() {
    assert_eq!(drawing_prompt().base3, Some(POINT));
}

#[test]
#[ignore = "DIMLINEAR still stores the first extension origin in XY"]
fn base3_dimlinear_should_keep_the_first_origin() {
    assert_eq!(dimension_prompt().base3, Some(POINT));
}

#[test]
#[ignore = "PEDIT tangent still uses the marked vertex XY without polyline elevation"]
fn base3_pedit_tangent_should_keep_the_marked_vertex() {
    assert_eq!(pedit_prompt().base3, Some(POINT));
}

#[test]
#[ignore = "AutoLISP getpoint retains three coordinates internally but publishes an XY base"]
fn base3_lisp_getpoint_should_publish_its_spatial_base() {
    assert_eq!(lisp_prompt().base3, Some(POINT));
}
