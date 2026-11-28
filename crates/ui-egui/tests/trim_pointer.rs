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

fn elevated_trim(noise: f64, pointer_secant: bool, move_after_release: bool) {
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

    assert_eq!(canvas.app.session.doc().unwrap().header.i64("PROJMODE", 0), 0);
    canvas.app.session.start("trim").unwrap();
    let picked = center + Vec3::Y * radius;
    canvas.frame(vec![Event::PointerMoved(canvas.screen(picked))]);
    assert_eq!(canvas.app.session.cursor_entity_point.unwrap().0, target);
    let preview = canvas.app.session.preview(canvas.app.session.cursor);
    assert_eq!(preview.len(), 1);
    let EntityKind::Arc(removed) = &preview[0].kind else { panic!("expected an arc preview") };
    assert_eq!((removed.center, removed.radius, removed.start, removed.end), (center, radius, 0.0, PI));

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
            elevated_trim(noise, pointer_secant, false);
        }
    }
}

#[test]
fn elevated_trim_uses_release_position_when_pointer_moves_later_in_the_same_frame() {
    elevated_trim(0.0, true, true);
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
