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

    // Hover over uncut circle - nothing can be trimmed so preview must be empty
    let p = s.preview(cadcraft_geom::Vec2::new(100.0, 105.0));
    assert!(p.is_empty(), "uncut object must not display fictitious removal preview");
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

    // Hovering on the remaining uncuttable piece must NOT produce a fictitious preview!
    let p_none = s.preview(cadcraft_geom::Vec2::new(-10.0, 0.0));
    assert!(p_none.is_empty(), "a piece with no valid cuts remaining must not display fictitious removal preview");
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

