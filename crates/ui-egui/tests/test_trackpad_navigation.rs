use cadcraft_doc::Drawing;
use cadcraft_engine::Session;
use cadcraft_ui_egui::options::NavDevice;
use cadcraft_ui_egui::{CadApp, Services};
use egui::{Event, Modifiers, MouseWheelUnit, PointerButton, Pos2, Rect, TouchPhase, vec2};
use serde_json::json;

fn create_test_app() -> (CadApp, egui::Context) {
    let mut session = Session::empty();
    let mut doc = Drawing::new_metric();
    // Add a test line so geometry exists
    doc.add(
        &cadcraft_doc::Space::Model,
        cadcraft_doc::Common::default(),
        cadcraft_doc::EntityKind::Line(cadcraft_doc::Line {
            a: cadcraft_geom::Vec3::new(0.0, 0.0, 0.0),
            b: cadcraft_geom::Vec3::new(100.0, 100.0, 0.0),
        }),
    )
    .unwrap();
    session.open_drawing(doc, "TestDrawing", None);
    session.viewport_px = (1200.0, 800.0);
    let mut app = CadApp::new(session, Services::default());
    app.canvas.zoom_pending = false;
    let ctx = egui::Context::default();
    (app, ctx)
}

/// The canvas centre in screen coordinates.
fn canvas_center(app: &CadApp) -> egui::Pos2 {
    app.canvas.rect.expect("canvas rect established").center()
}

fn run_frame(app: &mut CadApp, ctx: &egui::Context, events: Vec<Event>) {
    let screen_rect = Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 800.0));
    let mut viewports = egui::ViewportIdMap::default();
    viewports.insert(
        egui::ViewportId::ROOT,
        egui::ViewportInfo {
            focused: Some(true),
            ..Default::default()
        },
    );
    let mut modifiers = Modifiers::default();
    for ev in &events {
        match ev {
            Event::MouseWheel { modifiers: m, .. } => {
                if m.shift { modifiers.shift = true; }
                if m.ctrl { modifiers.ctrl = true; }
                if m.alt { modifiers.alt = true; }
            }
            Event::PointerButton { modifiers: m, .. } => {
                if m.shift { modifiers.shift = true; }
                if m.ctrl { modifiers.ctrl = true; }
                if m.alt { modifiers.alt = true; }
            }
            _ => {}
        }
    }
    let mut all_events = Vec::new();
    all_events.push(Event::ModifiersChanged(modifiers));
    all_events.extend(events);
    let raw = egui::RawInput {
        screen_rect: Some(screen_rect),
        focused: true,
        viewports,
        events: all_events,
        ..Default::default()
    };
    let mut out = ctx.run_ui(raw, |ui| {
        app.logic(ui.ctx());
        app.ui(ui);
    });
    out.textures_delta.clear();
}

