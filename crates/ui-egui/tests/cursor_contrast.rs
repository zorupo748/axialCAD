//! The thin crosshair and pickbox follow the background under the pointer, including in MSPACE.

use cadcraft_engine::Session;
use cadcraft_engine::doc::{Common, EntityKind, Space, Viewport};
use cadcraft_engine::geom::{Vec2, Vec3};
use cadcraft_ui_egui::{CadApp, Services, theme};
use egui::{Color32, Context, Pos2, Rect, Shape, Stroke, pos2, vec2};
use serde_json::json;

const DARK_INK: Color32 = Color32::from_rgb(0x20, 0x20, 0x20);
const LIGHT_INK: Color32 = Color32::from_rgb(0xe8, 0xe8, 0xe8);

struct Canvas {
    app: CadApp,
    ctx: Context,
    previous_theme: egui::Theme,
}

impl Canvas {
    fn new(paper: bool) -> Self {
        let previous_theme = theme::active();
        let mut session = Session::new();
        session.new_drawing(true);
        if paper {
            session.execute("layout.set", &json!({"name": "Layout1"})).unwrap();
        }
        session.execute("view.set", &json!({"center": [100, 100], "height": 400})).unwrap();
        session.settings.gridmode = false;
        session.settings.dynmode = false;
        let mut app = CadApp::new(session, Services::default());
        app.canvas.zoom_pending = false;
        app.ui.show_command_line = false;
        app.ui.show_ucs_icon = false;
        app.ui.show_viewcube = false;
        app.run("options", json!({"theme": "dark", "systemCursor": false, "PICKBOX": 4, "SELECTIONPREVIEW": 1})).unwrap();
        let mut canvas = Self { app, ctx: Context::default(), previous_theme };
        canvas.paint(pos2(640.0, 400.0));
        canvas
    }

    fn screen(&self, point: Vec2) -> Pos2 {
        self.app.canvas.xf.unwrap().to_screen(point)
    }

    fn paint(&mut self, at: Pos2) -> Vec<egui::epaint::ClippedShape> {
        let mut shapes = Vec::new();
        // Let layout and hover state settle, as in system_cursor.rs.
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 800.0))),
                events: vec![egui::Event::PointerMoved(at)],
                ..Default::default()
            };
            let mut output = self.ctx.run_ui(input, |ui| {
                self.app.logic(ui.ctx());
                self.app.ui(ui);
            });
            shapes = std::mem::take(&mut output.shapes);
            output.textures_delta.clear();
        }
        shapes
    }

    fn assert_ink(&mut self, at: Pos2, expected: Color32, label: &str) {
        let shapes = self.paint(at);
        assert!(self.app.canvas.rect.unwrap().contains(at), "{label}: pointer must be on the canvas");
        let mut arms = Vec::new();
        let mut boxes = Vec::new();
        for shape in &shapes {
            cursor_strokes(&shape.shape, at, &mut arms, &mut boxes);
        }
        assert_eq!(arms.len(), 4, "{label}: four cursor arms must be painted");
        assert_eq!(boxes.len(), 1, "{label}: the pickbox must be painted");
        for stroke in arms.iter().chain(&boxes) {
            assert_eq!(stroke.width, 1.0, "{label}: keep the thin cursor");
            assert_eq!(stroke.color, expected, "{label}: cursor ink");
        }
    }
}

impl Drop for Canvas {
    fn drop(&mut self) {
        theme::set_canvas(theme::Canvas::default());
        theme::set_active(self.previous_theme);
    }
}

/// Identify the actual cursor shapes by their position, independently of their colour.
/// The drawings are empty around the pointer, with grids and UCS icons hidden.
fn cursor_strokes(shape: &Shape, at: Pos2, arms: &mut Vec<Stroke>, boxes: &mut Vec<Stroke>) {
    match shape {
        Shape::LineSegment { points: [a, b], stroke } => {
            let direction = *b - *a;
            let from_pointer = *a - at;
            let cross = direction.x * from_pointer.y - direction.y * from_pointer.x;
            if direction.length() > 1.0 && cross.abs() / direction.length() < 0.01 && a.distance(at).min(b.distance(at)) < 16.0 {
                arms.push(*stroke);
            }
        }
        Shape::Rect(rect) if rect.rect.center().distance(at) < 0.01 && rect.rect.size() == vec2(12.0, 12.0) => {
            boxes.push(rect.stroke);
        }
        Shape::Vec(shapes) => shapes.iter().for_each(|s| cursor_strokes(s, at, arms, boxes)),
        _ => {}
    }
}

