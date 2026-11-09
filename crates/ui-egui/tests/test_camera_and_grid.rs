//! Verification and regression tests for 3D camera pan, zoom, orbit anchoring, and locked viewport.

use cadcraft_doc::{Common, Drawing, EntityKind, Line, Space, Viewport};
use cadcraft_engine::Session;
use cadcraft_engine::geom::{OrbitCamera, UcsFrame, Vec2, Vec3};
use egui::pos2;
use serde_json::json;

// =========================================================================
// 1. PAN TESTS: Medición y verificación de escala a múltiples zooms
// =========================================================================

#[test]
fn test_pan_projected_displacement_across_zooms() {
    let p = Vec3::new(10.0, 10.0, 5.0);
    let rect_w = 1200.0;
    let rect_h = 800.0;
    let c = pos2(rect_w as f32 / 2.0, rect_h as f32 / 2.0);

    for height in [1000.0, 50.0, 1.0, 0.01] {
        let mut cam = OrbitCamera::default();
        cam.set_preset("swiso");
        cam.height = height;
        let scale = rect_h / height;

        // Proyección inicial
        let q0 = cam.project_point(p);
        let s0 = pos2(c.x + (q0.x * scale) as f32, c.y - (q0.y * scale) as f32);

        // Pan solicitado: dx = 30.0 px, dy = -20.0 px
        cam.pan(30.0, -20.0, scale);

        // Proyección posterior
        let q1 = cam.project_point(p);
        let s1 = pos2(c.x + (q1.x * scale) as f32, c.y - (q1.y * scale) as f32);

        let delta_x = s1.x - s0.x;
        let delta_y = s1.y - s0.y;

        assert!((delta_x - 30.0).abs() < 1e-4, "Height {}: Delta X projected {} px != 30.0", height, delta_x);
        assert!((delta_y - (-20.0)).abs() < 1e-4, "Height {}: Delta Y projected {} px != -20.0", height, delta_y);
    }
}

// =========================================================================
// 2. ZOOM 3D TESTS: Anclaje ortográfico en base de cámara
// =========================================================================

#[test]
fn test_zoom_3d_anchor_preserves_screen_position() {
    let mut s = Session::empty();
    s.open_drawing(Drawing::new_metric(), "zoom_test", None);
    s.execute("view.swiso", &json!({})).unwrap();

    let target_pt = Vec3::new(50.0, 50.0, 25.0);
    let rect_h = 800.0;
    let rect_w = 1200.0;
    s.viewport_px = (rect_w, rect_h);

    let cam = s.camera();
    let scale = rect_h / cam.height;
    let q = cam.project_point(target_pt);
    let screen_before = pos2((rect_w / 2.0 + q.x * scale) as f32, (rect_h / 2.0 - q.y * scale) as f32);

    // Zoom in 2.5x sobre el punto de pantalla
    s.zoom_about_camera(2.5, q).unwrap();

    let cam_after = s.camera();
    let scale_after = rect_h / cam_after.height;
    let q_after = cam_after.project_point(target_pt);
    let screen_after = pos2((rect_w / 2.0 + q_after.x * scale_after) as f32, (rect_h / 2.0 - q_after.y * scale_after) as f32);

    let drift = ((screen_after.x - screen_before.x).powi(2) + (screen_after.y - screen_before.y).powi(2)).sqrt();
    assert!(drift < 1e-4, "Drift tras zoom 3D: {} px (debe ser < 1e-4 px)", drift);

    // Zoom out 0.4x (volver hacia atrás)
    let q_back = cam_after.project_point(target_pt);
    s.zoom_about_camera(0.4, q_back).unwrap();

    let cam_final = s.camera();
    let scale_final = rect_h / cam_final.height;
    let q_final = cam_final.project_point(target_pt);
    let screen_final = pos2((rect_w / 2.0 + q_final.x * scale_final) as f32, (rect_h / 2.0 - q_final.y * scale_final) as f32);

    let drift_final = ((screen_final.x - screen_before.x).powi(2) + (screen_final.y - screen_before.y).powi(2)).sqrt();
    assert!(drift_final < 1e-4, "Drift tras zoom out: {} px", drift_final);
}

#[test]
fn test_zoom_3d_cursor_at_edges_and_inclined_ucs() {
    let mut s = Session::empty();
    s.open_drawing(Drawing::new_metric(), "edges_test", None);
    s.execute("view.swiso", &json!({})).unwrap();

    // Activar UCS inclinado
    let ucs = UcsFrame::new(Vec3::new(10.0, 20.0, 5.0), Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 0.7071, 0.7071), Vec3::new(0.0, -0.7071, 0.7071));
    s.set_ucs(ucs).unwrap();

    let rect_h = 800.0;
    let rect_w = 1200.0;
    s.viewport_px = (rect_w, rect_h);

    // Probar esquinas y bordes
    for (hp_x, hp_y) in [(10.0, 10.0), (1190.0, 10.0), (10.0, 790.0), (1190.0, 790.0), (600.0, 400.0)] {
        let cam = s.camera();
        let scale = rect_h / cam.height;
        let q = Vec2::new((hp_x - rect_w / 2.0) / scale, -(hp_y - rect_h / 2.0) / scale);

        // Zoom 1.5x
        s.zoom_about_camera(1.5, q).unwrap();

        let cam_after = s.camera();
        let scale_after = rect_h / cam_after.height;
        let q_proj_screen = pos2((rect_w / 2.0 + (q.x / 1.5) * scale_after) as f32, (rect_h / 2.0 - (q.y / 1.5) * scale_after) as f32);
        assert!((q_proj_screen.x - hp_x as f32).abs() < 1e-3);
        assert!((q_proj_screen.y - hp_y as f32).abs() < 1e-3);
    }
}

