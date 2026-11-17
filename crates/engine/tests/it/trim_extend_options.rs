//! TRIM and EXTEND options (#397): Quick and Standard modes (TRIMEXTENDMODE), cuTting/Boundary
//! edges, Fence, Crossing, Edge (EDGEMODE), Project (PROJMODE), eRase and Undo, and the JSON
//! forms' `fence`, `crossing`, `edgeMode` and `mode`.

use cadcraft_engine::Session;
use cadcraft_engine::doc::EntityKind;
use serde_json::json;

type Seg = ((f64, f64), (f64, f64));

fn r(v: f64) -> f64 {
    (v * 1e6).round() / 1e6 + 0.0
}

/// Every line with its ends in a fixed order (rounded), sorted.
fn lines(s: &Session) -> Vec<Seg> {
    let mut v: Vec<Seg> = s
        .doc()
        .unwrap()
        .model
        .iter()
        .filter_map(|e| if let EntityKind::Line(l) = &e.kind { Some(((r(l.a.x), r(l.a.y)), (r(l.b.x), r(l.b.y)))) } else { None })
        .map(|(a, b)| if (a.0, a.1) <= (b.0, b.1) { (a, b) } else { (b, a) })
        .collect();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v
}

fn session(ls: &[&str]) -> Session {
    let mut s = Session::new();
    for l in ls {
        s.cmdline(&format!("LINE {l}")).unwrap();
        s.cmdline("").unwrap();
    }
    s
}

/// Two vertical edges at x = 0 and x = 10, crossed by horizontal lines at y = 0 and y = 2.
fn ladder() -> Session {
    session(&["0,-5 0,5", "10,-5 10,5", "-5,0 15,0", "-5,2 15,2"])
}

const EDGES: [Seg; 2] = [((0.0, -5.0), (0.0, 5.0)), ((10.0, -5.0), (10.0, 5.0))];

fn with_edges(more: &[Seg]) -> Vec<Seg> {
    let mut v: Vec<Seg> = EDGES.iter().chain(more).copied().collect();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v
}

fn header(s: &Session, name: &str) -> i64 {
    s.doc().unwrap().header.i64(name, -1)
}

#[test]
fn quick_mode_trims_to_all_objects_and_erases_an_object_with_no_edge() {
    let mut s = ladder();
    s.cmdline("LINE 20,20 25,20").unwrap();
    s.cmdline("").unwrap();
    s.script("TRIM 5,0 22,20\n\n").unwrap();
    assert!(s.running.is_none(), "{}", s.prompt_text());
    assert_eq!(lines(&s), with_edges(&[((-5.0, 0.0), (0.0, 0.0)), ((-5.0, 2.0), (15.0, 2.0)), ((10.0, 0.0), (15.0, 0.0))]));
}

#[test]
fn standard_mode_uses_the_selected_cutting_edges() {
    let mut s = ladder();
    // Mode Standard, cutting edge: the line at x = 0 only.
    s.script("TRIM O S 0,4\n\n5,0\n\n").unwrap();
    assert!(s.running.is_none(), "{}", s.prompt_text());
    assert_eq!(header(&s, "TRIMEXTENDMODE"), 0);
    assert_eq!(lines(&s), with_edges(&[((-5.0, 0.0), (0.0, 0.0)), ((-5.0, 2.0), (15.0, 2.0))]));
    // Standard mode is remembered: the next TRIM starts by asking for the edges; Enter selects
    // all, and an object no edge crosses is kept.
    s.cmdline("LINE 20,20 25,20").unwrap();
    s.cmdline("").unwrap();
    s.script("TRIM\n\n22,20\n\n").unwrap();
    assert!(lines(&s).contains(&((20.0, 20.0), (25.0, 20.0))));
}

#[test]
fn cutting_edges_option_in_quick_mode() {
    let mut s = ladder();
    s.script("TRIM T 10,4\n\n5,2\n\n").unwrap();
    assert!(s.running.is_none(), "{}", s.prompt_text());
    assert_eq!(header(&s, "TRIMEXTENDMODE"), -1, "cuTting edges leaves the mode alone");
    assert_eq!(lines(&s), with_edges(&[((-5.0, 0.0), (15.0, 0.0)), ((10.0, 2.0), (15.0, 2.0))]));
}

#[test]
fn fence_trims_every_crossed_object() {
    let mut s = ladder();
    s.script("TRIM F 5,-1 5,3\n\n\n").unwrap();
    assert!(s.running.is_none(), "{}", s.prompt_text());
    assert_eq!(
        lines(&s),
        with_edges(&[((-5.0, 0.0), (0.0, 0.0)), ((-5.0, 2.0), (0.0, 2.0)), ((10.0, 0.0), (15.0, 0.0)), ((10.0, 2.0), (15.0, 2.0))])
    );
}

#[test]
fn fence_crossing_one_line_twice_trims_both_parts() {
    let mut s = ladder();
    // A fence round the top crosses y = 2 left of x = 0 and right of x = 10.
    s.script("TRIM F -3,1 -3,6 13,6 13,1\n\n\n").unwrap();
    assert_eq!(lines(&s), with_edges(&[((-5.0, 0.0), (15.0, 0.0)), ((0.0, 2.0), (10.0, 2.0))]));
}

