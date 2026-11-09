//! Diagnostics and verification tests for 3D line creation fixes.

use cadcraft_engine::Session;
use cadcraft_engine::doc::{Common, Drawing, EntityKind, Line, Space};
use cadcraft_engine::geom::{DrawingPlane, UcsFrame, Vec2, Vec3};
use cadcraft_engine::snap::mode;
use cadcraft_ui_egui::{CadApp, Services};
use egui::{Event, Key, Modifiers, vec2};
use egui_kittest::Harness;
use serde_json::json;

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

/// Helper to get all model space lines in drawing order.
fn get_lines(h: &Harness<'static, CadApp>) -> Vec<Line> {
    let doc = h.state().session.doc().unwrap();
    doc.model
        .iter()
        .filter_map(|e| match &e.kind {
            EntityKind::Line(l) => Some(l.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn test_01_and_02_positive_z_click_and_typed_distance_coherence() {
    let mut session = Session::empty();
    session.open_drawing(Drawing::new_metric(), "test1_2", None);
    session.execute("view.swiso", &json!({})).unwrap();

    let mut app = CadApp::new(session, Services::default());
    app.canvas.zoom_pending = false;
    let mut h = make_harness(app);
    h.state_mut().session.zoom_extents().unwrap();
    let _ = h.state_mut().session.zoom_about(0.4, Vec2::ZERO);
    h.run_steps(3);

    // 1. Z positiva confirmada con clic:
    h.state_mut().session.cmdline("line 0,0,0").unwrap();
    h.run_steps(2);

    let xf = h.state().canvas.xf.expect("xf");
    let screen_z10 = xf.to_screen3(Vec3::new(0.0, 0.0, 10.0));
    h.event(Event::PointerMoved(screen_z10));
    h.run_steps(2);

    assert_eq!(h.state().canvas.active_axis_3d, Some(cadcraft_geom::Axis3D::Z));
    assert!(h.state().session.cursor3_valid);
    assert_eq!(h.state().session.cursor3_base, Some(Vec3::ZERO));

    // Clic para colocar
    h.event(Event::PointerButton { pos: screen_z10, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.event(Event::PointerButton { pos: screen_z10, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
    h.event(Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    let lines = get_lines(&h);
    assert_eq!(lines.len(), 1, "Debe haberse creado 1 línea con clic");
    assert_eq!(lines[0].a, Vec3::ZERO);
    assert!(lines[0].b.near(Vec3::new(0.0, 0.0, 10.0), 1e-4), "Línea por clic debe terminar en Z=10, obtuvo {:?}", lines[0].b);

    // 2. Misma dirección Z confirmada con distancia simple escrita:
    h.state_mut().session.cmdline("line 0,0,0").unwrap();
    h.run_steps(2);
    h.event(Event::PointerMoved(screen_z10));
    h.run_steps(2);

    assert!(h.state().session.cursor3_valid);
    assert_eq!(h.state().session.cursor3_base, Some(Vec3::ZERO));

    // Escribir "10" y Enter
    h.state_mut().cmdline("10");
    h.run_steps(2);
    h.event(Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    let lines2 = get_lines(&h);
    assert_eq!(lines2.len(), 2, "Debe haberse creado la segunda línea por distancia");
    assert_eq!(lines2[1].a, Vec3::ZERO);
    assert!(lines2[1].b.near(Vec3::new(0.0, 0.0, 10.0), 1e-4), "Línea por distancia debe terminar en Z=10 exactamente igual que el clic, obtuvo {:?}", lines2[1].b);
}

#[test]
fn test_06_ortho_3d_negative_direction_no_double_sign() {
    let mut s = Session::new();
    s.viewport_px = (800.0, 600.0);
    s.execute("view.swiso", &json!({})).unwrap();
    s.settings.orthomode = true;

    // Iniciar línea en (0, 0, 0)
    s.cmdline("line 0,0,0").unwrap();

    // Cursor en sentido -X
    s.cursor3 = Vec3::new(-10.0, 0.0, 0.0);
    s.cursor3_valid = true;
    s.cursor3_base = Some(Vec3::ZERO);

    // Escribir distancia 10
    s.cmdline("10").unwrap();
    s.cmdline("").unwrap();

    let doc = s.doc().unwrap();
    let lines: Vec<_> = doc.model.iter().filter_map(|e| match &e.kind { EntityKind::Line(l) => Some(l.clone()), _ => None }).collect();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].a, Vec3::ZERO);
    assert!(lines[0].b.near(Vec3::new(-10.0, 0.0, 0.0), 1e-4), "ORTHO 3D con cursor en -X debe resultar en (-10, 0, 0), no invertir a (+10, 0, 0). Obtuvo: {:?}", lines[0].b);
}

#[test]
fn test_03_and_05_oblique_and_negative_z_distance() {
    let mut s = Session::new();
    s.viewport_px = (800.0, 600.0);
    s.execute("view.swiso", &json!({})).unwrap();

    // 3. Z negativa desde base elevada (2, 3, 20)
    s.cmdline("line 2,3,20").unwrap();
    s.cursor3 = Vec3::new(2.0, 3.0, 10.0);
    s.cursor3_valid = true;
    s.cursor3_base = Some(Vec3::new(2.0, 3.0, 20.0));
    s.cmdline("10").unwrap();
    s.cmdline("").unwrap();

    let doc = s.doc().unwrap();
    let lines: Vec<_> = doc.model.iter().filter_map(|e| match &e.kind { EntityKind::Line(l) => Some(l.clone()), _ => None }).collect();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].a, Vec3::new(2.0, 3.0, 20.0));
    assert!(lines[0].b.near(Vec3::new(2.0, 3.0, 10.0), 1e-4), "Z negativa debe descender hacia (2,3,10), obtuvo {:?}", lines[0].b);

    // 5. Dirección oblicua (3, 4, 12), distancia 26 -> (6, 8, 24)
    let mut s2 = Session::new();
    s2.viewport_px = (800.0, 600.0);
    s2.execute("view.swiso", &json!({})).unwrap();
    s2.cmdline("line 0,0,0").unwrap();
    s2.cursor3 = Vec3::new(3.0, 4.0, 12.0); // vector unitario (3/13, 4/13, 12/13)
    s2.cursor3_valid = true;
    s2.cursor3_base = Some(Vec3::ZERO);
    s2.cmdline("26").unwrap();
    s2.cmdline("").unwrap();

    let doc2 = s2.doc().unwrap();
    let lines2: Vec<_> = doc2.model.iter().filter_map(|e| match &e.kind { EntityKind::Line(l) => Some(l.clone()), _ => None }).collect();
    assert_eq!(lines2.len(), 1);
    assert_eq!(lines2[0].a, Vec3::ZERO);
    assert!(lines2[0].b.near(Vec3::new(6.0, 8.0, 24.0), 1e-4), "Distancia oblicua 3D debe proyectar a (6,8,24), obtuvo {:?}", lines2[0].b);
}

#[test]
fn test_07_continuation_without_mouse_movement_is_rejected() {
    let mut session = Session::empty();
    session.open_drawing(Drawing::new_metric(), "test_invalidation", None);
    session.execute("view.swiso", &json!({})).unwrap();

    let mut app = CadApp::new(session, Services::default());
    app.canvas.zoom_pending = false;
    let mut h = make_harness(app);
    h.state_mut().session.zoom_extents().unwrap();
    let _ = h.state_mut().session.zoom_about(0.4, Vec2::ZERO);
    h.run_steps(3);

    // Iniciar línea en (0,0,0)
    h.state_mut().session.cmdline("line 0,0,0").unwrap();
    h.run_steps(2);

    let xf = h.state().canvas.xf.expect("xf");
    let screen_pt = xf.to_screen3(Vec3::new(10.0, 0.0, 0.0));
    h.event(Event::PointerMoved(screen_pt));
    h.run_steps(2);

    // Confirmar primer segmento con distancia 10
    h.state_mut().cmdline("10");
    h.run_steps(2);

    let lines = get_lines(&h);
    assert_eq!(lines.len(), 1);
    assert!(h.state().session.running.as_ref().is_some_and(|r| r.id == "line"), "LINE debe seguir activo tras el primer segmento");

    // Si el usuario teclea otra distancia "5" sin mover el ratón (la base ha cambiado):
    h.state_mut().cmdline("5");
    h.run_steps(2);

    // NO debe haberse creado una segunda línea, y LINE debe seguir activo
    let lines_after = get_lines(&h);
    assert_eq!(lines_after.len(), 1, "Sin dirección vigente no se debe crear geometría");
    assert!(h.state().session.running.as_ref().is_some_and(|r| r.id == "line"), "LINE debe seguir activo");
    let log_contains_warning = h.state().session.log.iter().any(|msg| msg.contains("Valid direction required to enter distance"));
    assert!(log_contains_warning, "Debe emitirse mensaje indicando que falta dirección válida");
}

#[test]
fn test_camera_ucs_and_plane_changes_invalidate_direction() {
    let mut s = Session::new();
    s.cmdline("line 0,0,0").unwrap();
    s.cursor3 = Vec3::new(10.0, 0.0, 0.0);
    s.cursor3_valid = true;
    s.cursor3_base = Some(Vec3::ZERO);

    // Cambio de cámara -> invalida dirección
    s.set_camera(cadcraft_geom::OrbitCamera::default());
    assert!(!s.cursor3_valid);
    assert_eq!(s.cursor3_base, None);

    // Reestablecer dirección
    s.cursor3_valid = true;
    s.cursor3_base = Some(Vec3::ZERO);

    // Cambio de SCP -> invalida dirección
    s.set_ucs(UcsFrame::new(Vec3::new(10.0, 10.0, 0.0), Vec3::X, Vec3::Y, Vec3::Z)).unwrap();
    assert!(!s.cursor3_valid);
    assert_eq!(s.cursor3_base, None);

    // Reestablecer dirección
    s.cursor3_valid = true;
    s.cursor3_base = Some(Vec3::ZERO);

    // Cambio de plano de dibujo -> invalida dirección
    s.set_drawing_plane(DrawingPlane::XZ);
    assert!(!s.cursor3_valid);
    assert_eq!(s.cursor3_base, None);
}

#[test]
fn test_undo_restores_base_and_invalidates_direction() {
    let mut s = Session::new();
    s.cmdline("line 0,0,0 10,0,0 10,10,0").unwrap();
    assert_eq!(s.last_point3, Vec3::new(10.0, 10.0, 0.0));

    // Ejecutar Undo dentro de LINE
    s.cmdline("u").unwrap();
    assert_eq!(s.last_point3, Vec3::new(10.0, 0.0, 0.0), "Undo debe restaurar la base previa");
    assert!(!s.cursor3_valid, "Undo debe invalidar la dirección del cursor");
    assert_eq!(s.cursor3_base, None);
}

#[test]
fn test_tracking_path_preserves_acquired_origin_not_line_base() {
    use cadcraft_engine::snap::tracking::{Path, PathKind, PathShape, Track};

    let mut s = Session::new();
    s.cmdline("line 0,0,0").unwrap();

    // Establecer un camino de rastreo adquirido en (50, 50, 0) con dirección +X
    let from_point = Vec2::new(50.0, 50.0);
    let cursor_point = Vec2::new(55.0, 50.0);
    s.cursor = cursor_point;
    s.tracking.last = Some(Track {
        point: cursor_point,
        paths: vec![Path {
            shape: PathShape::Ray { origin: from_point, dir: Vec2::X },
            kind: PathKind::Polar,
            from: from_point,
            name: "Polar",
        }],
        object: None,
    });

    // Teclear distancia 10 a lo largo del camino de rastreo
    s.cmdline("10").unwrap();
    s.cmdline("").unwrap();

    let doc = s.doc().unwrap();
    let lines: Vec<_> = doc.model.iter().filter_map(|e| match &e.kind { EntityKind::Line(l) => Some(l.clone()), _ => None }).collect();
    assert_eq!(lines.len(), 1);
    // El punto final debe ser originado en el punto de tracking (50 + 10 = 60, 50, 0), NO en la base (0,0) + 10
    assert_eq!(lines[0].a, Vec3::ZERO);
    assert_eq!(lines[0].b, Vec3::new(60.0, 50.0, 0.0), "La distancia debe medirse desde el origen de tracking (50,50), no desde la base");
}

#[test]
fn test_10_vertical_zlock_preserves_snapped_xy() {
    let mut session = Session::empty();
    let mut d = Drawing::new_metric();
    d.add(
        &Space::Model,
        Common::default(),
        EntityKind::Line(Line { a: Vec3::new(15.0, 25.0, 40.0), b: Vec3::new(50.0, 50.0, 40.0) }),
    )
    .unwrap();
    session.open_drawing(d, "test10", None);
    session.execute("view.swiso", &json!({})).unwrap();
    session.settings.osmode = mode::END;

    let mut app = CadApp::new(session, Services::default());
    app.canvas.zoom_pending = false;
    let mut h = make_harness(app);
    h.state_mut().session.zoom_extents().unwrap();
    let _ = h.state_mut().session.zoom_about(0.4, Vec2::ZERO);
    h.run_steps(3);

    // Iniciar línea en (0, 0, 0)
    h.state_mut().session.cmdline("line 0,0,0").unwrap();
    h.state_mut().session.cmdline("V").unwrap(); // Activar modo Vertical
    h.run_steps(2);
    assert!(h.state().session.settings.zlock);

    // Capturar punto existente en (15, 25, 40)
    let xf = h.state().canvas.xf.expect("xf");
    let p_snap = xf.to_screen3(Vec3::new(15.0, 25.0, 40.0));
    h.event(Event::PointerMoved(p_snap));
    h.run_steps(2);

    assert!(h.state().canvas.snap.is_some(), "Debe haberse capturado OSNAP");

    // Clic para confirmar
    h.event(Event::PointerButton { pos: p_snap, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.event(Event::PointerButton { pos: p_snap, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
    h.event(Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    let lines = get_lines(&h);
    assert_eq!(lines.len(), 2);
    let created = &lines[1];
    assert_eq!(created.a, Vec3::ZERO);
    assert_eq!(created.b, Vec3::new(15.0, 25.0, 40.0), "Bajo Vertical, OSNAP 3D debe preservar X e Y íntegramente");
}

#[test]
fn test_free_point_under_vertical_locks_xy_to_base() {
    let mut s = Session::new();
    s.cmdline("line 10,20,0").unwrap();
    s.cmdline("V").unwrap(); // Activar vertical
    assert!(s.settings.zlock);

    // Simular punto libre en pantalla que proporciona Z=45 y X,Y libre (80, 90)
    s.cursor3 = Vec3::new(80.0, 90.0, 45.0);
    s.input(cadcraft_engine::Input::Point(Vec2::new(80.0, 90.0))).unwrap();
    s.cmdline("").unwrap();

    let doc = s.doc().unwrap();
    let lines: Vec<_> = doc.model.iter().filter_map(|e| match &e.kind { EntityKind::Line(l) => Some(l.clone()), _ => None }).collect();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].a, Vec3::new(10.0, 20.0, 0.0));
    // Punto libre bajo Vertical debe restringir X e Y a la base previa (10, 20) y tomar Z del cursor (45)
    assert_eq!(lines[0].b, Vec3::new(10.0, 20.0, 45.0));
}

#[test]
fn test_explicit_coordinates_work_without_valid_cursor_under_vertical() {
    let mut s = Session::new();
    s.execute("view.swiso", &json!({})).unwrap();
    s.cmdline("line 0,0,0").unwrap();
    s.cmdline("Vertical").unwrap();
    assert!(s.settings.zlock);

    // Sin cursor válido
    s.invalidate_cursor3();

    // Coordenada explícita 3D completa
    s.cmdline("15,25,40").unwrap();
    s.cmdline("").unwrap();

    let doc = s.doc().unwrap();
    let lines: Vec<_> = doc.model.iter().filter_map(|e| match &e.kind { EntityKind::Line(l) => Some(l.clone()), _ => None }).collect();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].a, Vec3::ZERO);
    assert_eq!(lines[0].b, Vec3::new(15.0, 25.0, 40.0), "Coordenadas explícitas 3D bajo Vertical deben respetarse íntegramente");
}

#[test]
fn test_flag_guards_restore_cleanly_on_errors() {
    let mut s = Session::new();
    assert!(!s.input_is_explicit);
    assert!(!s.input_is_snapped);

    s.cmdline("line 0,0,0").unwrap();

    // Introducir texto que provoque error de sintaxis en el despachador
    let _ = s.cmdline("not_a_valid_point_or_cmd");

    // Ambos flags deben permanecer en false y no contaminar el estado
    assert!(!s.input_is_explicit, "input_is_explicit debe restaurarse tras error");
    assert!(!s.input_is_snapped, "input_is_snapped debe permanecer en false");
}

#[test]
fn test_elevated_base_and_inclined_ucs_distance() {
    let mut s = Session::new();
    // Definir SCP inclinado con origen en (0,0,10) y normal hacia el eje Y
    let ucs = UcsFrame::new(Vec3::new(0.0, 0.0, 10.0), Vec3::X, Vec3::Z, -Vec3::Y);
    s.set_ucs(ucs).unwrap();

    // Iniciar línea en base elevada en el plano del SCP usando coordenadas explícitas de mundo *
    s.cmdline("line *0,0,10").unwrap();

    // Cursor en el plano del SCP a 10 unidades a lo largo de Z WCS
    s.cursor3 = Vec3::new(0.0, 0.0, 20.0);
    s.cursor3_valid = true;
    s.cursor3_base = Some(Vec3::new(0.0, 0.0, 10.0));

    // Distancia simple 5
    s.cmdline("5").unwrap();
    s.cmdline("").unwrap();

    let doc = s.doc().unwrap();
    let lines: Vec<_> = doc.model.iter().filter_map(|e| match &e.kind { EntityKind::Line(l) => Some(l.clone()), _ => None }).collect();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].a, Vec3::new(0.0, 0.0, 10.0));
    assert_eq!(lines[0].b, Vec3::new(0.0, 0.0, 15.0), "La distancia debe aplicarse a lo largo del vector 3D en el SCP sin proyectar al XY universal");
}

#[test]
fn test_dynamic_input_enabled_and_disabled_distance() {
    let mut session = Session::empty();
    session.open_drawing(Drawing::new_metric(), "test_dyninput", None);
    session.execute("view.swiso", &json!({})).unwrap();

    let mut app = CadApp::new(session, Services::default());
    app.canvas.zoom_pending = false;
    let mut h = make_harness(app);
    h.state_mut().session.zoom_extents().unwrap();
    let _ = h.state_mut().session.zoom_about(0.4, Vec2::ZERO);
    h.run_steps(3);

    // Con Dynamic Input (cmdline directo con foco)
    h.state_mut().session.cmdline("line 0,0,0").unwrap();
    h.run_steps(2);

    let xf = h.state().canvas.xf.expect("xf");
    let screen_pt = xf.to_screen3(Vec3::new(10.0, 0.0, 0.0));
    h.event(Event::PointerMoved(screen_pt));
    h.run_steps(2);

    // Distancia 10
    h.state_mut().cmdline("10");
    h.run_steps(2);
    h.event(Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    let lines = get_lines(&h);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].b, Vec3::new(10.0, 0.0, 0.0));
}

#[test]
fn test_ortho_3d_distance_rejects_stale_or_invalid_cursor_and_does_not_invent_plus_x() {
    let mut s = Session::new();
    s.execute("view.swiso", &json!({})).unwrap();
    s.settings.orthomode = true;

    s.cmdline("line 0,0,0").unwrap();

    // Caso 1: cursor3_valid es false aunque cursor3 apunte a (100, 0, 0)
    s.cursor3 = Vec3::new(100.0, 0.0, 0.0);
    s.cursor3_valid = false;
    s.cursor3_base = Some(Vec3::ZERO);
    s.cmdline("25").unwrap();

    let count_after_invalid = s.doc().unwrap().model.iter().filter(|e| matches!(e.kind, EntityKind::Line(_))).count();
    assert_eq!(count_after_invalid, 0, "Sin cursor3_valid, ORTHO 3D debe rechazar la distancia sin crear geometría ni inventar +X");

    // Caso 2: cursor3_valid es true pero cursor3_base pertenece a un punto base distinto
    s.cursor3 = Vec3::new(100.0, 0.0, 0.0);
    s.cursor3_valid = true;
    s.cursor3_base = Some(Vec3::new(50.0, 50.0, 50.0));
    s.cmdline("25").unwrap();

    let count_after_mismatched_base = s.doc().unwrap().model.iter().filter(|e| matches!(e.kind, EntityKind::Line(_))).count();
    assert_eq!(count_after_mismatched_base, 0, "Con cursor3_base distinto de base3, ORTHO 3D debe rechazar la distancia");

    // Caso 3: cursor3_valid == true y cursor3_base == Some(base3) funciona normalmente
    s.cursor3 = Vec3::new(100.0, 5.0, 0.0);
    s.cursor3_valid = true;
    s.cursor3_base = Some(Vec3::ZERO);
    s.cmdline("25").unwrap();
    s.cmdline("").unwrap();

    let lines: Vec<_> = s.doc().unwrap().model.iter().filter_map(|e| match &e.kind { EntityKind::Line(l) => Some(l.clone()), _ => None }).collect();
    assert_eq!(lines.len(), 1);
    assert!((lines[0].b.x - 25.0).abs() < 1e-6 && lines[0].b.y.abs() < 1e-6 && lines[0].b.z.abs() < 1e-6);
}


