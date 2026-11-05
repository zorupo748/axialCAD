use std::sync::Arc;

use cadcraft_doc::{Block, DimKind, Dimension, Entity, EntityKind, Handle, Line, Space};
use cadcraft_geom::{Vec2, Vec3};
use serde_json::{Value, json};

use crate::{Input, Session};

fn h(v: &Value) -> Handle {
    Handle::parse_hex(v["handle"].as_str().unwrap()).unwrap()
}

fn dim(s: &Session, hd: Handle) -> Dimension {
    match &s.doc().unwrap().entity(hd).unwrap().kind {
        EntityKind::Dimension(d) => d.clone(),
        other => panic!("not a dimension: {other:?}"),
    }
}

fn line(s: &mut Session, a: [f64; 2], b: [f64; 2]) -> Handle {
    s.execute("line", &json!({ "points": [a, b] })).unwrap();
    s.doc().unwrap().model.last().unwrap().handle
}

fn near(a: Vec2, b: Vec2) -> bool {
    a.dist(b) < 1e-9
}

#[test]
fn properties_set_respects_locked_layer() {
    let mut s = Session::new();
    let l = line(&mut s, [0.0, 0.0], [10.0, 0.0]);
    s.execute("layer.new", &json!({ "name": "E1_LOCKED" })).unwrap();
    s.execute("properties.set", &json!({ "handle": l.hex(), "layer": "E1_LOCKED" })).unwrap();
    s.execute("layer.set", &json!({ "name": "E1_LOCKED", "locked": true })).unwrap();

    let (layer, original_color) = {
        let d = s.doc().unwrap();
        let e = d.entity(l).unwrap();
        (d.layer("E1_LOCKED").unwrap(), e.common.color)
    };
    assert!(layer.locked, "the test layer must be locked");
    assert_eq!(s.doc().unwrap().entity(l).unwrap().common.layer, "E1_LOCKED");
    let requested_color = cadcraft_color::Color::Index(1);
    assert_ne!(original_color, requested_color, "the requested color must differ from the original");

    // Check the drawing after either an Ok or Err result: the lock invariant is the behavior
    // under test, and an error must not hide a partial mutation.
    let _result = s.execute("properties.set", &json!({ "handle": l.hex(), "color": "red" }));
    let after = s.doc().unwrap().entity(l).unwrap();
    assert_eq!(after.common.color, original_color, "properties.set changed an entity on a locked layer");
}

#[test]
fn dimstyle_edit_updates_cached_dimension_representation() {
    fn rendered(d: &cadcraft_doc::Drawing, h: Handle) -> Vec<(cadcraft_render::Kind, Vec<Vec2>)> {
        let list = cadcraft_render::build(d, &Space::Model, &cadcraft_render::Options::default());
        list.prims.iter().filter(|p| p.handle == h).map(|p| (p.kind, list.points(p).to_vec())).collect()
    }

    let mut s = Session::new();
    s.execute("dimstyle", &json!({ "name": "S", "DIMASZ": 0.1 })).unwrap();
    let dh = h(&s.execute("dimlinear", &json!({ "p1": [0, 0], "p2": [10, 0], "at": [5, 3] })).unwrap());
    assert_eq!(dim(&s, dh).style, "S");

    let block_name = "*D_A1";
    {
        let d = s.doc_mut().unwrap();
        let mut block = Block::new(block_name);
        block.entities.push(Entity::new(d.new_handle(), EntityKind::Line(Line { a: Vec3::new(100.0, 100.0, 0.0), b: Vec3::new(110.0, 100.0, 0.0) })));
        d.blocks.insert(block_name.into(), Arc::new(block));
        d.modify_entity(dh, |e| {
            if let EntityKind::Dimension(dm) = &mut e.kind {
                dm.block = Some(block_name.into());
            }
        })
        .unwrap();
    }

    let before_dim = dim(&s, dh);
    assert_eq!(before_dim.block.as_deref(), Some(block_name));
    assert!(!before_dim.overrides.keys().any(|k| cadcraft_doc::DimStyle::field_name(k) == Some("arrowSize")));
    let before_style = s.doc().unwrap().dim_style("S").unwrap().arrow_size;
    let before_render = rendered(s.doc().unwrap(), dh);
    assert_eq!(s.doc().unwrap().block(block_name).unwrap().entities.len(), 1);
    assert_eq!(
        before_render,
        vec![(cadcraft_render::Kind::Polyline, vec![Vec2::new(100.0, 100.0), Vec2::new(110.0, 100.0)])],
        "precondition: rendered dimension must use the cached block geometry"
    );

    let new_arrow_size = 3.0;
    assert_ne!(before_style, new_arrow_size);
    s.execute("dimstyle", &json!({ "name": "S", "DIMASZ": new_arrow_size })).unwrap();
    let after_style = s.doc().unwrap().dim_style("S").unwrap().arrow_size;
    assert_eq!(after_style, new_arrow_size, "DIMSTYLE registry must accept the shared style change");

    let mut regenerated_doc = s.doc().unwrap().clone();
    regenerated_doc
        .modify_entity(dh, |e| {
            if let EntityKind::Dimension(dm) = &mut e.kind {
                dm.block = None;
            }
        })
        .unwrap();
    let regenerated = rendered(&regenerated_doc, dh);
    assert!(!regenerated.is_empty(), "updated style must produce dimension geometry");
    assert_ne!(regenerated, before_render, "the selected style change must affect rendered geometry");

    let after_render = rendered(s.doc().unwrap(), dh);
    assert_eq!(after_render, regenerated, "dimension representation still reflects its old cached block after DIMSTYLE changed");
}