#[test]
fn test_locked_viewport_navigation_does_not_modify_internal_scale() {
    let mut s = Session::empty();
    let mut d = Drawing::new_imperial();
    d.add(&Space::Model, Common::default(), EntityKind::Line(Line { a: Vec3::new(0.0, 0.0, 0.0), b: Vec3::new(20.0, 0.0, 0.0) })).unwrap();
    let paper = Space::Paper("Layout1".into());
    d.add(
        &paper,
        Common::default(),
        EntityKind::Viewport(Viewport {
            center: Vec3::new(5.0, 4.0, 0.0),
            width: 8.0,
            height: 6.0,
            view_center: Vec2::new(10.0, 0.0),
            view_height: 40.0,
            id: 2,
            locked: true,
            frozen_layers: Vec::new(),
            layer_colors: Vec::new(),
            clip: None,
            on: true,
        }),
    ).unwrap();

    s.open_drawing(d, "locked_vp", None);
    s.execute("layout.set", &json!({ "name": "Layout1" })).unwrap();
    s.execute("mspace", &json!({})).unwrap();

    let initial_vp_height = s.state().unwrap().active_viewport().unwrap().1.view_height;

    // Intentar zoom en viewport bloqueado
    let _ = s.zoom_about(2.0, Vec2::ZERO);

    // En viewport bloqueado, store_view traslada el papel, no la escala del modelo interno
    let (_h_vp, vp_entity) = s.state().unwrap().active_viewport().unwrap();
    assert_eq!(vp_entity.view_height, initial_vp_height, "Escala interna del viewport bloqueado no debe cambiar tras zoom");
}

// =========================================================================
// 3. ORBIT TESTS: Pivote en plano de trabajo y ausencia de salto previo
// =========================================================================

#[test]
fn test_begin_orbit_shifts_depth_without_any_screen_jump() {
    let mut s = Session::empty();
    s.open_drawing(Drawing::new_metric(), "orbit_test", None);
    s.execute("view.swiso", &json!({})).unwrap();

    let p_test = Vec3::new(45.0, 75.0, 15.0);
    let rect_h = 800.0;
    let rect_w = 1200.0;
    s.viewport_px = (rect_w, rect_h);

    let cam_before = s.camera();
    let scale_before = rect_h / cam_before.height;
    let q_before = cam_before.project_point(p_test);
    let screen_before = pos2((rect_w / 2.0 + q_before.x * scale_before) as f32, (rect_h / 2.0 - q_before.y * scale_before) as f32);

    // begin_orbit fija el pivote sobre el plano de trabajo a lo largo del rayo v
    s.begin_orbit();

    let cam_after = s.camera();
    let scale_after = rect_h / cam_after.height;
    let q_after = cam_after.project_point(p_test);
    let screen_after = pos2((rect_w / 2.0 + q_after.x * scale_after) as f32, (rect_h / 2.0 - q_after.y * scale_after) as f32);

    let diff = ((screen_after.x - screen_before.x).powi(2) + (screen_after.y - screen_before.y).powi(2)).sqrt();
    assert!(diff < 1e-10, "begin_orbit no debe producir ningún salto antes del giro (diff = {})", diff);
}

#[test]
fn test_orbit_distant_small_geometry_remains_stable() {
    let mut s = Session::empty();
    s.open_drawing(Drawing::new_metric(), "orbit_stable", None);
    s.execute("view.swiso", &json!({})).unwrap();

    // Dibujar geometría pequeña en (500, 500, 0)
    let p_work = Vec3::new(500.0, 500.0, 0.0);
    s.cmdline("line 500,500,0 @2,0,0").unwrap();
    s.cmdline("").unwrap();

    // Encuadrar la vista sobre la geometría pequeña
    let mut cam = s.camera();
    cam.target = p_work;
    cam.height = 5.0; // zoom cercano (campo de 5 unidades)
    s.set_camera(cam);

    s.viewport_px = (1200.0, 800.0);
    s.begin_orbit();

    let cam0 = s.camera();
    let scale0 = 800.0 / cam0.height;
    let q0 = cam0.project_point(p_work);
    let s0 = pos2((600.0 + q0.x * scale0) as f32, (400.0 - q0.y * scale0) as f32);

    // Girar 15 px
    s.orbit_step(15.0, 0.0).unwrap();

    let cam1 = s.camera();
    let scale1 = 800.0 / cam1.height;
    let q1 = cam1.project_point(p_work);
    let s1 = pos2((600.0 + q1.x * scale1) as f32, (400.0 - q1.y * scale1) as f32);

    let jump = ((s1.x - s0.x).powi(2) + (s1.y - s0.y).powi(2)).sqrt();
    assert!(jump < 1.0, "La geometría en el pivote debe permanecer fija durante la rotación (jump = {} px)", jump);
}

#[test]
fn test_orbit_grazing_angle_fallback_preserves_target() {
    let mut s = Session::empty();
    s.open_drawing(Drawing::new_metric(), "orbit_grazing", None);
    // Vista frontal (paralela al plano XY, rayo rasante)
    s.execute("view.front", &json!({})).unwrap();

    let cam_before = s.camera();
    s.begin_orbit();
    let cam_after = s.camera();

    assert_eq!(cam_before.target, cam_after.target, "En rayo rasante, el pivote debe conservarse sin error ni fallo");
}

// =========================================================================
// 5. INVERSIÓN DE EJES DE ÓRBITA Y SENSIBILIDAD ADAPTATIVA
// =========================================================================