#[test]
fn crossing_window_trims_what_it_touches() {
    let mut s = ladder();
    s.script("TRIM C 4,-1 6,1\n\n").unwrap();
    assert!(s.running.is_none(), "{}", s.prompt_text());
    assert_eq!(lines(&s), with_edges(&[((-5.0, 0.0), (0.0, 0.0)), ((-5.0, 2.0), (15.0, 2.0)), ((10.0, 0.0), (15.0, 0.0))]));
    // A pick in empty space opens a crossing window too.
    let mut s = ladder();
    s.script("TRIM 4,-4 6,1\n\n").unwrap();
    assert_eq!(lines(&s), with_edges(&[((-5.0, 0.0), (0.0, 0.0)), ((-5.0, 2.0), (15.0, 2.0)), ((10.0, 0.0), (15.0, 0.0))]));
}

#[test]
fn edge_extend_trims_to_an_implied_edge() {
    // A short cutting edge at x = 5 that doesn't reach y = 0.
    let ls = ["5,3 5,4", "-5,0 15,0"];
    let mut s = session(&ls);
    s.script("TRIM O S 5,3.5\n\n8,0\n\n").unwrap();
    assert_eq!(lines(&s), vec![((-5.0, 0.0), (15.0, 0.0)), ((5.0, 3.0), (5.0, 4.0))], "no implied edge: nothing to trim");
    let mut s = session(&ls);
    s.script("TRIM O S 5,3.5\n\nE E 8,0\n\n").unwrap();
    assert!(s.running.is_none(), "{}", s.prompt_text());
    assert_eq!(header(&s, "EDGEMODE"), 1);
    assert_eq!(lines(&s), vec![((-5.0, 0.0), (5.0, 0.0)), ((5.0, 3.0), (5.0, 4.0))]);
    // EXTEND to an implied edge.
    let mut s = session(&["5,3 5,4", "0,0 2,0"]);
    s.script("EXTEND O S 5,3.5\n\nE E 1.8,0\n\n").unwrap();
    assert_eq!(lines(&s), vec![((0.0, 0.0), (5.0, 0.0)), ((5.0, 3.0), (5.0, 4.0))]);
}

#[test]
fn erase_undo_and_project() {
    let mut s = ladder();
    s.script("TRIM R 5,2\n\nP V\n\n").unwrap();
    assert!(s.running.is_none(), "{}", s.prompt_text());
    assert_eq!(lines(&s), with_edges(&[((-5.0, 0.0), (15.0, 0.0))]));
    assert_eq!(header(&s, "PROJMODE"), 2);
    // Undo takes back the last trim only, then the erase.
    let mut s = ladder();
    s.script("TRIM 5,0 5,2 U\n\n").unwrap();
    assert_eq!(lines(&s), with_edges(&[((-5.0, 0.0), (0.0, 0.0)), ((-5.0, 2.0), (15.0, 2.0)), ((10.0, 0.0), (15.0, 0.0))]));
    s.script("TRIM R 10,4\n\nU U\n\n").unwrap();
    assert_eq!(lines(&s).len(), 5);
    // The whole TRIM is one undo step.
    s.script("TRIM 5,2 F 5,-1 5,1\n\n\n").unwrap();
    s.undo().unwrap();
    assert_eq!(lines(&s).len(), 5);
}

#[test]
fn extend_fence_and_crossing() {
    let mut s = session(&["10,-5 10,5", "0,0 4,0", "0,2 4,2"]);
    s.script("EXTEND F 3,-1 3,3\n\n\n").unwrap();
    assert!(s.running.is_none(), "{}", s.prompt_text());
    assert_eq!(lines(&s), vec![((0.0, 0.0), (10.0, 0.0)), ((0.0, 2.0), (10.0, 2.0)), ((10.0, -5.0), (10.0, 5.0))]);
    let mut s = session(&["10,-5 10,5", "0,0 4,0", "0,2 4,2"]);
    s.script("EXTEND C 3,-1 5,1\n\n").unwrap();
    assert_eq!(lines(&s), vec![((0.0, 0.0), (10.0, 0.0)), ((0.0, 2.0), (4.0, 2.0)), ((10.0, -5.0), (10.0, 5.0))]);
}

#[test]
fn json_forms_take_fence_crossing_edge_mode_and_mode() {
    let mut s = ladder();
    let r = s.execute("trim", &json!({ "fence": [[5, -1], [5, 3]] })).unwrap();
    assert_eq!(r["trimmed"], 2);
    assert_eq!(lines(&s).len(), 6);

    let mut s = ladder();
    let r = s.execute("trim", &json!({ "crossing": [[4, -1], [6, 1]] })).unwrap();
    assert_eq!(r["trimmed"], 1);

    let mut s = session(&["5,3 5,4", "-5,0 15,0"]);
    let h = s.doc().unwrap().model.last().unwrap().handle.hex();
    assert!(s.execute("trim", &json!({ "handle": h, "pick": [8, 0] })).is_err());
    s.execute("trim", &json!({ "handle": h, "pick": [8, 0], "edgeMode": "extend" })).unwrap();
    assert_eq!(lines(&s), vec![((-5.0, 0.0), (5.0, 0.0)), ((5.0, 3.0), (5.0, 4.0))]);

    let mut s = session(&["20,20 25,20"]);
    let h = s.doc().unwrap().model.last().unwrap().handle.hex();
    let r = s.execute("trim", &json!({ "handle": h, "pick": [22, 20], "mode": "quick" })).unwrap();
    assert_eq!(r["handles"], json!([]));
    assert!(lines(&s).is_empty());

    let mut s = ladder();
    assert!(s.execute("trim", &json!({ "fence": [[5, -1]] })).is_err());
    assert!(s.execute("trim", &json!({ "crossing": [[5, -1], [1, 1], [2, 2]] })).is_err());
    assert!(s.execute("trim", &json!({ "fence": [[5, -1], [5, 3]], "edgeMode": "sideways" })).is_err());
    assert!(s.execute("trim", &json!({ "fence": [[5, -1], [5, 3]], "mode": 3 })).is_err());
    assert_eq!(lines(&s).len(), 4);

    let mut s = session(&["10,-5 10,5", "0,0 4,0"]);
    let r = s.execute("extend", &json!({ "fence": [[3, -1], [3, 1]] })).unwrap();
    assert_eq!(r["extended"], 1);
    assert_eq!(lines(&s), vec![((0.0, 0.0), (10.0, 0.0)), ((10.0, -5.0), (10.0, 5.0))]);
}