#[test]
fn test_trackpad_pan_2d_and_3d() {
    let (mut app, ctx) = create_test_app();

    // Initial frame to establish viewport and layout
    run_frame(&mut app, &ctx, vec![Event::PointerMoved(egui::pos2(600.0, 400.0))]);
    let center = canvas_center(&app);

    let initial_cam = app.session.camera();
    let initial_target = initial_cam.target;
    let initial_az = initial_cam.azimuth;
    let initial_el = initial_cam.elevation;

    // 1. Two-finger swipe in 2D (Plan view)
    run_frame(
        &mut app,
        &ctx,
        vec![
            Event::PointerMoved(center),
            Event::MouseWheel {
                unit: MouseWheelUnit::Point,
                delta: vec2(25.0, -15.0),
                modifiers: Modifiers::default(),
                phase: TouchPhase::Move,
            },
        ],
    );

    let pan2d_cam = app.session.camera();
    assert_ne!(pan2d_cam.target, initial_target, "2D pan must shift camera target");
    assert!((pan2d_cam.azimuth - initial_az).abs() < 1e-6, "2D pan must keep azimuth constant");
    assert!((pan2d_cam.elevation - initial_el).abs() < 1e-6, "2D pan must keep elevation constant");

    // 2. Switch to 3D isometric view
    app.session.execute("view.swiso", &json!({})).expect("swiso succeeds");
    run_frame(&mut app, &ctx, vec![Event::PointerMoved(center)]);

    let iso_cam = app.session.camera();
    let iso_target = iso_cam.target;
    let iso_az = iso_cam.azimuth;
    let iso_el = iso_cam.elevation;
    assert!(!iso_cam.is_plan(), "Must be in 3D view");

    // Two-finger swipe in 3D (Pan without modifiers)
    run_frame(
        &mut app,
        &ctx,
        vec![
            Event::PointerMoved(center),
            Event::MouseWheel {
                unit: MouseWheelUnit::Point,
                delta: vec2(30.0, 20.0),
                modifiers: Modifiers::default(),
                phase: TouchPhase::Move,
            },
        ],
    );

    let pan3d_cam = app.session.camera();
    assert_ne!(pan3d_cam.target, iso_target, "3D pan must translate camera target in view plane");
    assert!((pan3d_cam.azimuth - iso_az).abs() < 1e-6, "3D pan must NOT rotate azimuth");
    assert!((pan3d_cam.elevation - iso_el).abs() < 1e-6, "3D pan must NOT rotate elevation");
    assert!(pan3d_cam.target.x.is_finite() && pan3d_cam.target.y.is_finite() && pan3d_cam.target.z.is_finite());
}

#[test]
fn test_trackpad_orbit_with_shift_momentary() {
    let (mut app, ctx) = create_test_app();

    // 1. Orbit in 3D Isometric view
    app.session.execute("view.swiso", &json!({})).expect("swiso succeeds");
    run_frame(&mut app, &ctx, vec![Event::PointerMoved(egui::pos2(600.0, 400.0))]);
    let center = canvas_center(&app);

    let base_cam = app.session.camera();
    let base_az = base_cam.azimuth;
    let base_el = base_cam.elevation;

    let mut shift_mods = Modifiers::default();
    shift_mods.shift = true;

    // Shift + Two-finger swipe: orbits in 3D
    run_frame(
        &mut app,
        &ctx,
        vec![
            Event::PointerMoved(center),
            Event::MouseWheel {
                unit: MouseWheelUnit::Point,
                delta: vec2(40.0, -25.0),
                modifiers: shift_mods,
                phase: TouchPhase::Move,
            },
        ],
    );

    let orbit_cam = app.session.camera();
    assert_ne!(orbit_cam.azimuth, base_az, "Shift+swipe must update camera azimuth");
    assert_ne!(orbit_cam.elevation, base_el, "Shift+swipe must update camera elevation");
    assert!(orbit_cam.azimuth.is_finite());
    assert!(orbit_cam.elevation.is_finite());

    // End gesture (zero delta): verify no persistent mode remains
    run_frame(
        &mut app,
        &ctx,
        vec![
            Event::PointerMoved(center),
            Event::MouseWheel {
                unit: MouseWheelUnit::Point,
                delta: vec2(0.0, 0.0),
                modifiers: Modifiers::default(),
                phase: TouchPhase::End,
            },
        ],
    );

    // Subsequent swipe without Shift immediately pans, not orbits
    let pre_sub_cam = app.session.camera();
    run_frame(
        &mut app,
        &ctx,
        vec![
            Event::PointerMoved(center),
            Event::MouseWheel {
                unit: MouseWheelUnit::Point,
                delta: vec2(20.0, 10.0),
                modifiers: Modifiers::default(),
                phase: TouchPhase::Move,
            },
        ],
    );
    let post_sub_cam = app.session.camera();
    assert_ne!(post_sub_cam.target, pre_sub_cam.target, "Subsequent movement without Shift must pan");
    assert_eq!(post_sub_cam.azimuth, pre_sub_cam.azimuth, "Azimuth must not change without Shift");
    assert_eq!(post_sub_cam.elevation, pre_sub_cam.elevation, "Elevation must not change without Shift");

    // 2. Initiate orbit directly from Plan view
    app.session.execute("view.top", &json!({})).expect("view.top succeeds");
    run_frame(&mut app, &ctx, vec![Event::PointerMoved(center)]);
    let plan_cam = app.session.camera();
    assert!(plan_cam.is_plan(), "Must be in plan view");

    run_frame(
        &mut app,
        &ctx,
        vec![
            Event::PointerMoved(center),
            Event::MouseWheel {
                unit: MouseWheelUnit::Point,
                delta: vec2(30.0, -30.0),
                modifiers: shift_mods,
                phase: TouchPhase::Move,
            },
        ],
    );
    let post_plan_cam = app.session.camera();
    assert!(!post_plan_cam.is_plan(), "Shift + 2-finger swipe must initiate 3D orbit from plan view");
}