#[test]
fn test_orbit_axis_inversion() {
    let mut s = Session::empty();
    s.open_drawing(Drawing::new_metric(), "orbit_inversion", None);
    s.viewport_px = (1200.0, 800.0);
    s.execute("view.swiso", &json!({})).unwrap();

    let cam0 = s.camera();
    s.begin_orbit();

    // Arrastre horizontal a la derecha (+dx): debe disminuir azimut (giro horario de la escena)
    s.orbit_step(50.0, 0.0).unwrap();
    let cam1 = s.camera();
    assert!(
        cam1.azimuth < cam0.azimuth || (cam0.azimuth - cam1.azimuth).abs() > std::f64::consts::PI,
        "Arrastre a la derecha (+dx) debe reducir el azimut: cam0={}, cam1={}",
        cam0.azimuth, cam1.azimuth
    );

    // Arrastre vertical hacia abajo (+dy): debe aumentar elevación (mirar hacia arriba)
    let el0 = cam1.elevation;
    s.orbit_step(0.0, 50.0).unwrap();
    let cam2 = s.camera();
    assert!(
        cam2.elevation > el0,
        "Arrastre hacia abajo (+dy) debe aumentar la elevación: el0={}, el1={}",
        el0, cam2.elevation
    );

    // Arrastre vertical hacia arriba (-dy): debe disminuir elevación
    let el1 = cam2.elevation;
    s.orbit_step(0.0, -50.0).unwrap();
    let cam3 = s.camera();
    assert!(
        cam3.elevation < el1,
        "Arrastre hacia arriba (-dy) debe disminuir la elevación: el1={}, el2={}",
        el1, cam3.elevation
    );
}

#[test]
fn test_orbit_sensitivity_adaptive_scaling() {
    let mut s = Session::empty();
    s.open_drawing(Drawing::new_metric(), "orbit_sensitivity", None);
    s.viewport_px = (1200.0, 800.0);
    s.execute("view.swiso", &json!({})).unwrap();

    // 1. Encuadre nominal: height == ref_height => M = 1.0
    let m_nominal = s.orbit_sensitivity_factor();
    assert!((m_nominal - 1.0).abs() < 1e-4, "En encuadre nominal, M debe ser 1.0 (obtenido {})", m_nominal);

    // 2. Acercamiento 4x (height = 0.25 * ref_height) => rho = 0.25 => M = sqrt(0.25) = 0.50
    let h_ref = s.state().unwrap().ref_height;
    let mut cam = s.camera();
    cam.height = h_ref * 0.25;
    s.set_camera(cam);
    let m_4x = s.orbit_sensitivity_factor();
    assert!((m_4x - 0.50).abs() < 1e-4, "Al acercar 4x, M debe ser 0.50 (obtenido {})", m_4x);

    // 3. Acercamiento extremo 100x => clamp en 0.15
    let mut cam = s.camera();
    cam.height = h_ref * 0.005;
    s.set_camera(cam);
    let m_extreme = s.orbit_sensitivity_factor();
    assert!((m_extreme - 0.15).abs() < 1e-4, "En zoom extremo, M debe clampear en 0.15 (obtenido {})", m_extreme);

    // 4. Alejamiento (height > ref_height) => clamp en 1.0
    let mut cam = s.camera();
    cam.height = h_ref * 4.0;
    s.set_camera(cam);
    let m_out = s.orbit_sensitivity_factor();
    assert!((m_out - 1.0).abs() < 1e-4, "Al alejar, M debe clampear en 1.0 (obtenido {})", m_out);

    // 5. Invarianza de escala: si dibujo y vista se escalan x1000
    s.state_mut().unwrap().ref_height = h_ref * 1000.0;
    let mut cam = s.camera();
    cam.height = (h_ref * 0.25) * 1000.0;
    s.set_camera(cam);
    let m_scaled = s.orbit_sensitivity_factor();
    assert!((m_scaled - 0.50).abs() < 1e-4, "Con dibujo y vista escalados x1000, M debe ser idéntico (0.50)");

    // 6. Factor constante durante el gesto
    s.begin_orbit();
    let frozen_m = s.orbit_sensitivity;
    assert!((frozen_m - 0.50).abs() < 1e-4);
    // Cambiar altura de cámara durante el gesto NO debe alterar orbit_sensitivity congelado
    let mut cam = s.camera();
    cam.height = h_ref * 0.001;
    s.set_camera(cam);
    assert_eq!(s.orbit_sensitivity, frozen_m, "El factor de sensibilidad debe permanecer fijo durante todo el gesto");
}

#[test]
fn test_orbit_sensitivity_separate_documents_and_viewports() {
    let mut s = Session::empty();
    // Doc 1: métrico (ref_height inicial ~ 297.0)
    let idx1 = s.open_drawing(Drawing::new_metric(), "Doc1", None);
    let ref1 = s.state().unwrap().ref_height;

    // Doc 2: con geometría pequeña encuadrada (ref_height ~ 20.0)
    let mut d2 = Drawing::new_metric();
    d2.add(&Space::Model, Common::default(), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(20.0, 10.0, 0.0) })).unwrap();
    let idx2 = s.open_drawing(d2, "Doc2", None);
    let ref2 = s.state().unwrap().ref_height;

    assert!(ref1 != ref2, "Dos documentos distintos deben mantener referencias independientes");

    // Verificar que al volver a Doc 1 se conserva su propia referencia
    s.active = idx1;
    assert_eq!(s.state().unwrap().ref_height, ref1);
    s.active = idx2;
    assert_eq!(s.state().unwrap().ref_height, ref2);
}

#[test]
fn test_orbit_sensitivity_open_zoomed_and_zoom_window() {
    let mut s = Session::empty();
    let mut d = Drawing::new_metric();
    d.add(&Space::Model, Common::default(), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(10.0, 10.0, 0.0) })).unwrap();
    s.open_drawing(d, "ZoomedTest", None);

    // Al abrir, sensibilidad nominal
    assert!((s.orbit_sensitivity_factor() - 1.0).abs() < 1e-4);

    // ZOOM Window sobre un detalle pequeño
    s.execute("zoom", &json!({ "mode": "window", "p1": [0.0, 0.0], "p2": [2.0, 2.0] })).unwrap();
    // Tras ZOOM Window, la nueva vista encuadrada restablece la sensibilidad nominal para ese detalle
    assert!((s.orbit_sensitivity_factor() - 1.0).abs() < 1e-4);
    assert!(s.state().unwrap().ref_height < 5.0, "ZOOM Window debe actualizar ref_height al detalle encuadrado");
}

// =========================================================================
// 6. CURSOR VIRTUAL Y ENVOLTORIO MODULAR EN BORDES
// =========================================================================

