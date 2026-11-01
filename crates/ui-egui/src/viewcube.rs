//! The navigation overlays on the drawing area: the ViewCube (Top face, Home, compass ring and
//! the WCS menu) and the UCS icon.
//!
//! Every overlay registers a widget that senses clicks *and* drags, so egui hands the pointer to
//! the overlay instead of the canvas underneath (a click on the ViewCube must never also pick or
//! start a selection window in the drawing). What the overlays do goes through commands:
//! `zoom.extents` (Top face, Home), `ui.toggle.viewcube`, `ui.toggle.ucsicon`.
//!
//! CADCraft's model view is a 2D plan view of the World Coordinate System: there is no view
//! twist, no 3D view direction and no user coordinate system yet, so the compass ring explains
//! that instead of rotating, and the WCS menu lists the only coordinate system there is.

use egui::{Color32, CursorIcon, Pos2, Rect, Sense, Stroke, pos2, vec2};
use serde_json::json;

use crate::CadApp;
use crate::theme::Tokens;

/// Shown when the compass ring is clicked or dragged.
/// Where the ViewCube's parts are on the canvas.
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub center: Pos2,
    /// Radius of the compass ring's centre line.
    pub ring: f32,
    pub face: Rect,
    pub home: Rect,
    /// The "WCS" coordinate-system menu under the cube.
    pub pill: Rect,
    /// Everything that belongs to the cube (the area kept away from the canvas).
    pub bounds: Rect,
}

impl Layout {
    pub fn new(canvas: Rect) -> Self {
        let center = pos2(canvas.right() - 92.0, canvas.top() + 82.0);
        let ring = 58.0;
        Layout {
            center,
            ring,
            face: Rect::from_center_size(center, vec2(44.0, 44.0)),
            home: Rect::from_center_size(center + vec2(-ring + 2.0, -ring + 2.0), vec2(20.0, 20.0)),
            pill: Rect::from_center_size(center + vec2(0.0, ring + 26.0), vec2(56.0, 16.0)),
            bounds: Rect::from_center_size(center, vec2(2.0 * (ring + 10.0), 2.0 * (ring + 10.0))),
        }
    }

    /// The part of the cube at `p` (not the pill, which is its own widget).
    pub fn part_at(&self, p: Pos2) -> Option<Part> {
        if self.home.contains(p) {
            return Some(Part::Home);
        }
        if self.face.contains(p) {
            let dx = p.x - self.center.x;
            let dy = p.y - self.center.y;
            // Center area: Top face (or toggle Bottom)
            if dx.abs() <= 11.0 && dy.abs() <= 11.0 {
                return Some(Part::Top);
            }
            // Corners: Isometric views
            if dx < -11.0 && dy < -11.0 {
                return Some(Part::NwIso);
            }
            if dx > 11.0 && dy < -11.0 {
                return Some(Part::NeIso);
            }
            if dx < -11.0 && dy > 11.0 {
                return Some(Part::SwIso);
            }
            if dx > 11.0 && dy > 11.0 {
                return Some(Part::SeIso);
            }
            // Edges: Orthogonal standard views
            if dy < -11.0 {
                return Some(Part::Back);
            }
            if dy > 11.0 {
                return Some(Part::Front);
            }
            if dx < -11.0 {
                return Some(Part::Left);
            }
            if dx > 11.0 {
                return Some(Part::Right);
            }
            return Some(Part::Top);
        }
        let d = p.distance(self.center);
        if (self.ring - 12.0..=self.ring + 12.0).contains(&d) {
            let dx = p.x - self.center.x;
            let dy = p.y - self.center.y;
            if dy < -35.0 && dx.abs() <= 20.0 {
                return Some(Part::CompassN);
            }
            if dy > 35.0 && dx.abs() <= 20.0 {
                return Some(Part::CompassS);
            }
            if dx > 35.0 && dy.abs() <= 20.0 {
                return Some(Part::CompassE);
            }
            if dx < -35.0 && dy.abs() <= 20.0 {
                return Some(Part::CompassW);
            }
            return Some(Part::Ring);
        }
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Top,
    Bottom,
    Front,
    Back,
    Left,
    Right,
    SwIso,
    SeIso,
    NeIso,
    NwIso,
    Home,
    CompassN,
    CompassS,
    CompassE,
    CompassW,
    Ring,
}

impl Part {
    fn tip(self) -> &'static str {
        match self {
            Part::Top => "Top View (click to switch Top / Bottom)",
            Part::Bottom => "Bottom View",
            Part::Front | Part::CompassS => "Front View (South)",
            Part::Back | Part::CompassN => "Back View (North)",
            Part::Left | Part::CompassW => "Left View (West)",
            Part::Right | Part::CompassE => "Right View (East)",
            Part::SwIso => "SW Isometric View",
            Part::SeIso => "SE Isometric View",
            Part::NeIso => "NE Isometric View",
            Part::NwIso => "NW Isometric View",
            Part::Home => "Home: SW Isometric View (zoom extents)",
            Part::Ring => "Compass Ring (drag to orbit)",
        }
    }
}