#[test]
fn trim_preview_circle_indicates_only_the_targeted_half() {
    let mut s = Session::new();
    // Circle at origin with radius 10, crossed by horizontal line through center
    s.cmdline("CIRCLE 0,0 10").unwrap();
    s.cmdline("LINE -20,0 20,0").unwrap();
    s.cmdline("").unwrap();

    let initial_count = s.doc().unwrap().model.len();
    assert_eq!(initial_count, 2);

    // Start interactive TRIM
    s.cmdline("TRIM").unwrap();

    // Verify pickbox is active but generic whole-entity hover highlight is suppressed
    assert!(s.picking_objects(), "cursor should show pickbox");
    assert!(s.current_prompt().unwrap().picks_objects(), "trim pick prompt should specify object picking mode");
    assert!(!s.previews_selection(), "generic whole-entity highlight must be suppressed during trim pick");

    // Hover over top half: cursor at (0, 10)
    let p_top = s.preview(cadcraft_geom::Vec2::new(0.0, 10.0));
    assert_eq!(p_top.len(), 1, "preview should produce exactly one piece to remove");
    assert_eq!(p_top[0].common.linetype, "DASHED", "removal preview must use DASHED linetype");
    if let EntityKind::Arc(a) = &p_top[0].kind {
        assert_eq!(a.center.x, 0.0);
        assert_eq!(a.center.y, 0.0);
        assert!((a.radius - 10.0).abs() < 1e-6);
        assert!((a.start - 0.0).abs() < 1e-4);
        assert!((a.end - std::f64::consts::PI).abs() < 1e-4);
    } else {
        panic!("expected Arc for top half removal preview, got {:?}", p_top[0].kind);
    }

    // Document must not be modified during hover
    assert_eq!(s.doc().unwrap().model.len(), initial_count);

    // Hover over bottom half: cursor at (0, -10)
    let p_bot = s.preview(cadcraft_geom::Vec2::new(0.0, -10.0));
    assert_eq!(p_bot.len(), 1, "preview should produce exactly one piece to remove");
    if let EntityKind::Arc(a) = &p_bot[0].kind {
        assert_eq!(a.center.x, 0.0);
        assert_eq!(a.center.y, 0.0);
        assert!((a.radius - 10.0).abs() < 1e-6);
        assert!((a.start - std::f64::consts::PI).abs() < 1e-4);
        assert!((a.end - 0.0).abs() < 1e-4);
        assert!((cadcraft_geom::ccw_sweep(a.start, a.end) - std::f64::consts::PI).abs() < 1e-4);
    } else {
        panic!("expected Arc for bottom half removal preview, got {:?}", p_bot[0].kind);
    }

    // Still unmodified
    assert_eq!(s.doc().unwrap().model.len(), initial_count);

    // Now click on top half to confirm trim
    s.cmdline("0,10").unwrap();

    // The circle is replaced by the remaining half (the bottom half: arc from PI to 0.0)
    let arcs: Vec<_> = s.doc().unwrap().model.iter().filter_map(|e| if let EntityKind::Arc(a) = &e.kind { Some(a) } else { None }).collect();
    assert_eq!(arcs.len(), 1, "one arc remains after trimming top half");
    assert!((arcs[0].start - std::f64::consts::PI).abs() < 1e-4);
    assert!((arcs[0].end - 0.0).abs() < 1e-4);
    assert!((cadcraft_geom::ccw_sweep(arcs[0].start, arcs[0].end) - std::f64::consts::PI).abs() < 1e-4);
}

#[test]
fn trim_preview_line_indicates_segment_to_remove() {
    let mut s = Session::new();
    // Line from (-10, 0) to (10, 0), cut by vertical line from (0, -5) to (0, 5)
    s.cmdline("LINE -10,0 10,0").unwrap();
    s.cmdline("").unwrap();
    s.cmdline("LINE 0,-5 0,5").unwrap();
    s.cmdline("").unwrap();

    s.cmdline("TRIM").unwrap();

    // Hover on right segment (x > 0)
    let p_right = s.preview(cadcraft_geom::Vec2::new(5.0, 0.0));
    assert_eq!(p_right.len(), 1);
    if let EntityKind::Line(l) = &p_right[0].kind {
        let (min_x, max_x) = if l.a.x < l.b.x { (l.a.x, l.b.x) } else { (l.b.x, l.a.x) };
        assert!((min_x - 0.0).abs() < 1e-4);
        assert!((max_x - 10.0).abs() < 1e-4);
    } else {
        panic!("expected Line removal preview");
    }

    // Hover on left segment (x < 0)
    let p_left = s.preview(cadcraft_geom::Vec2::new(-5.0, 0.0));
    assert_eq!(p_left.len(), 1);
    if let EntityKind::Line(l) = &p_left[0].kind {
        let (min_x, max_x) = if l.a.x < l.b.x { (l.a.x, l.b.x) } else { (l.b.x, l.a.x) };
        assert!((min_x - (-10.0)).abs() < 1e-4);
        assert!((max_x - 0.0).abs() < 1e-4);
    } else {
        panic!("expected Line removal preview");
    }
}