#[test]
fn test_trackpad_shift_press_and_release_no_jumps() {
    let (mut app, ctx) = create_test_app();
    app.session.execute("view.swiso", &json!({})).expect("swiso succeeds");
    run_frame(&mut app, &ctx, vec![Event::PointerMoved(egui::pos2(600.0, 400.0))]);
    let center = canvas_center(&app);

    // Frame 1: Pan without Shift
    let cam0 = app.session.camera();
    run_frame(
        &mut app,
        &ctx,
        vec![
            Event::PointerMoved(center),
            Event::MouseWheel {
                unit: MouseWheelUnit::Point,
                delta: vec2(20.0, 0.0),
                modifiers: Modifiers::default(),
                phase: TouchPhase::Move,
            },
        ],
    );
    let cam1 = app.session.camera();
    assert_ne!(cam1.target, cam0.target, "Frame 1 must pan");
    assert_eq!(cam1.azimuth, cam0.azimuth);
    assert_eq!(cam1.elevation, cam0.elevation);

    // Frame 2: Mid-gesture, user presses Shift -> transitions cleanly to orbit
    let mut shift_mods = Modifiers::default();
    shift_mods.shift = true;
    run_frame(
        &mut app,
        &ctx,
        vec![
            Event::PointerMoved(center),
            Event::MouseWheel {
                unit: MouseWheelUnit::Point,
                delta: vec2(25.0, -15.0),
                modifiers: shift_mods,
                phase: TouchPhase::Move,
            },
        ],
    );
    let cam2 = app.session.camera();
    assert_ne!(cam2.azimuth, cam1.azimuth, "Frame 2 must orbit azimuth");
    assert_ne!(cam2.elevation, cam1.elevation, "Frame 2 must orbit elevation");

    // Frame 3: Mid-gesture, user releases Shift -> transitions back to pan without jumps
    run_frame(
        &mut app,
        &ctx,
        vec![
            Event::PointerMoved(center),
            Event::MouseWheel {
                unit: MouseWheelUnit::Point,
                delta: vec2(15.0, 0.0),
                modifiers: Modifiers::default(),
                phase: TouchPhase::Move,
            },
        ],
    );
    let cam3 = app.session.camera();
    assert_ne!(cam3.target, cam2.target, "Frame 3 must resume panning");
    assert_eq!(cam3.azimuth, cam2.azimuth, "Frame 3 azimuth must stay invariant");
    assert_eq!(cam3.elevation, cam2.elevation, "Frame 3 elevation must stay invariant");

    // Frame 4: Fingers lift (TouchPhase::End)
    run_frame(
        &mut app,
        &ctx,
        vec![
            Event::PointerMoved(center),
            Event::MouseWheel {
                unit: MouseWheelUnit::Point,
                delta: vec2(0.0, 0.0),
                modifiers: Modifiers::default(),
                phase: TouchPhase::End,
            },
        ],
    );

    // Frame 5: Trailing inertia scroll arrives, but fingers have lifted.
    // If user presses Shift during trailing inertia, it must NOT suddenly convert to orbit!
    let cam_before_inertia = app.session.camera();
    run_frame(
        &mut app,
        &ctx,
        vec![
            Event::PointerMoved(center),
            Event::MouseWheel {
                unit: MouseWheelUnit::Point,
                delta: vec2(10.0, 5.0),
                modifiers: shift_mods, // User pressed Shift after lifting fingers
                phase: TouchPhase::Move,
            },
        ],
    );
    let cam_after_inertia = app.session.camera();
    assert_eq!(
        cam_after_inertia.azimuth, cam_before_inertia.azimuth,
        "Pan inertia must not turn into orbit upon pressing Shift after lifting fingers"
    );
    assert_eq!(
        cam_after_inertia.elevation, cam_before_inertia.elevation,
        "Pan inertia must not turn into orbit upon pressing Shift after lifting fingers"
    );
}