/// Paint the ViewCube's ring, compass letters, cube faces/corners and Home button, and handle the pointer.
/// The canvas paints the WCS pill itself; this returns the pointer's part for its highlight.
pub fn show(app: &mut CadApp, ui: &mut egui::Ui, l: &Layout) {
    let resp = ui.interact(l.bounds, ui.id().with("viewcube"), Sense::click_and_drag());
    let part = resp.hover_pos().and_then(|p| l.part_at(p));
    let p = ui.painter_at(l.bounds.union(l.pill));
    let ring_col = if part == Some(Part::Ring) { Color32::from_rgb(0x5c, 0x67, 0x7c) } else { Color32::from_rgb(0x48, 0x50, 0x5c) };
    p.circle_stroke(l.center, l.ring, Stroke::new(9.0, ring_col));
    p.circle_stroke(l.center, l.ring + 4.5, Stroke::new(1.0, Color32::from_rgb(0x5c, 0x65, 0x72)));
    let f = egui::FontId::proportional(17.0);
    let lc = Color32::from_rgb(0xc8, 0xcc, 0xd2);
    let l_hot = Color32::from_rgb(0x60, 0xaa, 0xff);
    let c = l.center;
    let ring = l.ring;
    p.text(
        c + vec2(0.0, -ring - 1.0),
        egui::Align2::CENTER_CENTER,
        crate::tl!("N"),
        f.clone(),
        if part == Some(Part::CompassN) { l_hot } else { lc },
    );
    p.text(c + vec2(0.0, ring + 1.0), egui::Align2::CENTER_CENTER, crate::tl!("S"), f.clone(), if part == Some(Part::CompassS) { l_hot } else { lc });
    p.text(c + vec2(ring + 1.0, 0.0), egui::Align2::CENTER_CENTER, crate::tl!("E"), f.clone(), if part == Some(Part::CompassE) { l_hot } else { lc });
    p.text(c + vec2(-ring - 1.0, 0.0), egui::Align2::CENTER_CENTER, crate::tl!("W"), f, if part == Some(Part::CompassW) { l_hot } else { lc });

    // Paint base face
    let face_col = Color32::from_rgb(0x9a, 0x9e, 0xa4);
    p.rect_filled(l.face, 2.0, face_col);
    p.rect_stroke(l.face, 2.0, Stroke::new(1.0, Color32::from_rgb(0x6c, 0x70, 0x76)), egui::StrokeKind::Inside);

    // Subtle grid lines dividing the 3x3 regions of the face
    let left_11 = c.x - 11.0;
    let right_11 = c.x + 11.0;
    let top_11 = c.y - 11.0;
    let bottom_11 = c.y + 11.0;
    let grid_stroke = Stroke::new(1.0, Color32::from_rgb(0x7a, 0x7e, 0x86));
    p.line_segment([pos2(left_11, l.face.top()), pos2(left_11, l.face.bottom())], grid_stroke);
    p.line_segment([pos2(right_11, l.face.top()), pos2(right_11, l.face.bottom())], grid_stroke);
    p.line_segment([pos2(l.face.left(), top_11), pos2(l.face.right(), top_11)], grid_stroke);
    p.line_segment([pos2(l.face.left(), bottom_11), pos2(l.face.right(), bottom_11)], grid_stroke);

    // Hover highlights for the interactive parts of the face
    let hot_col = Color32::from_rgba_premultiplied(0x40, 0x90, 0xf0, 0x55);
    match part {
        Some(Part::Top) => {
            p.rect_filled(Rect::from_min_max(pos2(left_11, top_11), pos2(right_11, bottom_11)), 1.0, hot_col);
        }
        Some(Part::Back) => {
            p.rect_filled(Rect::from_min_max(pos2(left_11, l.face.top()), pos2(right_11, top_11)), 1.0, hot_col);
        }
        Some(Part::Front) => {
            p.rect_filled(Rect::from_min_max(pos2(left_11, bottom_11), pos2(right_11, l.face.bottom())), 1.0, hot_col);
        }
        Some(Part::Left) => {
            p.rect_filled(Rect::from_min_max(pos2(l.face.left(), top_11), pos2(left_11, bottom_11)), 1.0, hot_col);
        }
        Some(Part::Right) => {
            p.rect_filled(Rect::from_min_max(pos2(right_11, top_11), pos2(l.face.right(), bottom_11)), 1.0, hot_col);
        }
        Some(Part::NwIso) => {
            p.rect_filled(Rect::from_min_max(l.face.left_top(), pos2(left_11, top_11)), 1.0, hot_col);
        }
        Some(Part::NeIso) => {
            p.rect_filled(Rect::from_min_max(pos2(right_11, l.face.top()), pos2(l.face.right(), top_11)), 1.0, hot_col);
        }
        Some(Part::SwIso) => {
            p.rect_filled(Rect::from_min_max(pos2(l.face.left(), bottom_11), pos2(left_11, l.face.bottom())), 1.0, hot_col);
        }
        Some(Part::SeIso) => {
            p.rect_filled(Rect::from_min_max(pos2(right_11, bottom_11), l.face.right_bottom()), 1.0, hot_col);
        }
        _ => {}
    }

    let view_name = app.session.camera().view_name();
    p.text(c, egui::Align2::CENTER_CENTER, view_name, egui::FontId::proportional(11.0), Color32::from_rgb(0x30, 0x34, 0x3a));
    paint_home(&p, l.home, part == Some(Part::Home));

    let resp = match part {
        Some(part) => resp.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(part.tip()),
        None => resp,
    };
    let pressed_on = resp.interact_pointer_pos().and_then(|o| l.part_at(o));
    if resp.dragged() {
        let d = resp.drag_delta();
        let _ = app.session.orbit_step(f64::from(d.x), f64::from(d.y));
    }
    if resp.clicked() {
        match pressed_on {
            Some(Part::Top) => {
                if app.session.camera().is_plan() {
                    let _ = app.run("view.bottom", json!({}));
                } else {
                    let _ = app.run("view.top", json!({}));
                }
            }
            Some(Part::Bottom) => {
                let _ = app.run("view.bottom", json!({}));
            }
            Some(Part::Front | Part::CompassS) => {
                let _ = app.run("view.front", json!({}));
            }
            Some(Part::Back | Part::CompassN) => {
                let _ = app.run("view.back", json!({}));
            }
            Some(Part::Left | Part::CompassW) => {
                let _ = app.run("view.left", json!({}));
            }
            Some(Part::Right | Part::CompassE) => {
                let _ = app.run("view.right", json!({}));
            }
            Some(Part::SwIso | Part::Home) => {
                let _ = app.run("view.swiso", json!({}));
            }
            Some(Part::SeIso) => {
                let _ = app.run("view.seiso", json!({}));
            }
            Some(Part::NeIso) => {
                let _ = app.run("view.neiso", json!({}));
            }
            Some(Part::NwIso) => {
                let _ = app.run("view.nwiso", json!({}));
            }
            Some(Part::Ring) => {}
            None => {}
        }
    }
    let mut action = None;
    resp.context_menu(|ui| {
        if ui.button("Top (Plan View)").clicked() {
            action = Some(("view.top", json!({})));
            ui.close();
        }
        if ui.button("Bottom View").clicked() {
            action = Some(("view.bottom", json!({})));
            ui.close();
        }
        if ui.button("Front View").clicked() {
            action = Some(("view.front", json!({})));
            ui.close();
        }
        if ui.button("Back View").clicked() {
            action = Some(("view.back", json!({})));
            ui.close();
        }
        if ui.button("Left View").clicked() {
            action = Some(("view.left", json!({})));
            ui.close();
        }
        if ui.button("Right View").clicked() {
            action = Some(("view.right", json!({})));
            ui.close();
        }
        ui.separator();
        if ui.button("SW Isometric").clicked() {
            action = Some(("view.swiso", json!({})));
            ui.close();
        }
        if ui.button("SE Isometric").clicked() {
            action = Some(("view.seiso", json!({})));
            ui.close();
        }
        if ui.button("NE Isometric").clicked() {
            action = Some(("view.neiso", json!({})));
            ui.close();
        }
        if ui.button("NW Isometric").clicked() {
            action = Some(("view.nwiso", json!({})));
            ui.close();
        }
        ui.separator();
        if ui.button("Home (SW ISO)").clicked() {
            action = Some(("view.swiso", json!({})));
            ui.close();
        }
        if ui.button("Hide ViewCube").clicked() {
            action = Some(("ui.toggle.viewcube", json!({"on": false})));
            ui.close();
        }
    });
    if let Some((id, params)) = action {
        let _ = app.run(id, params);
    }
    // The WCS pill (painted by the canvas) opens the coordinate-system menu.
    let pill = ui.interact(l.pill, ui.id().with("viewcube_wcs"), Sense::click_and_drag()).on_hover_cursor(CursorIcon::PointingHand);
    let pill = pill.on_hover_text("Coordinate system");
    ucs_menu(app, egui::Popup::menu(&pill));
}

