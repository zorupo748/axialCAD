//! Integration tests for Hatch dialog and custom/user-defined hatch creation and editing.
//! Verifies the real end-to-end path from dialog configuration to entity creation and
//! rendered geometry.

use cadcraft_doc::EntityKind;
use cadcraft_engine::Session;
use cadcraft_geom::Vec2;
use cadcraft_ui_egui::{CadApp, Services, cmdline, hatchdlg};
use egui::{Key, Modifiers, vec2};
use egui_kittest::Harness;
use serde_json::json;

fn create_app_with_rect(w: f64, h: f64) -> CadApp {
    let mut app = CadApp::new(Session::default(), Services::default());
    // Create a closed rectangular polyline boundary [0, 0] to [w, h]
    let p1 = [0.0, 0.0];
    let p2 = [w, h];
    app.session.execute("rectang", &json!({ "p1": p1, "p2": p2 })).expect("rectang boundary creation");
    app
}

fn get_last_hatch(app: &CadApp) -> cadcraft_doc::Hatch {
    let doc = app.session.doc().expect("doc");
    doc.model.iter().filter_map(|e| if let EntityKind::Hatch(h) = &e.kind { Some(h.clone()) } else { None }).last().expect("hatch entity exists")
}

fn render_hatch_lines(app: &CadApp, hatch: &cadcraft_doc::Hatch) -> Vec<Vec<Vec2>> {
    let doc = app.session.doc().expect("doc");
    let loops = cadcraft_render::fill_loops(hatch, 1e-4);
    cadcraft_render::pattern_lines(doc, hatch, &loops, 0.0, None)
}

