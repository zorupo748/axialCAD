use std::f64::consts::PI;

use cadcraft_engine::doc::{EntityKind, Point};
use cadcraft_engine::geom::Vec3;
use cadcraft_engine::{Input, Session};
use cadcraft_ui_egui::{CadApp, Services};
use egui::{Context, Event, PointerButton, Pos2, RawInput, Rect, vec2};
use serde_json::json;

struct Canvas {
    app: CadApp,
    ctx: Context,
    time: f64,
}

impl Canvas {
    fn frame(&mut self, events: Vec<Event>) {
        self.time += 0.5;
        self.app.logic(&self.ctx);
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 1000.0))),
            events,
            time: Some(self.time),
            ..Default::default()
        };
        self.ctx.run_ui(input, |ui| self.app.ui(ui)).textures_delta.clear();
    }

    fn screen(&self, point: Vec3) -> Pos2 {
        self.app.canvas.xf.unwrap().to_screen3(point)
    }

    fn click(&mut self, point: Vec3, next_hover: Option<Vec3>) {
        let pos = self.screen(point);
        let button = |pressed| Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Default::default() };
        let mut events = vec![Event::PointerMoved(pos), button(true), button(false)];
        if let Some(next) = next_hover {
            events.push(Event::PointerMoved(self.screen(next)));
        }
        self.frame(events);
    }
}

fn elevated_trim(projmode: i64, noise: f64, pointer_secant: bool, move_after_release: bool) {
    let mut app = CadApp::new(Session::new(), Services::default());
    app.session.execute("view.swiso", &json!({})).unwrap();
    app.session.set_drawing_plane_automatic();
    app.session.set_drawing_plane_offset_automatic();
    app.session.settings.orthomode = false;
    app.session.settings.axistrack = false;
    app.session.settings.osmode = cadcraft_engine::snap::mode::NOD;
    app.canvas.zoom_pending = false;
    let center = Vec3::new(0.0, 0.0, 30.0);
    let mut camera = app.session.camera();
    camera.target = center;
    camera.height = 115.0;
    app.session.set_camera(camera);
    let radius = 10.0;
    let a = Vec3::new(-40.0, 0.0, 30.0 + noise);
    let b = Vec3::new(40.0, 0.0, 30.0 + noise);
    let mut nodes = Vec::new();
    for p in [center, center + Vec3::X * radius].into_iter().chain([a, b].into_iter().filter(|_| pointer_secant)) {
        nodes.push(app.session.add_entity(EntityKind::Point(Point { p, angle: 0.0 })).unwrap());
    }
    let mut canvas = Canvas { app, ctx: Context::default(), time: 0.0 };
    canvas.frame(Vec::new());
    canvas.app.session.start("circle").unwrap();
    canvas.click(center, None);
    canvas.click(center + Vec3::X * radius, None);
    let (target, circle) = canvas
        .app
        .session
        .doc()
        .unwrap()
        .model
        .iter()
        .find_map(|e| match &e.kind {
            EntityKind::Circle(circle) => Some((e.handle, circle.clone())),
            _ => None,
        })
        .expect("circle drawn with the pointer");
    assert_eq!((circle.center, circle.radius), (center, radius));

    if pointer_secant {
        canvas.app.session.start("line").unwrap();
        canvas.click(a, None);
        canvas.click(b, None);
        canvas.app.session.input(Input::Enter).unwrap();
        assert!(canvas.app.session.doc().unwrap().model.iter().any(|e| matches!(&e.kind, EntityKind::Line(l) if l.a == a && l.b == b)));
    } else {
        canvas.app.session.execute("line", &json!({"points": [[a.x, a.y, a.z], [b.x, b.y, b.z]]})).unwrap();
    }
    for h in nodes {
        canvas.app.session.doc_mut().unwrap().remove_entity(h);
    }
    for x in [-4, 4] {
        canvas.app.session.execute("line", &json!({"points": [[x, -14, 0], [x, 14, 0]]})).unwrap();
    }

    canvas.app.session.doc_mut().unwrap().header.set_i64("PROJMODE", projmode);
    canvas.app.session.start("trim").unwrap();
    let picked = center + Vec3::Y * radius;
    canvas.frame(vec![Event::PointerMoved(canvas.screen(picked))]);
    assert_eq!(canvas.app.session.cursor_entity_point.unwrap().0, target);
    let preview = canvas.app.session.preview(canvas.app.session.cursor);
    if projmode == 0 && noise > 2e-5 * a.dist(b) {
        assert!(preview.is_empty(), "the secant is outside physical contact tolerance");
        canvas.click(picked, None);
        let doc = canvas.app.session.doc().unwrap();
        assert_eq!(doc.entity(target).unwrap().kind, EntityKind::Circle(circle));
        assert_eq!(doc.model.len(), 4);
        return;
    }
    assert_eq!(preview.len(), 1);
    let EntityKind::Arc(removed) = &preview[0].kind else { panic!("expected an arc preview") };
    assert_eq!((removed.center, removed.radius), (center, radius));
    let expected_start = if projmode == 1 { (4.0 / radius).acos() } else { 0.0 };
    assert!((removed.start - expected_start).abs() < 1e-9);
    assert!((removed.end - (PI - expected_start)).abs() < 1e-9);

    canvas.click(picked, move_after_release.then_some(center - Vec3::Y * radius));
    let doc = canvas.app.session.doc().unwrap();
    assert!(doc.entity(target).is_none());
    assert_eq!(doc.model.iter().filter(|e| matches!(&e.kind, EntityKind::Line(_))).count(), 3);
    let arc = doc
        .model
        .iter()
        .find_map(|e| match &e.kind {
            EntityKind::Arc(arc) => Some(arc),
            _ => None,
        })
        .expect("the complementary semicircle must survive");
    assert_eq!((arc.center, arc.radius, arc.start, arc.end), (center, radius, removed.end, removed.start));
}