#[test]
fn trim_preview_uncut_object_shows_no_fictitious_removal() {
    let mut s = Session::new();
    s.cmdline("CIRCLE 100,100 5").unwrap();

    s.cmdline("TRIM").unwrap();

    // Quick mode (default): uncut object shows full-erase preview (fallback)
    let p = s.preview(cadcraft_geom::Vec2::new(100.0, 105.0));
    assert_eq!(p.len(), 1, "Quick mode: uncut object shows full-erase preview");
    assert!(matches!(p[0].kind, EntityKind::Circle(_)));
}

#[test]
fn trim_preview_successive_cuts_on_same_object() {
    let mut s = Session::new();
    // Circle at origin with radius 5, crossed by two vertical lines at x = -3 and x = 3
    s.cmdline("CIRCLE 0,0 5").unwrap();
    s.cmdline("LINE -3,-10 -3,10").unwrap();
    s.cmdline("").unwrap();
    s.cmdline("LINE 3,-10 3,10").unwrap();
    s.cmdline("").unwrap();

    // Start interactive TRIM
    s.cmdline("TRIM").unwrap();

    // 1. Preview right cap (5, 0)
    let p1 = s.preview(cadcraft_geom::Vec2::new(5.0, 0.0));
    println!("p1 len: {}", p1.len());
    assert_eq!(p1.len(), 1, "preview 1 should have 1 piece");

    // 2. Click to trim right cap
    s.cmdline("5,0").unwrap();

    let doc1 = s.doc().unwrap();
    println!("Entities after trim 1: {}", doc1.model.len());
    for (i, e) in doc1.model.iter().enumerate() {
        println!("  e[{}]: handle={:?}, kind={:?}", i, e.handle, e.kind);
    }

    // 3. Hover over left cap (-5, 0)
    let p2_left = s.preview(cadcraft_geom::Vec2::new(-5.0, 0.0));
    println!("p2_left len: {}", p2_left.len());
    for (i, e) in p2_left.iter().enumerate() {
        println!("  p2_left[{}]: {:?}", i, e.kind);
    }

    // Hover over top segment (0, 5)
    let p2_top = s.preview(cadcraft_geom::Vec2::new(0.0, 5.0));
    println!("p2_top len: {}", p2_top.len());
    for (i, e) in p2_top.iter().enumerate() {
        println!("  p2_top[{}]: {:?}", i, e.kind);
    }

    // 4. Click to trim top segment (0, 5)
    s.cmdline("0,5").unwrap();

    let doc2 = s.doc().unwrap();
    println!("Entities after trim 2: {}", doc2.model.len());
    for (i, e) in doc2.model.iter().enumerate() {
        println!("  e[{}]: handle={:?}, kind={:?}", i, e.handle, e.kind);
    }
}

#[test]
fn trim_preview_successive_cuts_on_line() {
    let mut s = Session::new();
    s.cmdline("LINE 0,0 100,0").unwrap();
    s.cmdline("").unwrap();
    s.cmdline("LINE 20,-10 20,10").unwrap();
    s.cmdline("").unwrap();
    s.cmdline("LINE 40,-10 40,10").unwrap();
    s.cmdline("").unwrap();
    s.cmdline("LINE 60,-10 60,10").unwrap();
    s.cmdline("").unwrap();

    s.cmdline("TRIM").unwrap();

    // 1. Preview segment 0..20 (hover at 10, 0)
    let p1 = s.preview(cadcraft_geom::Vec2::new(10.0, 0.0));
    assert_eq!(p1.len(), 1);

    // 2. Click to trim 0..20
    s.cmdline("10,0").unwrap();

    // 3. Hover at 30, 0 (segment 20..40)
    let p2 = s.preview(cadcraft_geom::Vec2::new(30.0, 0.0));
    assert_eq!(p2.len(), 1);
}