/// A small house, drawn in code.
fn paint_home(p: &egui::Painter, r: Rect, hot: bool) {
    let t = Tokens::get();
    if hot {
        p.rect_filled(r, 3.0, t.control_hover);
    }
    let st = Stroke::new(1.3, if hot { t.text } else { t.text_dim });
    let c = r.center();
    let roof = [c + vec2(-6.5, 0.5), c + vec2(0.0, -6.0), c + vec2(6.5, 0.5)];
    p.line(roof.to_vec(), st);
    p.line(vec![c + vec2(-4.5, -1.0), c + vec2(-4.5, 6.0), c + vec2(4.5, 6.0), c + vec2(4.5, -1.0)], st);
    p.line_segment([c + vec2(-1.2, 6.0), c + vec2(-1.2, 2.5)], st);
    p.line_segment([c + vec2(1.2, 6.0), c + vec2(1.2, 2.5)], st);
    p.line_segment([c + vec2(-1.2, 2.5), c + vec2(1.2, 2.5)], st);
}

/// The UCS icon's arms (as drawn by the canvas at the lower-left corner): the X arm and the Y arm.
pub fn ucs_arms(canvas: Rect) -> [Rect; 2] {
    let o = pos2(canvas.left() + 36.0, canvas.bottom() - 34.0);
    [Rect::from_min_max(o + vec2(-8.0, -8.0), o + vec2(80.0, 8.0)), Rect::from_min_max(o + vec2(-8.0, -82.0), o + vec2(8.0, 8.0))]
}

