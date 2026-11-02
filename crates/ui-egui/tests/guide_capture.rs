//! Alignment guide capture and effective point tests.

use cadcraft_engine::Session;
use cadcraft_engine::doc::{Drawing, EntityKind};
use cadcraft_engine::geom::{Vec2, Vec3};
use cadcraft_engine::snap::mode;
use cadcraft_engine::snap::tracking::CursorQuery;
use cadcraft_ui_egui::{CadApp, Services};
use egui::{Event, Modifiers, PointerButton, Pos2, vec2};
use egui_kittest::Harness;
use serde_json::json;

#[test]
fn test_tracking_hysteresis_and_exact_projection() {
    let mut s = Session::new();
    s.execute("setvar", &json!({ "name": "OSMODE", "value": mode::END })).unwrap();
    s.settings.polarmode = true;

    // Draw a base line from (0,0) to (10,0)
    s.execute("line", &json!({ "points": [[0.0, 0.0], [10.0, 0.0]] })).unwrap();

    // Start a new line from (10.0, 10.0)
    let base = Vec2::new(10.0, 10.0);
    let tol = 0.5;
    let query = |now: f64| CursorQuery { base: Some(base), deferred: false, corner: false, aperture: 1.0, tol, now, shift: false };

    // 1. Move cursor near the horizontal tracking line (y = 10.0).
    // Cursor at (20.0, 10.3). Deviation in Y is 0.3 <= tol (0.5).
    let r1 = s.snap_cursor(Vec2::new(20.0, 10.3), &query(1.0));
    assert!(r1.track.is_some(), "Alignment guide must activate within tolerance");
    // CRITICAL: Point must be projected EXACTLY onto the horizontal axis (y = 10.0)
    assert_eq!(r1.point.y, 10.0, "Captured point must project exactly onto the axis");
    assert_eq!(r1.point.x, 20.0);

    // 2. Hysteresis: Move cursor slightly further, deviation 0.65 (between tol 0.5 and release_tol 0.8)
    let r2 = s.snap_cursor(Vec2::new(20.0, 10.65), &query(1.1));
    assert!(r2.track.is_some(), "Hysteresis must retain tracking between tol and 1.6*tol");
    assert_eq!(r2.point.y, 10.0, "Point remains projected exactly on axis under hysteresis");

    // 3. Move cursor beyond release threshold (deviation 1.0 > 0.8)
    let r3 = s.snap_cursor(Vec2::new(20.0, 11.0), &query(1.2));
    assert!(r3.track.is_none(), "Cursor beyond release tolerance must release tracking");
    assert_eq!(r3.point, Vec2::new(20.0, 11.0), "Released point returns to raw cursor");

    // 4. Moving back to 0.65 without active track does NOT acquire (requires <= 0.5)
    let r4 = s.snap_cursor(Vec2::new(20.0, 10.65), &query(1.3));
    assert!(r4.track.is_none(), "New track requires dd <= tol (0.5), not release_tol");
}

#[test]
fn test_geometric_osnap_has_priority_over_tracking() {
    let mut s = Session::new();
    s.execute("setvar", &json!({ "name": "OSMODE", "value": mode::END })).unwrap();
    // Add existing line with endpoint at (20.0, 10.2)
    s.execute("line", &json!({ "points": [[20.0, 10.2], [25.0, 10.2]] })).unwrap();

    let base = Vec2::new(10.0, 10.0);
    let q = CursorQuery { base: Some(base), deferred: false, corner: false, aperture: 1.0, tol: 0.5, now: 1.0, shift: false };

    // Cursor at (20.1, 10.15) is near both tracking axis (y=10.0) and endpoint (20.0, 10.2)
    let r = s.snap_cursor(Vec2::new(20.1, 10.15), &q);
    assert!(r.snap.is_some(), "Geometric Osnap must be found");
    assert_eq!(r.point, Vec2::new(20.0, 10.2), "Geometric Osnap has strict priority over alignment guide");
}

#[test]
fn test_polar_tracking_disabled_removes_guide_and_snap() {
    let mut s = Session::new();
    s.settings.polarmode = false;

    let base = Vec2::new(10.0, 10.0);
    let q = CursorQuery { base: Some(base), deferred: false, corner: false, aperture: 1.0, tol: 0.5, now: 1.0, shift: false };

    let r = s.snap_cursor(Vec2::new(20.0, 10.2), &q);
    assert!(r.track.is_none(), "Disabled polarmode must not show guide");
    assert_eq!(r.point, Vec2::new(20.0, 10.2), "No snap to tracking direction when polarmode is off");
}

fn hover(h: &mut Harness<'_, CadApp>, point: Vec2) -> Pos2 {
    let pos = h.state().canvas.xf.unwrap().to_screen(point);
    h.event(Event::PointerMoved(pos));
    h.run_steps(3);
    pos
}

fn click(h: &mut Harness<'_, CadApp>, pos: Pos2) {
    for pressed in [true, false] {
        h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
        h.run_steps(2);
    }
}

#[test]
fn test_line_interactive_click_commits_exact_aligned_coordinates() {
    let mut session = Session::empty();
    let d = Drawing::new_metric();
    session.open_drawing(d, "line_snap", None);
    session.execute("view.set", &json!({ "center": [10.0, 0.0], "height": 30.0 })).unwrap();
    session.settings.polarmode = true;
    session.settings.osmode = 0; // only polar tracking

    let mut app = CadApp::new(session, Services::default());
    app.canvas.zoom_pending = false;

    let mut h = Harness::builder().with_size(vec2(1600.0, 1000.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(5);

    // Start LINE command
    h.state_mut().start("line");
    h.run_steps(3);

    let p1 = Vec2::new(0.0, 0.0);
    let screen1 = hover(&mut h, p1);
    click(&mut h, screen1);

    // Verify first point registered as base
    assert!(h.state().session.current_prompt().unwrap().base.is_some());

    // Move cursor near horizontal axis: world (20.0, 0.05). Screen pos has small vertical deviation (approx 1.5 pixels).
    let p2_raw = Vec2::new(20.0, 0.05);
    let screen2 = hover(&mut h, p2_raw);

    // Verify tracking guide is active
    assert!(h.state().canvas.track.is_some(), "Horizontal polar guide must be active");

    // Click second point
    click(&mut h, screen2);

    // Finish command with Enter (empty commandline input)
    h.state_mut().cmdline("");
    h.run_steps(3);

    // Inspect created line entity
    let doc = h.state().session.doc().unwrap();
    let mut found_line = false;
    for entity in doc.model.iter() {
        if let EntityKind::Line(l) = &entity.kind {
            found_line = true;
            assert!(l.a.near(Vec3::ZERO, 1e-4), "Point A near (0,0,0), got {:?}", l.a);
            // CRITICAL: Point B must be EXACTLY aligned on Y = base.y (0.0), not the raw 0.05!
            assert_eq!(l.b.y, l.a.y, "Confirmed geometry must be exactly aligned with guide axis");
            assert!((l.b.x - 20.0).abs() < 1e-4);
        }
    }
    assert!(found_line, "Created line must exist in drawing");
}