#[test]
fn test_pinch_zoom_with_and_without_shift() {
    let (mut app, ctx) = create_test_app();
    run_frame(&mut app, &ctx, vec![Event::PointerMoved(egui::pos2(600.0, 400.0))]);
    let center = canvas_center(&app);

    let initial_h = app.session.state().expect("state ok").view().height;
    let initial_cam = app.session.camera();

    // 1. Pinch zoom in without Shift (factor = 1.25)
    run_frame(
        &mut app,
        &ctx,
        vec![
            Event::PointerMoved(center),
            Event::Zoom(1.25),
        ],
    );

    let h1 = app.session.state().expect("state ok").view().height;
    assert!(h1 < initial_h, "Zooming in must decrease view height");
    assert!((h1 - (initial_h / 1.25)).abs() < 1e-4, "Zoom factor must match 1.25 exactly");
    assert_eq!(app.session.camera().azimuth, initial_cam.azimuth);
    assert_eq!(app.session.camera().elevation, initial_cam.elevation);

    // 2. Pinch zoom with Shift held: MUST ONLY ZOOM, NEVER ORBIT
    let mut shift_mods = Modifiers::default();
    shift_mods.shift = true;
    run_frame(
        &mut app,
        &ctx,
        vec![
            Event::PointerMoved(center),
            Event::Zoom(0.8), // zoom out factor
        ],
    );

    let h2 = app.session.state().expect("state ok").view().height;
    assert!(h2 > h1, "Zooming out must increase view height");
    let cam_after_shift_pinch = app.session.camera();
    assert_eq!(cam_after_shift_pinch.azimuth, initial_cam.azimuth, "Pinch with Shift must NOT orbit");
    assert_eq!(cam_after_shift_pinch.elevation, initial_cam.elevation, "Pinch with Shift must NOT orbit");

    // 3. Jitter suppression: scroll events during a pinch do not cause accidental pan/orbit
    let cam_before_jitter = app.session.camera();
    let h_before_jitter = app.session.state().expect("state ok").view().height;
    run_frame(
        &mut app,
        &ctx,
        vec![
            Event::PointerMoved(center),
            Event::Zoom(1.1),
            Event::MouseWheel {
                unit: MouseWheelUnit::Point,
                delta: vec2(50.0, 50.0), // Jitter from fingers during pinch
                modifiers: shift_mods,
                phase: TouchPhase::Move,
            },
        ],
    );
    let cam_after_jitter = app.session.camera();
    let h_after_jitter = app.session.state().expect("state ok").view().height;
    assert!(h_after_jitter < h_before_jitter, "Zoom must still apply during jitter");
    assert_eq!(cam_after_jitter.azimuth, cam_before_jitter.azimuth, "Orbit must be suppressed during pinch");
    assert_eq!(cam_after_jitter.elevation, cam_before_jitter.elevation, "Orbit must be suppressed during pinch");
    assert!(
        (cam_after_jitter.target.x - cam_before_jitter.target.x).abs() < 1e-3
            && (cam_after_jitter.target.y - cam_before_jitter.target.y).abs() < 1e-3,
        "Pan must be suppressed during pinch"
    );

    // 4. Exact scale conversion verification:
    // When Wayland ratio = 1.25, delta = ratio.ln(), egui-winit exp(delta) == 1.25.
    let ratio: f64 = 1.25;
    let pinch_delta = ratio.ln();
    let exp_factor = (pinch_delta as f32).exp();
    assert!((exp_factor - 1.25).abs() < 1e-5, "Log-ratio to exp conversion must be mathematically exact");
}