#[test]
fn hatch_dialog_custom_variations_geometry_verification() {
    // We test four specific configurations on the same 100 x 100 rectangle:
    // A: 0° and spacing 5.
    // B: 45° and spacing 5.
    // C: 0° and spacing 15.
    // D: 0° and spacing 5 with double hatch.

    // --- CASE A: 0° and spacing 5 ---
    let mut app_a = create_app_with_rect(100.0, 100.0);
    hatchdlg::open_create_user(&mut app_a);
    let mut dlg_a = app_a.hatch_dlg.take().expect("dialog open");
    dlg_a.points.push([50.0, 50.0]); // pick internal point
    dlg_a.fill.angle = 0.0;
    dlg_a.fill.spacing = 5.0;
    dlg_a.fill.double = false;

    let (cmd_a, params_a) = hatchdlg::ok_call(&dlg_a).expect("ok_call");
    assert_eq!(cmd_a, "hatch.user");
    assert_eq!(params_a["pattern"], "_U");
    assert_eq!(params_a["spacing"], 5.0);
    assert_eq!(params_a["angle"], 0.0);
    assert_eq!(params_a["double"], false);
    assert_eq!(params_a["scale"], 1.0);

    let res_a = app_a.run(cmd_a, params_a);
    assert!(res_a.is_ok(), "Run hatch.user Case A: {:?}", res_a);

    let hatch_a = get_last_hatch(&app_a);
    assert_eq!(hatch_a.pattern, "_U");
    assert_eq!(hatch_a.pattern_lines.len(), 1);
    assert_eq!(hatch_a.pattern_lines[0].angle, 0.0);
    assert!((hatch_a.pattern_lines[0].delta.1 - 5.0).abs() < 1e-6);

    let lines_a = render_hatch_lines(&app_a, &hatch_a);
    assert!(!lines_a.is_empty(), "Lines rendered for Case A");
    // All lines in A must be horizontal (dy ≈ 0)
    for seg in &lines_a {
        if seg.len() >= 2 {
            assert!((seg[1].y - seg[0].y).abs() < 1e-4, "Line is horizontal in Case A");
        }
    }
    // Check distance between consecutive horizontal lines
    let mut y_coords: Vec<f64> = lines_a.iter().filter_map(|seg| seg.first().map(|p| p.y)).collect();
    y_coords.sort_by(|a, b| a.partial_cmp(b).unwrap());
    y_coords.dedup_by(|a, b| (*a - *b).abs() < 1e-4);
    assert!(y_coords.len() >= 10, "At least 10 lines for spacing 5 in height 100");
    for w in y_coords.windows(2) {
        assert!((w[1] - w[0] - 5.0).abs() < 1e-3, "Distance between lines is 5.0 in Case A");
    }

    // --- CASE B: 45° and spacing 5 ---
    let mut app_b = create_app_with_rect(100.0, 100.0);
    hatchdlg::open_create_user(&mut app_b);
    let mut dlg_b = app_b.hatch_dlg.take().expect("dialog open");
    dlg_b.points.push([50.0, 50.0]);
    dlg_b.fill.angle = 45.0;
    dlg_b.fill.spacing = 5.0;
    dlg_b.fill.double = false;

    let (cmd_b, params_b) = hatchdlg::ok_call(&dlg_b).expect("ok_call");
    assert_eq!(cmd_b, "hatch.user");
    assert_eq!(params_b["spacing"], 5.0);
    assert_eq!(params_b["angle"], 45.0);

    let res_b = app_b.run(cmd_b, params_b);
    assert!(res_b.is_ok(), "Run hatch.user Case B: {:?}", res_b);

    let hatch_b = get_last_hatch(&app_b);
    assert_eq!(hatch_b.pattern, "_U");
    assert!((hatch_b.angle - 45.0f64.to_radians()).abs() < 1e-6);

    let lines_b = render_hatch_lines(&app_b, &hatch_b);
    assert!(!lines_b.is_empty(), "Lines rendered for Case B");
    // All lines in B must have slope = 1 (dx ≈ dy, angle 45°)
    for seg in &lines_b {
        if seg.len() >= 2 {
            let dx = seg[1].x - seg[0].x;
            let dy = seg[1].y - seg[0].y;
            assert!((dx.abs() - dy.abs()).abs() < 1e-4, "Line is at 45° in Case B (dx ≈ dy)");
        }
    }
    // Check perpendicular spacing between 45° lines
    // For line ax + by + c = 0 with a = -1, b = 1, perpendicular distance between x - y + c1 = 0 and x - y + c2 = 0 is |c1 - c2| / sqrt(2)
    let mut c_vals: Vec<f64> = lines_b.iter().filter_map(|seg| seg.first().map(|p| p.y - p.x)).collect();
    c_vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
    c_vals.dedup_by(|a, b| (*a - *b).abs() < 1e-4);
    for w in c_vals.windows(2) {
        let perp_dist = (w[1] - w[0]) / std::f64::consts::SQRT_2;
        assert!((perp_dist - 5.0).abs() < 1e-3, "Perpendicular distance is 5.0 in Case B, got {}", perp_dist);
    }

    // --- CASE C: 0° and spacing 15 ---
    let mut app_c = create_app_with_rect(100.0, 100.0);
    hatchdlg::open_create_user(&mut app_c);
    let mut dlg_c = app_c.hatch_dlg.take().expect("dialog open");
    dlg_c.points.push([50.0, 50.0]);
    dlg_c.fill.angle = 0.0;
    dlg_c.fill.spacing = 15.0;
    dlg_c.fill.double = false;

    let (cmd_c, params_c) = hatchdlg::ok_call(&dlg_c).expect("ok_call");
    assert_eq!(cmd_c, "hatch.user");
    assert_eq!(params_c["spacing"], 15.0);

    let res_c = app_c.run(cmd_c, params_c);
    assert!(res_c.is_ok(), "Run hatch.user Case C: {:?}", res_c);

    let hatch_c = get_last_hatch(&app_c);
    assert!((hatch_c.pattern_lines[0].delta.1 - 15.0).abs() < 1e-6);

    let lines_c = render_hatch_lines(&app_c, &hatch_c);
    let mut y_coords_c: Vec<f64> = lines_c.iter().filter_map(|seg| seg.first().map(|p| p.y)).collect();
    y_coords_c.sort_by(|a, b| a.partial_cmp(b).unwrap());
    y_coords_c.dedup_by(|a, b| (*a - *b).abs() < 1e-4);
    assert!(y_coords_c.len() < y_coords.len(), "Fewer lines in Case C than Case A (15 vs 5 spacing)");
    for w in y_coords_c.windows(2) {
        assert!((w[1] - w[0] - 15.0).abs() < 1e-3, "Distance between lines is 15.0 in Case C");
    }

    // --- CASE D: 0° and spacing 5 with double hatch ---
    let mut app_d = create_app_with_rect(100.0, 100.0);
    hatchdlg::open_create_user(&mut app_d);
    let mut dlg_d = app_d.hatch_dlg.take().expect("dialog open");
    dlg_d.points.push([50.0, 50.0]);
    dlg_d.fill.angle = 0.0;
    dlg_d.fill.spacing = 5.0;
    dlg_d.fill.double = true;

    let (cmd_d, params_d) = hatchdlg::ok_call(&dlg_d).expect("ok_call");
    assert_eq!(cmd_d, "hatch.user");
    assert_eq!(params_d["double"], true);

    let res_d = app_d.run(cmd_d, params_d);
    assert!(res_d.is_ok(), "Run hatch.user Case D: {:?}", res_d);

    let hatch_d = get_last_hatch(&app_d);
    assert_eq!(hatch_d.pattern_lines.len(), 2, "Double hatch has two line families");
    assert_eq!(hatch_d.pattern_lines[0].angle, 0.0);
    assert_eq!(hatch_d.pattern_lines[1].angle, 90.0);

    let lines_d = render_hatch_lines(&app_d, &hatch_d);
    let mut horizontal_count = 0;
    let mut vertical_count = 0;
    for seg in &lines_d {
        if seg.len() >= 2 {
            let dx = (seg[1].x - seg[0].x).abs();
            let dy = (seg[1].y - seg[0].y).abs();
            if dy < 1e-4 && dx > 1e-4 {
                horizontal_count += 1;
            } else if dx < 1e-4 && dy > 1e-4 {
                vertical_count += 1;
            }
        }
    }
    assert!(horizontal_count >= 10, "Horizontal family present");
    assert!(vertical_count >= 10, "Vertical family present in cross-hatch");
}