#[test]
fn test_virtual_cursor_modular_wrapping() {
    use cadcraft_ui_egui::canvas::wrap_virtual_cursor_pos;
    use egui::{Rect, Pos2};

    let rect = Rect::from_min_max(Pos2::new(100.0, 100.0), Pos2::new(300.0, 200.0));
    // w = 200.0, h = 100.0

    // 1. Punto interior sin cruce
    let p_inside = Pos2::new(150.0, 150.0);
    assert_eq!(wrap_virtual_cursor_pos(p_inside, rect), p_inside);

    // 2. Envoltorio horizontal: Borde derecho -> reaparece por el izquierdo conservando exceso
    let p_right_overshoot = Pos2::new(305.0, 150.0); // +5 px de exceso sobre max.x (300)
    let p_wrapped_right = wrap_virtual_cursor_pos(p_right_overshoot, rect);
    assert_eq!(p_wrapped_right, Pos2::new(105.0, 150.0), "Debe reaparecer a min.x + 5 px");

    // 3. Envoltorio horizontal: Borde izquierdo -> reaparece por el derecho conservando exceso
    let p_left_overshoot = Pos2::new(95.0, 150.0); // -5 px bajo min.x (100)
    let p_wrapped_left = wrap_virtual_cursor_pos(p_left_overshoot, rect);
    assert_eq!(p_wrapped_left, Pos2::new(295.0, 150.0), "Debe reaparecer a max.x - 5 px");

    // 4. Envoltorio vertical: Borde inferior -> reaparece por el superior
    let p_bottom_overshoot = Pos2::new(150.0, 208.0); // +8 px sobre max.y (200)
    let p_wrapped_bottom = wrap_virtual_cursor_pos(p_bottom_overshoot, rect);
    assert_eq!(p_wrapped_bottom, Pos2::new(150.0, 108.0), "Debe reaparecer a min.y + 8 px");

    // 5. Envoltorio vertical: Borde superior -> reaparece por el inferior
    let p_top_overshoot = Pos2::new(150.0, 92.0); // -8 px bajo min.y (100)
    let p_wrapped_top = wrap_virtual_cursor_pos(p_top_overshoot, rect);
    assert_eq!(p_wrapped_top, Pos2::new(150.0, 192.0), "Debe reaparecer a max.y - 8 px");

    // 6. Envoltorio en esquina (ambos ejes simultáneos)
    let p_corner_overshoot = Pos2::new(310.0, 215.0);
    let p_wrapped_corner = wrap_virtual_cursor_pos(p_corner_overshoot, rect);
    assert_eq!(p_wrapped_corner, Pos2::new(110.0, 115.0), "Ambos excesos de esquina deben conservarse");

    // 7. Deltas mayores que el ancho/alto del viewport
    let p_large_delta = Pos2::new(550.0, 350.0); // +450 px en X (2.25 anchos), +250 px en Y (2.5 altos)
    let p_wrapped_large = wrap_virtual_cursor_pos(p_large_delta, rect);
    assert_eq!(p_wrapped_large, Pos2::new(150.0, 150.0), "Deltas gigantes deben envolver cíclicamente");
}

#[test]
fn test_viewcube_orbit_step_uninverted_and_independent_of_canvas_sensitivity() {
    let mut s = Session::empty();
    s.open_drawing(Drawing::new_metric(), "viewcube_sep", None);
    s.viewport_px = (1200.0, 800.0);
    s.execute("view.swiso", &json!({})).unwrap();

    // Simular que el último gesto del lienzo dejó sensibilidad mínima (0.15)
    s.orbit_sensitivity = 0.15;

    let cam0 = s.camera();
    // Arrastre del ViewCube hacia la derecha (+dx): debe incrementar el azimut y usar sensibilidad 1:1
    s.viewcube_orbit_step(50.0, 0.0).unwrap();
    let cam1 = s.camera();
    assert!(cam1.azimuth > cam0.azimuth, "El arrastre del ViewCube a la derecha debe avanzar el azimut (sin invertir)");

    // Comparar magnitud con órbita nominal 1:1
    let mut cam_expected = cam0;
    cam_expected.orbit(50.0, 0.0, (1200.0, 800.0));
    assert!((cam1.azimuth - cam_expected.azimuth).abs() < 1e-12, "ViewCube no debe heredar orbit_sensitivity del lienzo");
}