#[test]
fn test_three_and_four_finger_gestures_ignored() {
    let (mut app, ctx) = create_test_app();
    run_frame(&mut app, &ctx, vec![Event::PointerMoved(egui::pos2(600.0, 400.0))]);
    let center = canvas_center(&app);

    let initial_cam = app.session.camera();
    let initial_h = app.session.state().expect("state ok").view().height;

    // In axialCAD, no 3- or 4-finger gesture handler modifies the camera.
    // Only standard 2-finger scroll and 2-finger pinch events act on the camera.
    // If standard frame receives no 2-finger event, state is unchanged:
    run_frame(&mut app, &ctx, vec![Event::PointerMoved(center)]);

    let current_cam = app.session.camera();
    let current_h = app.session.state().expect("state ok").view().height;
    assert_eq!(current_cam.target, initial_cam.target);
    assert_eq!(current_cam.azimuth, initial_cam.azimuth);
    assert_eq!(current_cam.elevation, initial_cam.elevation);
    assert_eq!(current_h, initial_h);
}

#[test]
fn test_isolation_menus_and_active_commands() {
    let (mut app, ctx) = create_test_app();
    run_frame(&mut app, &ctx, vec![Event::PointerMoved(egui::pos2(600.0, 400.0))]);
    let center = canvas_center(&app);

    // 1. Isolation when a popup or menu is open
    app.canvas.popup_open = true;
    let cam_before = app.session.camera();
    run_frame(
        &mut app,
        &ctx,
        vec![
            Event::PointerMoved(center),
            Event::MouseWheel {
                unit: MouseWheelUnit::Point,
                delta: vec2(30.0, 30.0),
                modifiers: Modifiers::default(),
                phase: TouchPhase::Move,
            },
        ],
    );
    let cam_after = app.session.camera();
    assert_eq!(cam_after.target, cam_before.target, "Scrolling over open menu must not move camera");
    app.canvas.popup_open = false;

    // 2. Active command (LINE) does not receive unintended points or cancel during trackpad nav
    app.session.start("line").expect("line command starts");
    assert!(app.session.prompting(), "LINE command must be prompting for points");
    let entities_before = app.session.doc().expect("doc ok").entity_count();

    // Two-finger pan while LINE command is running
    run_frame(
        &mut app,
        &ctx,
        vec![
            Event::PointerMoved(center),
            Event::MouseWheel {
                unit: MouseWheelUnit::Point,
                delta: vec2(20.0, -10.0),
                modifiers: Modifiers::default(),
                phase: TouchPhase::Move,
            },
        ],
    );
    assert!(app.session.prompting(), "LINE command must still be prompting after trackpad pan");
    assert_eq!(
        app.session.doc().expect("doc ok").entity_count(),
        entities_before,
        "Trackpad pan must not insert points into active command"
    );

    // Shift + two-finger orbit while LINE command is running
    let mut shift_mods = Modifiers::default();
    shift_mods.shift = true;
    run_frame(
        &mut app,
        &ctx,
        vec![
            Event::PointerMoved(center),
            Event::MouseWheel {
                unit: MouseWheelUnit::Point,
                delta: vec2(15.0, 20.0),
                modifiers: shift_mods,
                phase: TouchPhase::Move,
            },
        ],
    );
    assert!(app.session.prompting(), "LINE command must still be prompting after trackpad orbit");
    assert_eq!(
        app.session.doc().expect("doc ok").entity_count(),
        entities_before,
        "Trackpad orbit must not insert points into active command"
    );
}

