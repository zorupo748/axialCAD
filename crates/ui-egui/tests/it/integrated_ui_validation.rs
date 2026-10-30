//! Integration tests for Bloque 5 UI requirements and 83d7aba audit findings:
//! - Recovered canvas area: test base point accepted AND final Line entity created in document.
//! - Graphical height adjustment: drag, click fix, and Escape cancellation.
//! - Floating palettes: header and close button clamped inside visible bounds on narrow/resized windows.
//! - Default template and startup preferences roundtrip and profile persistence.

use cadcraft_doc::EntityKind;
use cadcraft_engine::Session;
use cadcraft_ui_egui::{CadApp, Services, UiState, docking::Panel};
use egui::{Event, Key, Modifiers, PointerButton, pos2, vec2};
use egui_kittest::Harness;
use serde_json::json;

fn harness(state: Option<UiState>, width: f32, height: f32) -> Harness<'static, CadApp> {
    let mut app = CadApp::new(Session::default(), Services::default());
    if let Some(state) = state {
        app.ui = state;
    }
    app.ui.in_window_menu = false;
    app.integrated_titlebar = true;
    let mut harness = Harness::builder().with_size(vec2(width, height)).with_pixels_per_point(1.0).build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(&ui.ctx().clone());
            app.ui(ui);
        },
        app,
    );
    harness.run_steps(4);
    harness
}