/// Make the UCS icon clickable: hovering highlights it, a click (either button) opens the
/// coordinate-system menu.
pub fn ucs_icon(app: &mut CadApp, ui: &mut egui::Ui, canvas: Rect) {
    let t = Tokens::get();
    let [x, y] = ucs_arms(canvas);
    let rx = ui.interact(x, ui.id().with("ucs_x"), Sense::click_and_drag());
    let ry = ui.interact(y, ui.id().with("ucs_y"), Sense::click_and_drag());
    let resp = (rx | ry).on_hover_cursor(CursorIcon::PointingHand).on_hover_text("UCS icon: World Coordinate System");
    if resp.hovered() {
        let o = pos2(x.left() + 8.0, x.center().y);
        let st = Stroke::new(2.0, t.hover);
        let p = ui.painter_at(canvas);
        p.line_segment([o, o + vec2(60.0, 0.0)], st);
        p.line_segment([o, o + vec2(0.0, -60.0)], st);
    }
    // Opened with `Bool(true)`: a pointer-anchored popup records its position only then.
    let open = (resp.clicked() || resp.secondary_clicked()).then_some(egui::SetOpenCommand::Bool(true));
    ucs_menu(app, egui::Popup::menu(&resp).open_memory(open).at_pointer_fixed());
}