#[test]
fn dimstyle_edit_invalidates_only_matching_cached_dimensions_everywhere() {
    fn rendered(d: &cadcraft_doc::Drawing, h: Handle) -> Vec<(cadcraft_render::Kind, Vec<Vec2>)> {
        let list = cadcraft_render::build(d, &Space::Model, &cadcraft_render::Options::default());
        list.prims.iter().filter(|p| p.handle == h).map(|p| (p.kind, list.points(p).to_vec())).collect()
    }
    fn render_bounds(d: &cadcraft_doc::Drawing, h: Handle) -> cadcraft_geom::Bounds2 {
        cadcraft_geom::Bounds2::from_points(rendered(d, h).iter().flat_map(|(_, points)| points.iter().copied()))
    }
    fn cached_line(s: &mut Session, dimension: Handle, name: &str, x: f64) {
        let d = s.doc_mut().unwrap();
        let mut block = Block::new(name);
        block.entities.push(Entity::new(d.new_handle(), EntityKind::Line(Line { a: Vec3::new(x, 100.0, 0.0), b: Vec3::new(x + 10.0, 100.0, 0.0) })));
        d.blocks.insert(name.into(), Arc::new(block));
        d.modify_entity(dimension, |e| {
            if let EntityKind::Dimension(dm) = &mut e.kind {
                dm.block = Some(name.into())
            }
        })
        .unwrap();
    }

    let mut s = Session::new();
    s.execute("dimstyle", &json!({"name": "S", "DIMASZ": 0.1})).unwrap();
    s.execute("dimstyle", &json!({"name": "OTHER", "DIMASZ": 0.6, "current": false})).unwrap();
    let a = h(&s.execute("dimlinear", &json!({"p1": [0, 0], "p2": [10, 0], "at": [5, 3]})).unwrap());
    let b = h(&s.execute("dimlinear", &json!({"p1": [0, 5], "p2": [10, 5], "at": [5, 8]})).unwrap());
    s.execute("dimstyle.current", &json!({"name": "OTHER"})).unwrap();
    let unrelated = h(&s.execute("dimlinear", &json!({"p1": [0, 10], "p2": [10, 10], "at": [5, 13]})).unwrap());
    s.execute("dimstyle.current", &json!({"name": "S"})).unwrap();
    s.execute("properties.set", &json!({"handle": b.hex(), "arrowSize": 1.25})).unwrap();
    cached_line(&mut s, a, "*A1_A", 200.0);
    cached_line(&mut s, b, "*A1_B", 220.0);
    cached_line(&mut s, unrelated, "*A1_OTHER", 240.0);

    // A dimension in a block definition is a supported model entity and must be invalidated too.
    let mut block_dim = dim(&s, a);
    block_dim.block = Some("*A1_BLOCK_DIM".into());
    {
        let d = s.doc_mut().unwrap();
        let mut graphic = Block::new("*A1_BLOCK_DIM");
        graphic
            .entities
            .push(Entity::new(d.new_handle(), EntityKind::Line(Line { a: Vec3::new(260.0, 100.0, 0.0), b: Vec3::new(270.0, 100.0, 0.0) })));
        d.blocks.insert("*A1_BLOCK_DIM".into(), Arc::new(graphic));
        let mut container = Block::new("A1_CONTAINER");
        let block_dim_handle = d.new_handle();
        container.entities.push(Entity::new(block_dim_handle, EntityKind::Dimension(block_dim)));
        d.blocks.insert("A1_CONTAINER".into(), Arc::new(container));
        d.add(
            &Space::Model,
            Default::default(),
            EntityKind::Insert(cadcraft_doc::Insert {
                block: "A1_CONTAINER".into(),
                insert: Vec3::ZERO,
                scale: Vec3::new(1.0, 1.0, 1.0),
                rotation: 0.0,
                attribs: Vec::new(),
                cols: 1,
                rows: 1,
                col_spacing: 0.0,
                row_spacing: 0.0,
            }),
        )
        .unwrap();
    }

    let block_dim_handle =
        s.doc().unwrap().block("A1_CONTAINER").unwrap().entities.iter().find(|e| matches!(e.kind, EntityKind::Dimension(_))).unwrap().handle;
    let before_a = rendered(s.doc().unwrap(), a);
    let before_b = rendered(s.doc().unwrap(), b);
    let before_unrelated = rendered(s.doc().unwrap(), unrelated);
    let block_dim_block = |s: &Session| {
        let container = s.doc().unwrap().block("A1_CONTAINER").unwrap();
        match &container.entities.get(block_dim_handle).unwrap().kind {
            EntityKind::Dimension(dm) => dm.block.clone(),
            other => panic!("expected a dimension in the block definition, got {other:?}"),
        }
    };
    assert_eq!(before_a.len(), 1);
    assert_eq!(before_b.len(), 1);
    assert_eq!(before_unrelated.len(), 1);
    assert_eq!(block_dim_block(&s).as_deref(), Some("*A1_BLOCK_DIM"));
    assert_eq!(dim(&s, a).style, "S");
    assert_eq!(dim(&s, b).style, "S");
    assert_eq!(dim(&s, unrelated).style, "OTHER");
    assert_eq!(dim(&s, b).overrides.get("arrowSize"), Some(&json!(1.25)));

    s.execute("dimstyle", &json!({"name": "s", "DIMASZ": 3.0})).unwrap();
    let after_a = rendered(s.doc().unwrap(), a);
    let after_b = rendered(s.doc().unwrap(), b);
    let after_unrelated = rendered(s.doc().unwrap(), unrelated);
    assert_ne!(after_a, before_a, "edited style dimension A must render from updated style");
    assert_ne!(after_b, before_b, "edited style dimension B must render from updated style");
    assert_eq!(after_unrelated, before_unrelated, "unrelated style representation must remain intact");
    assert_eq!(block_dim_block(&s), None, "dimension in a block definition must invalidate its old graphics");
    assert_eq!(dim(&s, b).overrides.get("arrowSize"), Some(&json!(1.25)), "per-entity override must survive style editing");
    assert_eq!(s.doc().unwrap().dim_style("S").unwrap().arrow_size, 3.0, "style lookup follows case-insensitive name rules");

    s.undo().unwrap();
    assert_eq!(s.doc().unwrap().dim_style("S").unwrap().arrow_size, 0.1);
    assert_eq!(dim(&s, a).block.as_deref(), Some("*A1_A"));
    assert_eq!(dim(&s, b).block.as_deref(), Some("*A1_B"));
    assert_eq!(dim(&s, b).overrides.get("arrowSize"), Some(&json!(1.25)));
    assert_eq!(rendered(s.doc().unwrap(), a), before_a);
    assert_eq!(rendered(s.doc().unwrap(), b), before_b);
    assert_eq!(rendered(s.doc().unwrap(), unrelated), before_unrelated);
    assert_eq!(block_dim_block(&s).as_deref(), Some("*A1_BLOCK_DIM"));

    s.redo().unwrap();
    assert_eq!(s.doc().unwrap().dim_style("S").unwrap().arrow_size, 3.0);
    assert!(dim(&s, a).block.is_none() && dim(&s, b).block.is_none());
    assert_eq!(dim(&s, b).overrides.get("arrowSize"), Some(&json!(1.25)));
    assert_eq!(rendered(s.doc().unwrap(), a), after_a);
    assert_eq!(rendered(s.doc().unwrap(), b), after_b);
    assert_eq!(rendered(s.doc().unwrap(), unrelated), after_unrelated);
    assert_eq!(block_dim_block(&s), None);

    let saved = cadcraft_io::write_dxf(s.doc().unwrap());
    let reopened = cadcraft_io::read_dxf(saved.as_bytes()).unwrap();
    assert_eq!(reopened.dim_style("S").unwrap().arrow_size, 3.0);
    let reopened_b = match &reopened.entity(b).unwrap().kind {
        EntityKind::Dimension(dm) => dm,
        other => panic!("expected reopened dimension, got {other:?}"),
    };
    assert_eq!(reopened_b.overrides.get("arrowSize"), Some(&json!(1.25)));
    for (handle, expected) in [(a, &after_a), (b, &after_b)] {
        let expected_bounds = cadcraft_geom::Bounds2::from_points(expected.iter().flat_map(|(_, points)| points.iter().copied()));
        let actual_bounds = render_bounds(&reopened, handle);
        assert!(
            actual_bounds.min.dist(expected_bounds.min) < 1e-6 && actual_bounds.max.dist(expected_bounds.max) < 1e-6,
            "round-trip render bounds changed for {}: {actual_bounds:?} != {expected_bounds:?}",
            handle.hex()
        );
    }
}

#[test]
fn linear_dimension_follows_line_endpoint() {
    let mut s = Session::new();
    let l = line(&mut s, [0.0, 0.0], [10.0, 0.0]);
    let d = h(&s.execute("dimlinear", &json!({ "p1": [0, 0], "p2": [10, 0], "at": [5, -2] })).unwrap());
    let dm = dim(&s, d);
    assert_eq!(dm.assoc.len(), 2, "both origins attach to the line: {:?}", dm.assoc);
    assert!(dm.assoc.iter().all(|a| a.handle == l));
    // Move the line's end point: the dimension's second origin follows.
    s.execute("properties.set", &json!({ "handles": [l.hex()], "end": [12, 0] })).unwrap();
    let dm = dim(&s, d);
    assert!(near(dm.p14.xy(), Vec2::new(12.0, 0.0)), "{:?}", dm.p14);
    assert!(near(dm.p13.xy(), Vec2::ZERO));
    // The dimension line stays where it was.
    assert!(near(dm.defpt.xy(), Vec2::new(5.0, -2.0)));
    let g = cadcraft_render::dimension_in(s.doc().unwrap(), &dm);
    assert_eq!(g.value, "12.0000");
    // Undo restores both in one step.
    s.undo().unwrap();
    assert!(near(dim(&s, d).p14.xy(), Vec2::new(10.0, 0.0)));
}