#[test]
fn elevated_trim_keeps_the_complement_of_the_preview_with_exact_and_noisy_secants() {
    for noise in [0.0, 1e-6, 1e-3] {
        for pointer_secant in [false, true] {
            elevated_trim(0, noise, pointer_secant, false);
        }
    }
}

#[test]
fn elevated_trim_uses_release_position_when_pointer_moves_later_in_the_same_frame() {
    elevated_trim(0, 0.0, true, true);
}

#[test]
fn elevated_trim_in_ucs_applies_the_preview_including_ground_cutters() {
    elevated_trim(1, 1e-3, true, false);
}

#[test]
fn elevated_trim_in_none_preserves_the_circle_when_the_secant_misses() {
    elevated_trim(0, 0.01, true, false);
}

#[test]
fn elevated_bulged_polyline_is_picked_on_the_arc_instead_of_its_chord() {
    use cadcraft_engine::doc::LwPolyline;
    use cadcraft_engine::geom::{PolyVertex, Polyline, Vec2};

    let mut app = CadApp::new(Session::new(), Services::default());
    app.session.execute("view.swiso", &json!({})).unwrap();
    let elevation = 30.0;
    let mut camera = app.session.camera();
    camera.target = Vec3::new(0.0, 0.0, elevation);
    camera.height = 35.0;
    app.session.set_camera(camera);
    app.canvas.zoom_pending = false;
    let mut first = PolyVertex::new(Vec2::new(-5.0, 0.0));
    first.bulge = 1.0;
    let vertices = vec![first, PolyVertex::new(Vec2::new(5.0, 0.0))];
    let point_on_arc = Polyline { vertices: vertices.clone(), closed: false }.segments()[0].at(0.5).to3(elevation);
    let h =
        app.session.add_entity(EntityKind::LwPolyline(LwPolyline { vertices, closed: false, const_width: 0.0, elevation, plinegen: false })).unwrap();
    let mut canvas = Canvas { app, ctx: Context::default(), time: 0.0 };
    canvas.app.session.start("trim").unwrap();
    canvas.frame(Vec::new());
    canvas.frame(vec![Event::PointerMoved(canvas.screen(point_on_arc))]);
    assert_eq!(canvas.app.session.cursor_entity_point.map(|(handle, _, _)| handle), Some(h));
    canvas.frame(vec![Event::PointerMoved(canvas.screen(Vec3::new(0.0, 0.0, elevation)))]);
    assert!(canvas.app.session.cursor_entity_point.is_none());
}

