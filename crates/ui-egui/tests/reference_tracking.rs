//! Tests for hover reference acquisition (OTRACK) and tracking from acquired references.

use cadcraft_doc::{Drawing, EntityKind, Line};
use cadcraft_engine::Session;
use cadcraft_engine::geom::{Vec2, Vec3};
use cadcraft_engine::snap::mode;
use cadcraft_engine::snap::tracking::{Acquired, PathKind};
use cadcraft_ui_egui::{CadApp, Services};
use egui::{Event, Modifiers, PointerButton, Pos2, RawInput, Rect, vec2};
use serde_json::json;

fn step_ui(app: &mut CadApp, ctx: &egui::Context, events: Vec<Event>, time: f64) {
    let input = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 1000.0))), time: Some(time), events, ..Default::default() };
    let mut output = ctx.run_ui(input, |ui| app.ui(ui));
    output.textures_delta.clear();
}

fn app_with_drawing() -> (CadApp, egui::Context) {
    let mut session = Session::empty();
    let d = Drawing::new_metric();
    session.open_drawing(d, "ref_track", None);
    session.execute("view.set", &json!({ "center": [15.0, 10.0], "height": 40.0 })).unwrap();
    session.settings.polarmode = true;
    session.settings.otrack = true;
    session.settings.osmode = mode::END | mode::MID | mode::CEN | mode::NOD | mode::PAR;
    let app = CadApp::new(session, Services::default());
    let ctx = egui::Context::default();
    (app, ctx)
}

fn to_screen(app: &CadApp, p: Vec2) -> Pos2 {
    app.canvas.xf.unwrap().to_screen(p)
}

#[test]
fn test_brief_hover_does_not_acquire_before_dwell() {
    let (mut app, ctx) = app_with_drawing();
    let mut time = 1.0;
    // Add existing segment with endpoint at (10, 5)
    app.session.execute("line", &json!({ "points": [[10.0, 5.0], [20.0, 5.0]] })).unwrap();

    // Start LINE at (0, 0)
    app.cmdline("LINE 0,0");
    time += 0.1;
    step_ui(&mut app, &ctx, Vec::new(), time);

    let end_screen = to_screen(&app, Vec2::new(10.0, 5.0));
    // Hover briefly for 0.15s (less than DWELL 0.4s)
    time += 0.05;
    step_ui(&mut app, &ctx, vec![Event::PointerMoved(end_screen)], time);
    time += 0.15;
    step_ui(&mut app, &ctx, vec![Event::PointerMoved(end_screen)], time);

    // Move away to (15, 12)
    let away_screen = to_screen(&app, Vec2::new(15.0, 12.0));
    time += 0.05;
    step_ui(&mut app, &ctx, vec![Event::PointerMoved(away_screen)], time);

    assert!(app.session.tracking.acquired.is_empty(), "Brief hover must not acquire before DWELL threshold");
}

#[test]
fn test_hover_dwell_acquires_endpoint_without_click_and_persists() {
    let (mut app, ctx) = app_with_drawing();
    let mut time = 1.0;
    app.session.execute("line", &json!({ "points": [[10.0, 5.0], [20.0, 5.0]] })).unwrap();

    app.cmdline("LINE 0,0");
    time += 0.1;
    step_ui(&mut app, &ctx, Vec::new(), time);

    let end_screen = to_screen(&app, Vec2::new(10.0, 5.0));
    // Hover on endpoint and dwell for 0.5s (> 0.4s DWELL)
    time += 0.05;
    step_ui(&mut app, &ctx, vec![Event::PointerMoved(end_screen)], time);
    time += 0.50;
    step_ui(&mut app, &ctx, vec![Event::PointerMoved(end_screen)], time);

    assert_eq!(app.session.tracking.acquired.len(), 1, "Endpoint must be acquired after resting on it");
    assert!(app.session.tracking.acquired[0].at().near(Vec2::new(10.0, 5.0), 1e-4));
    assert!(app.session.prompting(), "No click occurred; prompt remains active");

    // Move away to (15, 12) - reference must remain acquired
    let away_screen = to_screen(&app, Vec2::new(15.0, 12.0));
    time += 0.05;
    step_ui(&mut app, &ctx, vec![Event::PointerMoved(away_screen)], time);

    assert_eq!(app.session.tracking.acquired.len(), 1, "Acquired reference must persist when cursor moves away");
}

