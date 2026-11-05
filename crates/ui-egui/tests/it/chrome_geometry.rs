//! The real app's painted geometry at native and compact window sizes.

use cadcraft_doc::{Common, Drawing, EntityKind, Layout, Space, Viewport};
use cadcraft_engine::Session;
use cadcraft_geom::{Vec2, Vec3};
use cadcraft_ui_egui::{CadApp, Services};
use egui::{Event, Modifiers, MouseWheelUnit, PointerButton, Pos2, Rect, Shape, TouchPhase, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

const TEMPLATE_LAYOUTS: [&str; 8] = [
    "Formato A4 vertical",
    "Formato A4 horizontal",
    "Formato A3",
    "Formato A2",
    "Lista de piezas A4",
    "Lista de piezas A3",
    "Lista de piezas A2",
    "Lista de piezas separada",
];

fn harness(width: f32, height: f32, scale: f32, message: bool) -> Harness<'static, CadApp> {
    let mut app = CadApp::new(Session::default(), Services::default());
    app.run("new", serde_json::json!({})).unwrap();
    app.ui.start_tab = false;
    app.ui.in_window_menu = false;
    app.integrated_titlebar = true;
    if message {
        app.set_status("Drawing saved successfully with a long status message");
    }
    let mut h = Harness::builder().with_size(vec2(width, height)).with_pixels_per_point(scale).build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(&ui.ctx().clone());
            app.canvas.cursor = Some(Vec2::new(1234.5, -9876.25));
            app.canvas.cursor3 = Some(Vec3::new(1234.5, -9876.25, 0.0));
            app.ui(ui);
        },
        app,
    );
    h.run_steps(5);
    h
}

fn template_harness(width: f32, height: f32) -> Harness<'static, CadApp> {
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
    session.open_drawing(d, "Plantilla A4", None);
    session.execute("layout.set", &json!({ "name": "Formato A4 vertical" })).unwrap();
    let vp_handle = session
        .doc()
        .unwrap()
        .space(&Space::Paper("Formato A4 vertical".into()))
        .and_then(|s| s.iter().next().map(|e| e.handle))
        .unwrap();
    session.state_mut().unwrap().selection.push(vp_handle);

    let mut app = CadApp::new(session, Services::default());
    app.ui.start_tab = false;
    app.ui.in_window_menu = false;
    app.integrated_titlebar = true;

    let mut h = Harness::builder().with_size(vec2(width, height)).build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(&ui.ctx().clone());
            app.canvas.cursor = Some(Vec2::new(285.44, 10.32));
            app.canvas.cursor3 = Some(Vec3::new(285.44, 10.32, 0.0));
            app.ui(ui);
        },
        app,
    );
    h.run_steps(4);
    h
}

fn visible_text(shape: &Shape, clip: Rect, result: &mut Vec<(String, Rect)>) {
    match shape {
        Shape::Text(text) => {
            let rect = text.visual_bounding_rect().intersect(clip);
            if rect.is_positive() {
                result.push((text.galley.text().to_string(), rect));
            }
        }
        Shape::Vec(shapes) => {
            for shape in shapes {
                visible_text(shape, clip, result);
            }
        }
        _ => {}
    }
}

fn status_bar_texts(h: &Harness<'_, CadApp>, height: f32) -> Vec<(String, Rect)> {
    let mut texts = Vec::new();
    for shape in &h.output().shapes {
        visible_text(&shape.shape, shape.clip_rect, &mut texts);
    }
    texts.retain(|(_, rect)| rect.center().y > height - 26.0);
    texts
}

