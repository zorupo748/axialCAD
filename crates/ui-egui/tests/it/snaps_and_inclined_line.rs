//! Regression tests for object snaps (endpoint, midpoint) and inclined view LINE creation.

use cadcraft_engine::Session;
use cadcraft_engine::doc::{Common, Drawing, EntityKind, Layout, Line, Space, Viewport};
use cadcraft_engine::geom::{Vec2, Vec3};
use cadcraft_engine::snap::mode;
use cadcraft_ui_egui::{CadApp, Services};
use egui::{Event, Key, Modifiers, vec2};
use egui_kittest::Harness;
use serde_json::json;

const TEMPLATE_LAYOUTS: &[&str] = &[
    "Formato A4 vertical",
    "Formato A4 horizontal",
    "Formato A3",
    "Formato A2",
    "Lista de piezas A4",
    "Lista de piezas A3",
    "Lista de piezas A2",
    "Lista de piezas separado",
];

fn make_harness(app: CadApp) -> Harness<'static, CadApp> {
    let mut h = Harness::builder().with_size(vec2(1280.0, 760.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(3);
    h
}

#[test]
fn test_endpoint_and_midpoint_snapping_plan_and_inclined() {
    let mut d = Drawing::new_metric();
    d.add(
        &Space::Model,
        Common::default(),
        EntityKind::Line(Line { a: Vec3::new(10.0, 10.0, 0.0), b: Vec3::new(30.0, 10.0, 0.0) }),
    )
    .unwrap();

    let mut session = Session::empty();
    session.open_drawing(d, "snap_test", None);
    session.settings.osmode = mode::END | mode::MID;
    session.execute("view.set", &json!({ "center": [20.0, 10.0], "height": 50.0 })).unwrap();

    let mut app = CadApp::new(session, Services::default());
    app.canvas.zoom_pending = false;
    let mut h = make_harness(app);

    // 1. Model space in Plan view: test Endpoint snap
    h.state_mut().session.start("line").unwrap();
    let xf = h.state().canvas.xf.expect("canvas xf");
    let end_screen = xf.to_screen3(Vec3::new(10.0, 10.0, 0.0));
    h.event(Event::PointerMoved(end_screen));
    h.run_steps(2);

    let snap_hit = h.state().canvas.snap.expect("Endpoint snap hit in plan view");
    assert_eq!(snap_hit.mode, mode::END);
    assert_eq!(snap_hit.name, "Endpoint");

    // 2. Model space in Plan view: test Midpoint snap
    let mid_screen = xf.to_screen3(Vec3::new(20.0, 10.0, 0.0));
    h.event(Event::PointerMoved(mid_screen));
    h.run_steps(2);

    let mid_hit = h.state().canvas.snap.expect("Midpoint snap hit in plan view");
    assert_eq!(mid_hit.mode, mode::MID);
    assert_eq!(mid_hit.name, "Midpoint");

    // Accept midpoint click and complete a segment
    h.event(Event::PointerButton { pos: mid_screen, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.event(Event::PointerButton { pos: mid_screen, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    let p2_screen = xf.to_screen3(Vec3::new(20.0, 25.0, 0.0));
    h.event(Event::PointerMoved(p2_screen));
    h.event(Event::PointerButton { pos: p2_screen, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.event(Event::PointerButton { pos: p2_screen, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
    h.event(Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    let doc = h.state().session.doc().unwrap();
    let lines: Vec<_> = doc.model.iter().filter_map(|e| match &e.kind { EntityKind::Line(l) => Some(l.clone()), _ => None }).collect();
    assert_eq!(lines.len(), 2);
    let drawn = &lines[1];
    assert!((drawn.a.x - 20.0).abs() < 1e-4);
    assert!((drawn.a.y - 10.0).abs() < 1e-4);
    assert_eq!(drawn.a.z, 0.0);
    assert_eq!(drawn.b.z, 0.0);

    // 3. Inclined 3D view: test Endpoint and Midpoint snap
    h.state_mut().session.execute("view.swiso", &json!({})).unwrap();
    h.run_steps(2);

    h.state_mut().session.start("line").unwrap();
    let xf_3d = h.state().canvas.xf.expect("canvas xf in 3D");
    let end_3d = xf_3d.to_screen3(Vec3::new(30.0, 10.0, 0.0));
    h.event(Event::PointerMoved(end_3d));
    h.run_steps(2);

    let snap_3d_end = h.state().canvas.snap.expect("Endpoint snap hit in 3D inclined view");
    assert_eq!(snap_3d_end.mode, mode::END);

    let mid_3d = xf_3d.to_screen3(Vec3::new(20.0, 10.0, 0.0));
    h.event(Event::PointerMoved(mid_3d));
    h.run_steps(2);

    let snap_3d_mid = h.state().canvas.snap.expect("Midpoint snap hit in 3D inclined view");
    assert_eq!(snap_3d_mid.mode, mode::MID);
    assert_eq!(snap_3d_mid.name, "Midpoint");
    assert_eq!(snap_3d_mid.point3, Some(Vec3::new(20.0, 10.0, 0.0)));

    h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
}



#[test]
fn test_line_creation_by_click_in_inclined_view_at_elevation_zero() {
    let mut session = Session::empty();
    session.open_drawing(Drawing::new_metric(), "inclined_line_test", None);
    session.execute("view.swiso", &json!({})).unwrap();

    let mut app = CadApp::new(session, Services::default());
    app.canvas.zoom_pending = false;
    let mut h = make_harness(app);

    h.state_mut().session.start("line").unwrap();
    let center_pt = h.state().canvas.rect.unwrap().center();
    h.event(Event::PointerMoved(center_pt));
    h.run_steps(2);

    let first_pt3 = h.state().canvas.cursor3.expect("first point resolves on ground");
    assert_eq!(first_pt3.z, 0.0, "first free point in inclined view must be at Z=0.0");

    // Click first point
    h.event(Event::PointerButton { pos: center_pt, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.event(Event::PointerButton { pos: center_pt, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    // Move cursor downwards on screen (where ground extends along isometric diagonal)
    let second_screen = center_pt + vec2(0.0, 60.0);
    h.event(Event::PointerMoved(second_screen));
    h.run_steps(2);

    let second_pt3 = h.state().canvas.cursor3.expect("second point resolves on ground");
    assert_eq!(second_pt3.z, 0.0, "second free point moving downward must NOT plunge below floor, got {}", second_pt3.z);

    // Click second point
    h.event(Event::PointerButton { pos: second_screen, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.event(Event::PointerButton { pos: second_screen, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    h.event(Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    let doc = h.state().session.doc().unwrap();
    let lines: Vec<_> = doc.model.iter().filter_map(|e| match &e.kind { EntityKind::Line(l) => Some(l.clone()), _ => None }).collect();
    assert_eq!(lines.len(), 1, "exactly one line segment should be created");
    assert_eq!(lines[0].a.z, 0.0, "start Z must be exactly 0.0");
    assert_eq!(lines[0].b.z, 0.0, "end Z must be exactly 0.0, not below floor");
    assert!(lines[0].a != lines[0].b, "line must have non-zero length");
}

#[test]
fn test_line_creation_with_user_configured_elevation() {
    let mut session = Session::empty();
    session.open_drawing(Drawing::new_metric(), "elevated_line_test", None);
    session.execute("view.swiso", &json!({})).unwrap();
    session.set_drawing_plane_offset(25.0);

    let mut app = CadApp::new(session, Services::default());
    app.canvas.zoom_pending = false;
    let mut h = make_harness(app);

    h.state_mut().session.start("line").unwrap();
    let center_pt = h.state().canvas.rect.unwrap().center();
    h.event(Event::PointerMoved(center_pt));
    h.run_steps(2);

    let first_pt3 = h.state().canvas.cursor3.expect("first point resolves on elevation");
    assert_eq!(first_pt3.z, 25.0, "first free point must resolve at configured elevation 25.0");

    h.event(Event::PointerButton { pos: center_pt, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.event(Event::PointerButton { pos: center_pt, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    let second_screen = center_pt + vec2(0.0, 60.0);
    h.event(Event::PointerMoved(second_screen));
    h.run_steps(2);

    let second_pt3 = h.state().canvas.cursor3.expect("second point resolves on elevation");
    assert_eq!(second_pt3.z, 25.0, "second free point must remain at configured elevation 25.0, got {}", second_pt3.z);

    h.event(Event::PointerButton { pos: second_screen, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.event(Event::PointerButton { pos: second_screen, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    h.event(Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    let doc = h.state().session.doc().unwrap();
    let lines: Vec<_> = doc.model.iter().filter_map(|e| match &e.kind { EntityKind::Line(l) => Some(l.clone()), _ => None }).collect();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].a.z, 25.0);
    assert_eq!(lines[0].b.z, 25.0);
}

#[test]
fn test_snaps_preserved_in_paper_and_mspace() {
    let mut d = Drawing::new_metric();
    // Model entity
    d.add(
        &Space::Model,
        Common::default(),
        EntityKind::Line(Line { a: Vec3::new(10.0, 10.0, 0.0), b: Vec3::new(50.0, 10.0, 0.0) }),
    )
    .unwrap();

    // Paper entity
    let paper = Space::Paper("Layout1".into());
    d.add(
        &paper,
        Common::default(),
        EntityKind::Line(Line { a: Vec3::new(20.0, 20.0, 0.0), b: Vec3::new(60.0, 20.0, 0.0) }),
    )
    .unwrap();

    // Viewport
    d.add(
        &paper,
        Common::default(),
        EntityKind::Viewport(Viewport {
            center: Vec3::new(100.0, 50.0, 0.0),
            width: 100.0,
            height: 80.0,
            view_center: Vec2::new(30.0, 10.0),
            view_height: 50.0,
            id: 2,
            locked: true,
            frozen_layers: Vec::new(),
            layer_colors: Vec::new(),
            clip: None,
            on: true,
        }),
    )
    .unwrap();

    let mut session = Session::empty();
    session.open_drawing(d, "layout_snaps", None);
    session.settings.osmode = mode::END | mode::MID;
    session.execute("layout.set", &json!({ "name": "Layout1" })).unwrap();
    session.execute("view.set", &json!({ "center": [60.0, 40.0], "height": 120.0 })).unwrap();

    let mut app = CadApp::new(session, Services::default());
    app.canvas.zoom_pending = false;
    let mut h = make_harness(app);

    // 1. In Paper space: test Endpoint and Midpoint snap on paper line
    h.state_mut().session.start("line").unwrap();
    let xf_paper = h.state().canvas.xf.expect("canvas xf in paper");
    let paper_end_screen = xf_paper.to_screen(Vec2::new(20.0, 20.0));
    h.event(Event::PointerMoved(paper_end_screen));
    h.run_steps(2);

    let paper_hit_end = h.state().canvas.snap.expect("Endpoint snap in paper space");
    assert_eq!(paper_hit_end.mode, mode::END);

    let paper_mid_screen = xf_paper.to_screen(Vec2::new(40.0, 20.0));
    h.event(Event::PointerMoved(paper_mid_screen));
    h.run_steps(2);

    let paper_hit_mid = h.state().canvas.snap.expect("Midpoint snap in paper space");
    assert_eq!(paper_hit_mid.mode, mode::MID);
    assert_eq!(paper_hit_mid.name, "Midpoint");

    h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    // 2. Activate MSPACE inside the viewport and test model snaps
    h.state_mut().session.execute("mspace", &json!({})).unwrap();
    h.run_steps(2);

    h.state_mut().session.start("line").unwrap();
    let xf_model = h.state().canvas.xf.expect("canvas xf in mspace");
    let model_mid_screen = xf_model.to_screen(Vec2::new(30.0, 10.0));
    h.event(Event::PointerMoved(model_mid_screen));
    h.run_steps(2);

    let model_hit = h.state().canvas.snap.expect("Midpoint snap in mspace");
    assert_eq!(model_hit.mode, mode::MID);

    h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
}

#[test]
fn test_line_creation_with_shifted_camera_target_z() {
    let mut session = Session::empty();
    session.open_drawing(Drawing::new_metric(), "shifted_camera_test", None);
    session.execute("view.swiso", &json!({})).unwrap();
    let mut cam = session.camera();
    cam.target = Vec3::new(500.0, 300.0, -2686.3);
    cam.height = 1000.0;
    session.set_camera(cam);

    let mut app = CadApp::new(session, Services::default());
    app.canvas.zoom_pending = false;
    let mut h = make_harness(app);

    h.state_mut().session.start("line").unwrap();
    let p1_screen = h.state().canvas.rect.unwrap().center();
    h.event(Event::PointerMoved(p1_screen));
    h.run_steps(2);

    let c3_first = h.state().canvas.cursor3;
    println!("Diagnostic: cursor3 first = {:?}", c3_first);
    let p1_pt3 = c3_first.expect("first point resolves");
    println!("Diagnostic: p1_pt3 = {:?}", p1_pt3);

    // Click first point
    h.event(Event::PointerButton { pos: p1_screen, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.event(Event::PointerButton { pos: p1_screen, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    // Move to second point
    let p2_screen = p1_screen + vec2(50.0, -30.0);
    h.event(Event::PointerMoved(p2_screen));
    h.run_steps(2);

    let c3_second = h.state().canvas.cursor3;
    println!("Diagnostic: cursor3 second = {:?}", c3_second);
    let p2_pt3 = c3_second.expect("second point resolves");
    println!("Diagnostic: p2_pt3 = {:?}", p2_pt3);

    // Click second point
    h.event(Event::PointerButton { pos: p2_screen, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.event(Event::PointerButton { pos: p2_screen, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    h.event(Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    let doc = h.state().session.doc().unwrap();
    let lines: Vec<_> = doc.model.iter().filter_map(|e| match &e.kind { EntityKind::Line(l) => Some(l.clone()), _ => None }).collect();
    assert_eq!(lines.len(), 1, "exactly one line segment should be created");
    println!("Diagnostic: line = {:?}", lines[0]);
    assert_eq!(lines[0].a.z, 0.0, "start Z must be exactly 0.0, got {}", lines[0].a.z);
    assert_eq!(lines[0].b.z, 0.0, "end Z must be exactly 0.0, got {}", lines[0].b.z);
}

#[test]
fn test_reproduce_drawing1_investigation() {
    let mut d = Drawing::new_metric();
    d.layouts.clear();
    for (idx, name) in TEMPLATE_LAYOUTS.iter().enumerate() {
        d.layouts.push(Layout::new(name, (idx + 1) as u32));
        let paper = Space::Paper((*name).to_string());
        d.add(
            &paper,
            Common::default(),
            EntityKind::Viewport(Viewport {
                id: 2,
                center: Vec3::new(148.5, 105.0, 0.0),
                width: 250.0,
                height: 170.0,
                view_center: Vec2::new(148.5, 105.0),
                view_height: 170.0,
                locked: true,
                frozen_layers: Vec::new(),
                layer_colors: Vec::new(),
                clip: None,
                on: true,
            }),
        )
        .unwrap();
    }
    let mut session = Session::empty();
    session.open_drawing(d, "Template Drawing", None);

    let mut app = CadApp::new(session, Services::default());
    app.canvas.zoom_pending = false;
    let mut h = make_harness(app);
    h.run_steps(2);

    println!("--- Initial state ---");
    println!("Space: {:?}", h.state().session.layout_space());
    println!("Session camera: {:?}", h.state().session.camera());
    println!("State camera: {:?}", h.state().session.state().unwrap().camera);
    println!("Drawing plane: {:?}", h.state().session.drawing_plane());
    println!("Elev offset: {:?}", h.state().session.effective_drawing_plane_offset());

    // Switch to Formato A4 vertical
    h.state_mut().session.execute("layout.set", &json!({ "name": "Formato A4 vertical" })).unwrap();
    h.run_steps(2);
    println!("--- In Layout Formato A4 vertical ---");
    println!("Space: {:?}", h.state().session.layout_space());
    println!("Session camera: {:?}", h.state().session.camera());
    println!("State camera: {:?}", h.state().session.state().unwrap().camera);

    // Enter MSPACE
    h.state_mut().session.execute("mspace", &json!({})).unwrap();
    h.run_steps(2);
    println!("--- In MSPACE ---");
    println!("Session camera: {:?}", h.state().session.camera());
    println!("State camera: {:?}", h.state().session.state().unwrap().camera);

    // Lock viewport
    let vh = h.state().session.state().unwrap().mspace.unwrap();
    h.state_mut().session.execute("viewport.set", &json!({ "handle": vh.hex(), "locked": true })).unwrap();
    h.run_steps(2);

    // Pan locked viewport: middle drag
    let center = h.state().canvas.rect.unwrap().center();
    h.event(Event::PointerButton { pos: center, button: egui::PointerButton::Middle, pressed: true, modifiers: Modifiers::NONE });
    h.event(Event::PointerMoved(center + vec2(100.0, -80.0)));
    h.run_steps(2);
    h.event(Event::PointerButton { pos: center + vec2(100.0, -80.0), button: egui::PointerButton::Middle, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    println!("--- After locked pan in MSPACE ---");
    println!("Session camera: {:?}", h.state().session.camera());
    println!("State camera: {:?}", h.state().session.state().unwrap().camera);

    // Switch back to Model
    h.state_mut().session.execute("layout.set", &json!({ "name": "Model" })).unwrap();
    h.run_steps(2);
    println!("--- Back in Model space ---");
    println!("Space: {:?}", h.state().session.layout_space());
    println!("Session camera: {:?}", h.state().session.camera());
    println!("State camera: {:?}", h.state().session.state().unwrap().camera);
    println!("Drawing plane: {:?}", h.state().session.drawing_plane());

    // Orbit into inclined view
    h.state_mut().session.orbit_step(120.0, -60.0).unwrap();
    h.run_steps(2);

    // Pan vertically in inclined view
    let center = h.state().canvas.rect.unwrap().center();
    h.event(Event::PointerButton { pos: center, button: egui::PointerButton::Middle, pressed: true, modifiers: Modifiers::NONE });
    h.event(Event::PointerMoved(center + vec2(0.0, 100.0)));
    h.run_steps(2);
    h.event(Event::PointerButton { pos: center + vec2(0.0, 100.0), button: egui::PointerButton::Middle, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    println!("--- After orbit AND pan in Model space ---");
    println!("Session camera: {:?}", h.state().session.camera());
    println!("State camera: {:?}", h.state().session.state().unwrap().camera);
    println!("Drawing plane: {:?}", h.state().session.drawing_plane());
    println!("Elev offset: {:?}", h.state().session.effective_drawing_plane_offset());
    println!("Manual plane offset? {:?}", h.state().session.drawing_plane_offset_is_manual());
    println!("Manual plane? {:?}", h.state().session.drawing_plane_is_manual());

    // Start LINE command
    h.state_mut().session.start("line").unwrap();
    h.run_steps(2);

    let p1_screen = center;
    h.event(Event::PointerMoved(p1_screen));
    h.run_steps(2);
    let c3_first = h.state().canvas.cursor3;
    println!("First cursor3: {:?}", c3_first);

    // Click first point
    h.event(Event::PointerButton { pos: p1_screen, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.event(Event::PointerButton { pos: p1_screen, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    let p2_screen = p1_screen + vec2(80.0, -50.0);
    h.event(Event::PointerMoved(p2_screen));
    h.run_steps(2);
    let c3_second = h.state().canvas.cursor3;
    println!("Second cursor3: {:?}", c3_second);

    // Click second point
    h.event(Event::PointerButton { pos: p2_screen, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.event(Event::PointerButton { pos: p2_screen, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    h.event(Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    let doc = h.state().session.doc().unwrap();
    let model_lines: Vec<_> = doc.model.iter().filter_map(|e| match &e.kind { EntityKind::Line(l) => Some(l.clone()), _ => None }).collect();
    assert_eq!(model_lines.len(), 1, "Model space line should be created");
    assert_eq!(model_lines[0].a.z, 0.0, "Model space line start Z must be 0.0");
    assert_eq!(model_lines[0].b.z, 0.0, "Model space line end Z must be 0.0");

    // Test drawing in MSPACE
    println!("--- Testing LINE in MSPACE ---");
    h.state_mut().session.execute("layout.set", &json!({ "name": "Formato A4 vertical" })).unwrap();
    h.run_steps(2);
    h.state_mut().session.execute("mspace", &json!({})).unwrap();
    h.run_steps(2);

    h.state_mut().session.start("line").unwrap();
    h.run_steps(2);
    let p1_screen = center;
    h.event(Event::PointerMoved(p1_screen));
    h.run_steps(2);
    println!("MSPACE line first cursor3: {:?}", h.state().canvas.cursor3);

    // Click first point
    h.event(Event::PointerButton { pos: p1_screen, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.event(Event::PointerButton { pos: p1_screen, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    let p2_screen = p1_screen + vec2(50.0, -30.0);
    h.event(Event::PointerMoved(p2_screen));
    h.run_steps(2);
    println!("MSPACE line second cursor3: {:?}", h.state().canvas.cursor3);

    h.event(Event::PointerButton { pos: p2_screen, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.event(Event::PointerButton { pos: p2_screen, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    h.event(Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    let doc = h.state().session.doc().unwrap();
    let mspace_lines: Vec<_> = doc.model.iter().filter_map(|e| match &e.kind { EntityKind::Line(l) => Some(l.clone()), _ => None }).collect();
    for (i, l) in mspace_lines.iter().enumerate() {
        println!("MSPACE line {i}: start = {:?}, end = {:?}", l.a, l.b);
        assert_eq!(l.a.z, 0.0, "MSPACE line start Z must be 0.0");
        assert_eq!(l.b.z, 0.0, "MSPACE line end Z must be 0.0");
    }
}

#[test]
fn test_multiple_consecutive_lines_in_slightly_tilted_view_with_shifted_camera_target() {
    let mut session = Session::empty();
    session.open_drawing(Drawing::new_metric(), "slightly_tilted_consecutive_lines", None);

    // Slightly tilted view: 5 degrees off plan view (elevation 85 deg, within 10 deg tolerance)
    let mut cam = session.camera();
    cam.elevation = 85.0f64.to_radians();
    cam.target = Vec3::new(100.0, 200.0, -2686.3);
    cam.height = 500.0;
    session.set_camera(cam);

    assert!(!session.camera().is_plan(), "Camera must not be perfectly in plan");
    assert_eq!(session.camera().principal_drawing_plane(), Some(cadcraft_engine::geom::DrawingPlane::XY), "Camera must still align within principal XY tolerance");

    let mut app = CadApp::new(session, Services::default());
    app.canvas.zoom_pending = false;
    let mut h = make_harness(app);

    // 1. Draw 3 consecutive connected line segments by clicking 4 points
    h.state_mut().session.start("line").unwrap();
    h.run_steps(2);

    let center = h.state().canvas.rect.unwrap().center();
    let click_positions = [
        center,
        center + vec2(60.0, -40.0),
        center + vec2(120.0, 20.0),
        center + vec2(40.0, 80.0),
    ];

    for pos in click_positions {
        h.event(Event::PointerMoved(pos));
        h.run_steps(2);
        let c3 = h.state().canvas.cursor3.expect("cursor resolves on ground");
        assert_eq!(c3.z, 0.0, "Point must resolve at Z=0.0 despite camera target Z=-2686.3");
        h.event(Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
        h.event(Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
        h.run_steps(2);
    }

    h.event(Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    let doc = h.state().session.doc().unwrap();
    let lines: Vec<_> = doc.model.iter().filter_map(|e| match &e.kind { EntityKind::Line(l) => Some(l.clone()), _ => None }).collect();
    assert_eq!(lines.len(), 3, "Expected 3 consecutive line segments");
    for (i, line) in lines.iter().enumerate() {
        assert_eq!(line.a.z, 0.0, "Segment {i} start Z must be 0.0, got {}", line.a.z);
        assert_eq!(line.b.z, 0.0, "Segment {i} end Z must be 0.0, got {}", line.b.z);
    }

    // 2. Now configure non-zero elevation offset 50.0 and draw 2 more segments
    h.state_mut().session.set_drawing_plane_offset(50.0);
    h.run_steps(2);

    h.state_mut().session.start("line").unwrap();
    h.run_steps(2);

    let click_positions_elev = [
        center + vec2(-50.0, -50.0),
        center + vec2(-100.0, 10.0),
        center + vec2(-20.0, 70.0),
    ];

    for pos in click_positions_elev {
        h.event(Event::PointerMoved(pos));
        h.run_steps(2);
        let c3 = h.state().canvas.cursor3.expect("cursor resolves on elevated plane");
        assert_eq!(c3.z, 50.0, "Point must resolve at manual elevation Z=50.0");
        h.event(Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
        h.event(Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
        h.run_steps(2);
    }

    h.event(Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    let doc = h.state().session.doc().unwrap();
    let all_lines: Vec<_> = doc.model.iter().filter_map(|e| match &e.kind { EntityKind::Line(l) => Some(l.clone()), _ => None }).collect();
    assert_eq!(all_lines.len(), 5, "Expected 5 total line segments (3 at Z=0, 2 at Z=50)");
    for (i, line) in all_lines[3..].iter().enumerate() {
        assert_eq!(line.a.z, 50.0, "Elevated segment {i} start Z must be 50.0, got {}", line.a.z);
        assert_eq!(line.b.z, 50.0, "Elevated segment {i} end Z must be 50.0, got {}", line.b.z);
    }
}

#[test]
fn test_template_layouts_consecutive_lines_inclined_and_panned() {
    let mut d = Drawing::new_metric();
    d.layouts.clear();
    for (idx, name) in TEMPLATE_LAYOUTS.iter().enumerate() {
        d.layouts.push(Layout::new(name, (idx + 1) as u32));
    }
    let mut session = Session::empty();
    session.open_drawing(d, "Template Drawing", None);

    assert_eq!(session.layout_space(), Space::Model);
    assert_eq!(session.drawing_plane(), cadcraft_engine::geom::DrawingPlane::XY);
    assert_eq!(session.effective_drawing_plane_offset(), 0.0);

    let mut app = CadApp::new(session, Services::default());
    app.canvas.zoom_pending = false;
    let mut h = make_harness(app);

    // Orbit into inclined view
    h.state_mut().session.orbit_step(100.0, -50.0).unwrap();
    h.run_steps(2);

    // Pan camera with middle mouse drag
    let center = h.state().canvas.rect.unwrap().center();
    h.event(Event::PointerButton { pos: center, button: egui::PointerButton::Middle, pressed: true, modifiers: Modifiers::NONE });
    h.event(Event::PointerMoved(center + vec2(80.0, 120.0)));
    h.run_steps(2);
    h.event(Event::PointerButton { pos: center + vec2(80.0, 120.0), button: egui::PointerButton::Middle, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    println!("Camera after orbit & pan: {:?}", h.state().session.camera());

    // Draw 3 consecutive lines
    h.state_mut().session.start("line").unwrap();
    h.run_steps(2);

    let points = [
        center,
        center + vec2(50.0, -30.0),
        center + vec2(100.0, 40.0),
        center + vec2(20.0, 90.0),
    ];

    for pos in points {
        h.event(Event::PointerMoved(pos));
        h.run_steps(2);
        let c3 = h.state().canvas.cursor3.expect("cursor resolves on ground in template");
        assert_eq!(c3.z, 0.0, "Point in template must resolve at Z=0.0");
        h.event(Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
        h.event(Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
        h.run_steps(2);
    }

    h.event(Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    let doc = h.state().session.doc().unwrap();
    let lines: Vec<_> = doc.model.iter().filter_map(|e| match &e.kind { EntityKind::Line(l) => Some(l.clone()), _ => None }).collect();
    assert_eq!(lines.len(), 3, "Expected 3 line segments drawn in template model space");
    for (i, line) in lines.iter().enumerate() {
        assert_eq!(line.a.z, 0.0, "Template line {i} start Z must be 0.0, got {}", line.a.z);
        assert_eq!(line.b.z, 0.0, "Template line {i} end Z must be 0.0, got {}", line.b.z);
    }

    // Now test switching to layout and back to model, then drawing
    h.state_mut().session.execute("layout.set", &json!({ "name": "Formato A4 vertical" })).unwrap();
    h.run_steps(2);
    h.state_mut().session.execute("layout.set", &json!({ "name": "Model" })).unwrap();
    h.run_steps(2);

    h.state_mut().session.start("line").unwrap();
    h.run_steps(2);

    let p1 = center + vec2(-40.0, -40.0);
    h.event(Event::PointerMoved(p1));
    h.run_steps(2);
    let c3_p1 = h.state().canvas.cursor3.expect("cursor resolves");
    assert_eq!(c3_p1.z, 0.0);
    h.event(Event::PointerButton { pos: p1, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.event(Event::PointerButton { pos: p1, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    let p2 = center + vec2(40.0, -40.0);
    h.event(Event::PointerMoved(p2));
    h.run_steps(2);
    let c3_p2 = h.state().canvas.cursor3.expect("cursor resolves");
    assert_eq!(c3_p2.z, 0.0);
    h.event(Event::PointerButton { pos: p2, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.event(Event::PointerButton { pos: p2, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    h.event(Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    let doc = h.state().session.doc().unwrap();
    let lines_after: Vec<_> = doc.model.iter().filter_map(|e| match &e.kind { EntityKind::Line(l) => Some(l.clone()), _ => None }).collect();
    assert_eq!(lines_after.len(), 4);
    assert_eq!(lines_after[3].a.z, 0.0);
    assert_eq!(lines_after[3].b.z, 0.0);
}