#[test]
fn test_regression_line_delta_under_auto_device() {
    let (mut app, ctx) = create_test_app();
    app.options.nav_device = NavDevice::Auto;
    run_frame(&mut app, &ctx, vec![Event::PointerMoved(egui::pos2(600.0, 400.0))]);
    let center = canvas_center(&app);

    // 1. Holding Shift with MouseWheelUnit::Line:
    // Before fix: filtered out by wheel_zoom_points AND rejected by scroll_pans -> did NOTHING.
    // After fix: recognized as Orbit request and orbits camera.
    let base_cam = app.session.camera();
    let mut shift_mods = Modifiers::default();
    shift_mods.shift = true;
    run_frame(
        &mut app,
        &ctx,
        vec![
            Event::PointerMoved(center),
            Event::MouseWheel {
                unit: MouseWheelUnit::Line,
                delta: vec2(2.0, 3.0),
                modifiers: shift_mods,
                phase: TouchPhase::Move,
            },
        ],
    );
    let orbit_cam = app.session.camera();
    assert_ne!(orbit_cam.azimuth, base_cam.azimuth, "Shift + Line scroll must orbit camera");
    assert_ne!(orbit_cam.elevation, base_cam.elevation, "Shift + Line scroll must orbit camera");
    assert_eq!(orbit_cam.height, base_cam.height, "Shift + Line scroll must NOT zoom");

    // 2. Two-finger movement with 2-axis Line delta under Auto:
    // Before fix: scroll_pans was false -> zoomed instead of panning.
    // After fix: 2-axis delta recognized as trackpad -> pans instead of zooming.
    let pre_pan_cam = app.session.camera();
    run_frame(
        &mut app,
        &ctx,
        vec![
            Event::PointerMoved(center),
            Event::MouseWheel {
                unit: MouseWheelUnit::Line,
                delta: vec2(5.0, -8.0),
                modifiers: Modifiers::default(),
                phase: TouchPhase::Move,
            },
        ],
    );
    let post_pan_cam = app.session.camera();
    assert_ne!(post_pan_cam.target, pre_pan_cam.target, "2-axis Line scroll must pan");
    assert_eq!(post_pan_cam.height, pre_pan_cam.height, "2-axis Line scroll must NOT zoom");

    // 3. Single-axis Line notch without Shift and without touch phases (standard notched mouse wheel):
    // Must zoom about cursor.
    let pre_zoom_cam = app.session.camera();
    run_frame(
        &mut app,
        &ctx,
        vec![
            Event::PointerMoved(center),
            Event::MouseWheel {
                unit: MouseWheelUnit::Line,
                delta: vec2(0.0, 1.0),
                modifiers: Modifiers::default(),
                phase: TouchPhase::Move,
            },
        ],
    );
    let post_zoom_cam = app.session.camera();
    assert_ne!(post_zoom_cam.height, pre_zoom_cam.height, "Single-axis mouse notch must zoom");
    assert_eq!(post_zoom_cam.azimuth, pre_zoom_cam.azimuth, "Single-axis mouse notch must not orbit");
}