fn circle_with_radial_cutters(nominal_radius: f64, gap: f64) -> (Canvas, cadcraft_engine::doc::Handle, cadcraft_engine::doc::Circle) {
    let mut app = CadApp::new(Session::new(), Services::default());
    app.session.execute("line", &json!({"points": [[0, 0, 0], [0, 0, 50]]})).unwrap();
    app.session.execute("view.swiso", &json!({})).unwrap();
    app.session.set_drawing_plane_automatic();
    app.session.set_drawing_plane_offset_automatic();
    app.session.settings.orthomode = false;
    app.session.settings.axistrack = false;
    app.session.settings.osmode = 0;
    app.canvas.zoom_pending = false;
    let center = Vec3::new(0.0, 0.0, 50.0);
    let mut camera = app.session.camera();
    camera.target = center;
    camera.height = 200.0;
    app.session.set_camera(camera);
    let mut canvas = Canvas { app, ctx: Context::default(), time: 0.0 };
    canvas.frame(Vec::new());

    canvas.app.session.start("circle").unwrap();
    canvas.app.session.cmdline("_END").unwrap();
    canvas.click(center, None);
    canvas.click(center + Vec3::X * nominal_radius, None);
    let (target, circle) = canvas
        .app
        .session
        .doc()
        .unwrap()
        .model
        .iter()
        .find_map(|e| match &e.kind {
            EntityKind::Circle(circle) => Some((e.handle, circle.clone())),
            _ => None,
        })
        .expect("circle drawn with a free pointer radius");
    assert_eq!(circle.center, center);
    assert!((circle.radius - nominal_radius).abs() < 1e-4, "free radius: {}", circle.radius);

    for sign in [1.0, -1.0] {
        let edge = center + Vec3::X * (sign * (circle.radius + gap));
        // Use a node snap only for the outer endpoint, to control its WCS gap.
        let node = canvas.app.session.add_entity(EntityKind::Point(Point { p: edge, angle: 0.0 })).unwrap();
        canvas.app.session.start("line").unwrap();
        canvas.app.session.cmdline("_END").unwrap();
        canvas.click(center, None);
        canvas.app.session.settings.osmode = cadcraft_engine::snap::mode::NOD;
        canvas.click(edge, None);
        canvas.app.session.settings.osmode = 0;
        canvas.app.session.input(Input::Enter).unwrap();
        canvas.app.session.doc_mut().unwrap().remove_entity(node);
        assert!(canvas.app.session.doc().unwrap().model.iter().any(|e| matches!(&e.kind, EntityKind::Line(l) if l.a == center && l.b == edge)));
    }
    (canvas, target, circle)
}

#[test]
fn oblique_auto_trim_with_two_radial_endpoint_cutters() {
    use cadcraft_engine::geom::{TAU, norm_angle};

    let mut failures = Vec::new();
    for mode in [0, 1] {
        for (nominal_radius, gap) in [(64.0, 0.0), (64.0, 1e-6), (64.0, -1e-6), (64.0, 1e-3), (64.0, -1e-3), (10.0, -1e-3)] {
            for upper in [true, false] {
                let (mut canvas, target, circle) = circle_with_radial_cutters(nominal_radius, gap);
                let center = circle.center;
                let case = format!("mode={mode} radius={nominal_radius} gap={gap:+} upper={upper}");
                eprintln!("radial endpoint test: {case}");
                canvas.app.session.doc_mut().unwrap().header.set_i64("PROJMODE", mode);
                canvas.app.session.start("trim").unwrap();
                let pick = center + Vec3::Y * if upper { circle.radius } else { -circle.radius };
                canvas.frame(vec![Event::PointerMoved(canvas.screen(pick))]);
                assert_eq!(canvas.app.session.cursor_entity_point.unwrap().0, target, "{case}");
                let preview = canvas.app.session.preview(canvas.app.session.cursor);
                canvas.click(pick, None);

                let doc = canvas.app.session.doc().unwrap();
                assert_eq!(doc.model.iter().filter(|e| matches!(e.kind, EntityKind::Line(_))).count(), 3, "{case}");
                let kept = doc.model.iter().find_map(|e| match &e.kind {
                    EntityKind::Arc(a) => Some(a),
                    _ => None,
                });
                // At radius 10 the 1e-3 gap exceeds the drawing-scale tolerance.
                if nominal_radius == 10.0 {
                    assert!(kept.is_none(), "{case}");
                    if mode == 0 {
                        assert!(preview.is_empty(), "{case}");
                        assert_eq!(doc.entity(target).unwrap().kind, EntityKind::Circle(circle), "{case}");
                    } else {
                        assert_eq!(preview.len(), 1, "{case}");
                        assert_eq!(preview[0].kind, EntityKind::Circle(circle), "{case}");
                        assert!(doc.entity(target).is_none(), "{case}");
                    }
                    continue;
                }
                let [preview_entity] = preview.as_slice() else {
                    failures.push(format!("{case}: preview count={}, target survived={}", preview.len(), doc.entity(target).is_some()));
                    continue;
                };
                let (EntityKind::Arc(removed), Some(kept)) = (&preview_entity.kind, kept) else {
                    failures.push(format!(
                        "{case}: preview={}, surviving arc={}, target survived={}",
                        preview_entity.kind.type_name(),
                        kept.is_some(),
                        doc.entity(target).is_some()
                    ));
                    continue;
                };
                let close_angle = |a: f64, b: f64| {
                    let d = norm_angle(a - b);
                    d.min(TAU - d) < 1e-9
                };
                let (start, end) = if upper { (0.0, PI) } else { (PI, 0.0) };
                assert!(close_angle(removed.start, start) && close_angle(removed.end, end), "{case}: {removed:?}");
                assert!(close_angle(kept.start, removed.end) && close_angle(kept.end, removed.start), "{case}: {kept:?}");
                assert_eq!((kept.center, kept.radius), (circle.center, circle.radius), "{case}");
                assert!(doc.entity(target).is_none(), "{case}");
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