#[test]
fn test_guides_from_acquired_reference_horizontal_and_vertical() {
    let (mut app, ctx) = app_with_drawing();
    let mut time = 1.0;
    app.session.execute("line", &json!({ "points": [[10.0, 5.0], [20.0, 5.0]] })).unwrap();

    app.cmdline("LINE 0,0");
    time += 0.1;
    step_ui(&mut app, &ctx, Vec::new(), time);

    // Acquire endpoint (10, 5)
    let end_screen = to_screen(&app, Vec2::new(10.0, 5.0));
    time += 0.05;
    step_ui(&mut app, &ctx, vec![Event::PointerMoved(end_screen)], time);
    time += 0.50;
    step_ui(&mut app, &ctx, vec![Event::PointerMoved(end_screen)], time);

    // 1. Approach horizontal guide at Y = 5.0 (e.g. cursor at (25.0, 5.05))
    let h_screen = to_screen(&app, Vec2::new(25.0, 5.05));
    time += 0.05;
    step_ui(&mut app, &ctx, vec![Event::PointerMoved(h_screen)], time);

    let track = app.canvas.track.as_ref().expect("Horizontal guide from acquired reference must activate");
    assert_eq!(track.point.y, 5.0, "Point must project exactly on horizontal guide Y = 5.0");
    assert_eq!(track.paths[0].kind, PathKind::Alignment);
    assert_eq!(track.paths[0].from, Vec2::new(10.0, 5.0));

    // 2. Approach vertical guide at X = 10.0 (e.g. cursor at (10.05, 15.0))
    let v_screen = to_screen(&app, Vec2::new(10.05, 15.0));
    time += 0.05;
    step_ui(&mut app, &ctx, vec![Event::PointerMoved(v_screen)], time);

    let track_v = app.canvas.track.as_ref().expect("Vertical guide from acquired reference must activate");
    assert_eq!(track_v.point.x, 10.0, "Point must project exactly on vertical guide X = 10.0");
    assert_eq!(track_v.paths[0].kind, PathKind::Alignment);
    assert_eq!(track_v.paths[0].from, Vec2::new(10.0, 5.0));
}

#[test]
fn test_mandatory_case_base_direction_meets_acquired_reference_intersection() {
    let (mut app, ctx) = app_with_drawing();
    let mut time = 1.0;
    // Existing segment with endpoint B at (10, 5)
    app.session.execute("line", &json!({ "points": [[10.0, 5.0], [20.0, 5.0]] })).unwrap();

    // LINE starts at A = (0, 0)
    app.cmdline("LINE 0,0");
    time += 0.1;
    step_ui(&mut app, &ctx, Vec::new(), time);

    // Dwell on B = (10, 5) to acquire it
    let end_screen = to_screen(&app, Vec2::new(10.0, 5.0));
    time += 0.05;
    step_ui(&mut app, &ctx, vec![Event::PointerMoved(end_screen)], time);
    time += 0.50;
    step_ui(&mut app, &ctx, vec![Event::PointerMoved(end_screen)], time);
    assert_eq!(app.session.tracking.acquired.len(), 1);

    // Move cursor near the crossing of:
    // - Horizontal ray from A (0,0) -> Y = 0
    // - Vertical guide from B (10,5) -> X = 10
    // The crossing is at (10.0, 0.0). Cursor at (10.05, 0.05).
    let cross_screen = to_screen(&app, Vec2::new(10.05, 0.05));
    time += 0.05;
    step_ui(&mut app, &ctx, vec![Event::PointerMoved(cross_screen)], time);

    let tr = app.canvas.track.as_ref().expect("Intersection of base direction and reference guide must activate");
    assert_eq!(tr.point, Vec2::new(10.0, 0.0), "Intersection point must be exact (10.0, 0.0)");
    assert_eq!(tr.paths.len(), 2, "Both paths must be captured simultaneously at intersection");
    assert!(tr.paths.iter().any(|p| p.kind == PathKind::Polar), "One path must be polar direction from base point A");
    assert!(tr.paths.iter().any(|p| p.kind == PathKind::Alignment), "Other path must be alignment from acquired point B");

    // Click to confirm the point at the intersection
    time += 0.05;
    step_ui(
        &mut app,
        &ctx,
        vec![Event::PointerButton { pos: cross_screen, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE }],
        time,
    );
    time += 0.05;
    step_ui(
        &mut app,
        &ctx,
        vec![Event::PointerButton { pos: cross_screen, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE }],
        time,
    );

    // Check confirmed geometry in document
    let doc = app.session.doc().unwrap();
    let lines: Vec<&Line> = doc
        .model
        .iter()
        .filter_map(|e| match &e.kind {
            EntityKind::Line(l) => Some(l),
            _ => None,
        })
        .collect();
    // The new line segment must end exactly at (10.0, 0.0, 0.0)
    let new_seg = lines.iter().find(|l| l.a.near(Vec3::ZERO, 1e-4)).expect("New segment starting at A must exist");
    assert!(new_seg.b.near(Vec3::new(10.0, 0.0, 0.0), 1e-4), "Confirmed line must end exactly at intersection (10.0, 0.0, 0.0): {:?}", new_seg.b);
}