#[test]
fn trim_sequential_successive_cuts_circle_to_arc() {
    let mut s = Session::new();
    // Circle at origin with radius 10, cut by two horizontal lines at y = -5 and y = 5
    s.cmdline("CIRCLE 0,0 10").unwrap();
    s.cmdline("LINE -20,-5 20,-5").unwrap();
    s.cmdline("").unwrap();
    s.cmdline("LINE -20,5 20,5").unwrap();
    s.cmdline("").unwrap();

    let initial_circle_handle = s.doc().unwrap().model.iter().find(|e| matches!(e.kind, EntityKind::Circle(_))).unwrap().handle;

    s.cmdline("TRIM").unwrap();

    // 1. First preview: top arc at (0, 10)
    let p1 = s.preview(cadcraft_geom::Vec2::new(0.0, 10.0));
    assert_eq!(p1.len(), 1, "first preview must show exactly one piece to remove");
    assert_eq!(p1[0].common.linetype, "DASHED", "preview must use DASHED linetype");
    let (p1_start, p1_end) = match &p1[0].kind {
        EntityKind::Arc(a) => (a.start, a.end),
        other => panic!("expected Arc removal preview, got {:?}", other),
    };
    // Expected top arc from ~30° (0.5236 rad) to ~150° (2.6180 rad)
    assert!((p1_start - (30.0_f64).to_radians()).abs() < 1e-3);
    assert!((p1_end - (150.0_f64).to_radians()).abs() < 1e-3);

    // 2. Confirm first trim: removes the top arc
    s.cmdline("0,10").unwrap();

    // Dynamically locate the resulting entity from the document, without assuming handles
    let doc1 = s.doc().unwrap();
    assert!(doc1.entity(initial_circle_handle).is_none(), "original circle handle must no longer exist");
    let arc1 = doc1.model.iter().find(|e| matches!(e.kind, EntityKind::Arc(_))).expect("new Arc entity must exist");
    let arc1_handle = arc1.handle;
    assert_ne!(arc1_handle, initial_circle_handle);

    // 3. Second preview: on the remaining geometry, hover at right cap (10, 0)
    let p2 = s.preview(cadcraft_geom::Vec2::new(10.0, 0.0));
    assert_eq!(p2.len(), 1, "second preview on remaining geometry must indicate the right cap to remove");
    let (p2_start, p2_end) = match &p2[0].kind {
        EntityKind::Arc(a) => (a.start, a.end),
        other => panic!("expected Arc removal preview for right cap, got {:?}", other),
    };
    // Right cap runs from ~330° (-30° = 5.7596 rad) to ~30° (0.5236 rad)
    assert!((p2_start - (330.0_f64).to_radians()).abs() < 1e-3);
    assert!((p2_end - (30.0_f64).to_radians()).abs() < 1e-3);

    // Also verify control object: hovering on line y=5 right end (15, 5) shows valid preview
    let p_line = s.preview(cadcraft_geom::Vec2::new(15.0, 5.0));
    assert_eq!(p_line.len(), 1, "control object (unmodified line) must still produce trim preview");

    // 4. Confirm second trim at (10, 0)
    s.cmdline("10,0").unwrap();

    // Dynamically locate the resulting arc after second trim
    let doc2 = s.doc().unwrap();
    assert!(doc2.entity(arc1_handle).is_none(), "first arc handle was replaced");
    let arc2 = doc2.model.iter().find(|e| matches!(e.kind, EntityKind::Arc(_))).expect("second replacement Arc must exist");
    let arc2_handle = arc2.handle;
    if let EntityKind::Arc(a) = &arc2.kind {
        // Remaining arc now runs CCW from 150° (2.618 rad) through bottom to 330° (5.7596 rad)
        assert!((a.start - (150.0_f64).to_radians()).abs() < 1e-3);
        assert!((a.end - (330.0_f64).to_radians()).abs() < 1e-3);
    }

    // 5. Third preview: on the remaining geometry, hover at bottom arc (0, -10)
    let p3 = s.preview(cadcraft_geom::Vec2::new(0.0, -10.0));
    assert_eq!(p3.len(), 1, "third preview on remaining geometry must indicate bottom arc to remove");
    if let EntityKind::Arc(a) = &p3[0].kind {
        // Bottom arc runs from 210° to 330°
        assert!((a.start - (210.0_f64).to_radians()).abs() < 1e-3);
        assert!((a.end - (330.0_f64).to_radians()).abs() < 1e-3);
    }

    // 6. Confirm third trim at (0, -10)
    s.cmdline("0,-10").unwrap();

    // Remaining piece is left cap between 150° and 210°.
    // It has NO remaining cutting edge crossings in its interior.
    let doc3 = s.doc().unwrap();
    let arc3 = doc3.model.iter().find(|e| matches!(e.kind, EntityKind::Arc(_))).expect("third replacement Arc must exist");
    assert_ne!(arc3.handle, arc2_handle);

    // Quick mode fallback: remaining uncuttable piece shows full-erase preview
    let p_none = s.preview(cadcraft_geom::Vec2::new(-10.0, 0.0));
    assert_eq!(p_none.len(), 1, "Quick mode: piece with no interior cuts shows full-erase preview");
    assert!(matches!(p_none[0].kind, EntityKind::Arc(_)));
}

#[test]
fn trim_sequential_successive_cuts_standard_mode() {
    let mut s = Session::new();
    s.cmdline("CIRCLE 0,0 10").unwrap();
    s.cmdline("LINE -20,-5 20,-5").unwrap();
    s.cmdline("").unwrap();
    s.cmdline("LINE -20,5 20,5").unwrap();
    s.cmdline("").unwrap();

    // Standard mode TRIM (TRIMEXTENDMODE = 0)
    s.doc_mut().unwrap().header.set_i64("TRIMEXTENDMODE", 0);

    s.cmdline("TRIM").unwrap();
    // Select all objects as cutting edges
    s.cmdline("ALL").unwrap();
    s.cmdline("").unwrap();

    // First preview
    let p1 = s.preview(cadcraft_geom::Vec2::new(0.0, 10.0));
    assert_eq!(p1.len(), 1);

    // Confirm first cut
    s.cmdline("0,10").unwrap();

    // Second preview on remaining geometry: must succeed even when cutting edges are tracked explicitly
    let p2 = s.preview(cadcraft_geom::Vec2::new(10.0, 0.0));
    assert_eq!(p2.len(), 1, "Standard mode with explicit cutting edges must retain valid preview on remaining geometry");

    // Confirm second cut
    s.cmdline("10,0").unwrap();

    // Third preview on remaining geometry
    let p3 = s.preview(cadcraft_geom::Vec2::new(0.0, -10.0));
    assert_eq!(p3.len(), 1);
}

