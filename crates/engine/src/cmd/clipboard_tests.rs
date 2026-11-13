use std::sync::Arc;

use cadcraft_doc::{Block, Entity, EntityKind, Insert, Line, MLeader, Space};
use cadcraft_geom::Vec3;
use serde_json::json;

use crate::Session;

fn add_line_block(s: &mut Session, name: &str, end_x: f64) {
    let d = s.doc_mut().unwrap();
    let mut block = Block::new(name);
    block.anonymous = true;
    let entity = Entity::new(d.new_handle(), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(end_x, 0.0, 0.0) }));
    block.entities.push(entity);
    d.blocks.insert(name.to_string(), Arc::new(block));
}

fn line_end_x(block: &Block) -> Option<f64> {
    block.entities.iter().find_map(|e| match &e.kind {
        EntityKind::Line(l) => Some(l.b.x),
        _ => None,
    })
}

#[test]
fn pasteclip_remaps_mleader_anonymous_block_collision() {
    let mut s = Session::new();
    add_line_block(&mut s, "*U1", 1.0);
    assert!(s.doc().unwrap().block("*U1").unwrap().anonymous);
    // The public MLEADER command rejects anonymous content blocks, but imported drawings can
    // contain this state. Construct that source entity directly, then exercise public COPYCLIP.
    let source_leader = s
        .doc_mut()
        .unwrap()
        .add(
            &Space::Model,
            Default::default(),
            EntityKind::MLeader(MLeader {
                leaders: vec![vec![Vec3::new(0.0, 0.0, 0.0)]],
                landing: Vec3::ZERO,
                dogleg: 0.0,
                text: None,
                style: "Standard".into(),
                arrow_size: 1.0,
                spline: false,
                block: Some(Insert {
                    block: "*U1".into(),
                    insert: Vec3::ZERO,
                    scale: Vec3::new(1.0, 1.0, 1.0),
                    rotation: 0.0,
                    attribs: Vec::new(),
                    cols: 1,
                    rows: 1,
                    col_spacing: 0.0,
                    row_spacing: 0.0,
                }),
            }),
        )
        .unwrap();
    let source_content = match &s.doc().unwrap().entity(source_leader).unwrap().kind {
        EntityKind::MLeader(m) => m.block.as_ref().unwrap().block.clone(),
        other => panic!("expected source MLeader, got {other:?}"),
    };
    assert_eq!(source_content, "*U1");

    s.execute("copyclip", &json!({ "handles": [source_leader.hex()] })).unwrap();
    {
        let clip = s.clipboard_source.as_ref().unwrap();
        let clip_block = clip.defs.block("*U1").expect("copyclip must include the referenced block");
        assert_eq!(line_end_x(clip_block), Some(1.0), "clipboard block must contain source geometry");
        assert!(
            s.clipboard.iter().any(|e| matches!(&e.kind, EntityKind::MLeader(m) if m.block.as_ref().is_some_and(|b| b.block == "*U1"))),
            "copyclip must retain the MLeader and its source block reference"
        );
    }

    s.new_drawing(false);
    add_line_block(&mut s, "*U1", 100.0);
    assert_eq!(line_end_x(s.doc().unwrap().block("*U1").unwrap()), Some(100.0));
    assert!(s.doc().unwrap().block("*U2").is_none(), "*U2 must be available for the source rename");

    let pasted = s.execute("pasteclip", &json!({ "at": [0, 0] })).unwrap();
    let handles = pasted["handles"].as_array().unwrap();
    let leaders: Vec<_> = handles
        .iter()
        .filter_map(|h| cadcraft_doc::Handle::parse_hex(h.as_str()?).and_then(|h| s.doc().ok()?.entity(h)))
        .filter(|e| matches!(e.kind, EntityKind::MLeader(_)))
        .collect();
    assert_eq!(leaders.len(), 1, "paste must create exactly one new MLeader");
    let leader = leaders[0];
    let referenced_name = match &leader.kind {
        EntityKind::MLeader(m) => m.block.as_ref().unwrap().block.clone(),
        _ => unreachable!(),
    };

    let d = s.doc().unwrap();
    let imported = d.blocks.values().find(|b| b.anonymous && line_end_x(b) == Some(1.0)).expect("source block must be imported");
    assert_eq!(imported.name, "*U2", "report the exact generated rename as additional evidence");
    let referenced = d.block(&referenced_name).expect("pasted MLeader reference must resolve");
    assert_eq!(
        line_end_x(referenced),
        Some(1.0),
        "pasted MLeader reference {referenced_name} resolved to block {}, not imported source block {}; content must match source geometry",
        referenced.name,
        imported.name,
    );
    assert_eq!(referenced.name, imported.name, "MLeader must reference the imported source block");

    s.undo().unwrap();
    assert!(s.doc().unwrap().block("*U2").is_none(), "undo must remove the imported source block");
    assert!(s.doc().unwrap().model.iter().all(|e| !matches!(e.kind, EntityKind::MLeader(_))), "undo must remove the pasted MLeader");
    assert_eq!(line_end_x(s.doc().unwrap().block("*U1").unwrap()), Some(100.0), "undo preserves the destination block");
    s.redo().unwrap();
    let redone = s.doc().unwrap();
    let redone_leader = redone.model.iter().find(|e| matches!(e.kind, EntityKind::MLeader(_))).expect("redo restores the pasted MLeader");
    let redone_ref = match &redone_leader.kind {
        EntityKind::MLeader(m) => m.block.as_ref().unwrap().block.as_str(),
        _ => unreachable!(),
    };
    assert_eq!(redone_ref, "*U2");
    assert_eq!(line_end_x(redone.block(redone_ref).unwrap()), Some(1.0));
}