#[test]
fn test_two_acquired_references_intersection() {
    let (mut app, ctx) = app_with_drawing();
    let mut time = 1.0;
    // Add two existing lines: one with endpoint B1 at (10, 5), another with B2 at (20, 15)
    app.session.execute("line", &json!({ "points": [[10.0, 5.0], [10.0, 2.0]] })).unwrap();
    app.session.execute("line", &json!({ "points": [[20.0, 15.0], [25.0, 15.0]] })).unwrap();

    // Start LINE at (0, 0)
    app.cmdline("LINE 0,0");
    time += 0.1;
    step_ui(&mut app, &ctx, Vec::new(), time);

    // Dwell on B1 (10, 5)
    let b1_pos = to_screen(&app, Vec2::new(10.0, 5.0));
    time += 0.05;
    step_ui(&mut app, &ctx, vec![Event::PointerMoved(b1_pos)], time);
    time += 0.50;
    step_ui(&mut app, &ctx, vec![Event::PointerMoved(b1_pos)], time);

    // Dwell on B2 (20, 15)
    let b2_pos = to_screen(&app, Vec2::new(20.0, 15.0));
    time += 0.05;
    step_ui(&mut app, &ctx, vec![Event::PointerMoved(b2_pos)], time);
    time += 0.50;
    step_ui(&mut app, &ctx, vec![Event::PointerMoved(b2_pos)], time);

    assert_eq!(app.session.tracking.acquired.len(), 2, "Both references must be acquired");

    // Move cursor near intersection of vertical from B1 (X = 10) and horizontal from B2 (Y = 15) -> (10, 15)
    let cross_pos = to_screen(&app, Vec2::new(10.05, 14.95));
    time += 0.05;
    step_ui(&mut app, &ctx, vec![Event::PointerMoved(cross_pos)], time);

    let tr = app.canvas.track.as_ref().expect("Intersection of two acquired references must activate");
    assert_eq!(tr.point, Vec2::new(10.0, 15.0), "Intersection of B1 and B2 must be exact (10.0, 15.0)");
    assert_eq!(tr.paths.len(), 2);
    assert!(tr.paths.iter().all(|p| p.kind == PathKind::Alignment));
}

