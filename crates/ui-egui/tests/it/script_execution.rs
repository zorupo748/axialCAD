//! Tests for .scr script file execution in the UI:
//! - Three entry points: menu Tools > Run Script..., command SCRIPT, alias SCR.
//! - File picker cancellation leaving state intact.
//! - Incremental frame execution with responsive budget.
//! - Cancellation with Escape.
//! - Blocking manual canvas inputs while active.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use cadcraft_engine::Session;
use cadcraft_ui_egui::{CadApp, Services};
use egui::{Event, Key, Modifiers, RawInput};

fn temp_script(name: &str, content: &[u8]) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("cadcraft-ui-test-{}-{nanos}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create unique temp dir");
    let path = dir.join(name);
    std::fs::write(&path, content).expect("write temp script");
    path.to_string_lossy().into_owned()
}

#[test]
fn test_three_entry_points_invoke_picker() {
    let pick_count = Arc::new(AtomicUsize::new(0));
    let last_dir_seen = Arc::new(Mutex::new(String::new()));

    let make_services = || {
        let pc = Arc::clone(&pick_count);
        let ld = Arc::clone(&last_dir_seen);
        Services {
            pick_script: Some(Box::new(move |dir: &str| {
                pc.fetch_add(1, Ordering::SeqCst);
                *ld.lock().unwrap() = dir.to_string();
                None // User cancels dialog
            })),
            ..Services::default()
        }
    };

    // 1. Menu entry point: Tools > Run Script... (invokes "script")
    let mut app1 = CadApp::new(Session::new(), make_services());
    cadcraft_ui_egui::menus::activate(&mut app1, "script");
    assert_eq!(pick_count.load(Ordering::SeqCst), 1, "Menu activate('script') must invoke pick_script");

    // 2. Command line typed SCRIPT
    let mut app2 = CadApp::new(Session::new(), make_services());
    app2.cmdline("SCRIPT");
    assert_eq!(pick_count.load(Ordering::SeqCst), 2, "Typing SCRIPT must invoke pick_script");

    // 3. Command line typed SCR (alias)
    let mut app3 = CadApp::new(Session::new(), make_services());
    app3.cmdline("SCR");
    assert_eq!(pick_count.load(Ordering::SeqCst), 3, "Typing SCR alias must invoke pick_script");
}

#[test]
fn test_picker_cancellation_leaves_active_command_and_drawing_intact() {
    let services = Services {
        pick_script: Some(Box::new(|_| None)), // Cancel dialog
        ..Services::default()
    };
    let mut app = CadApp::new(Session::new(), services);

    // Start a LINE command interactively
    app.start("line");
    assert!(app.session.running.is_some(), "LINE command should be running");
    let prev_running = app.session.running.as_ref().map(|r| r.id.clone());
    let prev_models = app.session.state().unwrap().doc.model.len();

    // Trigger SCRIPT via menu
    cadcraft_ui_egui::menus::activate(&mut app, "script");

    // Dialog was cancelled -> active command and drawing remain completely intact
    assert_eq!(app.session.running.as_ref().map(|r| r.id.clone()), prev_running);
    assert_eq!(app.session.state().unwrap().doc.model.len(), prev_models);
}

#[test]
fn test_incremental_execution_across_frames_and_status() {
    // Generate a script with 60 circles
    let mut scr = String::new();
    for i in 0..60 {
        scr.push_str(&format!("circle {i},0 1\n"));
    }
    let path = temp_script("incremental.scr", scr.as_bytes());

    let path_clone = path.clone();
    let services = Services {
        pick_script: Some(Box::new(move |_| Some(path_clone.clone()))),
        ..Services::default()
    };
    let mut app = CadApp::new(Session::new(), services);
    let ctx = egui::Context::default();

    // Trigger script via typed SCRIPT
    app.cmdline("script");
    assert!(app.is_running_script(), "Script should be running");

    // Frame 1
    let status_str = app.script_status().unwrap_or_default();
    assert!(status_str.contains("Running script"), "Status readout should show running script: {status_str}");
    assert!(status_str.contains("incremental.scr"), "Status readout should name script file: {status_str}");

    let mut frames = 0;
    while app.is_running_script() && frames < 100 {
        let mut out = ctx.run_ui(RawInput::default(), |ui| app.logic(ui.ctx()));
        out.textures_delta.clear();
        frames += 1;
    }

    assert!(!app.is_running_script(), "Script should finish");
    assert!(frames > 1, "60 lines with 25 lines/frame budget must take multiple frames (took {frames})");
    assert_eq!(app.session.state().unwrap().doc.model.len(), 60, "All 60 circles should be created");

    let _ = std::fs::remove_file(path);
}

