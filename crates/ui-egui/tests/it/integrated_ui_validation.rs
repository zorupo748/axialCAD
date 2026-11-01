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
    let mut h = harness(Some(ui), 1280.0, 760.0);
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
fn test_line_ground_acquisition_and_local_vertical_capture_are_distinct() {
    let mut ui = UiState::default();
    ui.show_palettes = false;
    let _ = ui.docking.set_visible(Panel::Layers, false);
    let _ = ui.docking.set_visible(Panel::Properties, false);
    let mut h = harness(Some(ui), 1280.0, 760.0);
    h.state_mut().session.execute("view.front", &json!({})).unwrap();
    let mut camera = h.state().session.camera();
    camera.azimuth = -1.3;
    camera.elevation = 0.3;
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
    let target = cadcraft_geom::Vec3::new(80.0, 50.0, 0.0);
    let target_screen = xf.to_screen3(target);
    assert!(xf.rect.contains(target_screen));
    let vertical_cursor = ground_center + egui::vec2(0.0, 6.0);
    h.event(Event::PointerMoved(vertical_cursor));
    h.run_steps(2);
    assert!(!h.state().canvas.ground_target_acquired, "the local vertical snap takes priority inside its acquire radius");
    assert!(h.state().canvas.axis_track_z_zero, "the local vertical Z=0 guide remains a distinct capture");
    assert!(h.state().canvas.cursor3.is_some_and(|point| point.dist(cadcraft_geom::Vec3::ZERO) < 0.1));

    let acquisition_cursor = ground_center + egui::vec2(0.0, 20.0);
    h.event(Event::PointerMoved(acquisition_cursor));
    h.run_steps(2);
    assert!(h.state().canvas.ground_target_acquired, "moving outside the local guide but inside the reference area acquires Z=0");
    assert!(!h.state().canvas.axis_track_z_zero, "acquisition replaces the released local guide");
    h.event(Event::PointerMoved(ground_center));
    h.run_steps(2);
    assert!(h.state().canvas.ground_target_acquired, "approaching the vertical guide does not release the acquired ground plane");
    assert!(!h.state().canvas.axis_track_z_zero, "the acquired plane remains the effective candidate near the vertical guide");

    h.event(Event::PointerMoved(target_screen));
    h.run_steps(2);
    assert!(
        h.state().canvas.cursor3.is_some_and(|point| point.dist(target) < 0.1),
        "the acquired plane follows the lateral cursor movement: {:?}",
        h.state().canvas.cursor3
    );
    let resolved = h.state().canvas.cursor3.unwrap();
    let preview = h.state().session.preview(h.state().canvas.cursor.unwrap());
    let preview_endpoint = preview.iter().find_map(|entity| match &entity.kind {
        EntityKind::Line(line) if line.a == cadcraft_geom::Vec3::new(0.0, 0.0, 100.0) => Some(line.b),
        _ => None,
    });
    assert!(
        preview_endpoint.is_some_and(|point| point.dist(resolved) < 0.1),
        "preview {preview_endpoint:?} must use the resolved ground cursor {resolved:?}"
    );
    click_at(&mut h, target_screen);
    let saved = h.state().session.doc().unwrap().model.iter().find_map(|entity| match &entity.kind {
        EntityKind::Line(line) if line.a.z == 100.0 => Some(line.b),
        _ => None,
    });
    assert!(
        saved.is_some_and(|point| point.dist(preview_endpoint.unwrap()) < 0.1 && point.z == 0.0),
        "the accepted diagonal must match the preview on global Z=0: saved={saved:?}, preview={preview_endpoint:?}"
    );
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
fn test_line_z0_option_uses_clicked_global_ground_point_once() {
    for (axistrack, other_oblique) in [(false, false), (true, false), (true, true)] {
        let mut ui = UiState::default();
        ui.show_palettes = false;
        let _ = ui.docking.set_visible(Panel::Layers, false);
        let _ = ui.docking.set_visible(Panel::Properties, false);
        let mut h = harness(Some(ui), 1280.0, 760.0);
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
fn test_line_automatically_acquires_visible_ground_target_and_saves_diagonal() {
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
        assert!(!h.state().session.current_prompt().unwrap().global_xy_zero, "automatic acquisition must not activate explicit Z=0 mode");

        let base = cadcraft_geom::Vec3::new(0.0, 0.0, 100.0);
        let target = cadcraft_geom::Vec3::new(500.0, 400.0, 0.0);
        let xf = h.state().canvas.xf.unwrap();
        let target_screen = xf.to_screen3(target);
        let ground_center = xf.to_screen3(cadcraft_geom::Vec3::ZERO);
        assert!(xf.rect.contains(target_screen), "target must be inside canvas: {target_screen:?}");
        let target_offset = target_screen.distance(ground_center);
        assert!(target_offset > 52.0, "decisive target must be more than 52 screen pixels beyond the ring center: {target_offset}");

        let free = cadcraft_geom::Vec3::new(350.0, 250.0, 100.0);
        let free_screen = xf.to_screen3(free);
        assert!(xf.rect.contains(free_screen));
        h.event(Event::PointerMoved(free_screen));
        h.run_steps(2);
        assert!(!h.state().canvas.ground_target_acquired, "outside the visible target, normal continuation remains free");
        if !orthomode && !zlock {
            assert!((h.state().canvas.cursor3.unwrap().z - base.z).abs() < 1e-6, "normal continuation preserves the elevated base height");
        }

        let acquisition_cursor = ground_center + vec2(12.0, 0.0);
        h.event(Event::PointerMoved(acquisition_cursor));
        h.run_steps(2);
        assert!(h.state().canvas.ground_target_acquired, "hovering inside the visible ground target acquires it");
        h.event(Event::PointerMoved(target_screen));
        h.run_steps(2);
        assert!(h.state().canvas.ground_target_acquired, "leaving the 40 px acquisition ring must not release the acquired plane");
        let resolved = h.state().canvas.cursor3.expect("camera ray intersects global Z=0");
        assert!(resolved.dist(target) < 0.5, "resolved cursor {resolved:?} must match independent target {target:?}");
        assert_eq!(resolved.z, 0.0);
        assert_eq!(h.state().session.settings.orthomode, orthomode, "acquisition must not change ORTHO");
        assert_eq!(h.state().session.settings.zlock, zlock, "acquisition must not change V");
        assert!(
            h.state().canvas.cursor3.is_some_and(|point| point.dist(target) < 0.5),
            "cursor must follow the distant acquired plane point: {:?}",
            h.state().canvas.cursor3
        );
        let preview = h.state().session.preview(h.state().canvas.cursor.unwrap());
        assert!(
            preview.iter().any(|entity| { matches!(&entity.kind, EntityKind::Line(line) if line.a == base && line.b.dist(target) < 0.5) }),
            "preview must use the same resolved point as the cursor"
        );
        h.event(Event::PointerButton { pos: target_screen, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
        h.run_steps(1);
        h.event(Event::PointerButton { pos: target_screen, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
        h.run_steps(3);

        assert!(h.state().session.last_point3.dist(target) < 0.5, "click must accept the point resolved for the cursor");
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
        let saved = lines.iter().find(|(a, _)| a.dist(base) < 0.5).map(|(_, b)| *b);
        assert!(saved.is_some_and(|point| point.dist(target) < 0.5), "saved LINE endpoint must be {target:?}, got {saved:?} in {lines:?}");
        assert!(!h.state().canvas.ground_target_acquired, "accepting the point clears the temporary target");
        assert!(!h.state().session.current_prompt().unwrap().global_xy_zero, "automatic acquisition does not set the explicit mode");
        assert_eq!(h.state().session.settings.orthomode, orthomode);
        assert_eq!(h.state().session.settings.zlock, zlock, "V is restored after accepting the acquired point");

        if !orthomode && !zlock && !axistrack {
            let next = cadcraft_geom::Vec3::new(140.0, -90.0, 0.0);
            let next_screen = h.state().canvas.xf.unwrap().to_screen3(next);
            h.event(Event::PointerMoved(next_screen));
            h.run_steps(2);
            assert!(!h.state().canvas.ground_target_acquired, "the next segment starts at Z=0 and does not retain prior acquisition state");
            assert!(h.state().canvas.cursor3.unwrap().dist(next) < 0.5, "the next segment returns to normal free continuation");
        } else if zlock {
            let vertical = target + cadcraft_geom::Vec3::new(0.0, 0.0, 50.0);
            let vertical_screen = h.state().canvas.xf.unwrap().to_screen3(vertical);
            h.event(Event::PointerMoved(vertical_screen));
            h.run_steps(2);
            let cursor = h.state().canvas.cursor3.unwrap();
            assert!((cursor.x - target.x).abs() < 0.5 && (cursor.y - target.y).abs() < 0.5, "V is restored for the next point");
            assert!(cursor.z > target.z, "V still permits vertical continuation after the one-shot acquired point");
        }

        h.event(Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
        h.run_steps(1);
        h.event(Event::Key { key: Key::Enter, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE });
        h.run_steps(2);
        assert!(h.state().session.running.is_none());
        assert!(!h.state().canvas.ground_target_acquired, "finishing LINE leaves no target state");
    }
}

#[test]
fn test_line_ground_target_preserves_3d_endpoint_snaps_and_rejects_parallel_view() {
    let mut ui = UiState::default();
    ui.show_palettes = false;
    let _ = ui.docking.set_visible(Panel::Layers, false);
    let _ = ui.docking.set_visible(Panel::Properties, false);
    let mut h = harness(Some(ui), 1280.0, 760.0);
    h.state_mut().session.execute("view.swiso", &json!({})).unwrap();
    let mut camera = h.state().session.camera();
    camera.height = 1800.0;
    h.state_mut().session.set_camera(camera);
    let elevated_endpoint = cadcraft_geom::Vec3::new(80.0, 50.0, 20.0);
    h.state_mut().session.execute("line", &json!({"points": [[80.0, 50.0, 20.0], [100.0, 50.0, 20.0]]})).unwrap();
    h.state_mut().session.settings.osmode = cadcraft_engine::snap::mode::END;
    type_and_submit(&mut h, "LINE");
    type_and_submit(&mut h, "0,0,100");

    let xf = h.state().canvas.xf.unwrap();
    let endpoint_screen = xf.to_screen3(elevated_endpoint);
    let ground_center = xf.to_screen3(cadcraft_geom::Vec3::ZERO);
    assert!(endpoint_screen.distance(ground_center) < 40.0, "elevated snap is deliberately inside the ground-target zone");
    h.event(Event::PointerMoved(endpoint_screen));
    h.run_steps(2);
    assert_eq!(h.state().canvas.cursor3, Some(elevated_endpoint), "real entity snaps retain their original WCS Z");
    assert_eq!(h.state().canvas.snap.map(|hit| hit.name), Some("Endpoint"));
    assert!(!h.state().canvas.ground_target_acquired, "the ground target cannot override a real entity snap");
    h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    h.state_mut().session.settings.osmode = 0;
    type_and_submit(&mut h, "LINE");
    type_and_submit(&mut h, "0,0,100");
    h.state_mut().session.execute("view.front", &json!({})).unwrap();
    h.run_steps(2);
    let front_xf = h.state().canvas.xf.unwrap();
    let front_target = front_xf.to_screen3(cadcraft_geom::Vec3::ZERO);
    assert!(
        front_xf.to_world3_with_offset(front_target, cadcraft_geom::DrawingPlane::XY, 0.0).is_none(),
        "the test view must have a camera ray parallel to global Z=0"
    );
    let stale_cursor = cadcraft_geom::Vec3::new(999.0, 888.0, 777.0);
    h.state_mut().canvas.cursor3 = Some(stale_cursor);
    h.state_mut().session.cursor3 = stale_cursor;
    h.event(Event::PointerMoved(front_target));
    h.run_steps(2);
    assert!(!h.state().canvas.ground_target_acquired, "a view parallel to Z=0 must not retain or fabricate an acquired point");
    assert_ne!(h.state().canvas.cursor3, Some(stale_cursor), "parallel view must not reuse the previous cursor");

    h.state_mut().session.execute("view.swiso", &json!({})).unwrap();
    h.run_steps(2);
    let restored_xf = h.state().canvas.xf.unwrap();
    let outside = restored_xf.to_screen3(cadcraft_geom::Vec3::new(350.0, 250.0, 100.0));
    h.event(Event::PointerMoved(outside));
    h.run_steps(2);
    assert!(!h.state().canvas.ground_target_acquired, "changing camera clears the old target candidate");
    h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
    assert!(!h.state().canvas.ground_target_acquired, "cancelling LINE leaves no target state");
}

#[test]
fn test_line_ground_target_free_rearm_canvas_exit_and_camera_change() {
    for use_options_menu in [false, true] {
        let mut ui = UiState::default();
        ui.show_palettes = false;
        let _ = ui.docking.set_visible(Panel::Layers, false);
        let _ = ui.docking.set_visible(Panel::Properties, false);
        let mut h = harness(Some(ui), 1280.0, 760.0);
        h.state_mut().session.execute("view.swiso", &json!({})).unwrap();
        let mut camera = h.state().session.camera();
        camera.height = 1800.0;
        h.state_mut().session.set_camera(camera);
        h.state_mut().session.settings.osmode = 0;
        h.state_mut().session.settings.axistrack = true;
        type_and_submit(&mut h, "LINE");
        type_and_submit(&mut h, "0,0,100");

        let xf = h.state().canvas.xf.unwrap();
        let center = xf.to_screen3(cadcraft_geom::Vec3::ZERO);
        let acquire = center + vec2(12.0, 0.0);
        h.event(Event::PointerMoved(acquire));
        h.run_steps(2);
        assert!(h.state().canvas.ground_target_acquired);

        if use_options_menu {
            h.event(Event::Key { key: Key::ArrowDown, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
            h.run_steps(2);
            let (row, keyword) =
                h.state().cmd.option_rows.iter().find(|(_, keyword)| keyword == "Free").cloned().expect("Free is available in prompt options");
            let option_pos = row.center();
            h.event(Event::PointerMoved(option_pos));
            h.run_steps(1);
            h.event(Event::PointerButton { pos: option_pos, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
            h.run_steps(1);
            h.event(Event::PointerButton { pos: option_pos, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
            h.run_steps(2);
            assert_eq!(keyword, "Free");
        } else {
            type_and_submit(&mut h, "F");
        }
        assert!(!h.state().canvas.ground_target_acquired, "Free/F releases the acquired target");
        assert_eq!(h.state().session.doc().unwrap().model.len(), 0, "Free does not place an entity");
        if !use_options_menu {
            h.event(Event::PointerMoved(center));
            h.run_steps(2);
            assert!(!h.state().canvas.ground_target_acquired, "F leaves the ground suppressed until the cursor exits the acquisition area");
            assert!(
                h.state().canvas.axis_track_z_zero,
                "Free still permits the local vertical Z=0 guide: cursor={:?}, axis={:?}, snap={:?}, base={:?}, axistrack={}",
                h.state().canvas.cursor3,
                h.state().canvas.active_axis_3d,
                h.state().canvas.snap,
                h.state().session.current_prompt().and_then(|prompt| prompt.base3),
                h.state().session.settings.axistrack
            );
            assert!(h.state().canvas.cursor3.is_some_and(|point| point.dist(cadcraft_geom::Vec3::ZERO) < 0.1));
        }

        h.event(Event::PointerMoved(acquire));
        h.run_steps(2);
        if use_options_menu {
            assert!(h.state().canvas.ground_target_acquired, "moving from the prompt option back into the ring is a fresh entry");
        } else {
            assert!(!h.state().canvas.ground_target_acquired, "Free cannot reacquire while the cursor remains in the acquisition ring");
        }

        h.event(Event::PointerMoved(center + vec2(60.0, 0.0)));
        h.run_steps(2);
        if use_options_menu {
            assert!(h.state().canvas.ground_target_acquired, "acquisition persists after leaving the ring");
        } else {
            assert!(!h.state().canvas.ground_target_acquired, "leaving the ring rearms acquisition without capturing a point");
            h.event(Event::PointerMoved(acquire));
            h.run_steps(2);
            assert!(h.state().canvas.ground_target_acquired, "re-entering the ring after leaving can acquire again");
        }

        let canvas = h.state().canvas.rect.unwrap();
        let outside = pos2(canvas.left() - 10.0, canvas.center().y);
        h.event(Event::PointerMoved(outside));
        h.run_steps(2);
        assert!(!h.state().canvas.ground_target_acquired, "leaving the canvas invalidates the acquired target");

        h.event(Event::PointerMoved(acquire));
        h.run_steps(2);
        assert!(h.state().canvas.ground_target_acquired, "re-entering the canvas can acquire a fresh target");
        let mut changed_camera = h.state().session.camera();
        changed_camera.azimuth += 0.3;
        h.state_mut().session.set_camera(changed_camera);
        h.run_steps(2);
        assert!(!h.state().canvas.ground_target_acquired, "changing camera invalidates the prior target");
        h.event(Event::PointerMoved(acquire));
        h.run_steps(2);
        assert!(!h.state().canvas.ground_target_acquired, "camera invalidation requires leaving and re-entering the new ring");
        let new_center = h.state().canvas.xf.unwrap().to_screen3(cadcraft_geom::Vec3::ZERO);
        h.event(Event::PointerMoved(new_center + vec2(60.0, 0.0)));
        h.run_steps(2);
        h.event(Event::PointerMoved(new_center + vec2(12.0, 0.0)));
        h.run_steps(2);
        assert!(h.state().canvas.ground_target_acquired, "a fresh entry after camera change acquires normally");

        h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
        h.run_steps(2);
        assert!(h.state().session.running.is_none());
        assert!(!h.state().canvas.ground_target_acquired, "cancelling LINE clears the acquired target");
    }
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
    let mut h = harness(Some(ui), 1280.0, 760.0);
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