#[test]
fn status_labels_never_overlap_at_standard_and_compact_sizes() {
    let mut overlaps = Vec::new();
    for (width, height) in [(1200.0, 760.0), (900.0, 560.0)] {
        for scale in [1.0, 2.0] {
            for message in [false, true] {
                let mut h = harness(width, height, scale, message);
                if let Ok(directory) = std::env::var("CRAFT_UI_VISUAL_DIR") {
                    std::fs::create_dir_all(&directory).unwrap();
                    h.render().unwrap().save(format!("{directory}/cad-{width}-{scale}-{message}.png")).unwrap();
                }
                let texts = status_bar_texts(&h, height);
                assert!(texts.iter().any(|(text, _)| text == "Model"));
                if width >= 1200.0 {
                    assert!(texts.iter().any(|(text, _)| text == "Layout2"));
                }
                for (i, (first, first_rect)) in texts.iter().enumerate() {
                    for (second, second_rect) in texts.iter().skip(i + 1) {
                        if first_rect.intersects(*second_rect) {
                            overlaps.push(format!("{width}px @{scale}: {first:?} {first_rect:?} overlaps {second:?} {second_rect:?}"));
                        }
                    }
                }
            }
        }
    }
    assert!(overlaps.is_empty(), "{}", overlaps.join("\n"));
}

#[test]
fn template_layouts_paper_space_controls_and_readout_never_overlap() {
    for width in [1600.0, 1200.0, 900.0, 620.0] {
        let height = 760.0;
        let h = template_harness(width, height);
        let sb = &h.state().status_bar;
        let strip = sb.tab_strip_rect.expect("layout tab strip rect");
        assert!(
            strip.right() <= sb.controls_left + 1e-3,
            "at {width}px: tab strip right ({}) must not cross controls_left ({})",
            strip.right(),
            sb.controls_left
        );
        let texts = status_bar_texts(&h, height);
        assert!(texts.iter().any(|(text, _)| text == "Model"), "Model tab must stay visible at {width}px");
        assert!(texts.iter().any(|(text, _)| text == "Formato A4 vertical"), "active tab must be visible at {width}px");
        for (i, (first, first_rect)) in texts.iter().enumerate() {
            for (second, second_rect) in texts.iter().skip(i + 1) {
                assert!(
                    !first_rect.intersects(*second_rect),
                    "at {width}px: status bar text {first:?} ({first_rect:?}) overlaps {second:?} ({second_rect:?})"
                );
            }
        }
        if width < 700.0 {
            // Ultra-narrow window uses the status overflow control rather than overlapping buttons.
            let _ = h.get_by_label("More status controls");
        }
    }

    // Verify all right-hand controls in Paper space are individually clickable with 8 long layouts loaded.
    let mut h = template_harness(1360.0, 760.0);
    assert_eq!(h.state().session.drawing_plane(), cadcraft_geom::DrawingPlane::XY);
    h.get_by_label("Plane: XY").click();
    h.run_steps(2);
    assert_eq!(h.state().session.drawing_plane(), cadcraft_geom::DrawingPlane::XZ);

    assert!(!h.state().session.settings.zlock);
    h.get_by_label("V-LOCK").click();
    h.run_steps(2);
    assert!(h.state().session.settings.zlock);

    // Toggle viewport lock via its status bar button.
    let locked_before = h
        .state()
        .session
        .doc()
        .unwrap()
        .space(&Space::Paper("Formato A4 vertical".into()))
        .unwrap()
        .iter()
        .find_map(|e| match &e.kind {
            EntityKind::Viewport(vp) => Some(vp.locked),
            _ => None,
        })
        .unwrap();
    assert!(locked_before);
    h.get_by_label("Lock viewport").click();
    h.run_steps(2);
    let locked_after = h
        .state()
        .session
        .doc()
        .unwrap()
        .space(&Space::Paper("Formato A4 vertical".into()))
        .unwrap()
        .iter()
        .find_map(|e| match &e.kind {
            EntityKind::Viewport(vp) => Some(vp.locked),
            _ => None,
        })
        .unwrap();
    assert!(!locked_after);

    // Click PAPER button to enter MSPACE without colliding with Plane: XZ.
    assert!(h.state().session.state().unwrap().mspace.is_none());
    h.get_by_label("PAPER").click();
    h.run_steps(2);
    assert!(h.state().session.state().unwrap().mspace.is_some());
    assert_eq!(h.state().session.drawing_plane(), cadcraft_geom::DrawingPlane::XZ);
}