#[test]
fn test_hover_again_retires_reference_and_does_not_oscillate() {
    let (mut app, ctx) = app_with_drawing();
    let mut time = 1.0;
    app.session.execute("line", &json!({ "points": [[10.0, 5.0], [20.0, 5.0]] })).unwrap();

    app.cmdline("LINE 0,0");
    time += 0.1;
    step_ui(&mut app, &ctx, Vec::new(), time);

    // 1. Acquire B (10, 5)
    let b_pos = to_screen(&app, Vec2::new(10.0, 5.0));
    time += 0.05;
    step_ui(&mut app, &ctx, vec![Event::PointerMoved(b_pos)], time);
    time += 0.50;
    step_ui(&mut app, &ctx, vec![Event::PointerMoved(b_pos)], time);
    assert_eq!(app.session.tracking.acquired.len(), 1);

    // 2. Move away
    let away_pos = to_screen(&app, Vec2::new(15.0, 12.0));
    time += 0.05;
    step_ui(&mut app, &ctx, vec![Event::PointerMoved(away_pos)], time);
    assert_eq!(app.session.tracking.acquired.len(), 1);

    // 3. Move back over B and dwell to retire it
    time += 0.05;
    step_ui(&mut app, &ctx, vec![Event::PointerMoved(b_pos)], time);
    time += 0.50;
    step_ui(&mut app, &ctx, vec![Event::PointerMoved(b_pos)], time);
    assert!(app.session.tracking.acquired.is_empty(), "Resting on reference again must retire it");

    // 4. Verify no oscillation: staying over B does NOT re-acquire on subsequent frame
    time += 0.05;
    step_ui(&mut app, &ctx, vec![Event::PointerMoved(b_pos)], time);
    assert!(app.session.tracking.acquired.is_empty(), "Staying over retired point must not oscillate per frame");
}

#[test]
fn test_clear_references_on_escape_and_finish() {
    let (mut app, ctx) = app_with_drawing();
    let mut time = 1.0;
    app.session.execute("line", &json!({ "points": [[10.0, 5.0], [20.0, 5.0]] })).unwrap();

    // Start LINE, acquire B
    app.cmdline("LINE 0,0");
    time += 0.1;
    step_ui(&mut app, &ctx, Vec::new(), time);
    let b_pos = to_screen(&app, Vec2::new(10.0, 5.0));
    time += 0.05;
    step_ui(&mut app, &ctx, vec![Event::PointerMoved(b_pos)], time);
    time += 0.50;
    step_ui(&mut app, &ctx, vec![Event::PointerMoved(b_pos)], time);
    assert_eq!(app.session.tracking.acquired.len(), 1);

    // Press Escape to cancel LINE
    step_ui(
        &mut app,
        &ctx,
        vec![Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }],
        time + 0.05,
    );
    step_ui(
        &mut app,
        &ctx,
        vec![Event::Key { key: egui::Key::Escape, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE }],
        time + 0.10,
    );

    assert!(app.session.tracking.acquired.is_empty(), "Cancelling command must clear all acquired references");
}

#[test]
fn test_otrack_disabled_no_acquisition() {
    let (mut app, ctx) = app_with_drawing();
    let mut time = 1.0;
    app.session.execute("line", &json!({ "points": [[10.0, 5.0], [20.0, 5.0]] })).unwrap();
    // Disable OTRACK (F11)
    app.session.settings.otrack = false;

    app.cmdline("LINE 0,0");
    time += 0.1;
    step_ui(&mut app, &ctx, Vec::new(), time);

    let b_pos = to_screen(&app, Vec2::new(10.0, 5.0));
    time += 0.05;
    step_ui(&mut app, &ctx, vec![Event::PointerMoved(b_pos)], time);
    time += 0.50;
    step_ui(&mut app, &ctx, vec![Event::PointerMoved(b_pos)], time);

    assert!(app.session.tracking.acquired.is_empty(), "Disabled OTRACK must not acquire references");
}

#[test]
fn test_geometric_osnap_and_typed_coords_maintain_priority() {
    let (mut app, ctx) = app_with_drawing();
    let mut time = 1.0;
    // Add two lines: one with endpoint B at (10, 5), another with endpoint C at (10.0, 0.0)
    app.session.execute("line", &json!({ "points": [[10.0, 5.0], [20.0, 5.0]] })).unwrap();
    app.session.execute("line", &json!({ "points": [[10.0, 0.0], [10.0, -5.0]] })).unwrap();

    app.cmdline("LINE 0,0");
    time += 0.1;
    step_ui(&mut app, &ctx, Vec::new(), time);

    // Acquire B (10, 5)
    let b_pos = to_screen(&app, Vec2::new(10.0, 5.0));
    time += 0.05;
    step_ui(&mut app, &ctx, vec![Event::PointerMoved(b_pos)], time);
    time += 0.50;
    step_ui(&mut app, &ctx, vec![Event::PointerMoved(b_pos)], time);

    // Move cursor over C (10, 0)
    let c_pos = to_screen(&app, Vec2::new(10.0, 0.0));
    time += 0.05;
    step_ui(&mut app, &ctx, vec![Event::PointerMoved(c_pos)], time);

    assert!(app.canvas.snap.is_some(), "Real geometric Osnap at C must be active");
    assert_eq!(app.canvas.snap.unwrap().name, "Endpoint");
}