#[test]
fn model_cursor_keeps_following_live_background_preferences() {
    let mut c = Canvas::new(false);
    let at = c.screen(Vec2::new(100.0, 100.0));
    c.assert_ink(at, LIGHT_INK, "default model");
    c.app.run("options", json!({"modelBackground": "#ffffff"})).unwrap();
    c.assert_ink(at, DARK_INK, "white model");
    c.app.run("options", json!({"modelBackground": null, "theme": "light"})).unwrap();
    c.assert_ink(at, LIGHT_INK, "reset model with light interface");
}

#[test]
fn paper_and_surround_follow_the_sheet_after_pan_and_zoom() {
    let mut c = Canvas::new(true);
    let at = c.screen(Vec2::new(30.0, 40.0));
    c.assert_ink(at, DARK_INK, "white paper");
    let outside = c.screen(Vec2::new(-20.0, 40.0));
    c.assert_ink(outside, LIGHT_INK, "gray surround");

    // Keep the pointer still while navigation moves the sheet out from underneath it.
    c.app.run("view.set", json!({"center": [500, 100], "height": 400})).unwrap();
    c.assert_ink(at, LIGHT_INK, "paper panned away");
    c.app.run("view.set", json!({"center": [100, 100], "height": 1000})).unwrap();
    c.assert_ink(at, LIGHT_INK, "paper zoomed away");
    c.app.run("view.set", json!({"center": [100, 100], "height": 400})).unwrap();
    c.assert_ink(at, DARK_INK, "paper returned");

    // One colour follows the centre even when long arms cross the sheet's boundary.
    c.app.run("options", json!({"CURSORSIZE": 100})).unwrap();
    let edge = c.screen(Vec2::new(0.0, 40.0));
    c.assert_ink(edge + vec2(2.0, 0.0), DARK_INK, "just inside paper");
    c.assert_ink(edge - vec2(2.0, 0.0), LIGHT_INK, "just outside paper");
}

#[test]
fn paper_preferences_are_independent_of_model_preferences() {
    let mut c = Canvas::new(true);
    let at = c.screen(Vec2::new(30.0, 40.0));
    c.app.run("options", json!({"modelBackground": "#ffffff", "paperBackground": "#202020"})).unwrap();
    c.assert_ink(at, LIGHT_INK, "dark paper with white model preference");
    c.assert_ink(c.screen(Vec2::new(-20.0, 40.0)), LIGHT_INK, "surround with white model preference");
    c.app.run("options", json!({"paperBackground": "#ffffff", "modelBackground": "#202020", "theme": "light"})).unwrap();
    c.assert_ink(at, DARK_INK, "white paper with dark model preference and light interface");
    c.app.run("layout.set", json!({"name": "Model"})).unwrap();
    c.assert_ink(at, LIGHT_INK, "switch to dark model");
    c.app.run("layout.set", json!({"name": "Layout1"})).unwrap();
    c.app.run("view.set", json!({"center": [100, 100], "height": 400})).unwrap();
    c.assert_ink(at, DARK_INK, "switch back to white paper");
}

#[test]
fn mspace_and_isometric_cursor_use_the_paper_display_transform() {
    let mut c = Canvas::new(true);
    c.app
        .session
        .doc_mut()
        .unwrap()
        .add(
            &Space::Paper("Layout1".into()),
            Common::default(),
            EntityKind::Viewport(Viewport {
                center: Vec3::new(70.0, 60.0, 0.0),
                width: 100.0,
                height: 80.0,
                view_center: Vec2::new(10000.0, 20000.0),
                view_height: 1000.0,
                id: 2,
                on: true,
                ..Default::default()
            }),
        )
        .unwrap();
    let at = c.screen(Vec2::new(70.0, 60.0));
    let outside = c.screen(Vec2::new(-20.0, 60.0));
    c.app.run("mspace", json!({})).unwrap();
    assert!(c.app.session.state().unwrap().mspace.is_some());
    c.app.session.settings.isodraft = true;
    c.app.run("options", json!({"CURSORSIZE": 100})).unwrap();
    c.assert_ink(at, DARK_INK, "MSPACE on paper");
    c.assert_ink(outside, LIGHT_INK, "MSPACE outside paper");
    c.app.run("view.set", json!({"center": [30000, -40000], "height": 4000})).unwrap();
    c.assert_ink(at, DARK_INK, "MSPACE after model pan and zoom");
    c.app.run("pspace", json!({})).unwrap();
    c.assert_ink(at, DARK_INK, "back in paper space");
}