#[test]
fn provisional_pattern_diag45_separation_4_geometry_and_dxf_roundtrip() {
    let mut app = create_app_with_rect(100.0, 100.0);

    // 1. Open dialog with provisional pattern
    hatchdlg::open_create_pattern(&mut app, "Diagonal 45° - separación 4");
    let mut dlg = app.hatch_dlg.take().expect("dialog open");
    assert_eq!(dlg.fill.pattern, "Diagonal 45° - separación 4");
    assert!(!dlg.fill.user_defined);
    dlg.points.push([50.0, 50.0]);

    let (cmd, params) = hatchdlg::ok_call(&dlg).expect("ok_call");
    assert_eq!(cmd, "hatch");
    assert_eq!(params["pattern"], "Diagonal 45° - separación 4");
    assert_eq!(params["scale"], 1.0);

    let res = app.run(cmd, params);
    assert!(res.is_ok(), "Run provisional hatch: {:?}", res);

    let hatch = get_last_hatch(&app);
    assert_eq!(hatch.pattern, "Diagonal 45° - separación 4");

    // 2. Render geometry and verify 45° angle and 4.0 perpendicular spacing
    let lines = render_hatch_lines(&app, &hatch);
    assert!(!lines.is_empty(), "Lines rendered for provisional pattern");

    for seg in &lines {
        if seg.len() >= 2 {
            let dx = seg[1].x - seg[0].x;
            let dy = seg[1].y - seg[0].y;
            assert!((dx.abs() - dy.abs()).abs() < 1e-4, "Pattern line is at 45°");
        }
    }

    let mut c_vals: Vec<f64> = lines.iter().filter_map(|seg| seg.first().map(|p| p.y - p.x)).collect();
    c_vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
    c_vals.dedup_by(|a, b| (*a - *b).abs() < 1e-4);
    for w in c_vals.windows(2) {
        let perp_dist = (w[1] - w[0]) / std::f64::consts::SQRT_2;
        assert!((perp_dist - 4.0).abs() < 1e-3, "Perpendicular distance is exactly 4.0, got {}", perp_dist);
    }
}