#[test]
fn moving_the_object_carries_the_dimension() {
    let mut s = Session::new();
    let l = line(&mut s, [0.0, 0.0], [10.0, 0.0]);
    let d = h(&s.execute("dimaligned", &json!({ "p1": [0, 0], "p2": [10, 0], "at": [5, -2] })).unwrap());
    s.execute("move", &json!({ "handles": [l.hex()], "from": [0, 0], "to": [3, 4] })).unwrap();
    let dm = dim(&s, d);
    assert!(near(dm.p13.xy(), Vec2::new(3.0, 4.0)) && near(dm.p14.xy(), Vec2::new(13.0, 4.0)), "{dm:?}");
    assert!(near(dm.defpt.xy(), Vec2::new(8.0, 2.0)), "rigid move carries the dimension line: {:?}", dm.defpt);
    // Moving both together keeps them attached and unchanged relative to each other.
    s.execute("move", &json!({ "handles": [l.hex(), d.hex()], "from": [0, 0], "to": [1, 0] })).unwrap();
    let dm = dim(&s, d);
    assert_eq!(dm.assoc.len(), 2);
    assert!(near(dm.p13.xy(), Vec2::new(4.0, 4.0)) && near(dm.defpt.xy(), Vec2::new(9.0, 2.0)), "{dm:?}");
}

#[test]
fn moving_the_object_keeps_the_ordinate_datum() {
    let mut s = Session::new();
    let l = line(&mut s, [5.0, 0.0], [5.0, 5.0]);
    let d = h(&s.execute("dimordinate", &json!({ "feature": [5, 5], "leader": [5, 8] })).unwrap());
    s.execute("move", &json!({ "handles": [l.hex()], "from": [5, 5], "to": [8, 5] })).unwrap();
    let dm = dim(&s, d);
    assert!(near(dm.p13.xy(), Vec2::new(8.0, 5.0)) && near(dm.p14.xy(), Vec2::new(8.0, 8.0)), "{dm:?}");
    assert!(near(dm.defpt.xy(), Vec2::ZERO), "datum must stay at the origin: {:?}", dm.defpt);
    let g = cadcraft_render::dimension_in(s.doc().unwrap(), &dm);
    assert_eq!(g.value, "8.0000");
}

#[test]
fn moving_the_line_keeps_a_free_continued_origin() {
    let mut s = Session::new();
    let l = line(&mut s, [0.0, 0.0], [10.0, 0.0]);
    s.execute("dimlinear", &json!({ "p1": [0, 0], "p2": [10, 0], "at": [5, -2] })).unwrap();
    s.execute("dimcontinue", &json!({ "points": [[25, 0]] })).unwrap();
    let d = s.doc().unwrap().model.last().unwrap().handle;
    s.execute("move", &json!({ "handles": [l.hex()], "delta": [3, 0] })).unwrap();
    let dm = dim(&s, d);
    assert!(near(dm.p13.xy(), Vec2::new(13.0, 0.0)), "attached origin follows: {dm:?}");
    assert!(near(dm.p14.xy(), Vec2::new(25.0, 0.0)), "free origin stays: {dm:?}");
    assert_eq!(cadcraft_render::dimension_in(s.doc().unwrap(), &dm).value, "12.0000");
    s.execute("move", &json!({ "handles": [l.hex()], "delta": [3, 0] })).unwrap();
    let dm = dim(&s, d);
    assert!(near(dm.p13.xy(), Vec2::new(16.0, 0.0)) && near(dm.p14.xy(), Vec2::new(25.0, 0.0)), "{dm:?}");
    assert_eq!(cadcraft_render::dimension_in(s.doc().unwrap(), &dm).value, "9.0000");
}

#[test]
fn interactive_dimlinear_select_object_is_associative() {
    let mut s = Session::new();
    s.cmdline("line 0,0 6,0").unwrap();
    s.cmdline("").unwrap();
    let l = s.doc().unwrap().model.last().unwrap().handle;
    s.cmdline("dimlinear").unwrap();
    s.cmdline("").unwrap(); // <select object>
    s.cmdline("3,0").unwrap();
    s.cmdline("3,-1").unwrap();
    assert!(s.running.is_none());
    let d = s.last_dim.unwrap();
    assert_eq!(dim(&s, d).assoc.len(), 2);
    // A grip-style edit through properties.
    s.execute("properties.set", &json!({ "handles": [l.hex()], "start": [-1, 0] })).unwrap();
    assert!(near(dim(&s, d).p13.xy(), Vec2::new(-1.0, 0.0)));
}

#[test]
fn radius_and_diameter_follow_circle() {
    let mut s = Session::new();
    s.execute("circle", &json!({ "center": [0, 0], "radius": 2 })).unwrap();
    let c = s.doc().unwrap().model.last().unwrap().handle;
    let r = h(&s.execute("dimradius", &json!({ "handle": c.hex(), "at": [5, 0] })).unwrap());
    let dd = h(&s.execute("dimdiameter", &json!({ "handle": c.hex(), "at": [0, 5] })).unwrap());
    assert_eq!(dim(&s, r).assoc.len(), 2);
    assert_eq!(dim(&s, dd).assoc.len(), 2);
    s.execute("properties.set", &json!({ "handles": [c.hex()], "radius": 3 })).unwrap();
    let g = cadcraft_render::dimension_in(s.doc().unwrap(), &dim(&s, r));
    assert_eq!(g.value, "R3.0000");
    let g = cadcraft_render::dimension_in(s.doc().unwrap(), &dim(&s, dd));
    assert_eq!(g.value, "⌀6.0000");
    s.execute("move", &json!({ "handles": [c.hex()], "from": [0, 0], "to": [10, 0] })).unwrap();
    assert!(near(dim(&s, r).defpt.xy(), Vec2::new(10.0, 0.0)));
}

#[test]
fn angular_between_lines_follows_rotation() {
    let mut s = Session::new();
    let a = line(&mut s, [0.0, 0.0], [10.0, 0.0]);
    let b = line(&mut s, [0.0, 0.0], [0.0, 10.0]);
    let d = h(&s.execute("dimangular", &json!({ "lines": [a.hex(), b.hex()], "at": [3, 3] })).unwrap());
    let dm = dim(&s, d);
    // Two picked lines make a two-line angular dimension (type 2), tied to both lines at all four points.
    assert_eq!(dm.kind, DimKind::Angular);
    assert_eq!(dm.assoc.len(), 4);
    assert_eq!(cadcraft_render::dimension_in(s.doc().unwrap(), &dm).value, "90°");
    // Swing the second line to 45°.
    s.execute("properties.set", &json!({ "handles": [b.hex()], "end": [10, 10] })).unwrap();
    assert_eq!(cadcraft_render::dimension_in(s.doc().unwrap(), &dim(&s, d)).value, "45°");
}