#[test]
fn object_selection_shows_pickbox_without_crosshair_and_transitions_cleanly() {
    let mut c = Canvas::new(false);
    let at = c.screen(Vec2::new(100.0, 100.0));

    let count_cursor = |c: &mut Canvas| -> (usize, usize) {
        let shapes = c.paint(at);
        let mut arms = Vec::new();
        let mut boxes = Vec::new();
        for shape in &shapes {
            cursor_strokes(&shape.shape, at, &mut arms, &mut boxes);
        }
        (arms.len(), boxes.len())
    };

    // Idle with PICKFIRST=1: both crosshair and pickbox
    assert_eq!(count_cursor(&mut c), (4, 1), "idle with PICKFIRST shows crosshair and pickbox");

    // TRIM object pick stage: only pickbox, no crosshair arms
    c.app.cmdline("TRIM");
    assert_eq!(count_cursor(&mut c), (0, 1), "TRIM pick stage shows small pickbox without crosshair");

    // Transition inside TRIM to Fence (point input): crosshair without pickbox
    c.app.cmdline("F");
    assert_eq!(count_cursor(&mut c), (4, 0), "TRIM Fence point prompt restores crosshair without pickbox");

    // Finish Fence (Enter) back to TRIM pick stage: pickbox without crosshair
    c.app.cmdline("");
    assert_eq!(count_cursor(&mut c), (0, 1), "returning to TRIM pick restores pickbox without crosshair");

    // Cancel back to idle
    c.app.session.cancel();
    assert_eq!(count_cursor(&mut c), (4, 1), "idle restores crosshair and pickbox");

    // Generic Accept::SELECT command (ERASE): pickbox without crosshair
    c.app.cmdline("ERASE");
    assert_eq!(count_cursor(&mut c), (0, 1), "ERASE selection shows pickbox without crosshair");
    c.app.session.cancel();

    // Point input command (LINE): crosshair without pickbox
    c.app.cmdline("LINE");
    assert_eq!(count_cursor(&mut c), (4, 0), "LINE point prompt shows crosshair without pickbox");
    c.app.session.cancel();

    // Multi-stage command (MOVE): object selection (pickbox only) -> base point (crosshair only) -> second point (crosshair only)
    c.app.session.cmdline("LINE 80,80 120,80").unwrap();
    c.app.session.cmdline("").unwrap();
    let line_h = c.app.session.doc().unwrap().model.last().unwrap().handle;
    c.app.cmdline("MOVE");
    assert_eq!(count_cursor(&mut c), (0, 1), "MOVE object selection shows pickbox without crosshair");
    c.app.session.input(cadcraft_engine::Input::Pick(vec![line_h])).unwrap();
    c.app.cmdline("");
    assert_eq!(count_cursor(&mut c), (4, 0), "MOVE base point prompt restores crosshair without pickbox");
    c.app.cmdline("0,0");
    let (arms_second, boxes_second) = count_cursor(&mut c);
    assert!(arms_second >= 4 && boxes_second == 0, "MOVE second point prompt keeps crosshair without pickbox");
    c.app.session.cancel();
}

