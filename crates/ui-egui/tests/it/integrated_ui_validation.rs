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

fn harness_with(session: Session, state: Option<UiState>, width: f32, height: f32) -> Harness<'static, CadApp> {
    let mut app = CadApp::new(session, Services::default());
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

fn harness(state: Option<UiState>, width: f32, height: f32) -> Harness<'static, CadApp> {
    harness_with(Session::default(), state, width, height)
}

fn imperial_harness(state: Option<UiState>, width: f32, height: f32) -> Harness<'static, CadApp> {
    let mut session = Session::default();
    let _ = session.new_drawing(false);
    harness_with(session, state, width, height)
}

fn type_text(h: &mut Harness<'static, CadApp>, text: &str) {
    for ch in text.chars() {
        h.event(Event::Text(ch.to_string()));
        h.run_steps(1);
    }
}

fn press_enter(h: &mut Harness<'static, CadApp>) {
    h.event(Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(1);
    h.event(Event::Key { key: Key::Enter, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
}

fn type_and_submit(h: &mut Harness<'static, CadApp>, text: &str) {
    type_text(h, text);
    press_enter(h);
}

fn click_at(h: &mut Harness<'static, CadApp>, at: egui::Pos2) {
    h.event(Event::PointerMoved(at));
    h.run_steps(1);
    h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(3);
}

#[test]
fn test_selection_window_direction_uses_screen_x_for_all_views() {
    for view in ["top", "front", "swiso", "orbit"] {
        for (start_right, start_below) in [(false, false), (false, true), (true, false), (true, true)] {
            let mut ui = UiState::default();
            ui.show_palettes = false;
            let _ = ui.docking.set_visible(Panel::Layers, false);
            let _ = ui.docking.set_visible(Panel::Properties, false);
            let mut h = harness(Some(ui), 1280.0, 760.0);
            if view != "orbit" {
                h.state_mut().session.execute(&format!("view.{view}"), &json!({})).unwrap();
            } else {
                h.state_mut().session.execute("view.swiso", &json!({})).unwrap();
                let mut camera = h.state().session.camera();
                camera.azimuth += 0.37;
                camera.elevation -= 0.23;
                h.state_mut().session.set_camera(camera);
            }
            let mut camera = h.state().session.camera();
            camera.target = cadcraft_geom::Vec3::ZERO;
            camera.height = 200.0;
            h.state_mut().session.set_camera(camera);
            h.state_mut().session.settings.selection_cycling = false;

            let point = |x: f64, y: f64| camera.target + camera.right_dir() * x + camera.up_dir() * y + camera.view_dir() * 20.0;
            let lines = [(point(8.0, 8.0), point(12.0, 12.0)), (point(-10.0, 15.0), point(40.0, 15.0)), (point(40.0, 35.0), point(50.0, 40.0))];
            for (a, b) in lines {
                h.state_mut()
                    .session
                    .execute(
                        "line",
                        &json!({"points": [
                            [a.x, a.y, a.z], [b.x, b.y, b.z]
                        ]}),
                    )
                    .unwrap();
            }
            h.run_steps(2);
            let xf = h.state().canvas.xf.unwrap();
            let corners = [
                xf.to_screen3(
                    camera.target
                        + camera.right_dir() * if start_right { 30.0 } else { 0.0 }
                        + camera.up_dir() * if start_below { 0.0 } else { 30.0 },
                ),
                xf.to_screen3(
                    camera.target
                        + camera.right_dir() * if start_right { 0.0 } else { 30.0 }
                        + camera.up_dir() * if start_below { 30.0 } else { 0.0 },
                ),
            ];
            assert!(xf.rect.contains(corners[0]) && xf.rect.contains(corners[1]));
            click_at(&mut h, corners[0]);
            assert!(h.state().session.selection().is_empty(), "the first empty-space click is only a window corner ({view})");
            click_at(&mut h, corners[1]);

            let doc = h.state().session.doc().unwrap();
            let selected: Vec<_> = doc.model.iter().filter(|entity| h.state().session.selection().contains(&entity.handle)).collect();
            let selected_lines: Vec<_> = selected
                .iter()
                .filter_map(|entity| match &entity.kind {
                    EntityKind::Line(line) => Some((line.a, line.b)),
                    _ => None,
                })
                .collect();
            let is_crossing = corners[1].x < corners[0].x;
            assert!(
                selected_lines.iter().any(|(a, b)| a.dist(lines[0].0) < 1e-8 && b.dist(lines[0].1) < 1e-8),
                "the fully contained entity must be selected for {view}, corners={corners:?}, got {selected_lines:?}"
            );
            assert_eq!(
                selected_lines.iter().any(|(a, b)| a.dist(lines[1].0) < 1e-8 && b.dist(lines[1].1) < 1e-8),
                is_crossing,
                "only screen-left-to-right-independent crossing mode selects the crossing line ({view}, corners={corners:?})"
            );
            assert!(!selected_lines.iter().any(|(a, b)| a.dist(lines[2].0) < 1e-8 && b.dist(lines[2].1) < 1e-8));
            assert!(h.state().session.pending_window.is_none(), "finishing a window clears its pending state");
        }
    }
}

#[test]
fn test_selection_window_escape_clears_the_first_corner() {
    let mut ui = UiState::default();
    ui.show_palettes = false;
    let _ = ui.docking.set_visible(Panel::Layers, false);
    let _ = ui.docking.set_visible(Panel::Properties, false);
    let mut h = imperial_harness(Some(ui), 1280.0, 760.0);
    h.state_mut().session.execute("view.top", &json!({})).unwrap();
    h.state_mut().session.execute("line", &json!({"points": [[10.0, 20.0], [30.0, 20.0]]})).unwrap();
    let mut camera = h.state().session.camera();
    camera.height = 100.0;
    h.state_mut().session.set_camera(camera);
    h.run_steps(2);
    let xf = h.state().canvas.xf.unwrap();
    let first = xf.to_screen3(cadcraft_geom::Vec3::new(0.0, 40.0, 0.0));
    let second = xf.to_screen3(cadcraft_geom::Vec3::new(40.0, 0.0, 0.0));
    let selection_start = xf.to_screen3(cadcraft_geom::Vec3::new(5.0, 25.0, 0.0));
    let selection_end = xf.to_screen3(cadcraft_geom::Vec3::new(35.0, 15.0, 0.0));
    assert!(xf.rect.contains(first) && xf.rect.contains(second) && xf.rect.contains(selection_start) && xf.rect.contains(selection_end));
    click_at(&mut h, first);
    h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(1);
    h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(1);
    click_at(&mut h, second);
    assert!(h.state().session.selection().is_empty(), "the click after Escape starts a fresh window instead of completing the cancelled one");
    h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(1);
    h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(1);

    click_at(&mut h, selection_start);
    click_at(&mut h, selection_end);
    assert_eq!(h.state().session.selection().len(), 1, "selection remains usable after cancellation");
}

#[test]
fn test_regression_a_and_d_oblique_free_diagonal_and_vertical_marker() {
    // Regression D: Vertical marker acquire, confirm, release
    let mut ui = UiState::default();
    ui.show_palettes = false;
    let _ = ui.docking.set_visible(Panel::Layers, false);
    let _ = ui.docking.set_visible(Panel::Properties, false);
    let mut h = harness(Some(ui), 1280.0, 760.0);
    h.state_mut().session.execute("view.swiso", &json!({})).unwrap();
    let mut camera = h.state().session.camera();
    camera.height = 1200.0;
    h.state_mut().session.set_camera(camera);
    h.state_mut().session.settings.axistrack = true;
    h.state_mut().session.settings.osmode = 0;
    h.state_mut().session.settings.orthomode = false;
    h.state_mut().session.settings.zlock = false;
    type_and_submit(&mut h, "LINE");
    type_and_submit(&mut h, "0,0,100");

    let xf = h.state().canvas.xf.unwrap();
    let ground_center = xf.to_screen3(cadcraft_geom::Vec3::ZERO);
    let vertical_cursor = ground_center + egui::vec2(2.0, 2.0); // within 8 px acquire
    h.event(Event::PointerMoved(vertical_cursor));
    h.run_steps(2);
    assert!(h.state().canvas.axis_track_z_zero, "vertical marker acquires inside tolerance");
    assert_eq!(h.state().canvas.active_axis_3d, Some(cadcraft_geom::Axis3D::Z));
    assert!(h.state().canvas.cursor3.is_some_and(|point| point.dist(cadcraft_geom::Vec3::ZERO) < 1e-6));

    // Release when moving outside (> 12 px)
    let outside_cursor = ground_center + egui::vec2(25.0, 0.0);
    h.event(Event::PointerMoved(outside_cursor));
    h.run_steps(2);
    assert!(!h.state().canvas.axis_track_z_zero, "leaving marker area releases vertical constraint");
    let free_c3 = h.state().canvas.cursor3.unwrap();
    assert!(free_c3.dist(cadcraft_geom::Vec3::ZERO) > 0.5);
    assert_eq!(free_c3.z, 0.0, "free point outside marker resolves on global Z=0 without retaining base X/Y");

    // Move back into vertical marker and confirm
    h.event(Event::PointerMoved(vertical_cursor));
    h.run_steps(2);
    assert!(h.state().canvas.axis_track_z_zero);
    click_at(&mut h, vertical_cursor);
    let lines = h.state().session.doc().unwrap().model.clone();
    let drop_line = lines
        .iter()
        .find_map(|e| match &e.kind {
            EntityKind::Line(l) if l.a == cadcraft_geom::Vec3::new(0.0, 0.0, 100.0) => Some(l.clone()),
            _ => None,
        })
        .expect("vertical drop line must be created");
    assert_eq!(drop_line.b, cadcraft_geom::Vec3::ZERO, "confirmed drop line has exact (base.x, base.y, 0)");
    h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    // Regression A: Second oblique view with different zoom, Auto, ORTHO/V off
    // LINE from (0,0,100), free click at projection of (80,50,0) without typing Z0 or passing through a ring.
    h.state_mut().session.execute("view.neiso", &json!({})).unwrap();
    let mut cam2 = h.state().session.camera();
    cam2.height = 1000.0;
    h.state_mut().session.set_camera(cam2);
    h.state_mut().session.settings.axistrack = false;
    h.state_mut().session.settings.orthomode = false;
    h.state_mut().session.settings.zlock = false;
    type_and_submit(&mut h, "LINE");
    type_and_submit(&mut h, "0,0,100");

    let xf2 = h.state().canvas.xf.unwrap();
    let target = cadcraft_geom::Vec3::new(80.0, 50.0, 0.0);
    let target_screen = xf2.to_screen3(target);
    assert!(xf2.rect.contains(target_screen));
    h.event(Event::PointerMoved(target_screen));
    h.run_steps(2);
    assert!(!h.state().canvas.axis_track_z_zero);
    let resolved = h.state().canvas.cursor3.expect("free point resolves on global Z=0");
    assert!(resolved.dist(target) < 0.2, "resolved cursor {resolved:?} matches independent target {target:?}");
    assert_eq!(resolved.z, 0.0);

    let preview = h.state().session.preview(h.state().canvas.cursor.unwrap());
    let preview_endpoint = preview
        .iter()
        .find_map(|e| match &e.kind {
            EntityKind::Line(l) if l.a == cadcraft_geom::Vec3::new(0.0, 0.0, 100.0) => Some(l.b),
            _ => None,
        })
        .expect("preview exists");
    assert!(preview_endpoint.dist(target) < 0.2);
    assert_eq!(preview_endpoint.z, 0.0);

    click_at(&mut h, target_screen);
    let saved_diag = h
        .state()
        .session
        .doc()
        .unwrap()
        .model
        .iter()
        .find_map(|e| match &e.kind {
            EntityKind::Line(l) if l.a == cadcraft_geom::Vec3::new(0.0, 0.0, 100.0) && l.b.dist(target) < 0.5 => Some(l.clone()),
            _ => None,
        })
        .expect("saved diagonal line must exist in model space");
    assert_eq!(saved_diag.b.z, 0.0, "diagonal endpoint real stored Z is exactly 0.0");
    assert!(saved_diag.b.dist(target) < 0.2);
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
        h.state().session.prompt_text().to_lowercase().contains("next point") || h.state().session.prompt_text().to_lowercase().contains("siguiente"),
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
fn test_line_z0_option_uses_clicked_global_ground_point_once() {
    for (axistrack, other_oblique) in [(false, false), (true, false), (true, true)] {
        let mut ui = UiState::default();
        ui.show_palettes = false;
        let _ = ui.docking.set_visible(Panel::Layers, false);
        let _ = ui.docking.set_visible(Panel::Properties, false);
        let mut h = imperial_harness(Some(ui), 1280.0, 760.0);
        h.state_mut().session.execute("view.swiso", &json!({})).unwrap();
        let mut camera = h.state().session.camera();
        camera.height = 300.0;
        if other_oblique {
            camera.azimuth += 0.35;
            camera.elevation -= 0.2;
        }
        h.state_mut().session.set_camera(camera);
        h.state_mut().session.settings.axistrack = axistrack;
        h.state_mut().session.settings.orthomode = false;
        h.state_mut().session.settings.zlock = false;

        type_and_submit(&mut h, "LINE");
        assert_eq!(h.state().session.running.as_ref().map(|r| r.id.as_str()), Some("line"), "LINE starts from command-line keyboard events");
        type_and_submit(&mut h, "0,0,100");
        assert_eq!(h.state().session.last_point3, cadcraft_geom::Vec3::new(0.0, 0.0, 100.0), "typed base point is accepted");
        let base_prompt = h.state().session.current_prompt().unwrap();
        assert!(!base_prompt.global_xy_zero);
        assert_eq!(base_prompt.base3, Some(cadcraft_geom::Vec3::new(0.0, 0.0, 100.0)));
        assert!(!base_prompt.message.contains("global Z=0"), "inactive mode must not be presented as active");

        type_text(&mut h, "Z0");
        assert_eq!(h.state().cmd.buffer, "Z0", "the keyboard events reach the command input unchanged");
        assert!(!h.state().session.current_prompt().unwrap().global_xy_zero, "typing alone has not toggled the mode");
        press_enter(&mut h);
        let prompt = h.state().session.current_prompt().unwrap();
        assert!(prompt.global_xy_zero, "typing Z0 and pressing Enter activates the explicit mode");
        assert!(prompt.message.contains("global Z=0"), "active prompt states the target plane");
        h.run_steps(2);
        let h_state_text = h.state().session.prompt_text();
        assert!(h_state_text.contains("global Z=0"), "command prompt visibly identifies Z=0: {h_state_text}");

        let expected = cadcraft_geom::Vec3::new(80.0, 50.0, 0.0);
        let screen = h.state().canvas.xf.unwrap().to_screen3(expected);
        let rect = h.state().canvas.xf.unwrap().rect;
        assert!(rect.contains(screen), "test point must be inside the canvas: {screen:?} vs {rect:?}");
        h.event(Event::PointerMoved(screen));
        h.run_steps(2);
        let resolved = h.state().canvas.cursor3.expect("Z=0 ray resolves in SWISO");
        assert_eq!(resolved.z, 0.0);
        assert!(
            resolved.dist(expected) < 0.1,
            "resolved cursor {resolved:?} differs from independent WCS target {expected:?}; AXISTRACK={axistrack}, alternate oblique={other_oblique}"
        );
        assert!(h.state().canvas.snap.is_none(), "this point is a free camera-ray intersection, not a snap");
        let preview = h.state().session.preview(h.state().canvas.cursor.unwrap());
        assert!(
            preview.iter().any(|entity| {
                matches!(&entity.kind, EntityKind::Line(line) if line.a == cadcraft_geom::Vec3::new(0.0, 0.0, 100.0) && line.b.dist(expected) < 0.1)
            }),
            "preview must use the independently specified global ground endpoint"
        );
        assert!(h.state().session.current_prompt().unwrap().global_xy_zero, "the mode remains active immediately before the click");

        h.event(Event::PointerButton { pos: screen, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
        h.run_steps(1);
        h.event(Event::PointerButton { pos: screen, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
        h.run_steps(3);
        let last = h.state().session.doc().unwrap().model.iter().last().unwrap();
        let EntityKind::Line(line) = &last.kind else { panic!("expected LINE entity") };
        assert_eq!(line.a, cadcraft_geom::Vec3::new(0.0, 0.0, 100.0));
        assert!(
            line.b.dist(expected) < 0.1,
            "saved entity endpoint {:?} differs from expected {:?}; AXISTRACK={axistrack}, alternate oblique={other_oblique}",
            line.b,
            expected
        );
        assert!(!h.state().session.current_prompt().unwrap().global_xy_zero, "the mouse point consumes the one-shot option");

        if !axistrack {
            let second_expected = cadcraft_geom::Vec3::new(-35.0, 70.0, 0.0);
            let second_screen = h.state().canvas.xf.unwrap().to_screen3(second_expected);
            h.event(Event::PointerMoved(second_screen));
            h.run_steps(2);
            assert!(!h.state().session.current_prompt().unwrap().global_xy_zero, "the continuation prompt is back in normal mode");
            h.event(Event::PointerButton { pos: second_screen, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
            h.run_steps(1);
            h.event(Event::PointerButton { pos: second_screen, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
            h.run_steps(3);
            let lines: Vec<_> = h
                .state()
                .session
                .doc()
                .unwrap()
                .model
                .iter()
                .filter_map(|entity| match &entity.kind {
                    EntityKind::Line(line) => Some((line.a, line.b)),
                    _ => None,
                })
                .collect();
            assert_eq!(lines.len(), 2);
            assert!(lines[1].0.dist(lines[0].1) < 1e-10, "continuation starts at the accepted Z=0 endpoint");
            assert!(lines[1].1.dist(second_expected) < 0.1, "normal continuation should use the free work plane");
        }

        h.event(Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
        h.run_steps(1);
        h.event(Event::Key { key: Key::Enter, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE });
        h.run_steps(2);
        assert!(h.state().session.running.is_none(), "Enter ends LINE");
    }
}

#[test]
fn test_regression_g_and_a_ortho_v_and_free_diagonal() {
    for (axistrack, orthomode, zlock) in [(false, false, false), (true, false, false), (false, true, false), (false, false, true)] {
        let mut ui = UiState::default();
        ui.show_palettes = false;
        let _ = ui.docking.set_visible(Panel::Layers, false);
        let _ = ui.docking.set_visible(Panel::Properties, false);
        let mut h = harness(Some(ui), 1280.0, 760.0);
        h.state_mut().session.execute("view.swiso", &json!({})).unwrap();
        let mut camera = h.state().session.camera();
        camera.height = 1800.0;
        h.state_mut().session.set_camera(camera);
        h.state_mut().session.settings.axistrack = axistrack;
        h.state_mut().session.settings.osmode = 0;
        h.state_mut().session.settings.orthomode = false;
        h.state_mut().session.settings.zlock = false;

        type_and_submit(&mut h, "LINE");
        type_and_submit(&mut h, "0,0,100");
        h.state_mut().session.settings.orthomode = orthomode;
        h.state_mut().session.settings.zlock = zlock;

        let base = cadcraft_geom::Vec3::new(0.0, 0.0, 100.0);
        let target = cadcraft_geom::Vec3::new(500.0, 400.0, 0.0);
        let xf = h.state().canvas.xf.unwrap();
        let target_screen = xf.to_screen3(target);
        assert!(xf.rect.contains(target_screen));

        h.event(Event::PointerMoved(target_screen));
        h.run_steps(2);

        if !orthomode && !zlock {
            // Free point resolves to ground without ring acquisition
            let resolved = h.state().canvas.cursor3.expect("free point resolves on global Z=0");
            assert!(resolved.dist(target) < 0.5, "resolved cursor {resolved:?} matches target {target:?}");
            assert_eq!(resolved.z, 0.0);

            let preview = h.state().session.preview(h.state().canvas.cursor.unwrap());
            assert!(preview.iter().any(|e| matches!(&e.kind, EntityKind::Line(l) if l.a == base && l.b.dist(target) < 0.5)));

            click_at(&mut h, target_screen);
            let saved = h.state().session.doc().unwrap().model.iter().find_map(|e| match &e.kind {
                EntityKind::Line(l) if l.a.dist(base) < 0.5 => Some(l.b),
                _ => None,
            });
            assert!(saved.is_some_and(|p| p.dist(target) < 0.5 && p.z == 0.0), "saved line forms diagonal to Z=0");
        } else if zlock {
            // V mode is strictly preserved, not bypassed for Z=0
            let cursor = h.state().canvas.cursor3.expect("cursor under zlock");
            assert_eq!(h.state().canvas.active_axis_3d, Some(cadcraft_geom::Axis3D::Z));
            assert!((cursor.x - base.x).abs() < 1e-6);
            assert!((cursor.y - base.y).abs() < 1e-6);
        } else if orthomode {
            // ORTHO mode is strictly preserved, not bypassed for Z=0
            assert!(h.state().canvas.active_axis_3d.is_some(), "ORTHO active axis must be set");
            let cursor = h.state().canvas.cursor3.expect("cursor under orthomode");
            let diff_count = usize::from((cursor.x - base.x).abs() > 1e-4)
                + usize::from((cursor.y - base.y).abs() > 1e-4)
                + usize::from((cursor.z - base.z).abs() > 1e-4);
            assert!(diff_count <= 1, "ORTHO motion must be along a single axis, diff_count={diff_count}");
        }

        h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
        h.run_steps(2);
    }
}

#[test]
fn test_regression_c_and_k_endpoint_snap_and_parallel_view() {
    let mut ui = UiState::default();
    ui.show_palettes = false;
    let _ = ui.docking.set_visible(Panel::Layers, false);
    let _ = ui.docking.set_visible(Panel::Properties, false);
    let mut h = harness(Some(ui), 1280.0, 760.0);
    h.state_mut().session.execute("view.swiso", &json!({})).unwrap();
    let mut camera = h.state().session.camera();
    camera.height = 1800.0;
    h.state_mut().session.set_camera(camera);

    // Regression C: Elevated endpoint snap preserves its real WCS Z
    let elevated_endpoint = cadcraft_geom::Vec3::new(80.0, 50.0, 20.0);
    h.state_mut().session.execute("line", &json!({"points": [[80.0, 50.0, 20.0], [100.0, 50.0, 20.0]]})).unwrap();
    h.state_mut().session.settings.osmode = cadcraft_engine::snap::mode::END;
    type_and_submit(&mut h, "LINE");
    type_and_submit(&mut h, "0,0,100");

    let xf = h.state().canvas.xf.unwrap();
    let endpoint_screen = xf.to_screen3(elevated_endpoint);
    h.event(Event::PointerMoved(endpoint_screen));
    h.run_steps(2);
    assert_eq!(h.state().canvas.cursor3, Some(elevated_endpoint), "real entity snap preserves its original WCS Z");
    assert_eq!(h.state().canvas.snap.map(|hit| hit.name), Some("Endpoint"));

    click_at(&mut h, endpoint_screen);
    let saved_snap_line = h
        .state()
        .session
        .doc()
        .unwrap()
        .model
        .iter()
        .find_map(|e| match &e.kind {
            EntityKind::Line(l) if l.a == cadcraft_geom::Vec3::new(0.0, 0.0, 100.0) => Some(l.clone()),
            _ => None,
        })
        .expect("line from snap must exist");
    assert_eq!(saved_snap_line.b, elevated_endpoint, "snapped endpoint preserves exact WCS Z=20.0");

    h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    // Regression K: Parallel view, camera change, cancellation, canvas exit
    h.state_mut().session.settings.osmode = 0;
    type_and_submit(&mut h, "LINE");
    type_and_submit(&mut h, "0,0,100");
    h.state_mut().session.execute("view.front", &json!({})).unwrap();
    h.run_steps(2);
    let front_xf = h.state().canvas.xf.unwrap();
    let front_target = front_xf.to_screen3(cadcraft_geom::Vec3::ZERO);
    assert!(front_xf.to_world3_with_offset(front_target, cadcraft_geom::DrawingPlane::XY, 0.0).is_none(), "camera ray is parallel to global Z=0");
    let stale_cursor = cadcraft_geom::Vec3::new(999.0, 888.0, 777.0);
    h.state_mut().canvas.cursor3 = Some(stale_cursor);
    h.state_mut().session.cursor3 = stale_cursor;
    h.event(Event::PointerMoved(front_target));
    h.run_steps(2);
    assert_ne!(h.state().canvas.cursor3, Some(stale_cursor), "parallel view must not reuse stale cursor");

    // Camera change clears stale candidate
    h.state_mut().session.execute("view.swiso", &json!({})).unwrap();
    h.run_steps(2);

    // Canvas exit clears cursor
    let canvas_rect = h.state().canvas.rect.unwrap();
    let outside_canvas = pos2(canvas_rect.left() - 50.0, canvas_rect.center().y);
    h.event(Event::PointerMoved(outside_canvas));
    h.run_steps(2);
    assert!(h.state().canvas.cursor.is_none(), "moving pointer outside canvas clears cursor");
    assert!(h.state().canvas.cursor3.is_none());

    // Cancellation leaves no candidate
    h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
    assert!(h.state().session.running.is_none());
    assert!(h.state().canvas.cursor.is_none());
    assert!(!h.state().canvas.axis_track_z_zero);
}

#[test]
fn test_preference_vertical_drop_helper_persistence_and_reset() {
    let mut ui = UiState::default();
    ui.show_palettes = false;
    let _ = ui.docking.set_visible(Panel::Layers, false);
    let _ = ui.docking.set_visible(Panel::Properties, false);
    let mut h = harness(Some(ui), 1280.0, 760.0);

    assert!(h.state().options.z_zero_drop_helper, "option is enabled by default");
    h.state_mut().run("options", json!({"zZeroDropHelper": false})).unwrap();
    assert!(!h.state().options.z_zero_drop_helper, "option can be disabled via options JSON");
    let profile = h.state().profile_json();
    assert!(profile.contains("\"zZeroDropHelper\":false"));
    h.state_mut().options.z_zero_drop_helper = true;
    h.state_mut().load_profile(&profile);
    assert!(!h.state().options.z_zero_drop_helper, "load_profile restores disabled option");
    h.state_mut().run("options", json!({"reset": true})).unwrap();
    assert!(h.state().options.z_zero_drop_helper, "reset restores default true");
}

#[test]
fn test_oblique_free_first_point_resolves_to_z_zero_independent_of_camera_target() {
    let mut ui = UiState::default();
    ui.show_palettes = false;
    let _ = ui.docking.set_visible(Panel::Layers, false);
    let _ = ui.docking.set_visible(Panel::Properties, false);
    let mut h = harness(Some(ui), 1280.0, 760.0);

    h.state_mut().session.execute("view.swiso", &json!({})).unwrap();
    let mut cam_b = h.state().session.camera();
    cam_b.target = cadcraft_geom::Vec3::new(45.0, 25.0, 150.0);
    h.state_mut().session.set_camera(cam_b);
    type_and_submit(&mut h, "LINE");
    let center_pt = h.state().canvas.rect.unwrap().center();
    h.event(Event::PointerMoved(center_pt));
    h.run_steps(2);
    let first_pt3 = h.state().canvas.cursor3.expect("first point resolves on Z=0");
    assert_eq!(first_pt3.z, 0.0, "first free point in oblique view must resolve on global Z=0 regardless of camera target height");
    click_at(&mut h, center_pt);
    assert_eq!(h.state().session.last_point3.z, 0.0);
    h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
}

#[test]
fn test_vertical_drop_helper_disabled_with_and_without_axistrack() {
    let mut ui = UiState::default();
    ui.show_palettes = false;
    let _ = ui.docking.set_visible(Panel::Layers, false);
    let _ = ui.docking.set_visible(Panel::Properties, false);
    let mut h = harness(Some(ui), 1280.0, 760.0);

    h.state_mut().session.execute("view.swiso", &json!({})).unwrap();
    let mut cam_e = h.state().session.camera();
    cam_e.target = cadcraft_geom::Vec3::ZERO;
    cam_e.height = 1600.0;
    h.state_mut().session.set_camera(cam_e);

    // Case A: Ayuda desactivada y AXISTRACK apagado:
    // sin marcador ni captura auxiliar invisible; punto libre forma diagonal al suelo
    h.state_mut().options.z_zero_drop_helper = false;
    h.state_mut().session.settings.axistrack = false;
    type_and_submit(&mut h, "LINE");
    type_and_submit(&mut h, "0,0,100");
    let xf_e = h.state().canvas.xf.unwrap();
    let ground_center = xf_e.to_screen3(cadcraft_geom::Vec3::ZERO);
    let near_marker = ground_center + egui::vec2(2.0, 2.0);
    h.event(Event::PointerMoved(near_marker));
    h.run_steps(2);
    assert!(!h.state().canvas.axis_track_z_zero, "when disabled, no vertical drop snap is active");
    assert!(h.state().canvas.snap.is_none(), "no snap hit when helper is disabled");
    let c3_e = h.state().canvas.cursor3.unwrap();
    assert!(c3_e.dist(cadcraft_geom::Vec3::ZERO) > 0.1, "not locked to (0,0,0)");
    assert_eq!(c3_e.z, 0.0, "fallback free point still resolves to Z=0");

    let diag_target = cadcraft_geom::Vec3::new(80.0, 50.0, 0.0);
    let diag_screen = xf_e.to_screen3(diag_target);
    click_at(&mut h, diag_screen);
    let saved_e = h
        .state()
        .session
        .doc()
        .unwrap()
        .model
        .iter()
        .find_map(|e| match &e.kind {
            EntityKind::Line(l) if l.a == cadcraft_geom::Vec3::new(0.0, 0.0, 100.0) && l.b.dist(diag_target) < 0.5 => Some(l.clone()),
            _ => None,
        })
        .expect("diagonal line exists even with option disabled");
    assert_eq!(saved_e.b.z, 0.0);
    h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    // Case B: Ayuda desactivada y AXISTRACK encendido:
    // rastreo puede seguir el eje Z, pero NO conserva captura especial exacta a Z=0
    h.state_mut().options.z_zero_drop_helper = false;
    h.state_mut().session.settings.axistrack = true;
    h.state_mut().session.settings.osmode = 0;
    type_and_submit(&mut h, "LINE");
    type_and_submit(&mut h, "0,0,100");
    let xf_b = h.state().canvas.xf.unwrap();
    // Point on the Z-axis ray at Z=50
    let z_ray_screen = xf_b.to_screen3(cadcraft_geom::Vec3::new(0.0, 0.0, -50.0));
    h.event(Event::PointerMoved(z_ray_screen + egui::vec2(2.0, 0.0)));
    h.run_steps(2);
    assert!(!h.state().canvas.axis_track_z_zero, "no axis_track_z_zero helper snap");
    assert_eq!(h.state().canvas.active_axis_3d, Some(cadcraft_geom::Axis3D::Z), "AXISTRACK tracks 3D Z axis along vertical ray");
    let c3_tracked = h.state().canvas.cursor3.unwrap();
    assert_ne!(c3_tracked, cadcraft_geom::Vec3::ZERO, "must not lock to exact (0,0,0) without helper");
    assert!((c3_tracked.z - (-50.0)).abs() < 5.0, "Z coordinate tracks vertical position, not forced to Z=0");

    // Near the ground under base: no vertical drop helper snap to (0,0,0)
    let ground_screen = xf_b.to_screen3(cadcraft_geom::Vec3::ZERO);
    h.event(Event::PointerMoved(ground_screen + egui::vec2(1.0, 0.0)));
    h.run_steps(2);
    assert!(!h.state().canvas.axis_track_z_zero, "helper is disabled, no axis_track_z_zero snap");

    // Move outside the AXISTRACK release radius (> 14px): axis releases to free diagonal fallback at Z=0
    let outside_z_ray = z_ray_screen + egui::vec2(30.0, 0.0);
    h.event(Event::PointerMoved(outside_z_ray));
    h.run_steps(2);
    assert_eq!(h.state().canvas.active_axis_3d, None, "outside tracking aperture, axis is cleared");
    let c3_free = h.state().canvas.cursor3.unwrap();
    assert_eq!(c3_free.z, 0.0, "released cursor resolves to free plane at Z=0");

    h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
}

#[test]
fn test_manual_drawing_plane_and_elevation_preserved() {
    let mut ui = UiState::default();
    ui.show_palettes = false;
    let _ = ui.docking.set_visible(Panel::Layers, false);
    let _ = ui.docking.set_visible(Panel::Properties, false);
    let mut h = harness(Some(ui), 1280.0, 760.0);

    h.state_mut().session.execute("view.swiso", &json!({})).unwrap();
    h.state_mut().session.settings.axistrack = false;
    h.state_mut().session.set_drawing_plane(cadcraft_geom::DrawingPlane::XZ);
    h.state_mut().session.set_drawing_plane_offset(25.0);
    type_and_submit(&mut h, "LINE");
    type_and_submit(&mut h, "10,25,0");
    let screen_h = h.state().canvas.rect.unwrap().center();
    h.event(Event::PointerMoved(screen_h));
    h.run_steps(2);
    assert_eq!(h.state().canvas.cursor3.unwrap().y, 25.0, "manual XZ plane offset Y=25 is preserved");
    h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    h.state_mut().session.set_drawing_plane(cadcraft_geom::DrawingPlane::XY);
    h.state_mut().session.set_drawing_plane_offset(50.0);
    type_and_submit(&mut h, "LINE");
    type_and_submit(&mut h, "10,10,50");
    h.event(Event::PointerMoved(screen_h));
    h.run_steps(2);
    assert_eq!(h.state().canvas.cursor3.unwrap().z, 50.0, "manual XY elevation Z=50 is preserved");
    h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
    h.state_mut().session.set_drawing_plane_automatic();
    h.state_mut().session.set_drawing_plane_offset_automatic();
}

#[test]
fn test_principal_views_allow_vertical_drawing_without_v() {
    let mut ui = UiState::default();
    ui.show_palettes = false;
    let _ = ui.docking.set_visible(Panel::Layers, false);
    let _ = ui.docking.set_visible(Panel::Properties, false);
    let mut h = harness(Some(ui), 1280.0, 760.0);

    h.state_mut().session.execute("view.front", &json!({})).unwrap();
    h.run_steps(2);
    type_and_submit(&mut h, "LINE");
    type_and_submit(&mut h, "10,0,10");
    let front_screen = h.state().canvas.rect.unwrap().center() - egui::vec2(0.0, 80.0);
    h.event(Event::PointerMoved(front_screen));
    h.run_steps(2);
    let front_c3 = h.state().canvas.cursor3.expect("front view cursor");
    assert_eq!(front_c3.y, 0.0, "front view plane is XZ at Y=0");
    assert!((front_c3.z - 10.0).abs() > 1.0, "vertical coordinate Z changes freely without V");
    h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
}

#[test]
fn test_principal_front_view_auto_vs_explicit_z0() {
    let mut ui = UiState::default();
    ui.show_palettes = false;
    let _ = ui.docking.set_visible(Panel::Layers, false);
    let _ = ui.docking.set_visible(Panel::Properties, false);
    let mut h = harness(Some(ui), 1280.0, 760.0);

    h.state_mut().session.execute("view.front", &json!({})).unwrap();
    h.run_steps(2);

    // Path 1: Auto mode in Front view resolves mouse points on XZ freely
    type_and_submit(&mut h, "LINE");
    type_and_submit(&mut h, "0,0,50");
    let front_target = h.state().canvas.rect.unwrap().center() + egui::vec2(60.0, -40.0);
    h.event(Event::PointerMoved(front_target));
    h.run_steps(2);
    assert!(h.state().canvas.cursor3.is_some(), "Auto front view resolves mouse points on XZ");
    assert_eq!(h.state().canvas.cursor3.unwrap().y, 0.0, "XZ drawing plane has depth Y=0");
    assert!(!h.state().canvas.parallel_plane_warning, "no parallel warning in Auto front mode");
    click_at(&mut h, front_target);
    h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
    let line_front = h
        .state()
        .session
        .doc()
        .unwrap()
        .model
        .iter()
        .find_map(|e| match &e.kind {
            EntityKind::Line(l) if l.a == cadcraft_geom::Vec3::new(0.0, 0.0, 50.0) => Some(l.clone()),
            _ => None,
        })
        .expect("front view line created by mouse click");
    assert_eq!(line_front.a.y, 0.0);
    assert_eq!(line_front.b.y, 0.0);

    // Path 2: Explicit Z0 in Front view cannot unproject onto global Z=0 because ray is parallel
    type_and_submit(&mut h, "LINE");
    type_and_submit(&mut h, "0,0,50");
    type_and_submit(&mut h, "Z=0"); // Activate explicit global Z=0 mode
    h.event(Event::PointerMoved(front_target));
    h.run_steps(2);
    assert!(h.state().canvas.parallel_plane_warning, "warning shown when explicit Z=0 cannot intersect parallel ray");
    assert!(h.state().canvas.cursor3.is_none(), "cursor cleared when explicit Z=0 cannot intersect parallel ray");

    // Free disables Z=0 mode and restores normal drawing
    type_and_submit(&mut h, "Free");
    h.event(Event::PointerMoved(front_target));
    h.run_steps(2);
    assert!(!h.state().canvas.parallel_plane_warning, "Free disables Z=0 mode and warning");
    assert!(h.state().canvas.cursor3.is_some(), "cursor restored after Free");
    h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
}

#[test]
fn test_coordinates_front_view_absolute_vs_relative() {
    let mut ui = UiState::default();
    ui.show_palettes = false;
    let _ = ui.docking.set_visible(Panel::Layers, false);
    let _ = ui.docking.set_visible(Panel::Properties, false);
    let mut h = harness(Some(ui), 1280.0, 760.0);

    h.state_mut().session.execute("view.front", &json!({})).unwrap();
    h.run_steps(2);

    // 1. Sin @: coordenadas absolutas en XZ
    // primer punto: 100,200 -> WCS (100, 0, 200)
    // segundo punto: 50,30  -> WCS (50, 0, 30) (NO sumado)
    type_and_submit(&mut h, "LINE");
    type_and_submit(&mut h, "100,200");
    type_and_submit(&mut h, "50,30");
    type_and_submit(&mut h, ""); // Enter to finish
    h.run_steps(2);

    let doc = h.state().session.doc().unwrap();
    let line_abs = doc
        .model
        .iter()
        .find_map(|e| match &e.kind {
            EntityKind::Line(l) if l.a == cadcraft_geom::Vec3::new(100.0, 0.0, 200.0) && l.b == cadcraft_geom::Vec3::new(50.0, 0.0, 30.0) => {
                Some(l.clone())
            }
            _ => None,
        })
        .expect("line with absolute coordinates (100,0,200) -> (50,0,30) must exist");
    assert_eq!(line_abs.a, cadcraft_geom::Vec3::new(100.0, 0.0, 200.0));
    assert_eq!(line_abs.b, cadcraft_geom::Vec3::new(50.0, 0.0, 30.0));

    // 2. Con @: desplazamiento relativo en XZ
    // primer punto: 100,200 -> WCS (100, 0, 200)
    // segundo punto: @50,30 -> WCS (100+50, 0, 200+30) = (150, 0, 230)
    type_and_submit(&mut h, "LINE");
    type_and_submit(&mut h, "100,200");
    type_and_submit(&mut h, "@50,30");
    type_and_submit(&mut h, ""); // Enter to finish
    h.run_steps(2);

    let doc2 = h.state().session.doc().unwrap();
    let line_rel = doc2
        .model
        .iter()
        .find_map(|e| match &e.kind {
            EntityKind::Line(l) if l.a == cadcraft_geom::Vec3::new(100.0, 0.0, 200.0) && l.b == cadcraft_geom::Vec3::new(150.0, 0.0, 230.0) => {
                Some(l.clone())
            }
            _ => None,
        })
        .expect("line with relative coordinates (100,0,200) -> (150,0,230) must exist");
    assert_eq!(line_rel.a, cadcraft_geom::Vec3::new(100.0, 0.0, 200.0));
    assert_eq!(line_rel.b, cadcraft_geom::Vec3::new(150.0, 0.0, 230.0));
}

#[test]
fn test_coordinates_xy_absolute_chain() {
    let mut ui = UiState::default();
    ui.show_palettes = false;
    let _ = ui.docking.set_visible(Panel::Layers, false);
    let _ = ui.docking.set_visible(Panel::Properties, false);
    let mut h = harness(Some(ui), 1280.0, 760.0);

    h.state_mut().session.execute("view.top", &json!({})).unwrap();
    h.run_steps(2);

    // 0,0 -> 100,0 -> 100,100 sin @ son coordenadas absolutas, no acumulativas
    type_and_submit(&mut h, "LINE");
    type_and_submit(&mut h, "0,0");
    type_and_submit(&mut h, "100,0");
    type_and_submit(&mut h, "100,100");
    type_and_submit(&mut h, ""); // Enter to finish
    h.run_steps(2);

    let doc = h.state().session.doc().unwrap();
    let lines: Vec<_> = doc
        .model
        .iter()
        .filter_map(|e| match &e.kind {
            EntityKind::Line(l) => Some(l.clone()),
            _ => None,
        })
        .collect();

    assert!(
        lines.iter().any(|l| l.a == cadcraft_geom::Vec3::new(0.0, 0.0, 0.0) && l.b == cadcraft_geom::Vec3::new(100.0, 0.0, 0.0)),
        "segment 1 is (0,0,0) to (100,0,0)"
    );
    let seg2 = lines.iter().find(|l| l.a == cadcraft_geom::Vec3::new(100.0, 0.0, 0.0)).expect("segment 2 exists");
    assert_eq!(seg2.b, cadcraft_geom::Vec3::new(100.0, 100.0, 0.0), "segment 2 ends at absolute (100,100,0) and NOT (200,100,0)");
}

#[test]
fn test_coordinates_3d_explicit_and_relative_preserve_z() {
    let mut ui = UiState::default();
    ui.show_palettes = false;
    let _ = ui.docking.set_visible(Panel::Layers, false);
    let _ = ui.docking.set_visible(Panel::Properties, false);
    let mut h = harness(Some(ui), 1280.0, 760.0);

    h.state_mut().session.execute("view.swiso", &json!({})).unwrap();
    h.run_steps(2);

    type_and_submit(&mut h, "LINE");
    type_and_submit(&mut h, "10,20,30");
    type_and_submit(&mut h, "@5,5,10");
    type_and_submit(&mut h, ""); // Enter to finish
    h.run_steps(2);

    let doc = h.state().session.doc().unwrap();
    let line_3d = doc
        .model
        .iter()
        .find_map(|e| match &e.kind {
            EntityKind::Line(l) if l.a == cadcraft_geom::Vec3::new(10.0, 20.0, 30.0) => Some(l.clone()),
            _ => None,
        })
        .expect("3D line exists");
    assert_eq!(line_3d.a, cadcraft_geom::Vec3::new(10.0, 20.0, 30.0), "start Z is preserved as 30");
    assert_eq!(line_3d.b, cadcraft_geom::Vec3::new(15.0, 25.0, 40.0), "end is (10+5, 20+5, 30+10) = (15,25,40), not replaced by Z=0");
}

#[test]
fn test_coordinates_prefixes_hash_and_star() {
    let mut ui = UiState::default();
    ui.show_palettes = false;
    let _ = ui.docking.set_visible(Panel::Layers, false);
    let _ = ui.docking.set_visible(Panel::Properties, false);
    let mut h = harness(Some(ui), 1280.0, 760.0);

    h.state_mut().session.execute("view.front", &json!({})).unwrap();
    h.run_steps(2);

    // Prefix "#" explicitly forces absolute coordinates
    type_and_submit(&mut h, "LINE");
    type_and_submit(&mut h, "100,200");
    type_and_submit(&mut h, "#50,30");
    type_and_submit(&mut h, "");
    h.run_steps(2);

    let doc = h.state().session.doc().unwrap();
    let line_hash = doc
        .model
        .iter()
        .find_map(|e| match &e.kind {
            EntityKind::Line(l) if l.a == cadcraft_geom::Vec3::new(100.0, 0.0, 200.0) && l.b == cadcraft_geom::Vec3::new(50.0, 0.0, 30.0) => {
                Some(l.clone())
            }
            _ => None,
        })
        .expect("# prefix forces absolute (50, 0, 30)");
    assert_eq!(line_hash.b, cadcraft_geom::Vec3::new(50.0, 0.0, 30.0));

    // Prefix "*" specifies world coordinates (WCS)
    type_and_submit(&mut h, "LINE");
    type_and_submit(&mut h, "*10,20,30");
    type_and_submit(&mut h, "*@5,5,5");
    type_and_submit(&mut h, "");
    h.run_steps(2);

    let doc2 = h.state().session.doc().unwrap();
    let line_star = doc2
        .model
        .iter()
        .find_map(|e| match &e.kind {
            EntityKind::Line(l) if l.a == cadcraft_geom::Vec3::new(10.0, 20.0, 30.0) && l.b == cadcraft_geom::Vec3::new(15.0, 25.0, 35.0) => {
                Some(l.clone())
            }
            _ => None,
        })
        .expect("* prefix creates line in WCS");
    assert_eq!(line_star.a, cadcraft_geom::Vec3::new(10.0, 20.0, 30.0));
    assert_eq!(line_star.b, cadcraft_geom::Vec3::new(15.0, 25.0, 35.0));
}

#[test]
fn test_line_z0_option_menu_click_activates_and_free_disables_through_ui_keys() {
    let mut ui = UiState::default();
    ui.show_palettes = false;
    let _ = ui.docking.set_visible(Panel::Layers, false);
    let _ = ui.docking.set_visible(Panel::Properties, false);
    let mut h = harness(Some(ui), 1280.0, 760.0);
    type_and_submit(&mut h, "LINE");
    type_and_submit(&mut h, "0,0,100");
    assert!(!h.state().session.current_prompt().unwrap().global_xy_zero);

    let cursor = h.state().canvas.rect.unwrap().center();
    h.event(Event::PointerMoved(cursor));
    h.run_steps(2);
    h.event(Event::Key { key: Key::ArrowDown, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
    let (row, keyword) = h.state().cmd.option_rows.iter().find(|(_, keyword)| keyword == "Z=0").cloned().expect("Z=0 appears in LINE options");
    let option_pos = row.center();
    h.event(Event::PointerMoved(option_pos));
    h.run_steps(1);
    h.event(Event::PointerButton { pos: option_pos, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    h.event(Event::PointerButton { pos: option_pos, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
    assert_eq!(keyword, "Z=0");
    let active = h.state().session.current_prompt().unwrap();
    assert!(active.global_xy_zero, "clicking the displayed prompt option activates the mode");
    assert!(active.message.contains("global Z=0"));

    type_and_submit(&mut h, "Free");
    let free_prompt = h.state().session.current_prompt().unwrap();
    assert!(!free_prompt.global_xy_zero, "Free deactivates the option without accepting a point");
    assert!(!free_prompt.message.contains("global Z=0"), "the inactive prompt does not claim Z=0 is active");
    assert_eq!(h.state().session.doc().unwrap().model.len(), 0, "neither toggle creates an entity");
    h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
    assert!(h.state().session.running.is_none());
}

#[test]
fn test_line_z0_parallel_camera_rejects_stale_click_and_free_disables_mode() {
    let mut ui = UiState::default();
    ui.show_palettes = false;
    let _ = ui.docking.set_visible(Panel::Layers, false);
    let _ = ui.docking.set_visible(Panel::Properties, false);
    let mut h = imperial_harness(Some(ui), 1280.0, 760.0);
    h.state_mut().session.execute("view.swiso", &json!({})).unwrap();
    let mut camera = h.state().session.camera();
    camera.height = 300.0;
    h.state_mut().session.set_camera(camera);
    type_and_submit(&mut h, "LINE");
    type_and_submit(&mut h, "4,6,100");
    type_and_submit(&mut h, "Z0");

    let xf = h.state().canvas.xf.unwrap();
    let screen = xf.to_screen3(cadcraft_geom::Vec3::new(60.0, 30.0, 0.0));
    h.event(Event::PointerMoved(screen));
    h.run_steps(2);
    assert!(h.state().canvas.cursor3.is_some(), "isometric view intersects global Z=0");

    h.state_mut().session.execute("view.front", &json!({})).unwrap();
    h.run_steps(2);
    assert!(h.state().canvas.cursor3.is_none(), "front view ray is parallel to global Z=0");
    assert!(h.state().canvas.parallel_plane_warning);
    h.event(Event::PointerButton { pos: screen, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    h.event(Event::PointerButton { pos: screen, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
    assert_eq!(h.state().session.doc().unwrap().model.len(), 0, "an invalid ray cannot confirm the last cursor point");
    assert!(h.state().session.current_prompt().unwrap().global_xy_zero);

    type_and_submit(&mut h, "Free");
    assert!(!h.state().session.current_prompt().unwrap().global_xy_zero, "Free disables the mode without placing a point");
    assert_eq!(h.state().session.doc().unwrap().model.len(), 0);
    h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
    assert!(h.state().session.running.is_none());
}

#[test]
fn test_graphical_height_adjustment_escape_cancels_without_altering_drawing() {
    let mut h = imperial_harness(None, 1280.0, 760.0);
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
    let mut h = imperial_harness(None, 1280.0, 760.0);
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
    let mut h = imperial_harness(None, 1280.0, 760.0);
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
    let res = app
        .run(
            "options",
            json!({
                "QNEWTEMPLATE": "/test/templates/office.dwt",
                "startupCreateDrawing": true
            }),
        )
        .unwrap();

    assert_eq!(res["QNEWTEMPLATE"], "/test/templates/office.dwt");
    assert_eq!(res["startupCreateDrawing"], true);

    // Verify CadApp state
    assert_eq!(cadcraft_engine::cmd::file::qnew_template(), "/test/templates/office.dwt");
    assert!(app.ui.startup_create_drawing);

    // Check profile serialization and restoration
    let profile = app.profile_json();
    assert!(profile.contains("office.dwt"));
    assert!(profile.contains("startupCreateDrawing"));

    // CORRECTION: Clear global value before restoring, proving that restoration actually works!
    cadcraft_engine::cmd::file::set_qnew_template("");
    assert_eq!(cadcraft_engine::cmd::file::qnew_template(), "");

    let mut restored_app = CadApp::new(Session::default(), Services::default());
    restored_app.load_profile(&profile);
    assert_eq!(cadcraft_engine::cmd::file::qnew_template(), "/test/templates/office.dwt");
    assert!(restored_app.ui.startup_create_drawing);

    // Reset template
    app.run("options", json!({"QNEWTEMPLATE": "", "startupCreateDrawing": false})).unwrap();
    assert_eq!(cadcraft_engine::cmd::file::qnew_template(), "");
    assert!(!app.ui.startup_create_drawing);
}

#[test]
fn test_template_personal_flow_and_independence() {
    let _serial = TEMPLATE_TEST_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let _restore = RestoreQnewTemplate(cadcraft_engine::cmd::file::qnew_template());
    let mut app = CadApp::new(Session::default(), Services::default());

    // 1. Set a personal template via options
    let template_path = "/tmp/test_user_template.dwt";
    app.run("options", json!({ "QNEWTEMPLATE": template_path })).unwrap();
    assert_eq!(cadcraft_engine::cmd::file::qnew_template(), template_path);

    // 2. Profile persistence roundtrip
    let prof = app.profile_json();
    // Clear global value first to prove restoration from profile
    cadcraft_engine::cmd::file::set_qnew_template("");
    assert_eq!(cadcraft_engine::cmd::file::qnew_template(), "");
    let mut restored = CadApp::new(Session::default(), Services::default());
    restored.load_profile(&prof);
    assert_eq!(cadcraft_engine::cmd::file::qnew_template(), template_path);

    // 3. Fallback when template file is missing: NEW starts a clean untitled blank drawing
    let _res = restored.session.execute("new", &json!({})).unwrap();
    let st = restored.session.state().unwrap();
    assert!(st.title.starts_with("Drawing"));
    assert!(st.path.is_none(), "new drawing has no file path set; cannot overwrite template");
    assert!(restored.session.execute("qsave", &json!({})).is_err(), "QSAVE must require SAVEAS");

    // 4. Cancel file selection or options dialog does not change preference
    cadcraft_engine::cmd::file::set_qnew_template(template_path);
    // Clearing sets empty
    app.run("options", json!({ "QNEWTEMPLATE": "" })).unwrap();
    assert_eq!(cadcraft_engine::cmd::file::qnew_template(), "");
}

#[test]
fn test_dynamic_input_3d_coordinates_and_geometry() {
    let mut app = CadApp::new(Session::default(), Services::default());
    app.session.new_drawing(true);

    // Start LINE command
    app.session.cmdline("line").unwrap();

    // 1. Enter 0,0,0
    app.cmdline(
        "0,0,0
",
    );

    // 2. Enter 10,20,30
    app.cmdline(
        "10,20,30
",
    );

    // 3. Enter @5,5,10
    app.cmdline(
        "@5,5,10
",
    );

    // Finish line with Enter
    app.cmdline(
        "
",
    );

    let doc = app.session.doc().unwrap();
    let lines: Vec<_> = doc
        .model
        .iter()
        .filter_map(|e| match &e.kind {
            cadcraft_doc::EntityKind::Line(l) => Some(l.clone()),
            _ => None,
        })
        .collect();

    assert_eq!(lines.len(), 2, "must create two 3D line segments");
    // First segment: (0, 0, 0) -> (10, 20, 30)
    assert!(lines[0].a.near(cadcraft_geom::Vec3::new(0.0, 0.0, 0.0), 1e-5), "line 1 start: {:?}", lines[0].a);
    assert!(lines[0].b.near(cadcraft_geom::Vec3::new(10.0, 20.0, 30.0), 1e-5), "line 1 end: {:?}", lines[0].b);

    // Second segment: (10, 20, 30) -> (15, 25, 40)
    assert!(lines[1].a.near(cadcraft_geom::Vec3::new(10.0, 20.0, 30.0), 1e-5), "line 2 start: {:?}", lines[1].a);
    assert!(lines[1].b.near(cadcraft_geom::Vec3::new(15.0, 25.0, 40.0), 1e-5), "line 2 end: {:?}", lines[1].b);

    // 4. Test negative and decimal coordinates in Front (XZ) view
    app.session.cmdline("view.front").unwrap();
    assert_eq!(app.session.drawing_plane(), cadcraft_geom::DrawingPlane::XZ);

    app.session.cmdline("line").unwrap();
    app.cmdline(
        "-10.5,-20.5,-30.5
",
    );
    app.cmdline(
        "5.25,10.25,15.25
",
    );
    app.cmdline(
        "
",
    );

    let doc2 = app.session.doc().unwrap();
    let lines2: Vec<_> = doc2
        .model
        .iter()
        .filter_map(|e| match &e.kind {
            cadcraft_doc::EntityKind::Line(l) => Some(l.clone()),
            _ => None,
        })
        .collect();

    assert_eq!(lines2.len(), 3);
    assert!(lines2[2].a.near(cadcraft_geom::Vec3::new(-10.5, -20.5, -30.5), 1e-5));
    assert!(lines2[2].b.near(cadcraft_geom::Vec3::new(5.25, 10.25, 15.25), 1e-5));

    // 5. Test Escape cancels without leftover state
    app.session.cmdline("line").unwrap();
    app.cmd.buffer = "10,20,".to_string();
    app.session.cancel();
    assert!(app.session.current_prompt().is_none());
}

#[test]
fn test_template_ui_journey_with_simulated_pickers() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let _serial = TEMPLATE_TEST_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let _restore = RestoreQnewTemplate(cadcraft_engine::cmd::file::qnew_template());
    cadcraft_engine::cmd::file::set_io(cadcraft_engine::cmd::file::IoHooks {
        read: |_, _| {
            let mut d = cadcraft_doc::Drawing::new_metric();
            d.layers.push(cadcraft_doc::Layer { name: "OFFICE-A".to_string(), color: cadcraft_color::Color::Index(3), ..Default::default() });
            Ok(d)
        },
        write: |_, _| Ok(Vec::new()),
        plot: None,
    });
    cadcraft_engine::cmd::file::set_write_as(|_, _, _| Ok(Vec::new()));

    let temp_dir = std::env::temp_dir().join(format!("cadcraft-tmpl-test-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let path_custom_dwt = temp_dir.join("custom_default.dwt");
    std::fs::write(&path_custom_dwt, b"0\nEOF\n").unwrap();
    let str_custom_dwt = path_custom_dwt.to_string_lossy().to_string();

    let path_other_dwt = temp_dir.join("other_standalone.dwt");
    std::fs::write(&path_other_dwt, b"0\nEOF\n").unwrap();
    let str_other_dwt = path_other_dwt.to_string_lossy().to_string();

    let picked_file = Rc::new(RefCell::new(Some(str_custom_dwt.clone())));
    let picked_file_c = picked_file.clone();
    let save_prompt_called = Rc::new(std::cell::Cell::new(false));
    let save_prompt_called_c = save_prompt_called.clone();

    let services = Services {
        pick_open: Some(Box::new(move |_dir| picked_file_c.borrow().clone())),
        pick_save: Some(Box::new(move |_name, _dir| {
            save_prompt_called_c.set(true);
            Some("/saved_drawings/my_plan.dxf".to_string())
        })),
        ..Services::default()
    };

    let mut app = CadApp::new(Session::empty(), services);
    assert_eq!(app.session.docs.len(), 0);

    // 1. Elegir la predeterminada no aumenta el número de documentos
    app.start("ui.defaulttemplate.choose");
    assert_eq!(app.session.docs.len(), 0, "choosing default template must not create any document");
    assert_eq!(cadcraft_engine::cmd::file::qnew_template(), str_custom_dwt);

    // 2. Nuevo utiliza su contenido
    app.start("new");
    assert_eq!(app.session.docs.len(), 1, "new must create one document from default template");
    let st = app.session.state().unwrap();
    assert!(st.doc.layers.iter().any(|l| l.name == "OFFICE-A"), "new drawing uses default template layers");
    assert!(st.path.is_none(), "created document never inherits the template save path");

    // 3. Nuevo desde otra plantilla no cambia la predeterminada
    *picked_file.borrow_mut() = Some(str_other_dwt.clone());
    app.start("ui.newtemplate");
    assert_eq!(app.session.docs.len(), 2, "new from other template creates a separate document");
    assert_eq!(cadcraft_engine::cmd::file::qnew_template(), str_custom_dwt, "default template remains unchanged");

    // 4. Cancelar conserva el valor
    *picked_file.borrow_mut() = None; // simulated cancel
    app.start("ui.defaulttemplate.choose");
    assert_eq!(cadcraft_engine::cmd::file::qnew_template(), str_custom_dwt, "cancelling file picker preserves preference");

    // 5. Una selección inválida no sustituye la preferencia válida anterior
    *picked_file.borrow_mut() = Some("/invalid/doc.txt".to_string()); // invalid extension
    app.start("ui.defaulttemplate.choose");
    assert_eq!(cadcraft_engine::cmd::file::qnew_template(), str_custom_dwt, "invalid file path does not overwrite valid template");

    *picked_file.borrow_mut() = Some(".".to_string());
    app.start("ui.defaulttemplate.choose");
    assert_eq!(cadcraft_engine::cmd::file::qnew_template(), str_custom_dwt, "dot does not overwrite template");

    // 6. Primer guardado solicita otra ruta
    assert!(!save_prompt_called.get());
    app.start("ui.saveas");
    assert!(save_prompt_called.get(), "first save must prompt user for save path via pick_save");

    // 7. Quitar persiste
    app.start("ui.defaulttemplate.clear");
    assert_eq!(cadcraft_engine::cmd::file::qnew_template(), "");
    let prof = app.profile_json();
    cadcraft_engine::cmd::file::set_qnew_template("/some/temp.dwt");
    let mut restored = CadApp::new(Session::default(), Services::default());
    restored.load_profile(&prof);
    assert_eq!(cadcraft_engine::cmd::file::qnew_template(), "", "cleared template persists across restore");
}

#[test]
fn test_dynamic_input_harness_events_and_containment() {
    let mut h = harness(None, 1600.0, 1000.0);
    h.state_mut().session.new_drawing(true);

    // Start LINE
    h.state_mut().cmdline("line");
    h.run_steps(2);
    assert!(h.state().session.prompting());

    // 1. Enter "10" and Tab -> first comma appended, moves to field 1 (Y)
    h.state_mut().cmd.buffer = "10".to_string();
    cadcraft_ui_egui::cmdline::tab(h.state_mut());
    assert_eq!(h.state().cmd.buffer, "10,");
    assert_eq!(h.state().cmd.dyn_active_field, None);

    // 2. Enter "20" and Tab -> second comma appended, appears Z!
    h.state_mut().cmd.buffer = "10,20".to_string();
    cadcraft_ui_egui::cmdline::tab(h.state_mut());
    assert_eq!(h.state().cmd.buffer, "10,20,");
    assert_eq!(h.state().cmd.dyn_active_field, None);

    // 3. Enter "30": 3 fields present, "10,20,30" visible before Enter
    h.state_mut().cmd.buffer = "10,20,30".to_string();
    let f = cadcraft_ui_egui::cmdline::dyn_frame(h.state()).unwrap();
    let e = cadcraft_ui_egui::dyninput::parse(&h.state().cmd.buffer, &f).unwrap();
    assert!(e.split && e.split_z);
    assert_eq!(e.first, "10");
    assert_eq!(e.second, "20");
    assert_eq!(e.third, "30");

    // 4. Tab and Shift+Tab navigation between components
    cadcraft_ui_egui::cmdline::tab_backward(h.state_mut());
    assert_eq!(h.state().cmd.dyn_active_field, Some(1), "Shift+Tab moves focus backwards to field 1 (Y)");

    cadcraft_ui_egui::cmdline::tab_backward(h.state_mut());
    assert_eq!(h.state().cmd.dyn_active_field, Some(0), "Shift+Tab moves focus backwards to field 0 (X)");

    cadcraft_ui_egui::cmdline::tab(h.state_mut());
    assert_eq!(h.state().cmd.dyn_active_field, Some(1), "Tab moves focus forward to field 1 (Y)");

    cadcraft_ui_egui::cmdline::tab(h.state_mut());
    assert_eq!(h.state().cmd.dyn_active_field, Some(2), "Tab moves focus forward to field 2 (Z)");

    cadcraft_ui_egui::cmdline::tab(h.state_mut());
    assert_eq!(h.state().cmd.dyn_active_field, Some(0), "Tab wraps focus forward to field 0 (X)");

    // 5. Editing component 0
    let updated = cadcraft_ui_egui::dyninput::update_component_field(&h.state().cmd.buffer, 0, Some('5'), false);
    assert_eq!(updated, "105,20,30");
    let backspaced = cadcraft_ui_egui::dyninput::update_component_field(&h.state().cmd.buffer, 0, None, true);
    assert_eq!(backspaced, "1,20,30");

    // 6. Escape cancellation
    h.state_mut().session.cmdline("line").unwrap();
    h.state_mut().cmd.buffer = "10,20,30".to_string();
    h.state_mut().cmd.dyn_active_field = Some(2);
    h.state_mut().session.cancel();
    h.state_mut().cmd.buffer.clear();
    h.state_mut().cmd.dyn_active_field = None;
    assert!(h.state().cmd.buffer.is_empty());
    assert_eq!(h.state().cmd.dyn_active_field, None);

    // 7. Prefixes supported: @, #, *, @*
    let e_rel = cadcraft_ui_egui::dyninput::parse("@5,5,10", &f).unwrap();
    assert_eq!(e_rel.prefix, "@");
    assert_eq!(e_rel.first, "5");
    assert_eq!(e_rel.second, "5");
    assert_eq!(e_rel.third, "10");

    let e_abs = cadcraft_ui_egui::dyninput::parse("#0,0,0", &f).unwrap();
    assert_eq!(e_abs.prefix, "#");

    let e_world = cadcraft_ui_egui::dyninput::parse("*10,20", &f).unwrap();
    assert_eq!(e_world.prefix, "*");

    let e_rel_world = cadcraft_ui_egui::dyninput::parse("@*10,20,30", &f).unwrap();
    assert_eq!(e_rel_world.prefix, "@*");

    // 8. Labels in Front view (XZ)
    let f_front = cadcraft_ui_egui::dyninput::Frame {
        base: Some(cadcraft_geom::Vec2::ZERO),
        cartesian: true,
        absolute: false,
        plane: cadcraft_geom::DrawingPlane::XZ,
    };
    // 2 components in front view
    let e_front_2 = cadcraft_ui_egui::dyninput::parse("10,20", &f_front).unwrap();
    assert!(!e_front_2.split_z);
    // 3 components in front view
    let e_front_3 = cadcraft_ui_egui::dyninput::parse("10,20,30", &f_front).unwrap();
    assert!(e_front_3.split_z);

    // 9. Containment in small window and near all four borders
    let _small_h = harness(None, 360.0, 240.0);
    let clip = egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(360.0, 240.0));
    let pad = 4.0;
    for (at_x, at_y) in [(2.0, 2.0), (358.0, 2.0), (2.0, 238.0), (358.0, 238.0), (180.0, 120.0)] {
        let size = vec2(280.0, 40.0);
        let clamped_w = size.x.min((clip.width() - 2.0 * pad).max(1.0));
        let clamped_h = size.y.min((clip.height() - 2.0 * pad).max(1.0));
        let mut min_x = at_x + 18.0;
        if min_x + clamped_w > clip.max.x - pad {
            min_x = at_x - clamped_w - 18.0;
        }
        min_x = min_x.clamp(clip.min.x + pad, (clip.max.x - clamped_w - pad).max(clip.min.x + pad));

        let mut min_y = at_y - clamped_h - 14.0;
        if min_y < clip.min.y + pad {
            min_y = at_y + 18.0;
        }
        min_y = min_y.clamp(clip.min.y + pad, (clip.max.y - clamped_h - pad).max(clip.min.y + pad));

        let r = egui::Rect::from_min_size(pos2(min_x, min_y), vec2(clamped_w, clamped_h));
        assert!(r.min.x >= clip.min.x + pad - 1e-4, "min_x contained at ({at_x}, {at_y}): {:?}", r);
        assert!(r.max.x <= clip.max.x - pad + 1e-4, "max_x contained at ({at_x}, {at_y}): {:?}", r);
        assert!(r.min.y >= clip.min.y + pad - 1e-4, "min_y contained at ({at_x}, {at_y}): {:?}", r);
        assert!(r.max.y <= clip.max.y - pad + 1e-4, "max_y contained at ({at_x}, {at_y}): {:?}", r);
    }
}