#[test]
fn interactive_angular_picks_polyline_segments() {
    // Rectangle sides are polyline segments, not lines (issue #10).
    let mut s = Session::new();
    s.cmdline("rectang 0,0 10,5").unwrap();
    s.cmdline("dimangular").unwrap();
    s.cmdline("5,0").unwrap();
    s.cmdline("0,2").unwrap();
    s.cmdline("2,2").unwrap();
    assert!(s.running.is_none());
    let d = s.last_dim.unwrap();
    assert_eq!(cadcraft_render::dimension_in(s.doc().unwrap(), &dim(&s, d)).value, "90°");
    // A polyline arc segment works like an arc.
    s.cmdline("pline 20,0 30,0 a 30,10").unwrap();
    s.cmdline("").unwrap();
    s.cmdline("dimangular").unwrap();
    let (cx, cy) = (30.0 + 5.0 * std::f64::consts::FRAC_1_SQRT_2, 5.0 - 5.0 * std::f64::consts::FRAC_1_SQRT_2);
    s.cmdline(&format!("{cx},{cy}")).unwrap();
    s.cmdline("40,5").unwrap();
    assert!(s.running.is_none());
    let d2 = s.last_dim.unwrap();
    assert_ne!(d, d2);
    assert_eq!(cadcraft_render::dimension_in(s.doc().unwrap(), &dim(&s, d2)).value, "180°");
}

#[test]
fn interactive_angular_picks_circle() {
    // Circle: the pick is the first endpoint, the center the vertex.
    let mut s = Session::new();
    s.cmdline("circle 0,0 5").unwrap();
    s.cmdline("dimangular").unwrap();
    s.cmdline("5,0").unwrap();
    assert!(s.current_prompt().unwrap().message.contains("Specify second angle endpoint"));
    s.cmdline("0,8").unwrap();
    s.cmdline("3,3").unwrap();
    assert!(s.running.is_none());
    let dm = dim(&s, s.last_dim.unwrap());
    assert!(near(dm.p15.xy(), Vec2::ZERO), "vertex at the center");
    assert_eq!(cadcraft_render::dimension_in(s.doc().unwrap(), &dm).value, "90°");
}

#[test]
fn erase_disassociate_and_reassociate() {
    let mut s = Session::new();
    let l = line(&mut s, [0.0, 0.0], [4.0, 0.0]);
    let d = h(&s.execute("dimlinear", &json!({ "p1": [0, 0], "p2": [4, 0], "at": [2, 1] })).unwrap());
    s.execute("dimdisassociate", &json!({ "handles": [d.hex()] })).unwrap();
    assert!(dim(&s, d).assoc.is_empty());
    s.execute("properties.set", &json!({ "handles": [l.hex()], "end": [5, 0] })).unwrap();
    assert!(near(dim(&s, d).p14.xy(), Vec2::new(4.0, 0.0)), "not associative any more");
    // Put the line back and reassociate.
    s.execute("properties.set", &json!({ "handles": [l.hex()], "end": [4, 0] })).unwrap();
    let r = s.execute("dimreassociate", &json!({ "handles": [d.hex()] })).unwrap();
    assert_eq!(r["associated"], 1);
    assert_eq!(dim(&s, d).assoc.len(), 2);
    // Erasing the object leaves the dimension where it is (and no longer attached).
    s.execute("erase", &json!({ "handles": [l.hex()] })).unwrap();
    let dm = dim(&s, d);
    assert!(dm.assoc.is_empty());
    assert!(near(dm.p14.xy(), Vec2::new(4.0, 0.0)));
}

#[test]
fn dimassoc_zero_makes_plain_dimensions() {
    let mut s = Session::new();
    line(&mut s, [0.0, 0.0], [4.0, 0.0]);
    s.doc_mut().unwrap().header.set_i64("DIMASSOC", 0);
    let d = h(&s.execute("dimlinear", &json!({ "p1": [0, 0], "p2": [4, 0], "at": [2, 1] })).unwrap());
    assert!(dim(&s, d).assoc.is_empty());
}

#[test]
fn dimstyle_commands() {
    let mut s = Session::new();
    let r = s.execute("dimstyle", &json!({ "name": "Arch", "DIMTSZ": 0.1, "DIMBLK": "_ARCHTICK", "DIMLUNIT": 4, "DIMTAD": 1, "bogus": 3 })).unwrap();
    assert_eq!(r["ignored"], json!(["bogus"]));
    let st = s.doc().unwrap().dim_style("Arch").unwrap().clone();
    assert_eq!(st.tick_size, 0.1);
    assert_eq!(st.arrow_block, "_ARCHTICK");
    assert_eq!(st.linear_unit, 4);
    assert_eq!(st.text_above, 1);
    let l = s.execute("dimstyle.list", &json!({ "name": "Arch" })).unwrap();
    assert_eq!(l["variables"]["DIMTSZ"], json!(0.1));
    assert_eq!(l["current"], "Arch");
    let d = h(&s.execute("dimlinear", &json!({ "p1": [0, 0], "p2": [18.5, 0], "at": [5, 2] })).unwrap());
    assert_eq!(cadcraft_render::dimension_in(s.doc().unwrap(), &dim(&s, d)).value, "1'-6 1/2\"");
    // In use: cannot delete; rename follows into dimensions.
    s.execute("dimstyle.current", &json!({ "name": "Standard" })).unwrap();
    assert!(s.execute("dimstyle.delete", &json!({ "name": "Arch" })).is_err());
    s.execute("dimstyle.rename", &json!({ "from": "Arch", "to": "Architectural" })).unwrap();
    assert_eq!(dim(&s, d).style, "Architectural");
    s.execute("erase", &json!({ "handles": [d.hex()] })).unwrap();
    s.execute("dimstyle.delete", &json!({ "name": "Architectural" })).unwrap();
    assert!(s.doc().unwrap().dim_style("Architectural").is_none());
    assert!(s.execute("dimstyle.delete", &json!({ "name": "Standard" })).is_err());
}

#[test]
fn dimstyle_rename_updates_block_dimensions() {
    let mut s = Session::new();
    s.execute("dimstyle", &json!({ "name": "DS", "DIMTXT": 2 })).unwrap();
    s.execute("dimlinear", &json!({ "p1": [0, 0], "p2": [10, 0], "at": [5, 3] })).unwrap();
    s.execute("selectall", &json!({})).unwrap();
    s.execute("block", &json!({ "name": "BD", "base": [0, 0], "keep": "convert" })).unwrap();
    let in_block = |s: &Session| {
        let b = s.doc().unwrap().block("BD").unwrap().clone();
        b.entities.iter().find_map(|e| if let EntityKind::Dimension(d) = &e.kind { Some(d.style.clone()) } else { None }).unwrap()
    };
    assert_eq!(in_block(&s), "DS");
    assert!(s.execute("dimstyle.delete", &json!({ "name": "DS" })).is_err());
    s.execute("dimstyle.rename", &json!({ "from": "DS", "to": "DS2" })).unwrap();
    assert_eq!(in_block(&s), "DS2");
}

#[test]
fn dimension_overrides() {
    let mut s = Session::new();
    let d = h(&s.execute("dimlinear", &json!({ "p1": [0, 0], "p2": [10, 0], "at": [5, 2] })).unwrap());
    s.execute("dimoverride", &json!({ "handles": [d.hex()], "DIMDEC": 1, "DIMPOST": "<> mm" })).unwrap();
    let dm = dim(&s, d);
    assert_eq!(dm.overrides.get("decimals"), Some(&json!(1)));
    assert_eq!(cadcraft_render::dimension_in(s.doc().unwrap(), &dm).value, "10.0 mm");
    // The interactive form.
    s.cmdline("dimoverride").unwrap();
    s.cmdline("DIMDEC").unwrap();
    s.cmdline("2").unwrap();
    s.cmdline("").unwrap();
    s.input(crate::Input::Pick(vec![d])).unwrap();
    s.cmdline("").unwrap();
    assert!(s.running.is_none());
    assert_eq!(cadcraft_render::dimension_in(s.doc().unwrap(), &dim(&s, d)).value, "10.00 mm");
    s.execute("dimstyle.override", &json!({ "handles": [d.hex()], "clear": true })).unwrap();
    assert!(dim(&s, d).overrides.is_empty());
    assert!(s.execute("dimoverride", &json!({ "handles": [d.hex()], "nope": 1 })).is_err());
    // Plain text override keeps working.
    s.execute("dimoverride", &json!({ "handles": [d.hex()], "text": "<> TYP" })).unwrap();
    assert_eq!(dim(&s, d).text, "<> TYP");
}