#[test]
fn test_sensitivity_template_init_incremental_zoom_and_tiled_viewports() {
    use cadcraft_engine::cmd::file::{IoHooks, set_io};
    set_io(IoHooks {
        read: |_, _| {
            let mut d = Drawing::new_metric();
            d.header.set("LIMMIN", cadcraft_doc::HVal::Point(Vec3::ZERO));
            d.header.set("LIMMAX", cadcraft_doc::HVal::Point(Vec3::new(420.0, 297.0, 0.0)));
            Ok(d)
        },
        write: |_, _| Ok(Vec::new()),
        plot: None,
    });

    let tmp_path = std::env::temp_dir().join(format!("test_egi_sens_{}.dwt", std::process::id()));
    std::fs::write(&tmp_path, b"dummy").unwrap();

    let mut s = Session::new();
    s.execute("new", &json!({ "template": tmp_path.to_str().unwrap() })).unwrap();
    let _ = std::fs::remove_file(&tmp_path);

    // 1. La referencia debe haberse inicializado tras el encuadre inicial de plantilla (297.0, no 10.0)
    let ref_init = s.state().unwrap().ref_height;
    assert!((ref_init - 297.0).abs() < 1.0, "ref_height tras abrir plantilla vacía con límites A3 debe ser ~297.0 (obtenido {})", ref_init);

    // 2. Zooms incrementales (ZOOM In, Scale, Center, Realtime) NO deben restablecer ref_height
    s.execute("zoom", &json!({ "mode": "in" })).unwrap();
    assert!((s.state().unwrap().ref_height - ref_init).abs() < 1e-9, "ZOOM In no debe restablecer ref_height");

    s.execute("zoom", &json!({ "mode": "scale", "factor": 2.0 })).unwrap();
    assert!((s.state().unwrap().ref_height - ref_init).abs() < 1e-9, "ZOOM Scale 2x no debe restablecer ref_height");
    assert!((s.orbit_sensitivity_factor() - 0.5).abs() < 1e-3, "Tras dos acercamientos 2x (total 4x), M debe ser 0.5");

    // 3. Conservación y restauración de ref_height por viewport/tile (VPORTS)
    // Restablecer vista a ref_init y dividir en 2 tiles
    s.execute("zoom", &json!({ "mode": "scale", "factor": 0.25 })).unwrap();
    s.execute("-vports", &json!({ "option": "2", "arrangement": "vertical" })).unwrap();

    // En tile 0: hacer ZOOM Window a una caja de altura 40 -> ref_height del tile 0 pasa a ~40
    s.execute("zoom", &json!({ "mode": "window", "p1": [0.0, 0.0], "p2": [40.0, 40.0] })).unwrap();
    let ref_tile0 = s.state().unwrap().ref_height;
    assert!(ref_tile0 < 60.0);
    // Acercar incrementalmente el tile 0 a la mitad de altura sin cambiar su ref_height
    s.execute("zoom", &json!({ "mode": "in" })).unwrap();
    assert!((s.state().unwrap().ref_height - ref_tile0).abs() < 1e-9);

    // Activar tile 1 (índice 1): debe restaurar su referencia original (~297.0)
    s.activate_tile(1).unwrap();
    assert!((s.state().unwrap().ref_height - ref_init).abs() < 1e-6, "Activar tile 1 debe restaurar su ref_height original");
    // Acercar incrementalmente el tile 1 (height pasa a ~74.25, ref_height sigue en ~297.0)
    s.execute("zoom", &json!({ "mode": "scale", "factor": 4.0 })).unwrap();
    assert!((s.state().unwrap().ref_height - ref_init).abs() < 1e-6);

    // Volver a activar tile 0: debe restaurar ref_tile0 (~40), NO reinicializarla a su view.height (~20)
    s.activate_tile(0).unwrap();
    assert!((s.state().unwrap().ref_height - ref_tile0).abs() < 1e-6, "Re-activar tile 0 debe restaurar su ref_height (~40), no su view.height (~20)");
}