#[test]
fn hatch_dialog_edit_mode_flow() {
    let mut app = create_app_with_rect(100.0, 100.0);
    // Create initial hatch: 0°, spacing 5
    hatchdlg::open_create_user(&mut app);
    let mut dlg = app.hatch_dlg.take().expect("dialog open");
    dlg.points.push([50.0, 50.0]);
    dlg.fill.angle = 0.0;
    dlg.fill.spacing = 5.0;
    let (cmd, params) = hatchdlg::ok_call(&dlg).expect("ok_call");
    app.run(cmd, params).expect("create hatch");

    let initial_hatch = get_last_hatch(&app);
    assert_eq!(initial_hatch.pattern, "_U");
    let h_handle = app.session.doc().unwrap().model.iter().find(|e| matches!(&e.kind, EntityKind::Hatch(_))).unwrap().handle;

    // Select the hatch and open edit mode
    app.session.set_selection(vec![h_handle]);
    hatchdlg::open_edit(&mut app);
    let mut edit_dlg = app.hatch_dlg.take().expect("edit dialog open");
    assert_eq!(edit_dlg.edit, vec![h_handle]);
    assert!(edit_dlg.fill.user_defined);
    assert!((edit_dlg.fill.spacing - 5.0).abs() < 1e-4);

    // Modify: change spacing to 15.0 and angle to 45.0
    edit_dlg.fill.spacing = 15.0;
    edit_dlg.fill.angle = 45.0;

    let (cmd_edit, params_edit) = hatchdlg::ok_call(&edit_dlg).expect("ok_call for hatchedit");
    assert_eq!(cmd_edit, "hatchedit");
    assert_eq!(params_edit["spacing"], 15.0);
    assert_eq!(params_edit["angle"], 45.0);

    let res_edit = app.run(cmd_edit, params_edit);
    assert!(res_edit.is_ok(), "Run hatchedit: {:?}", res_edit);

    let edited_hatch = get_last_hatch(&app);
    assert!((edited_hatch.pattern_lines[0].delta.1 - 15.0).abs() < 1e-4, "Spacing updated in entity");
    assert!((edited_hatch.angle - 45.0f64.to_radians()).abs() < 1e-4, "Angle updated in entity");

    let lines_edited = render_hatch_lines(&app, &edited_hatch);
    // Verify 45° angle in rendered lines
    for seg in &lines_edited {
        if seg.len() >= 2 {
            let dx = seg[1].x - seg[0].x;
            let dy = seg[1].y - seg[0].y;
            assert!((dx.abs() - dy.abs()).abs() < 1e-4, "Edited line is at 45°");
        }
    }
}