#[test]
fn pasteclip_mleader_block_without_collision_keeps_reference_and_content() {
    let mut s = Session::new();
    add_line_block(&mut s, "*U1", 1.0);
    let source_leader = s
        .doc_mut()
        .unwrap()
        .add(
            &Space::Model,
            Default::default(),
            EntityKind::MLeader(MLeader {
                leaders: vec![vec![Vec3::ZERO]],
                landing: Vec3::ZERO,
                dogleg: 0.0,
                text: None,
                style: "Standard".into(),
                arrow_size: 1.0,
                spline: false,
                block: Some(Insert {
                    block: "*U1".into(),
                    insert: Vec3::ZERO,
                    scale: Vec3::new(1.0, 1.0, 1.0),
                    rotation: 0.0,
                    attribs: Vec::new(),
                    cols: 1,
                    rows: 1,
                    col_spacing: 0.0,
                    row_spacing: 0.0,
                }),
            }),
        )
        .unwrap();
    s.execute("copyclip", &json!({"handles": [source_leader.hex()]})).unwrap();
    s.new_drawing(false);
    let pasted = s.execute("pasteclip", &json!({"at": [0, 0]})).unwrap();
    let handle = cadcraft_doc::Handle::parse_hex(pasted["handles"][0].as_str().unwrap()).unwrap();
    let d = s.doc().unwrap();
    let EntityKind::MLeader(m) = &d.entity(handle).unwrap().kind else { panic!("expected pasted MLeader") };
    assert_eq!(m.block.as_ref().map(|b| b.block.as_str()), Some("*U1"));
    assert_eq!(line_end_x(d.block("*U1").unwrap()), Some(1.0));
}

#[test]
fn pasteclip_mleader_without_block_content_stays_without_content() {
    let mut s = Session::new();
    let r = s.execute("mleader", &json!({"points": [[0, 0], [5, 0]], "text": "Note"})).unwrap();
    let h = cadcraft_doc::Handle::parse_hex(r["handle"].as_str().unwrap()).unwrap();
    assert!(matches!(&s.doc().unwrap().entity(h).unwrap().kind, EntityKind::MLeader(m) if m.block.is_none()));
    s.execute("copyclip", &json!({"handles": [h.hex()]})).unwrap();
    s.new_drawing(false);
    let pasted = s.execute("pasteclip", &json!({"at": [0, 0]})).unwrap();
    let h = cadcraft_doc::Handle::parse_hex(pasted["handles"][0].as_str().unwrap()).unwrap();
    assert!(matches!(&s.doc().unwrap().entity(h).unwrap().kind, EntityKind::MLeader(m) if m.block.is_none()));
}