#[test]
fn test_virtual_cursor_relative_lock_no_alternation_and_doc_switch_release() {
    use cadcraft_ui_egui::{CadApp, Services};
    use egui::{Event, Key, Modifiers, PointerButton, Pos2};

    let mut session = Session::empty();
    let idx1 = session.open_drawing(Drawing::new_metric(), "Doc1", None);
    let idx2 = session.open_drawing(Drawing::new_metric(), "Doc2", None);
    session.active = idx1;
    session.sync_active_doc_settings();

    let mut app = CadApp::new(session, Services::default());
    app.canvas.zoom_pending = false;
    let ctx = egui::Context::default();

    let run_frame = |app: &mut CadApp, events: Vec<Event>, focused: bool, close_requested: bool| -> Vec<egui::ViewportCommand> {
        let mut viewports = egui::ViewportIdMap::default();
        viewports.insert(
            egui::ViewportId::ROOT,
            egui::ViewportInfo {
                focused: Some(focused),
                events: if close_requested { vec![egui::ViewportEvent::Close] } else { Vec::new() },
                ..Default::default()
            },
        );
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 800.0))),
            focused,
            viewports,
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| {
            app.logic(ui.ctx());
            app.ui(ui);
        });
        out.textures_delta.clear();
        out.viewport_output
            .get(&egui::ViewportId::ROOT)
            .map(|v| v.commands.clone())
            .unwrap_or_default()
    };

    // Frame inicial de layout
    let _ = run_frame(&mut app, vec![Event::PointerMoved(pos2(600.0, 400.0))], true, false);
    let rect = app.canvas.rect.expect("canvas rect");
    let start_pt = rect.center();

    // Frame 1: presionar botón central en el centro del lienzo -> inicia navegación virtual,
    // oculta cursor y fija ancla interior mediante CursorPosition(anchor_pos)
    let cmds1 = run_frame(
        &mut app,
        vec![
            Event::PointerMoved(start_pt),
            Event::PointerButton { pos: start_pt, button: PointerButton::Middle, pressed: true, modifiers: Modifiers::NONE },
        ],
        true,
        false,
    );
    let vnav1 = app.canvas.virtual_nav.expect("Debe activarse virtual_nav al pulsar botón central");
    assert_eq!(vnav1.anchor_pos, start_pt);
    assert!(
        cmds1.iter().any(|c| matches!(c, egui::ViewportCommand::CursorVisible(false))),
        "El frame de inicio debe enviar CursorVisible(false)"
    );
    assert!(
        cmds1.iter().any(|c| matches!(c, egui::ViewportCommand::CursorPosition(p) if *p == start_pt)),
        "El frame de inicio debe enviar CursorPosition(anchor_pos) para activar el bloqueo en XWayland/X11"
    );
    if !cadcraft_ui_egui::canvas::uses_x11_backend() {
        assert!(
            cmds1.iter().any(|c| matches!(c, egui::ViewportCommand::CursorGrab(egui::viewport::CursorGrab::Locked))),
            "En backends no-X11 debe enviar CursorGrab::Locked"
        );
    }

    // Frame 2: llega evento relativo MouseMoved(30, 20) Y también PointerMoved absoluto distinto
    let cmds2 = run_frame(
        &mut app,
        vec![
            Event::PointerMoved(start_pt + egui::vec2(100.0, 100.0)),
            Event::MouseMoved(egui::vec2(30.0, 20.0)),
        ],
        true,
        false,
    );
    let vnav2 = app.canvas.virtual_nav.expect("virtual_nav activo");
    assert!(vnav2.using_relative, "Tras recibir MouseMoved debe fijar using_relative = true");
    assert_eq!(vnav2.virtual_pos, start_pt + egui::vec2(30.0, 20.0), "Solo debe contar el delta relativo (30, 20), sin doble conteo con PointerMoved");
    assert!(
        cmds2.iter().any(|c| matches!(c, egui::ViewportCommand::CursorPosition(p) if *p == start_pt)),
        "Cada frame con MouseMoved debe reanclar el puntero del SO en anchor_pos"
    );

    // Frame 3: frame sin MouseMoved pero con PointerMoved -> debe aportar delta cero (no alternar a absoluto)
    let _ = run_frame(
        &mut app,
        vec![Event::PointerMoved(start_pt + egui::vec2(250.0, 250.0))],
        true,
        false,
    );
    let vnav3 = app.canvas.virtual_nav.expect("virtual_nav sigue activo");
    assert_eq!(vnav3.virtual_pos, start_pt + egui::vec2(30.0, 20.0), "Un frame sin MouseMoved con using_relative activo debe aportar delta 0");

    // Frame 4: cambio de documento activo durante el gesto -> debe liberar virtual_nav y restaurar cursor
    app.session.active = idx2;
    let cmds4 = run_frame(&mut app, vec![Event::MouseMoved(egui::vec2(10.0, 10.0))], true, false);
    assert!(app.canvas.virtual_nav.is_none(), "El cambio de documento debe liberar virtual_nav aunque el botón siga pulsado");
    assert!(
        cmds4.iter().any(|c| matches!(c, egui::ViewportCommand::CursorGrab(egui::viewport::CursorGrab::None))),
        "Liberar por cambio de documento debe enviar CursorGrab::None"
    );
    assert!(
        !cmds4.iter().any(|c| matches!(c, egui::ViewportCommand::CursorVisible(true))),
        "Liberar sobre el lienzo no debe forzar CursorVisible(true)"
    );

    // Frame 5: liberación al soltar botón, cancelar (Escape), perder foco y solicitar cierre de ventana
    // 5a) Soltar botón
    let _ = run_frame(
        &mut app,
        vec![
            Event::PointerMoved(start_pt),
            Event::PointerButton { pos: start_pt, button: PointerButton::Middle, pressed: true, modifiers: Modifiers::NONE },
        ],
        true,
        false,
    );
    assert!(app.canvas.virtual_nav.is_some());
    let cmds_release = run_frame(
        &mut app,
        vec![Event::PointerButton { pos: start_pt, button: PointerButton::Middle, pressed: false, modifiers: Modifiers::NONE }],
        true,
        false,
    );
    assert!(app.canvas.virtual_nav.is_none(), "Soltar el botón debe liberar virtual_nav");
    assert!(cmds_release.iter().any(|c| matches!(c, egui::ViewportCommand::CursorGrab(egui::viewport::CursorGrab::None))));
    assert!(
        !cmds_release.iter().any(|c| matches!(c, egui::ViewportCommand::CursorVisible(true))),
        "Soltar botón sobre el lienzo no debe forzar CursorVisible(true)"
    );

    // 5b) Cancelar con Escape
    let _ = run_frame(
        &mut app,
        vec![
            Event::PointerMoved(start_pt),
            Event::PointerButton { pos: start_pt, button: PointerButton::Middle, pressed: true, modifiers: Modifiers::NONE },
        ],
        true,
        false,
    );
    assert!(app.canvas.virtual_nav.is_some());
    let cmds_esc = run_frame(
        &mut app,
        vec![Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }],
        true,
        false,
    );
    assert!(app.canvas.virtual_nav.is_none(), "Escape debe liberar virtual_nav");
    assert!(cmds_esc.iter().any(|c| matches!(c, egui::ViewportCommand::CursorGrab(egui::viewport::CursorGrab::None))));
    assert!(
        !cmds_esc.iter().any(|c| matches!(c, egui::ViewportCommand::CursorVisible(true))),
        "Escape sobre el lienzo no debe forzar CursorVisible(true)"
    );
    // Limpiar botón central
    let _ = run_frame(
        &mut app,
        vec![Event::PointerButton { pos: start_pt, button: PointerButton::Middle, pressed: false, modifiers: Modifiers::NONE }],
        true,
        false,
    );

    // 5c) Perder foco
    let _ = run_frame(
        &mut app,
        vec![
            Event::PointerMoved(start_pt),
            Event::PointerButton { pos: start_pt, button: PointerButton::Middle, pressed: true, modifiers: Modifiers::NONE },
        ],
        true,
        false,
    );
    assert!(app.canvas.virtual_nav.is_some());
    let cmds_unfocus = run_frame(&mut app, Vec::new(), false, false);
    assert!(app.canvas.virtual_nav.is_none(), "Perder foco debe liberar virtual_nav");
    assert!(cmds_unfocus.iter().any(|c| matches!(c, egui::ViewportCommand::CursorGrab(egui::viewport::CursorGrab::None))));
    assert!(cmds_unfocus.iter().any(|c| matches!(c, egui::ViewportCommand::CursorVisible(true))), "Perder foco debe restaurar visibilidad");
    let _ = run_frame(
        &mut app,
        vec![Event::PointerButton { pos: start_pt, button: PointerButton::Middle, pressed: false, modifiers: Modifiers::NONE }],
        true,
        false,
    );

    // 5d) Solicitud de cierre de ventana
    let _ = run_frame(
        &mut app,
        vec![
            Event::PointerMoved(start_pt),
            Event::PointerButton { pos: start_pt, button: PointerButton::Middle, pressed: true, modifiers: Modifiers::NONE },
        ],
        true,
        false,
    );
    assert!(app.canvas.virtual_nav.is_some());
    let cmds_close = run_frame(&mut app, Vec::new(), true, true);
    assert!(app.canvas.virtual_nav.is_none(), "Cerrar ventana debe liberar virtual_nav");
    assert!(cmds_close.iter().any(|c| matches!(c, egui::ViewportCommand::CursorGrab(egui::viewport::CursorGrab::None))));
    assert!(cmds_close.iter().any(|c| matches!(c, egui::ViewportCommand::CursorVisible(true))), "Cerrar ventana debe restaurar visibilidad");
}