#[test]
fn crossing_window_trims_all_interior_intervals_in_one_pass() {
    // H: horizontal line 0,0 to 100,0
    // C1: vertical at x=25 (-10 to 10)
    // C2: vertical at x=50 (-10 to 10)
    // C3: vertical at x=75 (-10 to 10)
    // Crossing window: a=(70,-10) to b=(30,10)  (right->left = crossing)
    // Quick mode: all objects are cutting edges
    let mut s = Session::new();
    s.cmdline("LINE 0,0 100,0").unwrap();
    s.cmdline("").unwrap();
    s.cmdline("LINE 25,-10 25,10").unwrap();
    s.cmdline("").unwrap();
    s.cmdline("LINE 50,-10 50,10").unwrap();
    s.cmdline("").unwrap();
    s.cmdline("LINE 75,-10 75,10").unwrap();
    s.cmdline("").unwrap();

    // Quick mode explicitly
    s.doc_mut().unwrap().header.set_i64("TRIMEXTENDMODE", 1);

    let result = s.execute("trim", &json!({
        "crossing": [[70.0, -10.0], [30.0, 10.0]],
        "mode": "quick"
    })).unwrap();

    assert_eq!(result["trimmed"], 2, "expected 2 entities trimmed in one pass");

    // Verify exact geometry using the lines() helper (rounded to 1e-6, sorted lexicographically)
    let expected = vec![
        // H pieces: 0-25 and 75-100 (both kept)
        ((0.0, 0.0), (25.0, 0.0)),
        // C1 intact at x=25
        ((25.0, -10.0), (25.0, 10.0)),
        // C3 intact at x=75
        ((75.0, -10.0), (75.0, 10.0)),
        // H pieces: 75-100 (kept)
        ((75.0, 0.0), (100.0, 0.0)),
        // C2 eliminated (selected by crossing window, trimmed completely in Quick)
    ];
    assert_eq!(lines(&s), expected, "geometry after one crossing pass in Quick mode");
}

#[test]
fn crossing_window_trims_multiple_interior_intervals_additional_cutters() {
    let mut s = Session::new();
    s.cmdline("LINE 0,0 120,0").unwrap();
    s.cmdline("").unwrap();
    s.cmdline("LINE 20,-10 20,10").unwrap();
    s.cmdline("").unwrap();
    s.cmdline("LINE 40,-10 40,10").unwrap();
    s.cmdline("").unwrap();
    s.cmdline("LINE 60,-10 60,10").unwrap();
    s.cmdline("").unwrap();
    s.cmdline("LINE 80,-10 80,10").unwrap();
    s.cmdline("").unwrap();
    s.cmdline("LINE 100,-10 100,10").unwrap();
    s.cmdline("").unwrap();

    s.doc_mut().unwrap().header.set_i64("TRIMEXTENDMODE", 1);

    let result = s.execute("trim", &json!({
        "crossing": [[90.0, -10.0], [30.0, 10.0]],
        "mode": "quick"
    })).unwrap();

    // H trimmed (1) + C1, C2, C3 eliminated (3) = 4
    assert_eq!(result["trimmed"], 4, "expected 4 entities trimmed/erased in one pass");

    let expected = vec![
        ((0.0, 0.0), (20.0, 0.0)),
        ((20.0, -10.0), (20.0, 10.0)),
        ((100.0, -10.0), (100.0, 10.0)),
        ((100.0, 0.0), (120.0, 0.0)),
    ];
    assert_eq!(lines(&s), expected, "geometry after multi-interval crossing pass");
}

#[test]
fn crossing_window_trims_independent_of_creation_order() {
    // Reverse creation order: C3, C2, C1, then H
    let mut s = Session::new();
    s.cmdline("LINE 75,-10 75,10").unwrap();
    s.cmdline("").unwrap();
    s.cmdline("LINE 50,-10 50,10").unwrap();
    s.cmdline("").unwrap();
    s.cmdline("LINE 25,-10 25,10").unwrap();
    s.cmdline("").unwrap();
    s.cmdline("LINE 0,0 100,0").unwrap();
    s.cmdline("").unwrap();

    s.doc_mut().unwrap().header.set_i64("TRIMEXTENDMODE", 1);

    let result = s.execute("trim", &json!({
        "crossing": [[70.0, -10.0], [30.0, 10.0]],
        "mode": "quick"
    })).unwrap();

    assert_eq!(result["trimmed"], 2);

    let expected = vec![
        ((0.0, 0.0), (25.0, 0.0)),
        ((25.0, -10.0), (25.0, 10.0)),
        ((75.0, -10.0), (75.0, 10.0)),
        ((75.0, 0.0), (100.0, 0.0)),
    ];
    assert_eq!(lines(&s), expected, "geometry identical regardless of creation order");
}