#[test]
fn trim_hover_paints_dashed_mask_and_suppresses_whole_entity_hover() {
    let mut c = Canvas::new(false);
    c.app.session.cmdline("CIRCLE 100,100 40").unwrap();
    c.app.session.cmdline("LINE 40,100 160,100").unwrap();
    c.app.session.cmdline("").unwrap();
    // Ensure DASHED linetype is present in the drawing to verify preview tessellation stays unbroken before screen-space dashing.
    let dashed_lt = cadcraft_engine::doc::library::standard_linetypes()
        .into_iter()
        .find(|l| l.name == "DASHED")
        .unwrap();
    c.app.session.doc_mut().unwrap().linetypes.push(dashed_lt);

    c.app.cmdline("TRIM");
    let top_half = c.screen(Vec2::new(100.0, 140.0));
    let center_screen = c.screen(Vec2::new(100.0, 100.0));
    let shapes = c.paint(top_half);

    // Whole-entity rollover hover must be suppressed
    assert_eq!(c.app.canvas.hover, None, "generic whole-entity hover must be suppressed during TRIM pick");

    // Cursor must be small pickbox only
    let mut arms = Vec::new();
    let mut boxes = Vec::new();
    for shape in &shapes {
        cursor_strokes(&shape.shape, top_half, &mut arms, &mut boxes);
    }
    assert_eq!(arms.len(), 0, "no crosshair arms during TRIM pick");
    assert_eq!(boxes.len(), 1, "pickbox shown during TRIM pick");

    // Check painted mask and dashed foreground strokes strictly in the top semicircle (y < center_screen.y)
    let bg_col = theme::Tokens::DARK.canvas;
    let mut mask_strokes = 0usize;
    let mut dashed_segments_top = 0usize;
    let mut dashed_segments_bottom = 0usize;

    fn inspect(shape: &Shape, bg_col: Color32, center_y: f32, mask_strokes: &mut usize, top: &mut usize, bot: &mut usize) {
        match shape {
            Shape::Path(p)
                if (p.stroke.width - 4.5).abs() < 0.01
                    && p.stroke.color == egui::epaint::ColorMode::Solid(bg_col) =>
            {
                *mask_strokes += 1;
            }
            Shape::LineSegment { points: [a, b], stroke } if (stroke.width - 2.0).abs() < 0.01 && stroke.color == LIGHT_INK => {
                let mid_y = 0.5 * (a.y + b.y);
                if mid_y <= center_y + 1.0 {
                    *top += 1;
                } else {
                    *bot += 1;
                }
            }
            Shape::Path(p)
                if (p.stroke.width - 2.0).abs() < 0.01
                    && p.stroke.color == egui::epaint::ColorMode::Solid(LIGHT_INK)
                    && !p.points.is_empty() =>
            {
                let mid_y = p.points.iter().map(|pt| pt.y).sum::<f32>() / p.points.len() as f32;
                if mid_y <= center_y + 1.0 {
                    *top += 1;
                } else {
                    *bot += 1;
                }
            }
            Shape::Vec(vs) => vs.iter().for_each(|s| inspect(s, bg_col, center_y, mask_strokes, top, bot)),
            _ => {}
        }
    }

    for s in &shapes {
        inspect(&s.shape, bg_col, center_screen.y, &mut mask_strokes, &mut dashed_segments_top, &mut dashed_segments_bottom);
    }

    assert!(mask_strokes >= 1, "expected underlying background mask stroke for trimmed fragment, got {mask_strokes}");
    assert!(dashed_segments_top >= 3, "expected multiple screen-space dashed strokes on top semicircle, got {dashed_segments_top}");
    assert_eq!(dashed_segments_bottom, 0, "bottom semicircle must NOT be highlighted when hovering top half");
}