/// The coordinate-system menu: the WCS (the only coordinate system so far) and the icon switch.
fn ucs_menu(app: &mut CadApp, popup: egui::Popup<'_>) {
    let mut hide = false;
    popup.show(|ui| {
        ui.radio(true, "World (WCS)").on_hover_text("CADCraft draws in the World Coordinate System; user coordinate systems aren't available yet.");
        ui.separator();
        if ui.button("Hide UCS Icon").clicked() {
            hide = true;
            ui.close();
        }
    });
    if hide {
        let _ = app.run("ui.toggle.ucsicon", json!({"on": false}));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_engine::Session;

    /// The app with the sample drawing, laid out once in a 1400×900 window.
    fn app_and_ctx() -> (CadApp, egui::Context) {
        let mut app = CadApp::new(Session::empty(), crate::Services::default());
        app.ui.in_window_menu = true;
        app.start("ui.sample");
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, Vec::new());
        frame(&mut app, &ctx, Vec::new());
        (app, ctx)
    }

    fn frame(app: &mut CadApp, ctx: &egui::Context, events: Vec<egui::Event>) {
        let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1400.0, 900.0))), events, ..Default::default() };
        let mut out = ctx.run_ui(raw, |ui| {
            app.logic(ui.ctx());
            app.ui(ui);
        });
        out.textures_delta.clear();
    }

    /// A real pointer click (move, press, release in separate frames, like the control channel).
    pub(crate) fn click(app: &mut CadApp, ctx: &egui::Context, at: Pos2, button: egui::PointerButton) {
        let m = egui::Modifiers::default();
        frame(app, ctx, vec![egui::Event::PointerMoved(at)]);
        frame(app, ctx, vec![egui::Event::PointerButton { pos: at, button, pressed: true, modifiers: m }]);
        frame(app, ctx, vec![egui::Event::PointerButton { pos: at, button, pressed: false, modifiers: m }]);
        frame(app, ctx, Vec::new());
    }

    /// A menu popup was shown in the last frame.
    fn menu_shown(ctx: &egui::Context) -> bool {
        ctx.memory(|m| m.areas().visible_layer_ids().iter().any(|l| l.order == egui::Order::Foreground))
    }

    fn canvas(app: &CadApp) -> Rect {
        app.canvas.rect.unwrap_or(Rect::NOTHING)
    }

    #[test]
    fn parts_are_found_where_they_are_drawn() {
        let l = Layout::new(Rect::from_min_size(pos2(200.0, 80.0), vec2(1000.0, 800.0)));
        assert_eq!(l.part_at(l.center), Some(Part::Top));
        assert_eq!(l.part_at(l.home.center()), Some(Part::Home));
        assert_eq!(l.part_at(l.center + vec2(0.0, -l.ring)), Some(Part::CompassN), "N");
        assert_eq!(l.part_at(l.center + vec2(l.ring, 0.0)), Some(Part::CompassE), "E");
        assert_eq!(l.part_at(l.center + vec2(-15.0, 15.0)), Some(Part::SwIso), "SW corner");
        assert_eq!(l.part_at(l.center + vec2(0.0, 15.0)), Some(Part::Front), "South edge");
        assert_eq!(l.part_at(l.center + vec2(30.0, 30.0)), None, "between the face and the ring");
        assert!(l.bounds.contains_rect(l.home) && l.bounds.contains_rect(l.face));
    }

    #[test]
    fn viewcube_clicks_reach_the_cube_not_the_drawing() {
        let (mut app, ctx) = app_and_ctx();
        let l = Layout::new(canvas(&app));

        // Click Home: switches to SW Isometric 3D view
        click(&mut app, &ctx, l.home.center(), egui::PointerButton::Primary);
        assert_eq!(app.session.camera().view_name(), "SW ISO", "Home button sets SW ISO view");
        assert!(app.session.pending_window.is_none());
        assert!(app.session.selection().is_empty());

        // Click Top: returns to Top plan view
        click(&mut app, &ctx, l.center, egui::PointerButton::Primary);
        assert!(app.session.camera().is_plan(), "Top face sets Plan view");
        click(&mut app, &ctx, l.center, egui::PointerButton::Primary);
        assert_eq!(app.session.camera().view_name(), "BOTTOM", "the Top face toggles to its opposite when already in Plan");
        assert!(app.session.camera().view_dir().near(cadcraft_geom::Vec3::new(0.0, 0.0, -1.0), 1e-6));
        click(&mut app, &ctx, l.center, egui::PointerButton::Primary);
        assert!(app.session.camera().is_plan(), "clicking the center again returns to Top");

        // Click Front (South edge): sets Front view
        click(&mut app, &ctx, l.center + vec2(0.0, 15.0), egui::PointerButton::Primary);
        assert_eq!(app.session.camera().view_name(), "FRONT", "South edge sets Front view");
        assert!(app.session.camera().view_dir().near(cadcraft_geom::Vec3::new(0.0, -1.0, 0.0), 1e-6));

        for (face_offset, compass_offset, preset, expected_view) in [
            (vec2(0.0, -15.0), vec2(0.0, -l.ring), "back", cadcraft_geom::Vec3::new(0.0, 1.0, 0.0)),
            (vec2(-15.0, 0.0), vec2(-l.ring, 0.0), "left", cadcraft_geom::Vec3::new(-1.0, 0.0, 0.0)),
            (vec2(15.0, 0.0), vec2(l.ring, 0.0), "right", cadcraft_geom::Vec3::new(1.0, 0.0, 0.0)),
        ] {
            click(&mut app, &ctx, l.center + face_offset, egui::PointerButton::Primary);
            assert_eq!(app.session.camera().view_name(), preset.to_ascii_uppercase());
            assert!(app.session.camera().view_dir().near(expected_view, 1e-6), "{preset} face vector");
            click(&mut app, &ctx, l.center + compass_offset, egui::PointerButton::Primary);
            assert_eq!(app.session.camera().view_name(), preset.to_ascii_uppercase());
            assert!(app.session.camera().view_dir().near(expected_view, 1e-6), "{preset} compass vector");
        }

        // Click SW corner: sets SW ISO view
        click(&mut app, &ctx, l.center + vec2(-15.0, 15.0), egui::PointerButton::Primary);
        assert_eq!(app.session.camera().view_name(), "SW ISO", "SW corner sets SW ISO view");

        let orbit_start = l.center + vec2(l.ring, 0.0);
        let orbit_end = orbit_start + vec2(20.0, 12.0);
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(orbit_start)]);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerButton {
                pos: orbit_start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::default(),
            }],
        );
        let before_orbit = app.session.camera();
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(orbit_end)]);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerButton {
                pos: orbit_end,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            }],
        );
        let after_orbit = app.session.camera();
        assert!(after_orbit.azimuth > before_orbit.azimuth, "dragging right advances azimuth");
        assert!(after_orbit.elevation < before_orbit.elevation, "dragging down lowers elevation");
        assert_eq!(after_orbit.view_name(), "SW ISO", "the label continues to identify the camera after orbiting");

        // A click in the drawing still reaches the drawing.
        let empty = canvas(&app).center() + vec2(0.0, -300.0);
        click(&mut app, &ctx, empty, egui::PointerButton::Primary);
        assert!(app.canvas.selection_window.is_some(), "an empty-space click starts a screen-space selection window");
    }

    #[test]
    fn ucs_icon_and_wcs_pill_open_the_coordinate_system_menu() {
        let (mut app, ctx) = app_and_ctx();
        let [x, _] = ucs_arms(canvas(&app));
        click(&mut app, &ctx, x.center(), egui::PointerButton::Primary);
        assert!(menu_shown(&ctx), "UCS icon menu");
        assert!(app.session.pending_window.is_none(), "the click stayed on the icon");
        let l = Layout::new(canvas(&app));
        click(&mut app, &ctx, l.pill.center(), egui::PointerButton::Primary);
        assert!(menu_shown(&ctx), "WCS menu");
        assert!(app.session.pending_window.is_none());
    }
}