#[test]
fn crossing_window_standard_mode_with_explicit_edges() {
    let mut s = Session::new();
    s.cmdline("LINE 0,0 100,0").unwrap();
    s.cmdline("").unwrap();
    s.cmdline("LINE 25,-10 25,10").unwrap();
    s.cmdline("").unwrap();
    s.cmdline("LINE 50,-10 50,10").unwrap();
    s.cmdline("").unwrap();
    s.cmdline("LINE 75,-10 75,10").unwrap();
    s.cmdline("").unwrap();

    let d = s.doc().unwrap();
    let h_c1 = d.model.iter().find(|e| matches!(&e.kind, EntityKind::Line(l) if (l.a.x - 25.0).abs() < 1e-6)).unwrap().handle;
    let h_c3 = d.model.iter().find(|e| matches!(&e.kind, EntityKind::Line(l) if (l.a.x - 75.0).abs() < 1e-6)).unwrap().handle;

    // Standard mode: only C1 and C3 are cutting edges. C2 is NOT a cutting edge.
    let result = s.execute("trim", &json!({
        "crossing": [[70.0, -10.0], [30.0, 10.0]],
        "mode": "standard",
        "edges": [h_c1.hex(), h_c3.hex()]
    })).unwrap();

    // H is trimmed between C1 and C3 (kept 0-25 and 75-100).
    // C2 is not cut by C1/C3, so in standard mode it is untouched.
    assert_eq!(result["trimmed"], 1);

    let expected = vec![
        ((0.0, 0.0), (25.0, 0.0)),
        ((25.0, -10.0), (25.0, 10.0)),
        ((50.0, -10.0), (50.0, 10.0)),
        ((75.0, -10.0), (75.0, 10.0)),
        ((75.0, 0.0), (100.0, 0.0)),
    ];
    assert_eq!(lines(&s), expected, "standard mode preserves uncut C2 and trims H across chosen edges");
}

#[test]
fn crossing_window_trim_internal_and_global_undo() {
    let mut s = Session::new();
    s.cmdline("LINE 0,0 100,0").unwrap();
    s.cmdline("").unwrap();
    s.cmdline("LINE 25,-10 25,10").unwrap();
    s.cmdline("").unwrap();
    s.cmdline("LINE 50,-10 50,10").unwrap();
    s.cmdline("").unwrap();
    s.cmdline("LINE 75,-10 75,10").unwrap();
    s.cmdline("").unwrap();

    let initial_lines = lines(&s);

    // Interactive TRIM with crossing window, then internal Undo 'U'
    s.script("TRIM C 70,-10 30,10\nU\n\n").unwrap();
    assert_eq!(lines(&s), initial_lines, "internal Undo inside TRIM restores initial geometry");

    // Interactive TRIM with crossing window confirmed
    s.script("TRIM C 70,-10 30,10\n\n").unwrap();
    assert_eq!(lines(&s).len(), 4, "trimmed geometry has 4 lines");

    // Global session undo
    s.undo().unwrap();
    assert_eq!(lines(&s), initial_lines, "global session undo restores initial geometry");
}

#[test]
fn crossing_window_trims_circle_multiple_intervals() {
    let mut s = Session::new();
    s.cmdline("CIRCLE 0,0 10").unwrap();
    s.cmdline("LINE 0,-15 0,15").unwrap();
    s.cmdline("").unwrap();
    s.cmdline("LINE -15,0 15,0").unwrap();
    s.cmdline("").unwrap();

    let d = s.doc().unwrap();
    let h_v = d.model.iter().find(|e| matches!(&e.kind, EntityKind::Line(l) if (l.a.x).abs() < 1e-6)).unwrap().handle;
    let h_h = d.model.iter().find(|e| matches!(&e.kind, EntityKind::Line(l) if (l.a.y).abs() < 1e-6)).unwrap().handle;

    // Standard mode with L1 and L2 as explicit cutting edges
    // Window covers top half: y in [2, 12], x in [-12, 12]
    // Removes Quadrant 1 (0 -> pi/2) and Quadrant 2 (pi/2 -> pi)
    let result = s.execute("trim", &json!({
        "crossing": [[12.0, 12.0], [-12.0, 2.0]],
        "mode": "standard",
        "edges": [h_v.hex(), h_h.hex()]
    })).unwrap();

    assert_eq!(result["trimmed"], 2, "circle and vertical line trimmed in pass 1");

    // The circle should be replaced by the surviving bottom semicircle (pi -> 2pi)
    let mut arcs_pass1: Vec<(f64, f64)> = s.doc().unwrap().model.iter().filter_map(|e| {
        if let EntityKind::Arc(a) = &e.kind {
            Some((r(a.start), r(a.end)))
        } else {
            None
        }
    }).collect();
    arcs_pass1.sort_by(|a, b| a.partial_cmp(b).unwrap());

    // Surviving arcs cover quadrants 3 and 4 (pi -> 1.5pi and 1.5pi -> 2pi)
    assert_eq!(arcs_pass1.len(), 2, "expected 2 surviving quadrant arcs");
    assert!((arcs_pass1[0].0 - std::f64::consts::PI).abs() < 1e-3);
    assert!((arcs_pass1[0].1 - 1.5 * std::f64::consts::PI).abs() < 1e-3);
    assert!((arcs_pass1[1].0 - 1.5 * std::f64::consts::PI).abs() < 1e-3);
    assert!((arcs_pass1[1].1 - std::f64::consts::TAU).abs() < 1e-3 || arcs_pass1[1].1.abs() < 1e-3);

    // Pass 2: repeat the exact same crossing window without undo
    let res2 = s.execute("trim", &json!({
        "crossing": [[12.0, 12.0], [-12.0, 2.0]],
        "mode": "standard",
        "edges": [h_v.hex(), h_h.hex()]
    }));
    assert!(res2.is_err(), "pass 2 finds no objects to trim in window");

    // Geometry after pass 2 must be identical to pass 1
    let mut arcs_pass2: Vec<(f64, f64)> = s.doc().unwrap().model.iter().filter_map(|e| {
        if let EntityKind::Arc(a) = &e.kind {
            Some((r(a.start), r(a.end)))
        } else {
            None
        }
    }).collect();
    arcs_pass2.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(arcs_pass1, arcs_pass2, "no change after second pass");
}

