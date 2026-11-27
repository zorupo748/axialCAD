//! Characterizes the projected trim reported against f7388ab. The same floor geometry
//! must still act as cutting edges when UCS projection is requested explicitly.

use cadcraft_engine::{Input, Session, doc::{Circle, EntityKind}};
use cadcraft_geom::{Vec2, Vec3};
use serde_json::json;

#[test]
fn full_secant_with_ground_cutters_replaces_the_circle_with_an_arc_in_ucs() {
    let mut s = Session::new();
    s.doc_mut().unwrap().header.set_i64("PROJMODE", 1);
    let center = Vec3::new(0.0, 0.0, 30.0);
    let target = s.add_entity(EntityKind::Circle(Circle { center, radius: 5.0 })).unwrap();
    s.execute("line", &json!({"points": [[-8,0,30], [8,0,30]]})).unwrap();
    for x in [-2, 2] {
        s.execute("line", &json!({"points": [[x,-8,0], [x,8,0]]})).unwrap();
    }

    let point = Vec2::new(0.0, 5.0);
    let point3 = Some(point.to3(30.0));
    s.start("trim").unwrap();
    s.is_interactive_ui = true;
    s.cursor_entity_point = Some((target, point, point3));
    let preview = s.preview(point);
    let EntityKind::Arc(removed) = &preview.first().expect("projected arc preview").kind else {
        panic!("expected a partial cut");
    };
    let a = 0.4_f64.acos();
    assert!((removed.start - a).abs() < 1e-9);
    assert!((removed.end - (std::f64::consts::PI - a)).abs() < 1e-9);
    s.input(Input::EntityPoint { handle: Some(target), point, point3 }).unwrap();
    let drawing = s.doc().unwrap();
    assert!(drawing.entity(target).is_none());
    let kept = drawing.model.iter().find_map(|entity| match &entity.kind {
        EntityKind::Arc(arc) => Some(arc),
        _ => None,
    }).expect("surviving arc");
    assert_eq!((kept.center, kept.radius, kept.start, kept.end), (center, 5.0, removed.end, removed.start));
}