#[test]
fn dimension_override_text_with_variables() {
    let mut s = Session::new();
    let d = h(&s.execute("dimlinear", &json!({ "p1": [0, 0], "p2": [10, 0], "at": [5, 2] })).unwrap());
    s.execute("dimoverride", &json!({ "handles": [d.hex()], "text": "<> OLD" })).unwrap();
    s.execute("dimoverride", &json!({ "handles": [d.hex()], "DIMTXT": 0.4 })).unwrap();
    let r = s.execute("dimoverride", &json!({ "handles": [d.hex()], "text": "<> TYP", "DIMDEC": 1 })).unwrap();
    assert_eq!(r["changed"], json!(1));
    let dm = dim(&s, d);
    assert_eq!(dm.text, "<> TYP");
    assert_eq!(dm.overrides.get("decimals"), Some(&json!(1)));
    assert!(dm.overrides.contains_key("textHeight"));
    // An unknown variable next to valid text is still an error and changes nothing.
    assert!(s.execute("dimoverride", &json!({ "handles": [d.hex()], "text": "X", "nope": 1 })).is_err());
    assert_eq!(dim(&s, d).text, "<> TYP");
}

#[test]
fn dimtedit_and_dimspace() {
    let mut s = Session::new();
    let a = h(&s.execute("dimlinear", &json!({ "p1": [0, 0], "p2": [10, 0], "at": [5, 1] })).unwrap());
    let b = h(&s.execute("dimlinear", &json!({ "p1": [0, 0], "p2": [6, 0], "at": [3, 1.3] })).unwrap());
    s.execute("dimtedit.left", &json!({ "handles": [a.hex()] })).unwrap();
    let g = cadcraft_render::dimension_in(s.doc().unwrap(), &dim(&s, a));
    assert!(g.text_pos.x < 5.0);
    s.execute("dimtedit.angle", &json!({ "handles": [a.hex()], "angle": 30 })).unwrap();
    let g = cadcraft_render::dimension_in(s.doc().unwrap(), &dim(&s, a));
    assert!((g.text_angle - 30f64.to_radians()).abs() < 1e-9);
    s.execute("dimtedit.home", &json!({ "handles": [a.hex()] })).unwrap();
    assert!(dim(&s, a).overrides.is_empty() && dim(&s, a).text_rotation == 0.0);
    s.execute("dimspace", &json!({ "base": a.hex(), "handles": [b.hex()], "spacing": 0.5 })).unwrap();
    assert!((dim(&s, b).defpt.y - 1.5).abs() < 1e-9, "{:?}", dim(&s, b).defpt);
}

#[test]
fn text_style_rename_follows_block_definition_text() {
    let mut s = Session::new();
    s.execute("style", &json!({ "name": "SRC", "height": 1, "backwards": true })).unwrap();
    let t = h(&s.execute("text", &json!({ "at": [0, 0], "text": "abc" })).unwrap());
    let m = h(&s.execute("mtext", &json!({ "at": [0, 2], "text": "def", "style": "SRC" })).unwrap());
    let a = h(&s.execute("attdef", &json!({ "tag": "TAG", "at": [0, 4] })).unwrap());
    let hs = [t.hex(), m.hex(), a.hex()];
    s.execute("block", &json!({ "name": "Label", "base": [0, 0], "handles": hs, "keep": "delete" })).unwrap();
    s.execute("style.current", &json!({ "name": "Standard" })).unwrap();
    s.execute("style.rename", &json!({ "from": "SRC", "to": "DST" })).unwrap();
    let d = s.doc().unwrap();
    let mut styles: Vec<String> = d
        .block("Label")
        .unwrap()
        .entities
        .iter()
        .filter_map(|e| match &e.kind {
            EntityKind::Text(t) => Some(format!("text {}", t.style)),
            EntityKind::MText(t) => Some(format!("mtext {}", t.style)),
            EntityKind::AttDef(a) => Some(format!("attdef {}", a.text.style)),
            _ => None,
        })
        .collect();
    styles.sort();
    assert_eq!(styles, ["attdef DST", "mtext DST", "text DST"]);
    assert!(d.text_style("DST").is_some_and(|st| st.backwards));
}

#[test]
fn dimspace_prompts_for_its_inputs() {
    let mut s = Session::new();
    let a = h(&s.execute("dimlinear", &json!({ "p1": [0, 0], "p2": [10, 0], "at": [0, 5] })).unwrap());
    let b = h(&s.execute("dimlinear", &json!({ "p1": [0, 0], "p2": [20, 0], "at": [0, 9] })).unwrap());
    // The menu / typed start used to fail with "`base` dimension is required".
    s.start("dimspace").unwrap();
    assert!(s.prompt_text().contains("Select base dimension"), "{}", s.prompt_text());
    s.input(Input::Pick(vec![a])).unwrap();
    assert!(s.prompt_text().contains("Select dimensions to space"), "{}", s.prompt_text());
    s.input(Input::Pick(vec![b])).unwrap();
    s.input(Input::Enter).unwrap();
    assert!(s.prompt_text().contains("Enter value"), "{}", s.prompt_text());
    s.input(Input::Text("2".into())).unwrap();
    assert!(s.running.is_none());
    assert!((dim(&s, a).defpt.y - 5.0).abs() < 1e-9, "{:?}", dim(&s, a).defpt);
    assert!((dim(&s, b).defpt.y - 7.0).abs() < 1e-9, "{:?}", dim(&s, b).defpt);
}

#[test]
fn text_style_commands() {
    let mut s = Session::new();
    let r = s
        .execute(
            "style",
            &json!({ "name": "Notes", "font": "Arial", "height": 0.25, "widthFactor": 0.8, "oblique": 15, "backwards": true, "upsideDown": true }),
        )
        .unwrap();
    assert_eq!(r["current"], "Notes");
    let st = s.doc().unwrap().text_style("Notes").unwrap().clone();
    assert!(st.backwards && st.upside_down);
    assert!((st.oblique.to_degrees() - 15.0).abs() < 1e-9);
    s.execute("text", &json!({ "at": [0, 0], "height": 0.25, "text": "X" })).unwrap();
    let l = s.execute("style.list", &json!({})).unwrap();
    assert_eq!(l["styles"].as_array().unwrap().len(), 2);
    s.execute("style.rename", &json!({ "from": "Notes", "to": "Labels" })).unwrap();
    assert_eq!(s.doc().unwrap().header.str("TEXTSTYLE", ""), "Labels");
    assert!(s.execute("style.delete", &json!({ "name": "Labels" })).is_err(), "current style");
    s.execute("style.current", &json!({ "name": "Standard" })).unwrap();
    let used = s.doc().unwrap().model.iter().any(|e| matches!(&e.kind, EntityKind::Text(t) if t.style == "Labels"));
    if used {
        assert!(s.execute("style.delete", &json!({ "name": "Labels" })).is_err(), "in use");
    } else {
        s.execute("style.delete", &json!({ "name": "Labels" })).unwrap();
    }
    assert!(s.execute("style.delete", &json!({ "name": "Standard" })).is_err());
}