#[test]
fn crossing_window_trims_polyline_multiple_intervals() {
    let mut s = Session::new();
    s.cmdline("PLINE 0,0 50,0 100,0").unwrap();
    s.cmdline("").unwrap();
    s.cmdline("LINE 25,-10 25,10").unwrap();
    s.cmdline("").unwrap();
    s.cmdline("LINE 50,-10 50,10").unwrap();
    s.cmdline("").unwrap();
    s.cmdline("LINE 75,-10 75,10").unwrap();
    s.cmdline("").unwrap();

    s.doc_mut().unwrap().header.set_i64("TRIMEXTENDMODE", 1);

    // Pass 1: crossing window 70,-10 to 30,10
    let result = s.execute("trim", &json!({
        "crossing": [[70.0, -10.0], [30.0, 10.0]],
        "mode": "quick"
    })).unwrap();

    // Polyline trimmed + C2 erased = 2
    assert_eq!(result["trimmed"], 2, "expected 2 entities trimmed/erased in pass 1");

    // Verify polyline geometry: surviving pieces must be 0->25 and 75->100
    let mut polylines_pass1: Vec<Vec<(f64, f64)>> = s.doc().unwrap().model.iter().filter_map(|e| {
        if let EntityKind::LwPolyline(p) = &e.kind {
            Some(p.vertices.iter().map(|v| (r(v.p.x), r(v.p.y))).collect())
        } else {
            None
        }
    }).collect();
    polylines_pass1.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let expected_polylines = vec![
        vec![(0.0, 0.0), (25.0, 0.0)],
        vec![(75.0, 0.0), (100.0, 0.0)],
    ];
    assert_eq!(polylines_pass1, expected_polylines, "polyline intervals 25-50 and 50-75 removed");

    // Pass 2: repeat the exact same crossing window without undo
    let res2 = s.execute("trim", &json!({
        "crossing": [[70.0, -10.0], [30.0, 10.0]],
        "mode": "quick"
    }));
    assert!(res2.is_err(), "pass 2 finds no objects to trim in window");

    let mut polylines_pass2: Vec<Vec<(f64, f64)>> = s.doc().unwrap().model.iter().filter_map(|e| {
        if let EntityKind::LwPolyline(p) = &e.kind {
            Some(p.vertices.iter().map(|v| (r(v.p.x), r(v.p.y))).collect())
        } else {
            None
        }
    }).collect();
    polylines_pass2.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(polylines_pass1, polylines_pass2, "no change after second pass");
}

#[test]
fn fallback_closed_polyline_selects_closest_to_first_corner_a() {
    // Closed square 0,0 to 20,20 cut at x=10 into left (x<=10) and right (x>=10) pieces.
    // When corner 'a' is on the left side (x=1), the pick nearest 'a' trims the left piece and keeps the right piece.
    let mut s1 = Session::new();
    s1.cmdline("PLINE 0,0 20,0 20,20 0,20 C").unwrap();
    s1.cmdline("LINE 10,-5 10,25").unwrap();
    s1.cmdline("").unwrap();
    s1.doc_mut().unwrap().header.set_i64("TRIMEXTENDMODE", 1);

    // a = (-5.0, 15.0), b = (25.0, 5.0). Window spans x in [-5, 25], y in [5, 15].
    // Center is (10.0, 10.0). Corner 'a' is on left (dist 5 to left side x=0 vs dist 25 to right side x=20).
    s1.execute("trim", &json!({
        "crossing": [[-5.0, 15.0], [25.0, 5.0]],
        "mode": "quick"
    })).unwrap();

    let plines1: Vec<_> = s1.doc().unwrap().model.iter().filter_map(|e| {
        if let EntityKind::LwPolyline(p) = &e.kind { Some(p.clone()) } else { None }
    }).collect();
    assert_eq!(plines1.len(), 1);
    // Kept piece must be the right piece (all vertices have x >= 10)
    for v in &plines1[0].vertices {
        assert!(v.p.x >= 10.0 - 1e-6, "expected kept piece to be right piece with x >= 10, got x={}", v.p.x);
    }

    // Now symmetric case: when corner 'a' is on the right side (x=25), the pick nearest 'a' trims the right piece and keeps the left piece.
    let mut s2 = Session::new();
    s2.cmdline("PLINE 0,0 20,0 20,20 0,20 C").unwrap();
    s2.cmdline("LINE 10,-5 10,25").unwrap();
    s2.cmdline("").unwrap();
    s2.doc_mut().unwrap().header.set_i64("TRIMEXTENDMODE", 1);

    // a = (25.0, 15.0), b = (-5.0, 5.0). Exact same window box, but corner 'a' is on the right.
    s2.execute("trim", &json!({
        "crossing": [[25.0, 15.0], [-5.0, 5.0]],
        "mode": "quick"
    })).unwrap();

    let plines2: Vec<_> = s2.doc().unwrap().model.iter().filter_map(|e| {
        if let EntityKind::LwPolyline(p) = &e.kind { Some(p.clone()) } else { None }
    }).collect();
    assert_eq!(plines2.len(), 1);
    // Kept piece must be the left piece (all vertices have x <= 10)
    for v in &plines2[0].vertices {
        assert!(v.p.x <= 10.0 + 1e-6, "expected kept piece to be left piece with x <= 10, got x={}", v.p.x);
    }
}