#[test]
fn trim_hover_line_between_two_limits_restores_and_confirms() {
    let mut c = Canvas::new(false);
    // Horizontal line from x=40 to x=160 at y=100, cut by two vertical lines at x=80 and x=120
    c.app.session.cmdline("LINE 40,100 160,100").unwrap();
    c.app.session.cmdline("").unwrap();
    c.app.session.cmdline("LINE 80,60 80,140").unwrap();
    c.app.session.cmdline("").unwrap();
    c.app.session.cmdline("LINE 120,60 120,140").unwrap();
    c.app.session.cmdline("").unwrap();
    // Isolated line that cannot be trimmed
    c.app.session.cmdline("LINE 40,170 160,170").unwrap();
    c.app.session.cmdline("").unwrap();

    c.app.cmdline("TRIM");
    let mid_span = c.screen(Vec2::new(100.0, 100.0));
    let left_cut_x = c.screen(Vec2::new(80.0, 100.0)).x;
    let right_cut_x = c.screen(Vec2::new(120.0, 100.0)).x;
    let bg_col = theme::Tokens::DARK.canvas;

    let count_trim_overlay = |shapes: &[egui::epaint::ClippedShape]| -> (usize, usize, usize) {
        let mut masks = 0usize;
        let mut inside_dashes = 0usize;
        let mut outside_dashes = 0usize;
        fn walk(s: &Shape, bg_col: Color32, lx: f32, rx: f32, masks: &mut usize, ins: &mut usize, outs: &mut usize) {
            match s {
                Shape::Path(p)
                    if (p.stroke.width - 4.5).abs() < 0.01
                        && p.stroke.color == egui::epaint::ColorMode::Solid(bg_col) =>
                {
                    *masks += 1;
                }
                Shape::LineSegment { points: [a, b], stroke } if (stroke.width - 2.0).abs() < 0.01 && stroke.color == LIGHT_INK => {
                    let mx = 0.5 * (a.x + b.x);
                    if mx >= lx - 1.0 && mx <= rx + 1.0 {
                        *ins += 1;
                    } else {
                        *outs += 1;
                    }
                }
                Shape::Vec(vs) => vs.iter().for_each(|x| walk(x, bg_col, lx, rx, masks, ins, outs)),
                _ => {}
            }
        }
        for cs in shapes {
            walk(&cs.shape, bg_col, left_cut_x, right_cut_x, &mut masks, &mut inside_dashes, &mut outside_dashes);
        }
        (masks, inside_dashes, outside_dashes)
    };

    // 1. Hover middle span between the two cutting lines
    let shapes = c.paint(mid_span);
    let (masks, in_dashes, out_dashes) = count_trim_overlay(&shapes);
    assert!(masks >= 1, "middle span must be masked over original continuous stroke");
    assert!(in_dashes >= 2, "middle span must show multiple dashed segments, got {in_dashes}");
    assert_eq!(out_dashes, 0, "outer spans of the line must not be dashed");

    // 2. Hover isolated line with no valid cut: no fictitious removal preview
    let isolated_pt = c.screen(Vec2::new(100.0, 170.0));
    let iso_shapes = c.paint(isolated_pt);
    assert_eq!(count_trim_overlay(&iso_shapes), (0, 0, 0), "no fictitious trim preview on non-intersecting entity");

    // 3. Move cursor to empty space: normal representation restored
    let empty_pt = c.screen(Vec2::new(20.0, 20.0));
    let empty_shapes = c.paint(empty_pt);
    assert_eq!(count_trim_overlay(&empty_shapes), (0, 0, 0), "moving cursor away restores normal representation");

    // 4. Confirm trim on middle span: removes only the middle segment, leaving 2 outer line segments + 2 cutting lines + 1 isolated line = 5 lines
    c.app.cmdline("100,100");
    c.app.session.cancel();
    assert_eq!(c.app.session.doc().unwrap().model.len(), 5, "confirming trim replaces the cut line with its two outer segments");
}

#[test]
fn trim_successive_cuts_on_same_object_ui() {
    let mut c = Canvas::new(false);
    c.app.session.cmdline("CIRCLE 100,100 40").unwrap();
    c.app.session.cmdline("LINE 40,100 160,100").unwrap();
    c.app.session.cmdline("").unwrap();
    c.app.session.cmdline("LINE 100,40 100,160").unwrap();
    c.app.session.cmdline("").unwrap();

    let dashed_lt = cadcraft_engine::doc::library::standard_linetypes()
        .into_iter()
        .find(|l| l.name == "DASHED")
        .unwrap();
    c.app.session.doc_mut().unwrap().linetypes.push(dashed_lt);

    c.app.cmdline("TRIM");

    let count_dashes = |shapes: &[egui::epaint::ClippedShape]| -> usize {
        let mut count = 0;
        fn walk(s: &Shape, count: &mut usize) {
            match s {
                Shape::LineSegment { stroke, .. } if (stroke.width - 2.0).abs() < 0.01 && stroke.color == LIGHT_INK => {
                    *count += 1;
                }
                Shape::Path(p) if (p.stroke.width - 2.0).abs() < 0.01 && p.stroke.color == egui::epaint::ColorMode::Solid(LIGHT_INK) => {
                    *count += 1;
                }
                Shape::Vec(vs) => vs.iter().for_each(|x| walk(x, count)),
                _ => {}
            }
        }
        for cs in shapes {
            walk(&cs.shape, &mut count);
        }
        count
    };

    // 1. Hover top-right quadrant (128.28, 128.28)
    let tr_pt = c.screen(Vec2::new(128.28, 128.28));
    let s1 = c.paint(tr_pt);
    let d1 = count_dashes(&s1);
    println!("Dashes on preview 1 (top-right): {d1}");
    assert!(d1 >= 2, "first preview must show dashed line");

    // 2. Click with mouse to trim top-right quadrant
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 800.0))),
        events: vec![
            egui::Event::PointerMoved(tr_pt),
            egui::Event::PointerButton { pos: tr_pt, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() },
            egui::Event::PointerButton { pos: tr_pt, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() },
        ],
        ..Default::default()
    };
    let _ = c.ctx.run_ui(input, |ui| {
        c.app.logic(ui.ctx());
        c.app.ui(ui);
    });

    println!("Entities after mouse click: {}", c.app.session.doc().unwrap().model.len());
    for (i, e) in c.app.session.doc().unwrap().model.iter().enumerate() {
        println!("  e[{}]: handle={:?}, kind={:?}", i, e.handle, e.kind);
    }

    // 3. Hover top-left quadrant (71.72, 128.28)
    let tl_pt = c.screen(Vec2::new(71.72, 128.28));
    let s2 = c.paint(tl_pt);
    let d2 = count_dashes(&s2);
    println!("Dashes on preview 2 (top-left): {d2}");
    assert!(d2 >= 2, "second preview on remaining geometry must show dashed line");

    // 4. Click with mouse to trim top-left quadrant
    let input2 = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 800.0))),
        events: vec![
            egui::Event::PointerMoved(tl_pt),
            egui::Event::PointerButton { pos: tl_pt, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() },
            egui::Event::PointerButton { pos: tl_pt, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() },
        ],
        ..Default::default()
    };
    let _ = c.ctx.run_ui(input2, |ui| {
        c.app.logic(ui.ctx());
        c.app.ui(ui);
    });

    println!("Entities after second mouse click: {}", c.app.session.doc().unwrap().model.len());
    for (i, e) in c.app.session.doc().unwrap().model.iter().enumerate() {
        println!("  e[{}]: handle={:?}, kind={:?}", i, e.handle, e.kind);
    }
}