#[test]
fn table_create_edit_merge() {
    let mut s = Session::new();
    let t = h(&s
        .execute(
            "table",
            &json!({ "at": [0, 10], "rows": 2, "cols": 3, "title": "Parts", "header": ["No", "Name", "Qty"], "cells": [["1", "Bolt", "4"]] }),
        )
        .unwrap());
    let get = |s: &Session| match &s.doc().unwrap().entity(t).unwrap().kind {
        EntityKind::Table(x) => x.clone(),
        _ => panic!(),
    };
    let tb = get(&s);
    assert_eq!(tb.row_heights.len(), 4);
    assert_eq!(tb.col_widths.len(), 3);
    assert_eq!(tb.cells[0][0].merged, Some((1, 3)));
    assert_eq!(tb.cells[2][1].text, "Bolt");
    s.execute("table.set", &json!({ "handle": t.hex(), "row": 3, "col": 1, "text": "Nut" })).unwrap();
    assert_eq!(get(&s).cells[3][1].text, "Nut");
    // Writing into a covered title cell writes the title.
    s.execute("table.set", &json!({ "handle": t.hex(), "row": 0, "col": 2, "text": "Bill of materials" })).unwrap();
    assert_eq!(get(&s).cells[0][0].text, "Bill of materials");
    s.execute("table.insertcol", &json!({ "handle": t.hex(), "col": 1 })).unwrap();
    let tb = get(&s);
    assert_eq!(tb.col_widths.len(), 4);
    assert_eq!(tb.cells[0][0].merged, Some((1, 4)), "title merge grows");
    assert_eq!(tb.cells[2][2].text, "Bolt");
    s.execute("table.deletecol", &json!({ "handle": t.hex(), "col": 1 })).unwrap();
    s.execute("table.insertrow", &json!({ "handle": t.hex() })).unwrap();
    assert_eq!(get(&s).row_heights.len(), 5);
    s.execute("table.merge", &json!({ "handle": t.hex(), "row": 2, "col": 0, "rows": 2, "cols": 1 })).unwrap();
    let tb = get(&s);
    assert_eq!(tb.cells[2][0].merged, Some((2, 1)));
    assert!(s.execute("table.deleterow", &json!({ "handle": t.hex(), "row": 2 })).is_ok());
    assert_eq!(get(&s).cells[2][0].merged, None, "a one-row merge is no merge");
    s.execute("table.unmerge", &json!({ "handle": t.hex(), "row": 0, "col": 1 })).unwrap();
    assert_eq!(get(&s).cells[0][0].merged, None);
    assert!(s.execute("table.set", &json!({ "handle": t.hex(), "row": 99, "col": 0, "text": "x" })).is_err());
    assert!(s.execute("table", &json!({ "at": [0, 0], "rows": 1_000_000, "cols": 3 })).is_err());
    // It renders: grid lines plus text.
    let dl = cadcraft_render::build(s.doc().unwrap(), &cadcraft_doc::Space::Model, &cadcraft_render::Options::default());
    assert!(dl.prims.len() > 10);
}