#[test]
fn pasteclip_remaps_mleader_nested_in_a_copied_block() {
    let mut s = Session::new();
    add_line_block(&mut s, "*U1", 1.0);
    let mut parent = Block::new("PARENT");
    parent.entities.push(Entity::new(
        s.doc_mut().unwrap().new_handle(),
        EntityKind::MLeader(MLeader {
            leaders: vec![vec![Vec3::ZERO]],
            landing: Vec3::ZERO,
            dogleg: 0.0,
            text: None,
            style: "Standard".into(),
            arrow_size: 1.0,
            spline: false,
            block: Some(Insert {
                block: "*U1".into(),
                insert: Vec3::ZERO,
                scale: Vec3::new(1.0, 1.0, 1.0),
                rotation: 0.0,
                attribs: Vec::new(),
                cols: 1,
                rows: 1,
                col_spacing: 0.0,
                row_spacing: 0.0,
            }),
        }),
    ));
    s.doc_mut().unwrap().blocks.insert("PARENT".into(), Arc::new(parent));
    let insert = s
        .doc_mut()
        .unwrap()
        .add(
            &Space::Model,
            Default::default(),
            EntityKind::Insert(Insert {
                block: "PARENT".into(),
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
    s.execute("copyclip", &json!({"handles": [insert.hex()]})).unwrap();
    assert!(s.clipboard_source.as_ref().unwrap().defs.block("*U1").is_some());
    s.new_drawing(false);
    add_line_block(&mut s, "*U1", 100.0);
    s.execute("pasteclip", &json!({"at": [0, 0]})).unwrap();
    let d = s.doc().unwrap();
    let copied_parent = d.block("PARENT").unwrap();
    let nested = copied_parent
        .entities
        .iter()
        .find_map(|e| match &e.kind {
            EntityKind::MLeader(m) => m.block.as_ref().map(|b| b.block.clone()),
            _ => None,
        })
        .expect("copied parent must retain nested MLeader block content");
    assert_eq!(nested, "*U2");
    assert_eq!(line_end_x(d.block(&nested).unwrap()), Some(1.0));
    assert_eq!(line_end_x(d.block("*U1").unwrap()), Some(100.0));
}

#[test]
fn copybase_acceptance_a_same_drawing_rect_corner_exact_offset() {
    let mut s = Session::new();
    s.cmdline("RECTANG 10,20 50,40").unwrap();
    let orig_h = s.doc().unwrap().model.iter().next().unwrap().handle;

    // COPYBASE with base at corner (10, 20, 0)
    s.execute("copybase", &json!({ "handles": [orig_h.hex()], "base": [10.0, 20.0, 0.0] })).unwrap();
    assert_eq!(s.clipboard_base, Vec3::new(10.0, 20.0, 0.0));

    // PASTECLIP at (100, 200, 0)
    let res = s.execute("pasteclip", &json!({ "at": [100.0, 200.0, 0.0] })).unwrap();
    let new_h_hex = res["handles"][0].as_str().unwrap();
    let new_h = cadcraft_doc::Handle::parse_hex(new_h_hex).unwrap();

    // Original must remain untouched at (10, 20) -> (50, 40)
    let doc = s.doc().unwrap();
    let orig_e = doc.entity(orig_h).unwrap();
    let EntityKind::LwPolyline(orig_poly) = &orig_e.kind else { panic!("expected polyline") };
    assert_eq!(orig_poly.vertices[0].p, cadcraft_geom::Vec2::new(10.0, 20.0));

    // Pasted entity must be shifted exactly by (90, 180, 0)
    let new_e = doc.entity(new_h).unwrap();
    let EntityKind::LwPolyline(new_poly) = &new_e.kind else { panic!("expected polyline") };
    assert_eq!(new_poly.vertices[0].p, cadcraft_geom::Vec2::new(100.0, 200.0));
    assert_eq!(new_poly.vertices[2].p, cadcraft_geom::Vec2::new(140.0, 220.0));
}

#[test]
fn copybase_acceptance_b_between_drawings_preserves_base() {
    let mut s = Session::empty();
    let d1 = s.new_drawing(false);
    s.cmdline("LINE 0,0 20,10").unwrap();
    let l_h = s.doc().unwrap().model.iter().next().unwrap().handle;

    s.execute("copybase", &json!({ "handles": [l_h.hex()], "base": [5.0, 2.5, 0.0] })).unwrap();
    assert_eq!(s.clipboard_base, Vec3::new(5.0, 2.5, 0.0));

    // Open second drawing
    let _d2 = s.new_drawing(false);
    assert_ne!(s.active, d1);
    assert_eq!(s.doc().unwrap().model.len(), 0);

    // Paste into second drawing at (50, 50, 0)
    let res = s.execute("pasteclip", &json!({ "at": [50.0, 50.0, 0.0] })).unwrap();
    let pasted_h = cadcraft_doc::Handle::parse_hex(res["handles"][0].as_str().unwrap()).unwrap();

    let doc = s.doc().unwrap();
    let e = doc.entity(pasted_h).unwrap();
    let EntityKind::Line(l) = &e.kind else { panic!("expected line") };
    // Displacement is (50, 50) - (5, 2.5) = (45, 47.5)
    // l.a was (0, 0) -> (45, 47.5)
    // l.b was (20, 10) -> (65, 57.5)
    assert_eq!(l.a, Vec3::new(45.0, 47.5, 0.0));
    assert_eq!(l.b, Vec3::new(65.0, 57.5, 0.0));
}

#[test]
fn copybase_acceptance_c_preselection_and_postselection() {
    let mut s = Session::new();
    s.cmdline("LINE 0,0 10,0").unwrap();
    s.cmdline("LINE 10,0 10,10").unwrap();
    let handles: Vec<_> = s.doc().unwrap().model.iter().map(|e| e.handle).collect();

    // 1. Pickfirst preselection workflow
    s.set_selection(vec![handles[0]]);
    s.start("copybase").unwrap();
    assert!(s.prompt_text().contains("Specify base point"));
    s.input(crate::Input::Point(cadcraft_geom::Vec2::new(0.0, 0.0))).unwrap();
    assert!(s.running.is_none(), "preselection ends prompt immediately on base point input");
    assert_eq!(s.clipboard.len(), 1);
    assert_eq!(s.clipboard[0].handle, handles[0]);

    // 2. Post-selection workflow (no preselection)
    s.set_selection(Vec::new());
    s.start("copybase").unwrap();
    assert!(s.prompt_text().contains("Specify base point"));
    s.input(crate::Input::Point(cadcraft_geom::Vec2::new(10.0, 0.0))).unwrap();
    assert!(s.prompt_text().contains("Select objects"));
    s.input(crate::Input::Pick(vec![handles[1]])).unwrap();
    s.input(crate::Input::Enter).unwrap();
    assert!(s.running.is_none());
    assert_eq!(s.clipboard.len(), 1);
    assert_eq!(s.clipboard[0].handle, handles[1]);
    assert_eq!(s.clipboard_base, Vec3::new(10.0, 0.0, 0.0));
}

#[test]
fn copybase_acceptance_d_cancel_and_empty_selection_safeguard() {
    let mut s = Session::new();
    s.cmdline("LINE 0,0 10,0").unwrap();
    let l_h = s.doc().unwrap().model.iter().next().unwrap().handle;
    s.execute("copybase", &json!({ "handles": [l_h.hex()], "base": [1.0, 2.0, 3.0] })).unwrap();
    assert_eq!(s.clipboard_base, Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(s.clipboard.len(), 1);

    // Cancel on base point prompt: clipboard remains untouched
    s.start("copybase").unwrap();
    s.input(crate::Input::Cancel).unwrap();
    assert!(s.running.is_none());
    assert_eq!(s.clipboard_base, Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(s.clipboard.len(), 1);

    // Empty selection on post-selection prompt: clipboard remains untouched
    s.start("copybase").unwrap();
    s.input(crate::Input::Point(cadcraft_geom::Vec2::new(99.0, 99.0))).unwrap();
    s.input(crate::Input::Enter).unwrap(); // empty selection
    assert!(s.running.is_none());
    assert_eq!(s.clipboard_base, Vec3::new(1.0, 2.0, 3.0), "clipboard base preserved on empty selection");
    assert_eq!(s.clipboard.len(), 1, "clipboard objects preserved on empty selection");
}

#[test]
fn copybase_acceptance_e_repeated_pasting_to_multiple_destinations() {
    let mut s = Session::new();
    s.cmdline("CIRCLE 0,0 5").unwrap();
    let c_h = s.doc().unwrap().model.iter().next().unwrap().handle;

    s.execute("copybase", &json!({ "handles": [c_h.hex()], "base": [0.0, 0.0, 0.0] })).unwrap();

    // Paste 1
    let r1 = s.execute("pasteclip", &json!({ "at": [10.0, 20.0, 0.0] })).unwrap();
    // Paste 2
    let r2 = s.execute("pasteclip", &json!({ "at": [30.0, 40.0, 0.0] })).unwrap();

    let h1 = cadcraft_doc::Handle::parse_hex(r1["handles"][0].as_str().unwrap()).unwrap();
    let h2 = cadcraft_doc::Handle::parse_hex(r2["handles"][0].as_str().unwrap()).unwrap();

    let doc = s.doc().unwrap();
    let EntityKind::Circle(c1) = &doc.entity(h1).unwrap().kind else { panic!() };
    let EntityKind::Circle(c2) = &doc.entity(h2).unwrap().kind else { panic!() };

    assert_eq!(c1.center, Vec3::new(10.0, 20.0, 0.0));
    assert_eq!(c2.center, Vec3::new(30.0, 40.0, 0.0));
    assert_eq!(c1.radius, 5.0);
    assert_eq!(c2.radius, 5.0);
}

#[test]
fn copybase_acceptance_f_spatial_nonzero_z_and_inclined_ucs() {
    let mut s = Session::new();
    // 3DPOLY line in space
    s.execute("3dpoly", &json!({ "points": [[0.0, 0.0, 10.0], [10.0, 20.0, 30.0]] })).unwrap();
    let h3d = s.doc().unwrap().model.iter().next().unwrap().handle;
    let orig = s.doc().unwrap().entity(h3d).unwrap();
    let EntityKind::Polyline3d(orig_p) = &orig.kind else { panic!("expected polyline3d") };
    assert_eq!(orig_p.points[0], Vec3::new(0.0, 0.0, 10.0));
    assert_eq!(orig_p.points[1], Vec3::new(10.0, 20.0, 30.0));

    // COPYBASE with 3D base point with non-zero Z: (0, 0, 10)
    s.execute("copybase", &json!({ "handles": [h3d.hex()], "base": [0.0, 0.0, 10.0] })).unwrap();
    assert_eq!(s.clipboard_base, Vec3::new(0.0, 0.0, 10.0));

    // Set an inclined UCS: origin at (100, 100, 50), tilted
    s.cmdline("ucs 3 100,100,50 101,100,50 100,100,51").unwrap();

    // In this UCS, point (0, 0, 0) corresponds to WCS (100, 100, 50)
    // Run interactive pasteclip providing Point3 in WCS
    s.start("pasteclip").unwrap();
    s.input(crate::Input::Point3(Vec3::new(100.0, 100.0, 50.0))).unwrap();

    let doc = s.doc().unwrap();
    let pasted_e = doc.model.iter().last().unwrap();
    let EntityKind::Polyline3d(p) = &pasted_e.kind else { panic!("expected polyline3d") };

    // Delta is (100, 100, 50) - (0, 0, 10) = (100, 100, 40)
    // Original pt0: (0, 0, 10) -> (100, 100, 50)
    // Original pt1: (10, 20, 30) -> (110, 120, 70)
    assert_eq!(p.points[0], Vec3::new(100.0, 100.0, 50.0));
    assert_eq!(p.points[1], Vec3::new(110.0, 120.0, 70.0));
}

#[test]
fn copybase_acceptance_h_solid3d_retains_solid3d_recipe_and_volume() {
    let mut s = Session::new();
    s.cmdline("RECTANG 0,0 40,30").unwrap();
    s.cmdline("EXTRUDE ALL  5").unwrap();

    let solid_h = s.doc().unwrap().model.iter().last().unwrap().handle;
    let orig_e = s.doc().unwrap().entity(solid_h).unwrap();
    let EntityKind::Solid3d(orig_solid) = &orig_e.kind else { panic!("expected solid3d") };
    let orig_eval = cadcraft_brep::BrepSolid::evaluate_solid(orig_solid).unwrap();
    let orig_vol = orig_eval.volume().unwrap();
    assert!((orig_vol - 6000.0).abs() < 1e-2);

    // COPYBASE with base at corner (0, 0, 0)
    s.execute("copybase", &json!({ "handles": [solid_h.hex()], "base": [0.0, 0.0, 0.0] })).unwrap();
    assert_eq!(s.clipboard_base, Vec3::new(0.0, 0.0, 0.0));

    // PASTECLIP shifted in X, Y, Z: at (100, 50, 25)
    let res = s.execute("pasteclip", &json!({ "at": [100.0, 50.0, 25.0] })).unwrap();
    let pasted_h = cadcraft_doc::Handle::parse_hex(res["handles"][0].as_str().unwrap()).unwrap();

    let doc = s.doc().unwrap();
    let pasted_e = doc.entity(pasted_h).unwrap();
    let EntityKind::Solid3d(pasted_solid) = &pasted_e.kind else {
        panic!("pasted solid must remain Solid3d and not degrade to loose faces");
    };

    // Evaluated B-Rep volume must be identical
    let pasted_eval = cadcraft_brep::BrepSolid::evaluate_solid(pasted_solid).unwrap();
    let pasted_vol = pasted_eval.volume().unwrap();
    assert!((pasted_vol - 6000.0).abs() < 1e-2);

    // Bounding box must be exactly translated
    assert_eq!(pasted_solid.bbox.min, Vec3::new(100.0, 50.0, 25.0));
    assert_eq!(pasted_solid.bbox.max, Vec3::new(140.0, 80.0, 30.0));
}