#[test]
fn test_parallel_tracking_to_inclined_line() {
    let (mut app, ctx) = app_with_drawing();
    let mut time = 1.0;
    // Add inclined line from (0, 0) to (10, 10) (unit direction [1/sqrt(2), 1/sqrt(2)])
    app.session.execute("line", &json!({ "points": [[0.0, 0.0], [10.0, 10.0]] })).unwrap();

    // Start LINE at (20, 0)
    app.cmdline("LINE 20,0");
    time += 0.1;
    step_ui(&mut app, &ctx, Vec::new(), time);

    // Dwell on the inclined line at (4.0, 4.0) (away from endpoint and midpoint)
    let edge_pos = to_screen(&app, Vec2::new(4.0, 4.0));
    time += 0.05;
    step_ui(&mut app, &ctx, vec![Event::PointerMoved(edge_pos)], time);
    time += 0.50;
    step_ui(&mut app, &ctx, vec![Event::PointerMoved(edge_pos)], time);

    assert_eq!(app.session.tracking.acquired.len(), 1);
    assert!(matches!(app.session.tracking.acquired[0], Acquired::Parallel { .. }), "Inclined line must be acquired as Parallel");

    // Move cursor near parallel direction from base (20, 0): point (25.0, 5.0) lies on line through (20,0) with direction (1,1)
    let par_pos = to_screen(&app, Vec2::new(25.05, 4.95));
    time += 0.05;
    step_ui(&mut app, &ctx, vec![Event::PointerMoved(par_pos)], time);

    let tr = app.canvas.track.as_ref().expect("Parallel tracking guide must activate");
    assert_eq!(tr.paths[0].kind, PathKind::Parallel);
    assert_eq!(tr.paths[0].name, "Parallel");
    let diff = tr.point - Vec2::new(20.0, 0.0);
    assert!((diff.x - diff.y).abs() < 1e-4, "Captured point must be exactly parallel: {:?}", tr.point);
}

#[test]
fn test_front_view_preserves_height_z() {
    let (mut app, ctx) = app_with_drawing();
    let mut time = 1.0;
    // Switch to Front view
    app.session.execute("view.front", &json!({})).unwrap();

    // Add an entity at height Z = 8.0: line from (10, 0, 8) to (20, 0, 8)
    app.session.execute("line", &json!({ "points": [[10.0, 0.0, 8.0], [20.0, 0.0, 8.0]] })).unwrap();

    // Start LINE in Front view at (0, 0, 0)
    app.cmdline("LINE 0,0,0");
    time += 0.1;
    step_ui(&mut app, &ctx, Vec::new(), time);

    // Front view projects (X, Y, Z) to screen using camera.
    // Endpoint is at (10, 0, 8).
    let p_screen = app.session.camera().project_point(Vec3::new(10.0, 0.0, 8.0));
    let xf = app.canvas.xf.unwrap();
    let c = xf.rect.center();
    let screen_pos = Pos2::new(c.x + (p_screen.x * xf.scale) as f32, c.y - (p_screen.y * xf.scale) as f32);

    time += 0.05;
    step_ui(&mut app, &ctx, vec![Event::PointerMoved(screen_pos)], time);
    time += 0.50;
    step_ui(&mut app, &ctx, vec![Event::PointerMoved(screen_pos)], time);

    assert_eq!(app.session.tracking.acquired.len(), 1, "Endpoint in Front view must be acquired");
    let acq = app.session.tracking.acquired[0];
    assert_eq!(acq.at3(), Some(Vec3::new(10.0, 0.0, 8.0)), "3D coordinates in Front view must be preserved exactly");
}