#[test]
fn test_regressions_cursor_policy_and_navigation_restoration() {
    use cadcraft_ui_egui::{CadApp, Services};
    use egui::{Event, Key, Modifiers, PointerButton, Pos2};

    let run_frame_full = |app: &mut CadApp, ctx: &egui::Context, events: Vec<Event>, focused: bool| -> (Vec<egui::ViewportCommand>, egui::CursorIcon) {
        let mut viewports = egui::ViewportIdMap::default();
        viewports.insert(
            egui::ViewportId::ROOT,
            egui::ViewportInfo {
                focused: Some(focused),
                ..Default::default()
            },
        );
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 800.0))),
            focused,
            viewports,
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| {
            app.logic(ui.ctx());
            app.ui(ui);
        });
        out.textures_delta.clear();
        let cmds = out
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map(|v| v.commands.clone())
            .unwrap_or_default();
        (cmds, out.platform_output.cursor_icon)
    };

    // 1. Paso por ResizeVertical -> canvas con cursor None -> órbita -> liberación
    {
        let mut session = Session::empty();
        session.open_drawing(Drawing::new_metric(), "Doc", None);
        let mut app = CadApp::new(session, Services::default());
        app.canvas.zoom_pending = false;
        let ctx = egui::Context::default();
        let center = Pos2::new(600.0, 400.0);

        // Warmup: asentar layout
        let _ = run_frame_full(&mut app, &ctx, vec![Event::PointerMoved(center)], true);
        let (_, icon1) = run_frame_full(&mut app, &ctx, Vec::new(), true);
        assert_eq!(icon1, egui::CursorIcon::None, "Sobre el canvas el cursor es None");

        // Simular que el ratón pasó por un elemento de redimensionamiento vertical (ej. grip)
        let bar_top_pos = Pos2::new(600.0, 770.0);
        let _ = run_frame_full(&mut app, &ctx, vec![Event::PointerMoved(bar_top_pos)], true);
        let (_, icon_grip) = run_frame_full(&mut app, &ctx, Vec::new(), true);
        if icon_grip == egui::CursorIcon::ResizeVertical {
            assert_eq!(icon_grip, egui::CursorIcon::ResizeVertical);
        }

        // Volver al centro del canvas: cursor debe ser None
        let _ = run_frame_full(&mut app, &ctx, vec![Event::PointerMoved(center)], true);
        let (_, icon_canvas) = run_frame_full(&mut app, &ctx, Vec::new(), true);
        assert_eq!(icon_canvas, egui::CursorIcon::None, "Al volver al canvas el cursor vuelve a None");

        // Iniciar órbita con Shift + Middle
        let (cmds_start, _) = run_frame_full(
            &mut app,
            &ctx,
            vec![
                Event::PointerButton { pos: center, button: PointerButton::Middle, pressed: true, modifiers: Modifiers::SHIFT },
            ],
            true,
        );
        assert!(app.canvas.virtual_nav.is_some(), "virtual_nav activado");
        assert!(cmds_start.iter().any(|c| matches!(c, egui::ViewportCommand::CursorVisible(false))));

        // Liberar órbita soltando botón central
        let (cmds_rel, icon_rel) = run_frame_full(
            &mut app,
            &ctx,
            vec![
                Event::PointerButton { pos: center, button: PointerButton::Middle, pressed: false, modifiers: Modifiers::SHIFT },
            ],
            true,
        );
        assert!(app.canvas.virtual_nav.is_none(), "virtual_nav liberado");
        assert!(cmds_rel.iter().any(|c| matches!(c, egui::ViewportCommand::CursorGrab(egui::viewport::CursorGrab::None))));
        assert!(
            !cmds_rel.iter().any(|c| matches!(c, egui::ViewportCommand::CursorVisible(true))),
            "Al liberar órbita sobre canvas, no debe enviar CursorVisible(true)"
        );
        assert_eq!(icon_rel, egui::CursorIcon::None, "La salida final de UI debe seguir siendo CursorIcon::None");
    }

    // 2. Misma secuencia con LINE y cancelación previa
    {
        let mut session = Session::empty();
        session.open_drawing(Drawing::new_metric(), "Doc", None);
        let mut app = CadApp::new(session, Services::default());
        app.canvas.zoom_pending = false;
        let ctx = egui::Context::default();
        let center = Pos2::new(600.0, 400.0);

        // Warmup
        let _ = run_frame_full(&mut app, &ctx, vec![Event::PointerMoved(center)], true);
        let _ = run_frame_full(&mut app, &ctx, Vec::new(), true);

        // Iniciar LINE
        app.cmdline("LINE");
        let _ = run_frame_full(&mut app, &ctx, vec![Event::PointerMoved(center)], true);
        assert!(app.session.prompting(), "LINE debe estar esperando puntos");

        // Cancelar LINE con Escape
        let _ = run_frame_full(
            &mut app,
            &ctx,
            vec![Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }],
            true,
        );
        assert!(!app.session.prompting(), "Escape debe cancelar LINE");

        // Iniciar órbita
        let _ = run_frame_full(
            &mut app,
            &ctx,
            vec![
                Event::PointerButton { pos: center, button: PointerButton::Middle, pressed: true, modifiers: Modifiers::SHIFT },
            ],
            true,
        );
        assert!(app.canvas.virtual_nav.is_some());

        // Liberar órbita
        let (cmds_rel, icon_rel) = run_frame_full(
            &mut app,
            &ctx,
            vec![
                Event::PointerButton { pos: center, button: PointerButton::Middle, pressed: false, modifiers: Modifiers::SHIFT },
            ],
            true,
        );
        assert!(app.canvas.virtual_nav.is_none());
        assert!(!cmds_rel.iter().any(|c| matches!(c, egui::ViewportCommand::CursorVisible(true))));
        assert_eq!(icon_rel, egui::CursorIcon::None, "Con LINE cancelado, el retorno a canvas conserva None");
    }

    // 3. Retorno a canvas con system_cursor activado
    {
        let mut session = Session::empty();
        session.open_drawing(Drawing::new_metric(), "Doc", None);
        let mut app = CadApp::new(session, Services::default());
        app.canvas.zoom_pending = false;
        app.ui.system_cursor = true;
        let ctx = egui::Context::default();
        let center = Pos2::new(600.0, 400.0);

        // Warmup
        let _ = run_frame_full(&mut app, &ctx, vec![Event::PointerMoved(center)], true);
        let (_, icon_init) = run_frame_full(&mut app, &ctx, Vec::new(), true);
        assert_eq!(icon_init, egui::CursorIcon::Crosshair, "Con system_cursor activado, el cursor es Crosshair");

        // Iniciar órbita
        let (cmds_start, _) = run_frame_full(
            &mut app,
            &ctx,
            vec![
                Event::PointerButton { pos: center, button: PointerButton::Middle, pressed: true, modifiers: Modifiers::SHIFT },
            ],
            true,
        );
        assert!(app.canvas.virtual_nav.is_some());
        assert!(cmds_start.iter().any(|c| matches!(c, egui::ViewportCommand::CursorVisible(false))));

        // Liberar órbita
        let (cmds_rel, icon_rel) = run_frame_full(
            &mut app,
            &ctx,
            vec![
                Event::PointerButton { pos: center, button: PointerButton::Middle, pressed: false, modifiers: Modifiers::SHIFT },
            ],
            true,
        );
        assert!(app.canvas.virtual_nav.is_none());
        assert_eq!(icon_rel, egui::CursorIcon::Crosshair, "Con system_cursor activado, la UI solicita Crosshair al liberar");
        assert!(cmds_rel.iter().any(|c| matches!(c, egui::ViewportCommand::CursorGrab(egui::viewport::CursorGrab::None))));
    }

    // 4. Retorno a una zona de UI que requiere cursor visible (menú / título)
    {
        let mut session = Session::empty();
        session.open_drawing(Drawing::new_metric(), "Doc", None);
        let mut app = CadApp::new(session, Services::default());
        app.canvas.zoom_pending = false;
        let ctx = egui::Context::default();
        let center = Pos2::new(600.0, 400.0);
        let top_menu_pos = Pos2::new(100.0, 10.0);

        // Warmup
        let _ = run_frame_full(&mut app, &ctx, vec![Event::PointerMoved(center)], true);
        let _ = run_frame_full(&mut app, &ctx, Vec::new(), true);

        // Iniciar órbita
        let _ = run_frame_full(
            &mut app,
            &ctx,
            vec![
                Event::PointerMoved(center),
                Event::PointerButton { pos: center, button: PointerButton::Middle, pressed: true, modifiers: Modifiers::SHIFT },
            ],
            true,
        );
        assert!(app.canvas.virtual_nav.is_some());

        // Mover y liberar sobre la barra superior / menú
        let (cmds_rel, icon_rel) = run_frame_full(
            &mut app,
            &ctx,
            vec![
                Event::PointerMoved(top_menu_pos),
                Event::PointerButton { pos: top_menu_pos, button: PointerButton::Middle, pressed: false, modifiers: Modifiers::SHIFT },
            ],
            true,
        );
        assert!(app.canvas.virtual_nav.is_none());
        assert_eq!(icon_rel, egui::CursorIcon::Default, "Al soltar sobre la barra de menú/título el cursor debe ser Default");
        assert!(cmds_rel.iter().any(|c| matches!(c, egui::ViewportCommand::CursorGrab(egui::viewport::CursorGrab::None))));
    }

    // 5. Liberación por Escape y pérdida de foco
    {
        let mut session = Session::empty();
        session.open_drawing(Drawing::new_metric(), "Doc", None);
        let mut app = CadApp::new(session, Services::default());
        app.canvas.zoom_pending = false;
        let ctx = egui::Context::default();
        let center = Pos2::new(600.0, 400.0);

        // Warmup
        let _ = run_frame_full(&mut app, &ctx, vec![Event::PointerMoved(center)], true);
        let _ = run_frame_full(&mut app, &ctx, Vec::new(), true);

        // 5a. Escape durante órbita
        let _ = run_frame_full(
            &mut app,
            &ctx,
            vec![
                Event::PointerMoved(center),
                Event::PointerButton { pos: center, button: PointerButton::Middle, pressed: true, modifiers: Modifiers::SHIFT },
            ],
            true,
        );
        assert!(app.canvas.virtual_nav.is_some());
        let (cmds_esc, icon_esc) = run_frame_full(
            &mut app,
            &ctx,
            vec![Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }],
            true,
        );
        assert!(app.canvas.virtual_nav.is_none(), "Escape debe liberar navegación");
        assert_eq!(icon_esc, egui::CursorIcon::None, "Sobre el canvas el cursor es None tras Escape");
        assert!(!cmds_esc.iter().any(|c| matches!(c, egui::ViewportCommand::CursorVisible(true))));
        // Limpiar botón
        let _ = run_frame_full(&mut app, &ctx, vec![Event::PointerButton { pos: center, button: PointerButton::Middle, pressed: false, modifiers: Modifiers::NONE }], true);

        // 5b. Pérdida de foco durante órbita
        let _ = run_frame_full(
            &mut app,
            &ctx,
            vec![
                Event::PointerMoved(center),
                Event::PointerButton { pos: center, button: PointerButton::Middle, pressed: true, modifiers: Modifiers::SHIFT },
            ],
            true,
        );
        assert!(app.canvas.virtual_nav.is_some());
        let (cmds_unfocus, _) = run_frame_full(&mut app, &ctx, Vec::new(), false);
        assert!(app.canvas.virtual_nav.is_none(), "Perder foco debe liberar navegación");
        assert!(cmds_unfocus.iter().any(|c| matches!(c, egui::ViewportCommand::CursorVisible(true))), "Perder foco debe forzar visibilidad");
        assert!(cmds_unfocus.iter().any(|c| matches!(c, egui::ViewportCommand::CursorGrab(egui::viewport::CursorGrab::None))));
    }
}