#[test]
fn trim_successive_cuts_on_line_ui() {
    let mut c = Canvas::new(false);
    c.app.session.cmdline("LINE 0,100 200,100").unwrap();
    c.app.session.cmdline("").unwrap();
    c.app.session.cmdline("LINE 50,50 50,150").unwrap();
    c.app.session.cmdline("").unwrap();
    c.app.session.cmdline("LINE 100,50 100,150").unwrap();
    c.app.session.cmdline("").unwrap();
    c.app.session.cmdline("LINE 150,50 150,150").unwrap();
    c.app.session.cmdline("").unwrap();

    let dashed_lt = cadcraft_engine::doc::library::standard_linetypes()
        .into_iter()
        .find(|l| l.name == "DASHED")
        .unwrap();
    c.app.session.doc_mut().unwrap().linetypes.push(dashed_lt);

    c.app.cmdline("TRIM");

    let count_dashes = |shapes: &[egui::epaint::ClippedShape]| -> usize {
        let mut count = 0;
        fn walk(s: &Shape, count: &mut usize) {
            match s {
                Shape::LineSegment { stroke, .. } if (stroke.width - 2.0).abs() < 0.01 && stroke.color == LIGHT_INK => {
                    *count += 1;
                }
                Shape::Path(p) if (p.stroke.width - 2.0).abs() < 0.01 && p.stroke.color == egui::epaint::ColorMode::Solid(LIGHT_INK) => {
                    *count += 1;
                }
                Shape::Vec(vs) => vs.iter().for_each(|x| walk(x, count)),
                _ => {}
            }
        }
        for cs in shapes {
            walk(&cs.shape, &mut count);
        }
        count
    };

    // 1. Hover first span (25, 100)
    let p1 = c.screen(Vec2::new(25.0, 100.0));
    let s1 = c.paint(p1);
    let d1 = count_dashes(&s1);
    println!("Line d1: {d1}");
    assert!(d1 >= 2);

    // 2. Click to trim first span
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 800.0))),
        events: vec![
            egui::Event::PointerMoved(p1),
            egui::Event::PointerButton { pos: p1, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() },
            egui::Event::PointerButton { pos: p1, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() },
        ],
        ..Default::default()
    };
    let _ = c.ctx.run_ui(input, |ui| {
        c.app.logic(ui.ctx());
        c.app.ui(ui);
    });

    // 3. Hover second span (75, 100)
    let p2 = c.screen(Vec2::new(75.0, 100.0));
    let s2 = c.paint(p2);
    let d2 = count_dashes(&s2);
    println!("Line d2: {d2}");
    assert!(d2 >= 2);
}