#[test]
fn table_interactive() {
    let mut s = Session::new();
    s.cmdline("table").unwrap();
    s.cmdline("3").unwrap();
    s.cmdline("2").unwrap();
    s.cmdline("0,0").unwrap();
    assert!(s.running.is_none());
    match &s.doc().unwrap().model.last().unwrap().kind {
        EntityKind::Table(t) => {
            assert_eq!(t.col_widths.len(), 3);
            assert_eq!(t.row_heights.len(), 4, "title + header + 2 data rows");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn hostile_annotation_params_never_panic() {
    let mut s = Session::new();
    for (id, p) in [
        ("dimstyle", json!({ "name": "X", "DIMDEC": -5, "DIMTAD": 1e300, "DIMCLRD": "nonsense", "DIMBLK": 7 })),
        ("dimoverride", json!({ "handles": ["zz"], "DIMDEC": 1 })),
        ("dimreassociate", json!({ "handles": [1, 2, 3] })),
        ("dimangular", json!({ "lines": ["1"], "at": [0, 0] })),
        ("dimlinear", json!({ "object": "ffff", "at": [0, 0] })),
        ("table.merge", json!({ "handle": "1", "row": 0, "col": 0, "rows": 1e9, "cols": -1 })),
        ("table", json!({ "at": [0, 0], "cols": 0 })),
        ("style", json!({ "name": "" })),
        ("style.rename", json!({ "from": "Standard", "to": "Y" })),
        ("dimspace", json!({ "base": "1", "handles": [] })),
        ("dimtedit", json!({ "handles": [], "mode": "angle" })),
    ] {
        let _ = s.execute(id, &p);
    }
    let d = h(&s.execute("dimlinear", &json!({ "p1": [0, 0], "p2": [1, 0], "at": [0, 1] })).unwrap());
    let _ = s.execute("dimoverride", &json!({ "handles": [d.hex()], "DIMTXT": -1, "DIMSCALE": 0, "DIMLUNIT": 99 }));
    let _ = cadcraft_render::build(s.doc().unwrap(), &cadcraft_doc::Space::Model, &cadcraft_render::Options::default());
    assert!(matches!(dim(&s, d).kind, DimKind::Linear { .. }));
}

#[test]
fn mtext_rejects_nonpositive_height() {
    let mut s = Session::new();
    for height in [0.0, -2.0] {
        assert!(s.execute("mtext", &json!({ "at": [0, 0], "text": "hi", "height": height })).is_err(), "height {height}");
    }
    assert!(s.doc().unwrap().model.is_empty());
    s.execute("mtext", &json!({ "at": [0, 0], "text": "hi", "height": 2 })).unwrap();
    assert_eq!(s.doc().unwrap().model.len(), 1);
}

#[test]
fn setvar_dim_variables_apply_to_new_dimensions() {
    // Issue #66: a DIM* variable set with SETVAR is a style override that new dimensions take on.
    let mut s = Session::new();
    let linear = |s: &mut Session, y: f64| h(&s.execute("dimlinear", &json!({ "p1": [0, 0], "p2": [100, 0], "at": [50, y] })).unwrap());
    let text_height = |s: &Session, hd: Handle| {
        let d = s.doc().unwrap();
        let dm = dim(s, hd);
        let st = d.dim_style(&dm.style).cloned().unwrap();
        cadcraft_render::dimension_geometry(&dm, &st, d.header.f64("DIMSCALE", 1.0)).text_height
    };
    let first = linear(&mut s, -20.0);
    let base = text_height(&s, first);
    s.execute("setvar", &json!({ "name": "DIMSCALE", "value": 24 })).unwrap();
    let scaled = linear(&mut s, -40.0);
    assert!((text_height(&s, scaled) / base - 24.0).abs() < 1e-9);
    assert_eq!(dim(&s, scaled).overrides.get("scale"), Some(&json!(24.0)));
    assert!(dim(&s, first).overrides.is_empty(), "existing dimensions keep their size");
    assert!((text_height(&s, first) - base).abs() < 1e-12);
    // Extents use the same scale as the geometry: room for 2.5 text heights beyond the points.
    let d = s.doc().unwrap();
    let margin = |hd: Handle, span: f64| (cadcraft_doc::entity_bounds(d, d.entity(hd).unwrap(), 0).height() - span) / 2.0;
    assert!((margin(first, 20.0) - 2.5 * base).abs() < 1e-9);
    assert!((margin(scaled, 40.0) - 2.5 * text_height(&s, scaled)).abs() < 1e-9);

    // Making a style current clears the overrides.
    s.execute("dimstyle.current", &json!({ "name": "Standard" })).unwrap();
    assert_eq!(crate::sysvars::get(&s, "DIMSCALE"), Some(json!(1.0)));
    let plain = linear(&mut s, -60.0);
    assert!(dim(&s, plain).overrides.is_empty());

    // Editing the current style updates the variables, so the issue's workaround keeps working.
    s.execute("dimstyle", &json!({ "name": "Standard", "DIMSCALE": 24 })).unwrap();
    assert_eq!(crate::sysvars::get(&s, "DIMSCALE"), Some(json!(24.0)));
    let restyled = linear(&mut s, -80.0);
    assert!(dim(&s, restyled).overrides.is_empty());
    assert!((text_height(&s, restyled) / base - 24.0).abs() < 1e-9);

    // A DIM* variable the header doesn't carry reads from the style and can be overridden too.
    let dec = crate::sysvars::get(&s, "DIMDEC").unwrap();
    assert_eq!(dec, json!(s.doc().unwrap().dim_style("Standard").unwrap().decimals));
    s.execute("setvar", &json!({ "name": "DIMDEC", "value": 1 })).unwrap();
    let one = linear(&mut s, -100.0);
    let one = dim(&s, one);
    assert_eq!(one.overrides.get("decimals"), Some(&json!(1)));
    assert_eq!(cadcraft_render::dimension_in(s.doc().unwrap(), &one).value, "100.0");

    // DIMSCALE 0 (AutoCAD: scale to the layout viewport) never shrinks a dimension to nothing.
    s.execute("setvar", &json!({ "name": "DIMSCALE", "value": 0 })).unwrap();
    let zero = linear(&mut s, -120.0);
    assert!((text_height(&s, zero) - base).abs() < 1e-12);
}

#[test]
fn linetype_rename_updates_references() {
    fn line(s: &mut Session, y: f64) -> Handle {
        let r = s.execute("line", &json!({ "points": [[0.0, y], [10.0, y]] })).unwrap();
        Handle::parse_hex(r["handles"][0].as_str().unwrap()).unwrap()
    }
    let mut s = Session::new();
    s.execute("linetype", &json!({ "load": "DASHED" })).unwrap();
    s.execute("layer.new", &json!({ "name": "A", "linetype": "DASHED", "current": true })).unwrap();
    let by_layer = line(&mut s, 0.0);
    s.execute("linetype", &json!({ "current": "DASHED" })).unwrap();
    let explicit = line(&mut s, 2.0);
    let in_block = line(&mut s, 4.0);
    s.execute("block", &json!({ "name": "B", "base": [0, 0], "handles": [in_block.hex()], "keep": "delete" })).unwrap();
    s.execute("rename", &json!({ "table": "linetype", "from": "DASHED", "to": "MYDASH" })).unwrap();
    let d = s.doc().unwrap();
    assert!(d.linetype("DASHED").is_none());
    assert!(d.linetype("MYDASH").is_some());
    assert_eq!(d.layers.iter().find(|l| l.name == "A").unwrap().linetype, "MYDASH");
    assert_eq!(d.layers.iter().find(|l| l.name == "0").unwrap().linetype, "Continuous");
    assert_eq!(d.entity(by_layer).unwrap().common.linetype, "ByLayer");
    assert_eq!(d.entity(explicit).unwrap().common.linetype, "MYDASH");
    let block: Vec<&str> = d.blocks["B"].entities.iter().map(|e| e.common.linetype.as_str()).collect();
    assert_eq!(block, ["MYDASH"]);
    assert_eq!(d.header.str("CELTYPE", ""), "MYDASH");
    // An existing target name and the reserved linetypes are refused.
    assert!(s.execute("rename", &json!({ "table": "linetype", "from": "MYDASH", "to": "Continuous" })).is_err());
    assert!(s.execute("rename", &json!({ "table": "linetype", "from": "ByLayer", "to": "X" })).is_err());
    assert_eq!(s.doc().unwrap().entity(by_layer).unwrap().common.linetype, "ByLayer");
}

#[test]
fn directrizzr_creates_continuous_arrow_without_content_or_landing_and_supports_undo_redo() {
    fn mleader_at(s: &Session, idx: usize) -> cadcraft_doc::MLeader {
        match &s.doc().unwrap().model.iter().nth(idx).unwrap().kind {
            EntityKind::MLeader(m) => m.clone(),
            other => panic!("expected MLeader at {idx}, got {other:?}"),
        }
    }

    let spec = crate::find_command("directrizzr").expect("directrizzr command must be registered");
    assert_eq!(spec.id, "directrizzr");
    assert!(spec.interactive.is_some());

    let mut s = Session::new();

    // Two-point interactive invocation terminated with Enter.
    s.script("directrizzr 0,0 10,5\n\n").unwrap();
    assert!(s.running.is_none());
    assert_eq!(s.doc().unwrap().model.len(), 1);
    let m2 = mleader_at(&s, 0);
    assert!(m2.text.is_none(), "directrizzr must not create MText content");
    assert!(m2.block.is_none(), "directrizzr must not create block content");
    assert_eq!(m2.dogleg, 0.0, "directrizzr must not add an automatic horizontal landing");
    assert!(!m2.spline, "directrizzr must use straight segments");
    assert_eq!(m2.leaders, vec![vec![Vec3::new(0.0, 0.0, 0.0)]]);
    assert_eq!(m2.landing, Vec3::new(10.0, 5.0, 0.0));
    assert_eq!(m2.leader_paths(), vec![vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 5.0)]]);

    // Multi-point interactive invocation: continuous segments through all marked points, finished by Enter.
    let pts = [Vec2::new(20.0, 0.0), Vec2::new(30.0, 10.0), Vec2::new(45.0, 10.0), Vec2::new(55.0, 25.0)];
    s.start("directrizzr").unwrap();
    assert_eq!(s.prompt_text(), "DIRECTRIZZR Specify leader arrowhead location:");
    for (i, &pt) in pts.iter().enumerate() {
        s.input(Input::Point(pt)).unwrap();
        assert!(s.running.is_some(), "directrizzr must wait for Enter after point {i}");
        assert_eq!(s.prompt_text(), "DIRECTRIZZR Specify next point:");
    }
    s.input(Input::Enter).unwrap();
    assert!(s.running.is_none(), "Enter after >= 2 points must finish directrizzr");
    assert_eq!(s.doc().unwrap().model.len(), 2);

    let m_multi = mleader_at(&s, 1);
    assert!(m_multi.text.is_none());
    assert!(m_multi.block.is_none());
    assert_eq!(m_multi.dogleg, 0.0);
    assert!(!m_multi.spline);
    assert_eq!(
        m_multi.leaders,
        vec![vec![Vec3::new(20.0, 0.0, 0.0), Vec3::new(30.0, 10.0, 0.0), Vec3::new(45.0, 10.0, 0.0)]]
    );
    assert_eq!(m_multi.landing, Vec3::new(55.0, 25.0, 0.0));
    assert_eq!(m_multi.leader_paths(), vec![pts.to_vec()]);

    // Rendered primitives: exactly one filled arrowhead triangle (`Kind::Tris` with tip at pts[0]) and
    // one continuous polyline whose vertices follow the segment path and end at the last point with no landing extension.
    let h_multi = s.doc().unwrap().model.iter().nth(1).unwrap().handle;
    let list = cadcraft_render::build(s.doc().unwrap(), &Space::Model, &cadcraft_render::Options::default());
    let prims: Vec<_> = list.prims.iter().filter(|p| p.handle == h_multi).collect();
    assert_eq!(prims.len(), 2, "expected only polyline + arrowhead tris, got {prims:?}");
    let poly = prims.iter().find(|p| p.kind == cadcraft_render::Kind::Polyline).expect("leader polyline");
    let arrow = prims.iter().find(|p| p.kind == cadcraft_render::Kind::Tris).expect("filled arrowhead");
    let poly_pts = list.points(poly);
    let arrow_pts = list.points(arrow);
    assert!(arrow_pts.iter().any(|p| p.dist(pts[0]) < 1e-9), "arrowhead must be placed at first point: {arrow_pts:?}");
    assert_eq!(poly_pts.len(), pts.len(), "leader polyline must have one vertex per point with no extra landing segment");
    for (actual, expected) in poly_pts.iter().skip(1).zip(pts.iter().skip(1)) {
        assert!(actual.dist(*expected) < 1e-9, "segment continuity mismatch: {actual:?} != {expected:?}");
    }

    // Programmatic execution also produces a continuous arrow without landing or content.
    let json_h = h(&s.execute("directrizzr", &json!({ "points": [[0, 10], [5, 15], [12, 15]] })).unwrap());
    let EntityKind::MLeader(m_json) = &s.doc().unwrap().entity(json_h).unwrap().kind else {
        panic!("expected MLeader");
    };
    assert!(m_json.text.is_none() && m_json.block.is_none() && m_json.dogleg == 0.0);
    assert_eq!(m_json.leader_paths(), vec![vec![Vec2::new(0.0, 10.0), Vec2::new(5.0, 15.0), Vec2::new(12.0, 15.0)]]);

    // Undo / Redo round-trip.
    assert_eq!(s.doc().unwrap().model.len(), 3);
    s.undo().unwrap();
    assert_eq!(s.doc().unwrap().model.len(), 2);
    s.undo().unwrap();
    assert_eq!(s.doc().unwrap().model.len(), 1);
    s.redo().unwrap();
    assert_eq!(s.doc().unwrap().model.len(), 2);
    assert_eq!(mleader_at(&s, 1).leader_paths(), vec![pts.to_vec()]);
}

#[test]
fn directrizzr_cancel_and_early_enter_leave_document_dirty_revision_and_history_intact() {
    // 1. Clean document: Escape at 0, 1, and 2 points, and Enter at 0 and 1 point.
    let mut clean = Session::new();
    let rev0 = clean.state().unwrap().revision;
    assert!(!clean.state().unwrap().is_dirty());
    assert_eq!(clean.state().unwrap().undo.len(), 0);
    assert_eq!(clean.state().unwrap().redo.len(), 0);

    for pts in [vec![], vec![Vec2::new(0.0, 0.0)], vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 5.0)]] {
        clean.start("directrizzr").unwrap();
        for p in pts {
            clean.input(Input::Point(p)).unwrap();
        }
        clean.cancel();
        assert!(clean.running.is_none());
        assert!(clean.doc().unwrap().model.is_empty());
        assert!(!clean.state().unwrap().is_dirty(), "Escape must not dirty a clean document");
        assert_eq!(clean.state().unwrap().revision, rev0, "Escape must not bump revision");
        assert_eq!(clean.state().unwrap().undo.len(), 0);
        assert_eq!(clean.state().unwrap().redo.len(), 0);
    }

    for pts in [vec![], vec![Vec2::new(0.0, 0.0)]] {
        clean.start("directrizzr").unwrap();
        for p in pts {
            clean.input(Input::Point(p)).unwrap();
        }
        clean.input(Input::Enter).unwrap();
        assert!(clean.running.is_none(), "Enter before 2 points must cancel directrizzr without opening options");
        assert!(clean.doc().unwrap().model.is_empty());
        assert!(!clean.state().unwrap().is_dirty());
        assert_eq!(clean.state().unwrap().revision, rev0);
        assert_eq!(clean.state().unwrap().undo.len(), 0);
        assert_eq!(clean.state().unwrap().redo.len(), 0);
    }

    // 2. Modified document with active Undo and Redo stacks.
    let mut modified = Session::new();
    let _l1 = line(&mut modified, [0.0, 0.0], [5.0, 0.0]);
    let _l2 = line(&mut modified, [0.0, 2.0], [5.0, 2.0]);
    modified.undo().unwrap();
    let rev_mod = modified.state().unwrap().revision;
    let undo_len = modified.state().unwrap().undo.len();
    let redo_len = modified.state().unwrap().redo.len();
    assert!(modified.state().unwrap().is_dirty());
    assert_eq!(modified.doc().unwrap().model.len(), 1);
    assert_eq!(redo_len, 1);

    modified.start("directrizzr").unwrap();
    modified.input(Input::Point(Vec2::new(1.0, 1.0))).unwrap();
    modified.input(Input::Point(Vec2::new(4.0, 3.0))).unwrap();
    modified.cancel();

    assert!(modified.running.is_none());
    assert_eq!(modified.doc().unwrap().model.len(), 1, "cancel must not leave residual entities");
    assert!(modified.state().unwrap().is_dirty());
    assert_eq!(modified.state().unwrap().revision, rev_mod, "cancel must not alter revision");
    assert_eq!(modified.state().unwrap().undo.len(), undo_len, "cancel must not push undo history");
    assert_eq!(modified.state().unwrap().redo.len(), redo_len, "cancel must not clear redo history");
}

