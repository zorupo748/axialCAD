//! Integration tests for Bloque 5 UI requirements and 83d7aba audit findings:
//! - Recovered canvas area: test base point accepted AND final Line entity created in document.
//! - Graphical height adjustment: drag, click fix, and Escape cancellation.
//! - Floating palettes: header and close button clamped inside visible bounds on narrow/resized windows.
//! - Default template and startup preferences roundtrip and profile persistence.

use cadcraft_doc::EntityKind;
use cadcraft_engine::Session;
use cadcraft_ui_egui::{CadApp, Services, UiState, docking::Panel};
use egui::{Event, Key, Modifiers, PointerButton, pos2, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use serde_json::json;
use std::sync::Mutex;

static TEMPLATE_TEST_LOCK: Mutex<()> = Mutex::new(());

struct RestoreQnewTemplate(String);

impl Drop for RestoreQnewTemplate {
    fn drop(&mut self) {
        cadcraft_engine::cmd::file::set_qnew_template(&self.0);
    }
}

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
    h.state_mut().session.execute("view.swiso", &json!({})).unwrap();
    h.state_mut().session.set_drawing_plane(cadcraft_geom::DrawingPlane::XZ);
    h.state_mut().session.set_drawing_plane_offset(20.0);
    h.run_steps(2);

    h.state_mut().canvas.height_adjust = true;
    h.state_mut().canvas.height_adjust_anchor = Some(cadcraft_geom::Vec3::new(0.0, 20.0, 0.0));
    h.state_mut().canvas.height_adjust_start_offset = 20.0;
    h.state_mut().canvas.height_adjust_start_manual = true;
    h.state_mut().canvas.height_adjust_start_plane_manual = true;

    let target = h.state().canvas.xf.unwrap().to_screen3(cadcraft_geom::Vec3::new(0.0, 35.0, 0.0));
    h.event(Event::PointerMoved(target));
    h.run_steps(3);
    assert!((h.state().session.drawing_plane_offset() - 35.0).abs() < 1e-6, "XZ offset follows world Y");

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
fn test_graphical_height_adjustment_escape_restores_automatic_mode() {
    let mut h = harness(None, 1280.0, 760.0);
    h.state_mut().session.execute("view.swiso", &json!({})).unwrap();
    let mut camera = h.state().session.camera();
    camera.target.z = 20.0;
    h.state_mut().session.set_camera(camera);
    h.run_steps(2);
    assert!(!h.state().session.drawing_plane_offset_is_manual());
    assert!(!h.state().session.drawing_plane_is_manual());

    let active_plane = h.state().session.drawing_plane();
    h.state_mut().session.set_drawing_plane(active_plane);
    h.state_mut().session.set_drawing_plane_offset(0.0);
    h.state_mut().canvas.height_adjust = true;
    h.state_mut().canvas.height_adjust_anchor = Some(cadcraft_geom::Vec3::ZERO);
    let stored_offset = h.state().session.drawing_plane_offset();
    h.state_mut().canvas.height_adjust_start_offset = stored_offset;
    h.state_mut().canvas.height_adjust_start_manual = false;
    h.state_mut().canvas.height_adjust_start_plane_manual = false;

    let target = h.state().canvas.xf.unwrap().to_screen3(cadcraft_geom::Vec3::new(0.0, 0.0, 35.0));
    h.event(Event::PointerMoved(target));
    h.run_steps(3);
    assert!(h.state().session.drawing_plane_offset_is_manual(), "adjusting opts into manual mode");
    assert!(h.state().session.drawing_plane_is_manual(), "adjusting pins the automatically selected plane");
    assert!((h.state().session.effective_drawing_plane_offset() - 35.0).abs() < 1e-6);

    h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(1);
    h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(3);

    assert!(!h.state().canvas.height_adjust, "Escape exits manual adjustment");
    assert!(!h.state().session.drawing_plane_offset_is_manual(), "Escape restores automatic mode");
    assert!(!h.state().session.drawing_plane_is_manual(), "Escape restores automatic plane selection");
    assert_eq!(h.state().session.effective_drawing_plane_offset(), 0.0);
    assert_eq!(h.state().session.drawing_plane(), cadcraft_geom::DrawingPlane::XY);
}

#[test]
fn test_elevation_status_right_click_returns_to_automatic_mode() {
    let mut h = harness(None, 1280.0, 760.0);
    h.state_mut().session.execute("view.swiso", &json!({})).unwrap();
    h.state_mut().session.set_drawing_plane(cadcraft_geom::DrawingPlane::XZ);
    let mut camera = h.state().session.camera();
    camera.target.y = 20.0;
    h.state_mut().session.set_camera(camera);
    h.state_mut().session.set_drawing_plane_offset(35.0);
    h.run_steps(2);
    assert!(h.state().session.drawing_plane_offset_is_manual());

    let elev = h.get_by_role_and_label(egui::accesskit::Role::Button, "Elev: +35.00");
    let elev_rect = elev.rect();
    let plane_rect = h.get_by_role_and_label(egui::accesskit::Role::Button, "Plane: XZ").rect();
    let vlock_rect = h.get_by_role_and_label(egui::accesskit::Role::Button, "V-LOCK").rect();
    assert!(elev_rect.width() >= 70.0, "elevation widget bounds are too narrow: {elev_rect:?}");
    assert!(!elev_rect.intersects(plane_rect), "elevation label must not overlap the plane control");
    assert!(!elev_rect.intersects(vlock_rect), "elevation label must not overlap V-LOCK");
    elev.click_button(PointerButton::Secondary);
    h.run_steps(3);

    assert!(!h.state().session.drawing_plane_offset_is_manual());
    assert!(!h.state().session.drawing_plane_is_manual());
    assert_eq!(h.state().session.drawing_plane(), cadcraft_geom::DrawingPlane::XY);
    assert_eq!(h.state().session.effective_drawing_plane_offset(), 0.0);
}

#[test]
fn test_graphical_height_adjustment_click_keeps_the_new_plane_offset() {
    let mut h = harness(None, 1280.0, 760.0);
    h.state_mut().session.execute("view.swiso", &json!({})).unwrap();
    h.state_mut().session.set_drawing_plane(cadcraft_geom::DrawingPlane::YZ);
    h.state_mut().session.set_drawing_plane_offset(2.0);
    h.state_mut().session.start("line").unwrap();
    h.run_steps(2);

    h.state_mut().canvas.height_adjust = true;
    h.state_mut().canvas.height_adjust_anchor = Some(cadcraft_geom::Vec3::new(2.0, 0.0, 0.0));
    h.state_mut().canvas.height_adjust_start_offset = 2.0;
    let target = h.state().canvas.xf.unwrap().to_screen3(cadcraft_geom::Vec3::new(4.0, 0.0, 0.0));
    h.event(Event::PointerMoved(target));
    h.run_steps(3);
    assert!((h.state().session.drawing_plane_offset() - 4.0).abs() < 1e-6, "YZ offset follows world X");

    h.event(Event::PointerButton { pos: target, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    h.event(Event::PointerButton { pos: target, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(3);
    assert!(!h.state().canvas.height_adjust, "click confirms height adjustment");
    assert!((h.state().session.drawing_plane_offset() - 4.0).abs() < 1e-6, "confirmation keeps the selected offset");
    assert!(h.state().session.current_prompt().and_then(|prompt| prompt.base).is_none(), "confirmation click must not place LINE's first point");
}

#[test]
fn test_parallel_projection_clears_canvas_cursor_and_does_not_accept_stale_point() {
    let mut h = harness(None, 1280.0, 760.0);
    h.state_mut().session.execute("view.swiso", &json!({})).unwrap();
    h.state_mut().session.set_drawing_plane(cadcraft_geom::DrawingPlane::XZ);
    h.state_mut().session.set_drawing_plane_offset(9.0);
    h.state_mut().session.start("line").unwrap();
    h.run_steps(2);

    let xf = h.state().canvas.xf.unwrap();
    let world_point = xf.to_world3_with_offset(xf.rect.center(), cadcraft_geom::DrawingPlane::XZ, 9.0).unwrap();
    let target = xf.to_screen3(world_point);
    h.event(Event::PointerMoved(target));
    h.run_steps(2);
    assert!(h.state().canvas.cursor.is_some(), "isometric view provides a valid XZ cursor point");
    let last_valid = h.state().session.cursor3;

    h.state_mut().session.execute("view.top", &json!({})).unwrap();
    h.run_steps(2);
    assert!(h.state().canvas.cursor.is_none(), "top-view ray is parallel to XZ");
    assert_eq!(h.state().session.cursor3, last_valid, "the engine retains only the last real point");

    h.event(Event::PointerButton { pos: target, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    h.event(Event::PointerButton { pos: target, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
    assert!(h.state().session.current_prompt().and_then(|prompt| prompt.base).is_none(), "invalid click must not place the stale cursor");

    assert!(h.state_mut().cmdline("5,6").is_none());
    assert_eq!(h.state().session.last_point3, cadcraft_geom::DrawingPlane::XZ.embed(cadcraft_geom::Vec2::new(5.0, 6.0), 9.0));

    h.state_mut().session.execute("view.swiso", &json!({})).unwrap();
    h.run_steps(2);
    h.event(Event::PointerMoved(target));
    h.run_steps(2);
    assert!(h.state().canvas.cursor.is_some(), "cursor becomes valid again in isometric view");
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
    let _serial = TEMPLATE_TEST_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let _restore = RestoreQnewTemplate(cadcraft_engine::cmd::file::qnew_template());
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