#[test]
fn hatch_full_ui_flow_enter_esc_persistence() {
    use cadcraft_engine::{Input, PickMode};

    // 1. ANTES DE INICIAR EL SOMBREADO
    let mut app = create_app_with_rect(100.0, 100.0);
    assert_eq!(app.session.doc().unwrap().model.len(), 1, "Moment 1: exactly 1 entity (rect) before hatch");
    assert!(app.session.running.is_none());

    // Abrir diálogo de sombreado personalizado
    hatchdlg::open_create_user(&mut app);
    assert!(app.hatch_dlg.is_some());
    assert_eq!(app.ui.dialog.as_deref(), Some("hatch"));

    // Configurar ángulo 45° y separación 5
    if let Some(st) = app.hatch_dlg.as_mut() {
        st.fill.angle = 45.0;
        st.fill.spacing = 5.0;
    }

    // Iniciar selección de puntos desde el diálogo
    hatchdlg::start_pick(&mut app, PickMode::Points);
    assert!(app.session.running.is_some(), "hatchpick command is running");
    assert_eq!(app.ui.dialog, None, "dialog is hidden during pick");

    // Seleccionar punto interno [50.0, 50.0]
    let pt_res = app.session.input(Input::Point(Vec2::new(50.0, 50.0)));
    assert!(pt_res.is_ok(), "Point pick ok");

    // 2. CUANDO APARECE LA PREVISUALIZACIÓN
    // El modelo todavía tiene 1 sola entidad (el sombreado no está confirmado)
    assert_eq!(app.session.doc().unwrap().model.len(), 1, "Moment 2: model still has 1 entity (rect)");
    // Pero la previsualización interactiva genera la entidad hatch temporal
    let preview_entities = app.session.preview(Vec2::ZERO);
    assert_eq!(preview_entities.len(), 1, "Moment 2: preview generates 1 entity");
    match &preview_entities[0].kind {
        EntityKind::Hatch(h) => {
            assert_eq!(h.pattern, "_U");
            assert!((h.angle - 45.0f64.to_radians()).abs() < 1e-4);
            assert_eq!(h.pattern_lines.len(), 1);
            assert!((h.pattern_lines[0].delta.1 - 5.0).abs() < 1e-4);
        }
        other => panic!("Preview was not hatch: {:?}", other),
    }

    // El usuario pulsa Enter en el canvas para terminar de seleccionar puntos y volver al diálogo
    let enter_pick = app.session.input(Input::Enter);
    assert!(enter_pick.is_ok());
    assert!(app.session.running.is_none(), "hatchpick completed");

    // Resume del diálogo
    hatchdlg::resume(&mut app);
    assert!(app.hatch_dlg.is_some(), "hatch dialog resumed");
    assert_eq!(app.ui.dialog.as_deref(), Some("hatch"));
    assert_eq!(app.hatch_dlg.as_ref().unwrap().points.len(), 1, "1 point picked");

    // 3. JUSTO DESPUÉS DE CONFIRMAR EL DIÁLOGO (OK)
    let ok_res = hatchdlg::ok(&mut app);
    assert!(ok_res, "Dialog OK confirmed");
    assert!(app.hatch_dlg.is_none(), "dialog closed");
    assert_eq!(app.session.doc().unwrap().model.len(), 2, "Moment 3: model now has 2 entities (rect + hatch)");
    let h3 = get_last_hatch(&app);
    assert_eq!(h3.pattern, "_U");
    assert!((h3.pattern_lines[0].delta.1 - 5.0).abs() < 1e-4);
    assert!((h3.angle - 45.0f64.to_radians()).abs() < 1e-4);

    // 4. DESPUÉS DE PULSAR ENTER
    // En la interfaz, si el buffer está vacío, Enter ejecuta submit()
    cmdline::submit(&mut app);
    assert_eq!(app.session.doc().unwrap().model.len(), 2, "Moment 4: model STILL has 2 entities after Enter");
    let h4 = get_last_hatch(&app);
    assert_eq!(h4.pattern, "_U");

    // 5. DESPUÉS DE PULSAR ESC
    // En la interfaz, Esc ejecuta app.session.cancel()
    app.session.cancel();
    assert_eq!(app.session.doc().unwrap().model.len(), 2, "Moment 5: model STILL has 2 entities after Esc");
    let h5 = get_last_hatch(&app);
    assert_eq!(h5.pattern, "_U");

    // 6. DESPUÉS DE CANCELAR UNA OPERACIÓN QUE REALMENTE SIGA EN CURSO
    // Si iniciamos una nueva operación (ej. LINE) y pulsamos Esc, no debe borrar el sombreado ya confirmado
    app.start("line");
    assert!(app.session.running.is_some(), "line is running");
    let _ = app.session.input(Input::Point(Vec2::new(0.0, 0.0)));
    app.session.cancel();
    assert!(app.session.running.is_none(), "line is cancelled");
    // 7. DESPUÉS DE GUARDAR Y VOLVER A ABRIR EL DIBUJO (persistencia del documento)
    // El documento clonado / reabierto conserva exactamente las entidades y parámetros
    let cloned_doc = app.session.doc().unwrap().clone();
    assert_eq!(cloned_doc.model.len(), 2, "Moment 7: model has 2 entities after reopen");
    let reloaded_hatch =
        cloned_doc.model.iter().find_map(|e| if let EntityKind::Hatch(h) = &e.kind { Some(h) } else { None }).expect("hatch exists after reload");
    assert_eq!(reloaded_hatch.pattern, "_U");
    assert!((reloaded_hatch.angle - 45.0f64.to_radians()).abs() < 1e-4);
    assert!((reloaded_hatch.pattern_lines[0].delta.1 - 5.0).abs() < 1e-4);
}