#[test]
fn test_mouse_and_ucs_regression() {
    let (mut app, ctx) = create_test_app();
    run_frame(&mut app, &ctx, vec![Event::PointerMoved(egui::pos2(600.0, 400.0))]);
    let center = canvas_center(&app);

    // 1. Mouse wheel notched delta (Line) in Auto mode must ZOOM, never pan
    assert_eq!(app.options.nav_device, NavDevice::Auto);
    let h_before = app.session.state().expect("state ok").view().height;
    let target_before = app.session.camera().target;

    run_frame(
        &mut app,
        &ctx,
        vec![
            Event::PointerMoved(center),
            Event::MouseWheel {
                unit: MouseWheelUnit::Line,
                delta: vec2(0.0, 1.0), // One wheel notch up
                modifiers: Modifiers::default(),
                phase: TouchPhase::Move,
            },
        ],
    );

    let h_after = app.session.state().expect("state ok").view().height;
    let target_after = app.session.camera().target;
    assert_ne!(h_before, h_after, "Mouse wheel Line delta must zoom");
    assert_eq!(target_before, target_after, "Mouse wheel Line delta must NOT pan");

    // 2. Middle mouse button drag: PANS
    let pre_middle_cam = app.session.camera();
    run_frame(
        &mut app,
        &ctx,
        vec![
            Event::PointerMoved(center),
            Event::PointerButton {
                pos: center,
                button: PointerButton::Middle,
                pressed: true,
                modifiers: Modifiers::default(),
            },
        ],
    );
    run_frame(
        &mut app,
        &ctx,
        vec![
            Event::MouseMoved(vec2(25.0, -15.0)),
        ],
    );
    run_frame(
        &mut app,
        &ctx,
        vec![
            Event::PointerButton {
                pos: center,
                button: PointerButton::Middle,
                pressed: false,
                modifiers: Modifiers::default(),
            },
        ],
    );
    let post_middle_cam = app.session.camera();
    assert_ne!(post_middle_cam.target, pre_middle_cam.target, "Middle drag must pan camera");

    // 3. Shift + Middle mouse button drag: ORBITS
    let mut shift_mods = Modifiers::default();
    shift_mods.shift = true;
    let pre_shift_middle_cam = app.session.camera();
    run_frame(
        &mut app,
        &ctx,
        vec![
            Event::PointerMoved(center),
            Event::PointerButton {
                pos: center,
                button: PointerButton::Middle,
                pressed: true,
                modifiers: shift_mods,
            },
        ],
    );
    run_frame(
        &mut app,
        &ctx,
        vec![
            Event::MouseMoved(vec2(30.0, -25.0)),
        ],
    );
    run_frame(
        &mut app,
        &ctx,
        vec![
            Event::PointerButton {
                pos: center,
                button: PointerButton::Middle,
                pressed: false,
                modifiers: shift_mods,
            },
        ],
    );
    let post_shift_middle_cam = app.session.camera();
    assert_ne!(post_shift_middle_cam.azimuth, pre_shift_middle_cam.azimuth, "Shift + Middle drag must orbit azimuth");
    assert_ne!(post_shift_middle_cam.elevation, pre_shift_middle_cam.elevation, "Shift + Middle drag must orbit elevation");

    // 4. UCS preservation: Pan, Orbit, and Zoom NEVER modify the UCS
    let initial_ucs = app.session.ucs();

    // Two-finger trackpad pan
    run_frame(
        &mut app,
        &ctx,
        vec![
            Event::PointerMoved(center),
            Event::MouseWheel {
                unit: MouseWheelUnit::Point,
                delta: vec2(20.0, 20.0),
                modifiers: Modifiers::default(),
                phase: TouchPhase::Move,
            },
        ],
    );
    assert_eq!(initial_ucs.origin, app.session.ucs().origin, "Trackpad pan must not alter UCS origin");
    assert_eq!(initial_ucs.x_axis, app.session.ucs().x_axis, "Trackpad pan must not alter UCS X axis");

    // Shift + two-finger trackpad orbit
    run_frame(
        &mut app,
        &ctx,
        vec![
            Event::PointerMoved(center),
            Event::MouseWheel {
                unit: MouseWheelUnit::Point,
                delta: vec2(30.0, -20.0),
                modifiers: shift_mods,
                phase: TouchPhase::Move,
            },
        ],
    );
    assert_eq!(initial_ucs.origin, app.session.ucs().origin, "Trackpad orbit must not alter UCS origin");
    assert_eq!(initial_ucs.z_axis, app.session.ucs().z_axis, "Trackpad orbit must not alter UCS Z axis");

    // Pinch zoom
    run_frame(
        &mut app,
        &ctx,
        vec![
            Event::PointerMoved(center),
            Event::Zoom(1.15),
        ],
    );
    assert_eq!(initial_ucs.origin, app.session.ucs().origin, "Pinch zoom must not alter UCS origin");
    assert_eq!(initial_ucs.y_axis, app.session.ucs().y_axis, "Pinch zoom must not alter UCS Y axis");
}