#[test]
fn test_escape_cancels_active_script() {
    let mut scr = String::new();
    for i in 0..100 {
        scr.push_str(&format!("circle {i},0 1\n"));
    }
    let path = temp_script("escape.scr", scr.as_bytes());

    let path_clone = path.clone();
    let services = Services {
        pick_script: Some(Box::new(move |_| Some(path_clone.clone()))),
        ..Services::default()
    };
    let mut app = CadApp::new(Session::new(), services);
    let ctx = egui::Context::default();

    // Start script
    app.cmdline("script");
    assert!(app.is_running_script());

    // Execute first slice (25 lines)
    let mut out1 = ctx.run_ui(RawInput::default(), |ui| app.logic(ui.ctx()));
    out1.textures_delta.clear();
    assert!(app.is_running_script());
    let entities_after_f1 = app.session.state().unwrap().doc.model.len();
    assert!(entities_after_f1 > 0 && entities_after_f1 < 100);

    // Frame with Escape key event
    let raw_escape = RawInput {
        events: vec![Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        ..Default::default()
    };
    let mut out2 = ctx.run_ui(raw_escape, |ui| app.logic(ui.ctx()));
    out2.textures_delta.clear();

    // Script must be cancelled
    assert!(!app.is_running_script(), "Script must be cancelled by Escape");
    // Partial changes must be retained
    assert_eq!(app.session.state().unwrap().doc.model.len(), entities_after_f1);
    // Cancellation reported
    assert!(app.session.log.iter().any(|l| l.contains("cancelled by user")));

    let _ = std::fs::remove_file(path);
}

#[test]
fn test_layer_sequence_with_datos_and_enter_event_across_frames() {
    let script_content = "\
; 1
; 2
; 3
; 4
; 5
; 6
; 7
; 8
; 9
; 10
; 11
; 12
; 13
_-LAYER
_M
SUELO
_C
8
SUELO
_M
PILAR
_C
7
PILAR
_M
DATOS
_C
3
DATOS
_M
AUX
_C
4
AUX
_M
SOL
_C
1
SOL
_M
TEXTO
_C
2
TEXTO

; 46
; 47
; 48
; 49
; 50
";
    let path = temp_script("layer_seq.scr", script_content.as_bytes());
    let path_clone = path.clone();
    let services = Services {
        pick_script: Some(Box::new(move |_| Some(path_clone.clone()))),
        ..Services::default()
    };
    let mut app = CadApp::new(Session::new(), services);
    let ctx = egui::Context::default();

    // User starts script
    app.cmdline("script");

    // Frame 1: executes lines 1..25, ending at line 25 (_M).
    // An Enter key event from dialog confirmation enters the event stream:
    let raw_enter = RawInput {
        events: vec![Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        ..Default::default()
    };
    let mut out1 = ctx.run_ui(raw_enter, |ui| {
        app.logic(ui.ctx());
        app.ui(ui);
    });
    out1.textures_delta.clear();

    // Frame 2: executes line 26 (DATOS) and subsequent lines up to budget
    let mut out2 = ctx.run_ui(RawInput::default(), |ui| {
        app.logic(ui.ctx());
        app.ui(ui);
    });
    out2.textures_delta.clear();

    // Must reach beyond line 45 and create all layers
    let doc = app.session.doc().unwrap();
    assert!(doc.layer("SUELO").is_some());
    assert!(doc.layer("PILAR").is_some());
    assert!(doc.layer("DATOS").is_some());
    assert!(doc.layer("AUX").is_some());
    assert!(doc.layer("SOL").is_some());
    assert!(doc.layer("TEXTO").is_some());

    let _ = std::fs::remove_file(path);
}

#[test]
fn test_synthetic_multientity_script_quantities_and_geometries() {
    let geom_scr = "\
_-LAYER
_M
SUELO
_C
8
SUELO
_M
PILAR
_C
7
PILAR
_M
DATOS
_C
3
DATOS
_M
AUX
_C
4
AUX
_M
SOL
_C
1
SOL
_M
TEXTO
_C
2
TEXTO

CLAYER
SUELO
_3DFACE
-1.0000,-1.0000,0.0000
5.0000,-1.0000,0.0000
5.0000,6.0000,0.0000
-1.0000,6.0000,0.0000

CLAYER
PILAR
_CYLINDER
2.0000,3.0000,0.0000
0.06
_A
2.0000,3.0000,3.0000
_-MTEXT
2.2000,3.2000,3.0000
_H
0.25
4.2250,2.8000,3.0000
Pilar H=3

CLAYER
DATOS
_SPHERE
1.0000,0.0000,1.0000
0.1
_SPHERE
0.0000,4.5000,0.0000
0.1
_LINE
1.0000,0.0000,0.0000
1.0000,0.0000,1.0000

CLAYER
TEXTO
_-MTEXT
1.1500,0.0000,1.1500
_H
0.25
3.1750,-0.4000,1.1500
A (1,0,1)

_-MTEXT
0.1500,4.5000,0.1500
_H
0.25
2.6250,4.1000,0.1500
B (0,4.5,0)

CLAYER
AUX
_CYLINDER
1.0000,0.0000,1.0000
0.02
_A
0.0000,4.5000,0.0000
_SPHERE
0.5000,2.2500,0.5000
0.07
CLAYER
TEXTO
_-MTEXT
0.6500,2.2500,0.6500
_H
0.25
4.0250,1.8500,0.6500
M (punto medio)

CLAYER
AUX
_3DFACE
-1.0000,1.8056,0.0000
-1.0000,2.5167,3.2000
4.0000,3.6278,3.2000
4.0000,2.9167,0.0000

CLAYER
SOL
_SPHERE
2.0000,3.0000,2.3750
0.1
_CYLINDER
2.0000,3.0000,0.0000
0.09
_A
2.0000,3.0000,2.3750
CLAYER
TEXTO
_-MTEXT
2.2000,3.2000,2.4750
_H
0.25
4.9000,2.8000,2.4750
C  h=2.375 m

CLAYER
SOL
_CYLINDER
1.0000,0.0000,1.0000
0.035
_A
2.0000,3.0000,2.3750
_CYLINDER
2.0000,3.0000,2.3750
0.035
_A
0.0000,4.5000,0.0000
CLAYER
TEXTO
_-MTEXT
1.2000,1.4000,2.0000
_H
0.25
5.0250,1.0000,2.0000
AC = CB = 3.448 m

";
    let path = temp_script("geom_test.scr", geom_scr.as_bytes());

    let mut session = Session::new();
    let res = session.start_script_file(&path, geom_scr);
    assert!(res.is_ok(), "start script must succeed: {res:?}");

    while session.is_script_running() {
        match session.step_script() {
            cadcraft_engine::ScriptStep::Progress { .. } => {}
            cadcraft_engine::ScriptStep::Done { .. } => break,
            cadcraft_engine::ScriptStep::Error { line, line_text, error, .. } => {
                panic!("script failed at line {line} ('{line_text}'): {error}");
            }
            cadcraft_engine::ScriptStep::EofPending { command, .. } => {
                panic!("script reached EOF with pending command: {command}");
            }
            cadcraft_engine::ScriptStep::Cancelled { .. } => {
                panic!("script cancelled unexpectedly");
            }
        }
    }

    let doc = session.doc().expect("document must exist");

    // 1. Verify Layers and their colors
    let l_suelo = doc.layer("SUELO").expect("layer SUELO must exist");
    assert_eq!(l_suelo.color, cadcraft_color::Color::Index(8));

    let l_pilar = doc.layer("PILAR").expect("layer PILAR must exist");
    assert_eq!(l_pilar.color, cadcraft_color::Color::Index(7));

    let l_datos = doc.layer("DATOS").expect("layer DATOS must exist");
    assert_eq!(l_datos.color, cadcraft_color::Color::Index(3));

    let l_aux = doc.layer("AUX").expect("layer AUX must exist");
    assert_eq!(l_aux.color, cadcraft_color::Color::Index(4));

    let l_sol = doc.layer("SOL").expect("layer SOL must exist");
    assert_eq!(l_sol.color, cadcraft_color::Color::Index(1));

    let l_texto = doc.layer("TEXTO").expect("layer TEXTO must exist");
    assert_eq!(l_texto.color, cadcraft_color::Color::Index(2));

    // 2. Count primary entities in model space (excluding internal mesh discretization faces)
    use cadcraft_doc::{EntityKind, FeatureOp};

    let mut face3ds = Vec::new();
    let mut cylinders = Vec::new();
    let mut spheres = Vec::new();
    let mut lines = Vec::new();
    let mut mtexts = Vec::new();

    for ent in doc.model.iter() {
        match &ent.kind {
            EntityKind::Face3d(f) => face3ds.push((ent.common.layer.as_str(), f)),
            EntityKind::Solid3d(s) => {
                let first_op = s.recipe.features.first().map(|f| &f.op).expect("solid must have features");
                match first_op {
                    FeatureOp::Cylinder { center, radius, height, axis } => {
                        cylinders.push((ent.common.layer.as_str(), *center, *radius, *height, *axis));
                    }
                    FeatureOp::Sphere { center, radius } => {
                        spheres.push((ent.common.layer.as_str(), *center, *radius));
                    }
                    _ => panic!("unexpected solid feature"),
                }
            }
            EntityKind::Line(l) => lines.push((ent.common.layer.as_str(), l)),
            EntityKind::MText(m) => mtexts.push((ent.common.layer.as_str(), m)),
            _ => {}
        }
    }

    assert_eq!(face3ds.len(), 2, "must create exactly 2 Face3d entities");
    assert_eq!(cylinders.len(), 5, "must create exactly 5 Cylinder solid entities");
    assert_eq!(spheres.len(), 4, "must create exactly 4 Sphere solid entities");
    assert_eq!(lines.len(), 1, "must create exactly 1 Line entity");
    assert_eq!(mtexts.len(), 6, "must create exactly 6 MText entities");

    // Verify 3DFACE 1: SUELO
    let (layer_f1, f1) = face3ds[0];
    assert_eq!(layer_f1, "SUELO");
    assert_eq!(f1.corners[0], cadcraft_geom::Vec3::new(-1.0, -1.0, 0.0));
    assert_eq!(f1.corners[1], cadcraft_geom::Vec3::new(5.0, -1.0, 0.0));
    assert_eq!(f1.corners[2], cadcraft_geom::Vec3::new(5.0, 6.0, 0.0));
    assert_eq!(f1.corners[3], cadcraft_geom::Vec3::new(-1.0, 6.0, 0.0));

    // Verify 3DFACE 2: AUX (plano mediador)
    let (layer_f2, f2) = face3ds[1];
    assert_eq!(layer_f2, "AUX");
    assert_eq!(f2.corners[0], cadcraft_geom::Vec3::new(-1.0, 1.8056, 0.0));
    assert_eq!(f2.corners[1], cadcraft_geom::Vec3::new(-1.0, 2.5167, 3.2));
    assert_eq!(f2.corners[2], cadcraft_geom::Vec3::new(4.0, 3.6278, 3.2));
    assert_eq!(f2.corners[3], cadcraft_geom::Vec3::new(4.0, 2.9167, 0.0));

    // Verify Cylinders
    // Cylinder 1: PILAR (2,3,0) -> (2,3,3), r=0.06
    let (c1_layer, c1_center, c1_r, c1_h, c1_axis) = cylinders[0];
    assert_eq!(c1_layer, "PILAR");
    assert_eq!(c1_center, cadcraft_geom::Vec3::new(2.0, 3.0, 0.0));
    assert!((c1_r - 0.06).abs() < 1e-9);
    assert!((c1_h - 3.0).abs() < 1e-9);
    assert_eq!(c1_axis, cadcraft_geom::Vec3::new(0.0, 0.0, 1.0));

    // Cylinder 2: AUX (1,0,1) -> (0,4.5,0), r=0.02 (oblique)
    let (c2_layer, c2_center, c2_r, c2_h, c2_axis) = cylinders[1];
    assert_eq!(c2_layer, "AUX");
    assert_eq!(c2_center, cadcraft_geom::Vec3::new(1.0, 0.0, 1.0));
    assert!((c2_r - 0.02).abs() < 1e-9);
    let expected_delta = cadcraft_geom::Vec3::new(-1.0, 4.5, -1.0);
    assert!((c2_h - expected_delta.len()).abs() < 1e-6);
    assert!((c2_axis - expected_delta.normalized()).len() < 1e-6);

    // Cylinder 3: SOL (2,3,0) -> (2,3,2.375), r=0.09
    let (c3_layer, c3_center, c3_r, c3_h, _c3_axis) = cylinders[2];
    assert_eq!(c3_layer, "SOL");
    assert_eq!(c3_center, cadcraft_geom::Vec3::new(2.0, 3.0, 0.0));
    assert!((c3_r - 0.09).abs() < 1e-9);
    assert!((c3_h - 2.375).abs() < 1e-9);

    // Cylinder 4: SOL cable AC (1,0,1) -> (2,3,2.375), r=0.035
    let (c4_layer, c4_center, c4_r, c4_h, _) = cylinders[3];
    assert_eq!(c4_layer, "SOL");
    assert_eq!(c4_center, cadcraft_geom::Vec3::new(1.0, 0.0, 1.0));
    assert!((c4_r - 0.035).abs() < 1e-9);
    let delta_ac = cadcraft_geom::Vec3::new(1.0, 3.0, 1.375);
    assert!((c4_h - delta_ac.len()).abs() < 1e-3);

    // Cylinder 5: SOL cable CB (2,3,2.375) -> (0,4.5,0), r=0.035
    let (c5_layer, c5_center, c5_r, c5_h, _) = cylinders[4];
    assert_eq!(c5_layer, "SOL");
    assert_eq!(c5_center, cadcraft_geom::Vec3::new(2.0, 3.0, 2.375));
    assert!((c5_r - 0.035).abs() < 1e-9);
    let delta_cb = cadcraft_geom::Vec3::new(-2.0, 1.5, -2.375);
    assert!((c5_h - delta_cb.len()).abs() < 1e-3);

    // Verify Spheres
    // Sphere 1: DATOS (1,0,1) r=0.1
    assert_eq!(spheres[0], ("DATOS", cadcraft_geom::Vec3::new(1.0, 0.0, 1.0), 0.1));
    // Sphere 2: DATOS (0,4.5,0) r=0.1
    assert_eq!(spheres[1], ("DATOS", cadcraft_geom::Vec3::new(0.0, 4.5, 0.0), 0.1));
    // Sphere 3: AUX (0.5,2.25,0.5) r=0.07
    assert_eq!(spheres[2], ("AUX", cadcraft_geom::Vec3::new(0.5, 2.25, 0.5), 0.07));
    // Sphere 4: SOL (2,3,2.375) r=0.1
    assert_eq!(spheres[3], ("SOL", cadcraft_geom::Vec3::new(2.0, 3.0, 2.375), 0.1));

    // Verify Line 3D
    let (l_layer, line_ent) = lines[0];
    assert_eq!(l_layer, "DATOS");
    assert_eq!(line_ent.a, cadcraft_geom::Vec3::new(1.0, 0.0, 0.0));
    assert_eq!(line_ent.b, cadcraft_geom::Vec3::new(1.0, 0.0, 1.0));

    // Verify MTexts
    let texts: Vec<(&str, &str, f64, cadcraft_geom::Vec3)> = mtexts
        .iter()
        .map(|(ly, m)| (*ly, m.contents.as_str(), m.height, m.insert))
        .collect();

    assert_eq!(texts[0].0, "PILAR");
    assert_eq!(texts[0].1, "Pilar H=3");
    assert_eq!(texts[0].2, 0.25);
    assert_eq!(texts[0].3, cadcraft_geom::Vec3::new(2.2, 3.2, 3.0));

    assert_eq!(texts[1].0, "TEXTO");
    assert_eq!(texts[1].1, "A (1,0,1)");
    assert_eq!(texts[1].2, 0.25);
    assert_eq!(texts[1].3, cadcraft_geom::Vec3::new(1.15, 0.0, 1.15));

    assert_eq!(texts[2].0, "TEXTO");
    assert_eq!(texts[2].1, "B (0,4.5,0)");
    assert_eq!(texts[2].2, 0.25);
    assert_eq!(texts[2].3, cadcraft_geom::Vec3::new(0.15, 4.5, 0.15));

    assert_eq!(texts[3].0, "TEXTO");
    assert_eq!(texts[3].1, "M (punto medio)");
    assert_eq!(texts[3].2, 0.25);
    assert_eq!(texts[3].3, cadcraft_geom::Vec3::new(0.65, 2.25, 0.65));

    assert_eq!(texts[4].0, "TEXTO");
    assert_eq!(texts[4].1, "C  h=2.375 m");
    assert_eq!(texts[4].2, 0.25);
    assert_eq!(texts[4].3, cadcraft_geom::Vec3::new(2.2, 3.2, 2.475));

    assert_eq!(texts[5].0, "TEXTO");
    assert_eq!(texts[5].1, "AC = CB = 3.448 m");
    assert_eq!(texts[5].2, 0.25);
    assert_eq!(texts[5].3, cadcraft_geom::Vec3::new(1.2, 1.4, 2.0));

    let _ = std::fs::remove_file(path);
}

#[test]
fn test_synthetic_script_halts_at_unsupported_command_with_line_number() {
    let full_content = "\
; Synthetic script testing error halt and line reporting
_-LAYER
_M
TEST_LAYER
_C
1
TEST_LAYER

_LINE
0,0,0
10,10,0

_VSCURRENT
2
_LINE
10,10,0
20,20,0
";
    let path = temp_script("full_test.scr", full_content.as_bytes());

    let mut session = Session::new();
    let res = session.start_script_file(&path, full_content);
    assert!(res.is_ok(), "start script must succeed: {res:?}");

    let mut halted = false;
    while session.is_script_running() {
        match session.step_script() {
            cadcraft_engine::ScriptStep::Progress { .. } => {}
            cadcraft_engine::ScriptStep::Done { .. } => break,
            cadcraft_engine::ScriptStep::Error { line, error, .. } => {
                assert_eq!(line, 13, "Must halt at line 13 on _VSCURRENT, halted at {line}: {error}");
                assert!(error.contains("_VSCURRENT"), "Error must mention _VSCURRENT: {error}");
                halted = true;
                break;
            }
            cadcraft_engine::ScriptStep::EofPending { .. } | cadcraft_engine::ScriptStep::Cancelled { .. } => break,
        }
    }
    assert!(halted, "Synthetic script must halt on line 13 _VSCURRENT");
    let _ = std::fs::remove_file(path);
}

#[test]
fn test_cylinder_and_sphere_reject_invalid_inputs_and_cleanup() {
    let mut session = Session::new();

    // 1. Cylinder with non-positive radius
    let err1 = session.script("cylinder 0,0,0 -5 10");
    assert!(err1.is_err(), "negative radius must be rejected");

    // 2. Cylinder with zero axis length
    let err2 = session.script("cylinder 0,0,0 1 _A 0,0,0");
    assert!(err2.is_err(), "coincident axis endpoint must be rejected");

    // 3. Sphere with negative radius
    let err3 = session.script("sphere 0,0,0 -1");
    assert!(err3.is_err(), "negative sphere radius must be rejected");

    // 4. State must be clean (no stuck interactive command)
    assert!(session.running.is_none(), "Session running command must be None after errors");
}

#[test]
fn test_undo_redo_of_phase_a_entities() {
    let mut session = Session::new();
    let prev_count = session.doc().unwrap().model.len();

    // Create a 3dface
    session.script("3dface 0,0,0 10,0,0 10,10,0 0,10,0 \n").unwrap();
    assert_eq!(session.doc().unwrap().model.len(), prev_count + 1);

    // Undo
    session.undo().unwrap();
    assert_eq!(session.doc().unwrap().model.len(), prev_count);

    // Redo
    session.redo().unwrap();
    assert_eq!(session.doc().unwrap().model.len(), prev_count + 1);
}

#[test]
fn test_invalid_option_keyword_halts_script_with_diagnosis() {
    let scr = "_-LAYER\n_INVALIDOPTION\nSUELO\n";
    let path = temp_script("invalid_opt.scr", scr.as_bytes());
    let path_clone = path.clone();
    let services = Services {
        pick_script: Some(Box::new(move |_| Some(path_clone.clone()))),
        ..Services::default()
    };
    let mut app = CadApp::new(Session::new(), services);
    let ctx = egui::Context::default();

    app.load_script(&path);

    let mut out = ctx.run_ui(RawInput::default(), |ui| {
        app.logic(ui.ctx());
        app.ui(ui);
    });
    out.textures_delta.clear();

    assert!(!app.is_running_script(), "Script must halt after invalid option");
    assert!(
        app.status.as_ref().is_some_and(|(s, _)| s.contains("line 2") && s.contains("Invalid option keyword")),
        "Status: {:?}",
        app.status
    );
    assert!(app.session.running.is_none(), "Running command state must be cleaned up");

    let _ = std::fs::remove_file(path);
}