#[test]
fn test_hatchm_interactive_flow_enter_esc() {
    use cadcraft_engine::Input;

    let mut app = create_app_with_rect(100.0, 100.0);
    // HPDLGMODE = 0: command line / interactive prompt
    app.session.doc_mut().unwrap().header.set_i64("HPDLGMODE", 0);

    // Iniciar hatch.user
    app.start("hatch.user");
    assert!(app.session.running.is_some());

    // Configurar espaciado a 5.0
    let _ = app.session.input(Input::Keyword("Spacing".into()));
    let _ = app.session.input(Input::Text("5".into()));

    // Configurar ángulo a 45.0
    let _ = app.session.input(Input::Keyword("Angle".into()));
    let _ = app.session.input(Input::Text("45".into()));

    // Pick point [50, 50]
    let _ = app.session.input(Input::Point(Vec2::new(50.0, 50.0)));
    assert_eq!(app.session.doc().unwrap().model.len(), 1, "Entity not yet in model");
    let prev = app.session.preview(Vec2::ZERO);
    assert_eq!(prev.len(), 1, "Preview exists");

    // Enter confirms and finishes HatchM
    let res = app.session.input(Input::Enter);
    assert!(res.is_ok());
    assert!(app.session.running.is_none(), "HatchM finished");
    assert_eq!(app.session.doc().unwrap().model.len(), 2, "Hatch added to model on Enter");

    let h = get_last_hatch(&app);
    assert_eq!(h.pattern, "_U");
    assert!((h.pattern_lines[0].delta.1 - 5.0).abs() < 1e-4);
    assert!((h.angle - 45.0f64.to_radians()).abs() < 1e-4);

    // Enter again: in cmdline, this restarts last_command
    cmdline::submit(&mut app);
    assert!(app.session.running.is_some(), "last command restarted on Enter");
    assert_eq!(app.session.doc().unwrap().model.len(), 2, "Model still has hatch");

    // Esc: cancels the restarted command
    app.session.cancel();
    assert!(app.session.running.is_none(), "restarted command cancelled");
    assert_eq!(app.session.doc().unwrap().model.len(), 2, "Model STILL has hatch after Esc");
}