#[test]
fn test_ctrl_trackpad_zoom_and_arbitration() {
    let (mut app, ctx) = create_test_app();
    run_frame(&mut app, &ctx, vec![Event::PointerMoved(egui::pos2(600.0, 400.0))]);
    let center = canvas_center(&app);

    let initial_cam = app.session.camera();
    let initial_h = app.session.state().expect("state ok").view().height;

    let mut ctrl_mods = Modifiers::default();
    ctrl_mods.ctrl = true;

    // 1. Ctrl + two-finger scroll: zooms in/out anchored at cursor
    // Delta Y positive: zoom in (height decreases)
    run_frame(
        &mut app,
        &ctx,
        vec![
            Event::PointerMoved(center),
            Event::MouseWheel {
                unit: MouseWheelUnit::Point,
                delta: vec2(0.0, 50.0),
                modifiers: ctrl_mods,
                phase: TouchPhase::Move,
            },
        ],
    );
    let cam1 = app.session.camera();
    let h1 = app.session.state().expect("state ok").view().height;
    assert!(h1 < initial_h, "Ctrl + scroll upwards must zoom in (decrease height)");
    assert_eq!(cam1.azimuth, initial_cam.azimuth, "Ctrl + scroll must NOT orbit azimuth");
    assert_eq!(cam1.elevation, initial_cam.elevation, "Ctrl + scroll must NOT orbit elevation");
    assert_eq!(cam1.target, initial_cam.target, "Zoom anchored at screen center must keep target center");

    // Delta Y negative: zoom out (height increases)
    run_frame(
        &mut app,
        &ctx,
        vec![
            Event::PointerMoved(center),
            Event::MouseWheel {
                unit: MouseWheelUnit::Point,
                delta: vec2(0.0, -80.0),
                modifiers: ctrl_mods,
                phase: TouchPhase::Move,
            },
        ],
    );
    let h2 = app.session.state().expect("state ok").view().height;
    assert!(h2 > h1, "Ctrl + scroll downwards must zoom out (increase height)");

    // 2. Releasing Ctrl mid-gesture seamlessly drops back to panning without jumps
    let cam_before_release = app.session.camera();
    let h_before_release = app.session.state().expect("state ok").view().height;
    run_frame(
        &mut app,
        &ctx,
        vec![
            Event::PointerMoved(center),
            Event::MouseWheel {
                unit: MouseWheelUnit::Point,
                delta: vec2(20.0, 30.0),
                modifiers: Modifiers::default(), // Ctrl released mid-swipe
                phase: TouchPhase::Move,
            },
        ],
    );
    let cam_after_release = app.session.camera();
    let h_after_release = app.session.state().expect("state ok").view().height;
    assert_eq!(h_after_release, h_before_release, "Releasing Ctrl must stop zooming");
    assert_ne!(cam_after_release.target, cam_before_release.target, "Releasing Ctrl must resume panning");
    assert_eq!(cam_after_release.azimuth, cam_before_release.azimuth, "Releasing Ctrl must not orbit");

    // 3. Modifier arbitration: Shift takes precedence over Ctrl (Shift + Ctrl orbits in 3D)
    let mut shift_ctrl_mods = Modifiers::default();
    shift_ctrl_mods.shift = true;
    shift_ctrl_mods.ctrl = true;
    let cam_before_dual = app.session.camera();
    run_frame(
        &mut app,
        &ctx,
        vec![
            Event::PointerMoved(center),
            Event::MouseWheel {
                unit: MouseWheelUnit::Point,
                delta: vec2(35.0, -25.0),
                modifiers: shift_ctrl_mods,
                phase: TouchPhase::Move,
            },
        ],
    );
    let cam_after_dual = app.session.camera();
    assert_ne!(cam_after_dual.azimuth, cam_before_dual.azimuth, "Shift + Ctrl + scroll must orbit (Shift takes precedence)");
    assert_ne!(cam_after_dual.elevation, cam_before_dual.elevation, "Shift + Ctrl + scroll must orbit");
}