#[test]
fn directrizzr_preserves_remembered_mleader_options() {
    let mut s = Session::new();
    let c = h(&s.execute("circle", &json!({ "center": [0, 0], "radius": 2 })).unwrap());
    s.execute("block", &json!({ "name": "TAG", "base": [0, 0], "handles": [c.hex()], "keep": "delete" })).unwrap();

    // Configure non-default remembered options on normal MLEADER.
    s.script("mleader l o l p a y c b TAG i m 3 f 45 s 30 x\n").unwrap();
    s.cancel();

    let remembered_before = s.last_used.mleader;
    let remembered_block_before = s.last_used.mleader_block.clone();
    assert_eq!(remembered_before.order, crate::LeaderOrder::Landing);
    assert_eq!(remembered_before.leader_type, crate::LeaderType::Spline);
    assert!(remembered_before.landing);
    assert!(remembered_before.content);
    assert!(remembered_before.block);
    assert!(!remembered_before.block_center);
    assert_eq!(remembered_before.max_points, 3);
    assert_eq!(remembered_block_before, "TAG");

    // Run directrizzr with 4 points (exceeding MLEADER's remembered max_points = 3) and arbitrary angles
    // (ignoring MLEADER's 45/30 degree constraints, spline, landing-first order, landing dogleg, and block).
    s.script("directrizzr 0,0 7,2 13,9 21,4\n\n").unwrap();
    assert_eq!(s.doc().unwrap().model.len(), 1);
    let EntityKind::MLeader(m_dir) = &s.doc().unwrap().model.last().unwrap().kind else {
        panic!("expected MLeader from directrizzr");
    };
    assert!(m_dir.text.is_none());
    assert!(m_dir.block.is_none());
    assert!(!m_dir.spline);
    assert_eq!(m_dir.dogleg, 0.0);
    assert_eq!(
        m_dir.leader_paths(),
        vec![vec![Vec2::new(0.0, 0.0), Vec2::new(7.0, 2.0), Vec2::new(13.0, 9.0), Vec2::new(21.0, 4.0)]]
    );

    // Verify normal MLEADER's remembered options are completely untouched.
    assert_eq!(s.last_used.mleader, remembered_before);
    assert_eq!(s.last_used.mleader_block, remembered_block_before);

    // Starting normal MLEADER again still uses Landing first and remembered options.
    s.start("mleader").unwrap();
    assert!(s.prompt_text().contains("Specify leader landing location"));
    s.cancel();
}