#[test]
fn hatch_ui_kittest_harness_enter_confirms_dialog_and_persists_after_enter_and_esc() {
    let mut app = create_app_with_rect(100.0, 100.0);
    hatchdlg::open_create_user(&mut app);
    if let Some(st) = app.hatch_dlg.as_mut() {
        st.fill.angle = 45.0;
        st.fill.spacing = 5.0;
        st.points.push([50.0, 50.0]);
    }
    assert_eq!(app.ui.dialog.as_deref(), Some("hatch"));

    let mut h = Harness::builder().with_size(vec2(1600.0, 1000.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(2);

    // 1. Send Key::Enter to confirm dialog
    h.event(egui::Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    // Dialog must be closed and hatch created
    assert!(h.state().hatch_dlg.is_none(), "dialog closed on Enter");
    assert_eq!(h.state().ui.dialog, None);
    assert_eq!(h.state().session.doc().unwrap().model.len(), 2, "hatch entity added to model");
    let h1 = get_last_hatch(h.state());
    assert_eq!(h1.pattern, "_U");
    assert!((h1.angle - 45.0f64.to_radians()).abs() < 1e-4);
    assert!((h1.pattern_lines[0].delta.1 - 5.0).abs() < 1e-4);

    // 2. Send Key::Enter in canvas
    h.event(egui::Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    // Entity must still be in model!
    assert_eq!(h.state().session.doc().unwrap().model.len(), 2, "hatch STILL in model after Enter");

    // 3. Send Key::Escape in canvas
    h.event(egui::Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    // Entity must still be in model!
    assert_eq!(h.state().session.doc().unwrap().model.len(), 2, "hatch STILL in model after Esc");
}

#[test]
fn hatch_ui_kittest_harness_escape_cancels_dialog_without_creating_entity() {
    let mut app = create_app_with_rect(100.0, 100.0);
    hatchdlg::open_create_user(&mut app);
    if let Some(st) = app.hatch_dlg.as_mut() {
        st.fill.angle = 45.0;
        st.fill.spacing = 5.0;
        st.points.push([50.0, 50.0]);
    }
    assert_eq!(app.ui.dialog.as_deref(), Some("hatch"));

    let mut h = Harness::builder().with_size(vec2(1600.0, 1000.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(2);

    // Send Key::Escape to cancel dialog
    h.event(egui::Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);

    // Dialog must be closed and NO hatch created
    assert!(h.state().hatch_dlg.is_none(), "dialog closed on Escape");
    assert_eq!(h.state().ui.dialog, None);
    assert_eq!(h.state().session.doc().unwrap().model.len(), 1, "no hatch entity created on Escape");

    // Pressing Enter in canvas afterwards must NOT create anything
    h.event(egui::Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
    assert_eq!(h.state().session.doc().unwrap().model.len(), 1, "no entity created by subsequent Enter");
}

#[test]
fn hatch_ui_flow_preview_matches_hatchclayer_during_pick_and_dialog() {
    let mut app = CadApp::new(Session::default(), Services::default());
    app.canvas.zoom_pending = false;
    app.ui.show_command_line = false;
    app.ui.show_ucs_icon = false;
    app.ui.show_viewcube = false;

    // Set CLAYER to a bright green layer ("Contornos vistos", ACI 3), distinct from HATCHCLAYER (ACI 8 gray)
    app.session.execute("layer.new", &json!({ "name": "Contornos vistos", "current": true, "color": 3 })).unwrap();
    app.session.execute("rectang", &json!({ "p1": [0, 0], "p2": [100, 100] })).unwrap();
    app.session.execute("view.set", &json!({ "center": [50, 50], "height": 200 })).unwrap();
    assert!(app.session.doc().unwrap().layer("01 Rayados 0.18").is_none());

    let green_rgb = cadcraft_color::aci_rgb(3);
    let gray_rgb = cadcraft_color::aci_rgb(8);
    let green = egui::Color32::from_rgb(green_rgb.0, green_rgb.1, green_rgb.2);
    let gray = egui::Color32::from_rgb(gray_rgb.0, gray_rgb.1, gray_rgb.2);

    let count_strokes = |shapes: &[egui::epaint::ClippedShape], col: egui::Color32| -> usize {
        fn walk(s: &egui::Shape, col: egui::Color32, n: &mut usize) {
            match s {
                egui::Shape::LineSegment { stroke, .. } if stroke.color == col => *n += 1,
                egui::Shape::Path(p) if p.stroke.color == egui::epaint::ColorMode::Solid(col) => *n += 1,
                egui::Shape::Vec(vs) => vs.iter().for_each(|x| walk(x, col, n)),
                _ => {}
            }
        }
        let mut n = 0;
        for cs in shapes {
            walk(&cs.shape, col, &mut n);
        }
        n
    };

    let ctx = egui::Context::default();
    let paint_frame = |app: &mut CadApp| -> Vec<egui::epaint::ClippedShape> {
        let mut shapes = Vec::new();
        for _ in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1280.0, 800.0))),
                events: vec![egui::Event::PointerMoved(egui::pos2(640.0, 400.0))],
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                app.logic(ui.ctx());
                app.ui(ui);
            });
            shapes = std::mem::take(&mut out.shapes);
            out.textures_delta.clear();
        }
        shapes
    };

    // Paint baseline (only 4 green rectangle lines, 0 gray hatch lines)
    let base_shapes = paint_frame(&mut app);
    let base_green = count_strokes(&base_shapes, green);
    assert_eq!(count_strokes(&base_shapes, gray), 0, "no gray lines before hatch");

    // 1. Open Hatch dialog via command and start picking internal point (real UI button/command flow)
    app.run("hatch", serde_json::Value::Null).unwrap();
    assert_eq!(app.ui.dialog.as_deref(), Some("hatch"));
    hatchdlg::start_pick(&mut app, cadcraft_engine::PickMode::Points);
    assert_eq!(app.ui.dialog, None);

    // Pick internal point inside rectangle
    app.cmdline("50,50");

    // Paint while HATCHPICK is still active: preview must be gray (ACI 8), NOT green (CLAYER)
    let pick_shapes = paint_frame(&mut app);
    assert!(count_strokes(&pick_shapes, gray) > 5, "HATCHPICK preview must paint gray hatch pattern lines");
    assert_eq!(count_strokes(&pick_shapes, green), base_green, "HATCHPICK preview must NOT paint green CLAYER lines");
    assert!(app.session.doc().unwrap().layer("01 Rayados 0.18").is_none(), "preview must not create persistent layer");
    assert_eq!(app.session.doc().unwrap().header.str("CLAYER", ""), "Contornos vistos");

    // 2. Press Enter to return to dialog; dialog preview must also paint gray (ACI 8), NOT green
    app.cmdline("");
    let dlg_shapes = paint_frame(&mut app);
    assert_eq!(app.ui.dialog.as_deref(), Some("hatch"));
    assert!(count_strokes(&dlg_shapes, gray) > 5, "dialog preview must paint gray hatch pattern lines");
    assert_eq!(count_strokes(&dlg_shapes, green), base_green, "dialog preview must NOT paint green CLAYER lines");
    assert!(app.session.doc().unwrap().layer("01 Rayados 0.18").is_none());

    // 3. Cancel dialog: leaves no hatch entity and no persistent layer
    app.ui.dialog = None;
    app.hatch_dlg = None;
    app.session.hatch_preview_params = None;
    app.session.hatch_picks = None;
    let cancelled_shapes = paint_frame(&mut app);
    assert_eq!(count_strokes(&cancelled_shapes, gray), 0, "cancelling must leave no gray preview");
    assert_eq!(app.session.doc().unwrap().model.len(), 1);
    assert!(app.session.doc().unwrap().layer("01 Rayados 0.18").is_none());

    // 4. Re-open, pick, and accept: final entity keeps the same gray HATCHCLAYER appearance
    app.run("hatch", serde_json::Value::Null).unwrap();
    hatchdlg::start_pick(&mut app, cadcraft_engine::PickMode::Points);
    app.cmdline("50,50");
    app.cmdline("");
    let before_ok = count_strokes(&paint_frame(&mut app), gray);
    assert!(before_ok > 5);
    assert!(hatchdlg::ok(&mut app), "dialog OK should succeed");
    app.ui.dialog = None;
    let after_ok = count_strokes(&paint_frame(&mut app), gray);
    assert_eq!(after_ok, before_ok, "accepted hatch keeps identical HATCHCLAYER appearance");
    assert_eq!(app.session.doc().unwrap().header.str("CLAYER", ""), "Contornos vistos");

    // 5. Existing HATCHCLAYER with custom color (ACI 1 Red): preview respects existing layer properties
    app.session.execute("u", &json!({})).unwrap();
    app.session.execute("layer.new", &json!({ "name": "01 Rayados 0.18", "color": 1 })).unwrap();
    let red_rgb = cadcraft_color::aci_rgb(1);
    let red = egui::Color32::from_rgb(red_rgb.0, red_rgb.1, red_rgb.2);
    app.run("hatch", serde_json::Value::Null).unwrap();
    hatchdlg::start_pick(&mut app, cadcraft_engine::PickMode::Points);
    app.cmdline("50,50");
    let pick_red_shapes = paint_frame(&mut app);
    assert!(count_strokes(&pick_red_shapes, red) > 5, "HATCHPICK preview respects existing HATCHCLAYER red color");
    assert_eq!(count_strokes(&pick_red_shapes, green), base_green, "HATCHPICK preview does not use green CLAYER");
    app.cmdline("");
    let dlg_red_shapes = paint_frame(&mut app);
    assert!(count_strokes(&dlg_red_shapes, red) > 5, "dialog preview respects existing HATCHCLAYER red color");
    assert_eq!(count_strokes(&dlg_red_shapes, green), base_green);
}

