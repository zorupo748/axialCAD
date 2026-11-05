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