#[test]
fn layout_tabs_wheel_scrolls_horizontally_without_canvas_zoom_or_tab_activation() {
    let height = 760.0;
    let mut h = template_harness(1280.0, height);
    let strip = h.state().status_bar.tab_strip_rect.unwrap();
    let max_scroll = h.state().status_bar.max_tab_scroll;
    assert!(max_scroll > 200.0, "8 template layouts must overflow the strip at 1280px (max_scroll={max_scroll})");
    assert_eq!(h.state().status_bar.tab_scroll, 0.0);

    // Last tab is initially clipped out of view.
    let initial_texts = status_bar_texts(&h, height);
    assert!(!initial_texts.iter().any(|(t, _)| t == "Lista de piezas separada"));

    let initial_view = h.state().session.state().unwrap().paper_view();
    let hover_pt: Pos2 = strip.center();

    // Scroll down with normal vertical mouse wheel (no Shift) over the layout tab strip.
    for _ in 0..20 {
        h.input_mut().events.push(Event::PointerMoved(hover_pt));
        h.input_mut().events.push(Event::MouseWheel {
            unit: MouseWheelUnit::Line,
            delta: vec2(0.0, -1.5),
            modifiers: Modifiers::NONE,
            phase: TouchPhase::Move,
        });
        h.step();
    }

    // Scroll clamped at max_scroll; active layout and canvas view are untouched.
    assert!((h.state().status_bar.tab_scroll - max_scroll).abs() < 1e-3);
    assert_eq!(h.state().session.layout_space(), Space::Paper("Formato A4 vertical".into()));
    let view_after_wheel = h.state().session.state().unwrap().paper_view();
    assert_eq!(view_after_wheel.center, initial_view.center);
    assert_eq!(view_after_wheel.height, initial_view.height);

    // The last tab ("Lista de piezas separada") is now visible inside the strip and clickable.
    let end_texts = status_bar_texts(&h, height);
    let (_, last_rect) = end_texts
        .iter()
        .find(|(t, _)| t == "Lista de piezas separada")
        .expect("last layout tab must be visible after scrolling to the right end");
    assert!(strip.contains(last_rect.center()));

    // Click the last tab at its visible center to activate it.
    let click_pt = pos2(last_rect.center().x, strip.center().y);
    h.input_mut().events.push(Event::PointerMoved(click_pt));
    h.input_mut().events.push(Event::PointerButton {
        pos: click_pt,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.step();
    h.input_mut().events.push(Event::PointerButton {
        pos: click_pt,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(2);
    assert_eq!(h.state().session.layout_space(), Space::Paper("Lista de piezas separada".into()));
    let max_scroll_after_switch = h.state().status_bar.max_tab_scroll;
    assert!((h.state().status_bar.tab_scroll - max_scroll_after_switch).abs() < 1e-3);

    // Touchpad horizontal scroll and vertical wheel up scroll back to 0.0 and clamp at 0.0.
    h.input_mut().events.push(Event::PointerMoved(hover_pt));
    h.input_mut().events.push(Event::MouseWheel {
        unit: MouseWheelUnit::Point,
        delta: vec2(60.0, 0.0),
        modifiers: Modifiers::NONE,
        phase: TouchPhase::Move,
    });
    h.step();
    assert!((h.state().status_bar.tab_scroll - (max_scroll_after_switch - 60.0)).abs() < 1e-3);

    for _ in 0..25 {
        h.input_mut().events.push(Event::PointerMoved(hover_pt));
        h.input_mut().events.push(Event::MouseWheel {
            unit: MouseWheelUnit::Line,
            delta: vec2(0.0, 2.0),
            modifiers: Modifiers::NONE,
            phase: TouchPhase::Move,
        });
        h.step();
    }
    assert_eq!(h.state().status_bar.tab_scroll, 0.0);
    // Active layout is still "Lista de piezas separada" even though we scrolled back to explore the first tabs!
    assert_eq!(h.state().session.layout_space(), Space::Paper("Lista de piezas separada".into()));
    let start_texts = status_bar_texts(&h, height);
    assert!(start_texts.iter().any(|(t, _)| t == "Formato A4 vertical"));

    // Model remains pinned and clickable at all times.
    h.get_by_label("Model").click();
    h.run_steps(2);
    assert_eq!(h.state().session.layout_space(), Space::Model);
}

#[test]
fn activating_layout_reveals_minimally_and_window_resize_recomputes_bounds() {
    let mut h = template_harness(1200.0, 760.0);
    assert_eq!(h.state().status_bar.tab_scroll, 0.0);

    // Activating a layout further right via command reveals it with the minimum scroll delta.
    h.state_mut().run("layout.set", json!({ "name": "Lista de piezas A4" })).unwrap();
    h.run_steps(2);
    let scroll_for_a4_list = h.state().status_bar.tab_scroll;
    let max_scroll_1200 = h.state().status_bar.max_tab_scroll;
    assert!(scroll_for_a4_list > 0.0 && scroll_for_a4_list < max_scroll_1200);
    let texts = status_bar_texts(&h, 760.0);
    assert!(texts.iter().any(|(t, _)| t == "Lista de piezas A4"));

    // Manual wheel scrolling from there is preserved across subsequent frames without snapping back.
    let hover_pt = h.state().status_bar.tab_strip_rect.unwrap().center();
    h.input_mut().events.push(Event::PointerMoved(hover_pt));
    h.input_mut().events.push(Event::MouseWheel {
        unit: MouseWheelUnit::Point,
        delta: vec2(0.0, 80.0),
        modifiers: Modifiers::NONE,
        phase: TouchPhase::Move,
    });
    h.step();
    let manual_scroll = h.state().status_bar.tab_scroll;
    assert!((manual_scroll - (scroll_for_a4_list - 80.0)).abs() < 1e-3);
    h.run_steps(5);
    assert_eq!(h.state().status_bar.tab_scroll, manual_scroll);

    // Resizing the window wider increases the strip width and lowers max_tab_scroll dynamically.
    h.set_size(vec2(1700.0, 760.0));
    h.run_steps(3);
    let strip_1700 = h.state().status_bar.tab_strip_rect.unwrap();
    let max_scroll_1700 = h.state().status_bar.max_tab_scroll;
    assert!(max_scroll_1700 < max_scroll_1200);
    assert!(h.state().status_bar.tab_scroll <= max_scroll_1700);
    assert!(strip_1700.right() <= h.state().status_bar.controls_left + 1e-3);
}

#[test]
fn quick_access_toolbar_actions_work() {
    let mut h = harness(900.0, 560.0, 1.0, false);
    let mac = h.ctx.os().is_mac();
    let before = h.state().session.docs.len();
    let new_label = format!("New Drawing ({})", if mac { "⌘N" } else { "Ctrl+N" });
    let print_label = format!("Print ({})", if mac { "⌘P" } else { "Ctrl+P" });
    h.get_by_label(&new_label).click();
    h.run_steps(3);
    assert_eq!(h.state().session.docs.len(), before + 1);

    h.get_by_label(&print_label).click();
    h.run_steps(3);
    assert_eq!(h.state().ui.dialog.as_deref(), Some("plot"));
}

#[test]
fn quick_access_toolbar_undo_redo_reachable() {
    let mut h = harness(1200.0, 760.0, 1.0, false);
    let mac = h.ctx.os().is_mac();
    let undo_label = format!("Undo ({})", if mac { "⌘Z" } else { "Ctrl+Z" });
    let redo_label = format!("Redo ({})", if mac { "⇧⌘Z" } else { "Ctrl+Shift+Z" });
    h.get_by_label(&undo_label).click();
    h.run_steps(2);
    h.get_by_label(&redo_label).click();
    h.run_steps(2);
}

#[test]
fn locked_viewport_navigation_preserves_scale_framing_and_smooth_interaction() {
    let mut h = template_harness(1400.0, 800.0);
    // Find viewport in "Formato A4 vertical"
    let vp_handle = h
        .state()
        .session
        .doc()
        .unwrap()
        .space(&Space::Paper("Formato A4 vertical".into()))
        .and_then(|s| s.iter().next().map(|e| e.handle))
        .unwrap();

    // 1. Enter MSPACE
    h.state_mut().run("mspace", json!({ "handle": vp_handle.hex() })).unwrap();
    // 2. Set scale to 1:50 and framing (unlocking first since template viewport was initially locked)
    h.state_mut().run("viewport.set", json!({ "handle": vp_handle.hex(), "locked": false, "scale": 0.02, "center": [400.0, 200.0] })).unwrap();
    // 3. Lock viewport via status bar button
    h.run_steps(2);
    h.get_by_label("Lock viewport").click();
    h.run_steps(2);

    let doc = h.state().session.doc().unwrap();
    let vp_e = doc.entity(vp_handle).unwrap();
    let EntityKind::Viewport(vp_locked) = &vp_e.kind else { panic!("not viewport") };
    assert!(vp_locked.locked);
    assert_eq!(vp_locked.view_center, Vec2::new(400.0, 200.0));
    assert!((vp_locked.height / vp_locked.view_height - 0.02).abs() < 1e-6);

    let paper_view_before = h.state().session.state().unwrap().paper_view();

    // 4. Middle mouse drag over canvas to pan paper sheet (20 successive frames)
    let canvas_center = h.state().canvas.rect.unwrap().center();
    for step in 0..20 {
        let p_start = canvas_center + vec2(step as f32 * 2.0, 0.0);
        let p_end = p_start + vec2(15.0, -10.0);

        h.input_mut().events.push(Event::PointerMoved(p_start));
        h.input_mut().events.push(Event::PointerButton {
            pos: p_start,
            button: PointerButton::Middle,
            pressed: true,
            modifiers: Modifiers::NONE,
        });
        h.step();
        h.input_mut().events.push(Event::PointerMoved(p_end));
        h.step();
        h.input_mut().events.push(Event::PointerButton {
            pos: p_end,
            button: PointerButton::Middle,
            pressed: false,
            modifiers: Modifiers::NONE,
        });
        h.step();

        let paper_now = h.state().session.state().unwrap().paper_view();
        assert!(
            (paper_now.height - paper_view_before.height).abs() < 1e-4,
            "step {step}: paper view height ran away: before {}, now {}",
            paper_view_before.height,
            paper_now.height
        );
        assert!(paper_now.center.is_finite());
        assert!(paper_now.height.is_finite());

        let doc = h.state().session.doc().unwrap();
        let EntityKind::Viewport(vp) = &doc.entity(vp_handle).unwrap().kind else { panic!() };
        assert_eq!(vp.view_center, Vec2::new(400.0, 200.0));
        assert!((vp.height / vp.view_height - 0.02).abs() < 1e-6);
    }

    // 5. Wheel scrolling over layout tabs does not affect canvas view or viewport
    let tab_strip = h.state().status_bar.tab_strip_rect.unwrap();
    let tab_hover = tab_strip.center();
    let paper_before_wheel = h.state().session.state().unwrap().paper_view();
    h.input_mut().events.push(Event::PointerMoved(tab_hover));
    h.input_mut().events.push(Event::MouseWheel {
        unit: MouseWheelUnit::Line,
        delta: vec2(0.0, -5.0),
        modifiers: Modifiers::NONE,
        phase: TouchPhase::Move,
    });
    h.step();
    let paper_after_wheel = h.state().session.state().unwrap().paper_view();
    assert_eq!(paper_before_wheel, paper_after_wheel);

    // 6. Switch to another layout and return
    h.state_mut().run("layout.set", json!({ "name": "Formato A3" })).unwrap();
    h.run_steps(2);
    assert_eq!(h.state().session.layout_space(), Space::Paper("Formato A3".into()));

    h.state_mut().run("layout.set", json!({ "name": "Formato A4 vertical" })).unwrap();
    h.run_steps(2);
    assert_eq!(h.state().session.layout_space(), Space::Paper("Formato A4 vertical".into()));
    let doc = h.state().session.doc().unwrap();
    let EntityKind::Viewport(vp_final) = &doc.entity(vp_handle).unwrap().kind else { panic!() };
    assert!(vp_final.locked);
    assert_eq!(vp_final.view_center, Vec2::new(400.0, 200.0));
    assert!((vp_final.height / vp_final.view_height - 0.02).abs() < 1e-6);
}