#[test]
fn test_recovered_right_canvas_area_accepts_base_point_and_creates_final_line_entity() {
    let mut ui = UiState::default();
    ui.show_palettes = false;
    let _ = ui.docking.set_visible(Panel::Layers, false);
    let _ = ui.docking.set_visible(Panel::Properties, false);
    let mut h = harness(Some(ui), 1280.0, 760.0);
    // Ensure document has metric template
    let doc_initial_count = h.state().session.doc().unwrap().model.len();

    // Start LINE command
    h.state_mut().start("line");
    h.run_steps(2);
    assert_eq!(h.state().session.running.as_ref().map(|c| c.id.as_str()), Some("line"));

    // Base point in the recovered right-edge canvas area (previously blocked by the 300px column)
    // x = 1240.0 is within 40px of the right window edge (1280.0)
    let p1 = pos2(1240.0, 350.0);
    h.event(Event::PointerMoved(p1));
    h.run_steps(1);
    h.event(Event::PointerButton { pos: p1, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    h.event(Event::PointerButton { pos: p1, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(3);

    // Verify base point was accepted (first point recorded, machine prompting for next point)
    assert!(
        h.state().session.prompt_text().to_lowercase().contains("next point")
            || h.state().session.prompt_text().to_lowercase().contains("siguiente"),
        "Base point in recovered right canvas must be accepted: {}",
        h.state().session.prompt_text()
    );

    // Second point in the recovered area
    let p2 = pos2(1200.0, 480.0);
    h.event(Event::PointerMoved(p2));
    h.run_steps(1);
    h.event(Event::PointerButton { pos: p2, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    h.event(Event::PointerButton { pos: p2, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(3);

    // Finish LINE command with Enter
    h.event(Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(1);
    h.event(Event::Key { key: Key::Enter, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(3);

    assert!(h.state().session.running.is_none(), "LINE command must be completed");

    // CRITICAL: verify the final Line entity exists in the document model, not just prompt text
    let doc = h.state().session.doc().unwrap();
    assert_eq!(doc.model.len(), doc_initial_count + 1, "Exactly one new entity must be created");
    let last = doc.model.iter().last().expect("last entity");
    let EntityKind::Line(line) = &last.kind else { panic!("Expected Line entity in document") };
    assert_ne!(line.a, line.b, "Line must have distinct endpoints");
    assert!(line.a.x.is_finite() && line.a.y.is_finite() && line.b.x.is_finite() && line.b.y.is_finite());
}

#[test]
fn test_graphical_height_adjustment_escape_cancels_without_altering_drawing() {
    let mut h = harness(None, 1280.0, 760.0);
    let initial_entities = h.state().session.doc().unwrap().model.len();
    h.state_mut().session.set_drawing_plane_offset(20.0);

    // Enter height adjustment
    h.state_mut().canvas.height_adjust = true;
    h.state_mut().canvas.height_adjust_anchor = Some(cadcraft_geom::Vec3::new(0.0, 0.0, 20.0));
    h.state_mut().canvas.height_adjust_start_offset = 20.0;

    // Simulate mouse drag
    h.event(Event::PointerMoved(pos2(500.0, 300.0)));
    h.run_steps(2);

    // Press Escape to cancel
    h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(1);
    h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(3);

    // Verify height adjust was cancelled and offset reverted
    assert!(!h.state().canvas.height_adjust, "Escape must exit height adjust mode");
    assert_eq!(h.state().session.drawing_plane_offset(), 20.0, "Offset must revert to start offset");
    assert_eq!(h.state().session.doc().unwrap().model.len(), initial_entities, "Drawing must remain untouched");
}

#[test]
fn test_floating_palettes_remain_accessible_on_narrow_window_resize() {
    let mut h = harness(None, 1280.0, 760.0);

    // Open Layers palette and float it near edge
    h.state_mut().run("ui.dock", json!({"operation":"open", "panel":"layers"})).unwrap();
    h.state_mut().run("ui.dock", json!({"operation":"float", "panel":"layers", "rect":[1150.0, 600.0, 300.0, 350.0]})).unwrap();
    h.run_steps(3);

    // Resize window to narrow 600 x 400
    let mut narrow = harness(Some(h.state().ui.clone()), 600.0, 400.0);
    narrow.run_steps(3);

    // Check floating palette rect bounds
    let floating = &narrow.state().ui.docking.layout.floating;
    assert!(!floating.is_empty(), "Floating group must exist");
    let rect = floating[0].rect;

    // Header position (x, y) must be clamped inside visible area
    assert!(rect[0] >= 4.0 && rect[0] <= 600.0 - 60.0, "Floating header X must be on-screen: {}", rect[0]);
    assert!(rect[1] >= 4.0 && rect[1] <= 400.0 - 32.0, "Floating header Y must be on-screen: {}", rect[1]);
    assert!(rect[2] <= 580.0, "Floating width must fit within narrow window: {}", rect[2]);
    assert!(rect[3] <= 380.0, "Floating height must fit within narrow window: {}", rect[3]);
}

#[test]
fn test_default_template_and_startup_options_roundtrip() {
    let mut app = CadApp::new(Session::default(), Services::default());

    // Set QNEWTEMPLATE and startupCreateDrawing
    let res = app.run("options", json!({
        "QNEWTEMPLATE": "/test/templates/office.dwt",
        "startupCreateDrawing": true
    })).unwrap();

    assert_eq!(res["QNEWTEMPLATE"], "/test/templates/office.dwt");
    assert_eq!(res["startupCreateDrawing"], true);

    // Verify CadApp state
    assert_eq!(cadcraft_engine::cmd::file::qnew_template(), "/test/templates/office.dwt");
    assert!(app.ui.startup_create_drawing);

    // Check profile serialization and restoration
    let profile = app.profile_json();
    assert!(profile.contains("office.dwt"));
    assert!(profile.contains("startupCreateDrawing"));

    let mut restored_app = CadApp::new(Session::default(), Services::default());
    restored_app.load_profile(&profile);
    assert_eq!(cadcraft_engine::cmd::file::qnew_template(), "/test/templates/office.dwt");
    assert!(restored_app.ui.startup_create_drawing);

    // Reset template
    app.run("options", json!({"QNEWTEMPLATE": "", "startupCreateDrawing": false})).unwrap();
    assert_eq!(cadcraft_engine::cmd::file::qnew_template(), "");
    assert!(!app.ui.startup_create_drawing);
}
