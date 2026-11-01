//! The navigation overlays on the drawing area: the camera-oriented ViewCube, compass ring,
//! Home button, WCS menu and UCS icon.
//!
//! Overlay pointer regions are reserved from canvas picking so a ViewCube click never also picks
//! or starts a selection window in the drawing. View changes go through commands.
//!
use cadcraft_geom::{OrbitCamera, Vec3};
use egui::{Color32, CursorIcon, Pos2, Rect, Sense, Stroke, pos2, vec2};
use serde_json::json;

use crate::CadApp;
use crate::theme::Tokens;

/// The ViewCube's parts and controls positioned over the canvas.
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub center: Pos2,
    /// Radius of the compass ring's centre line.
    pub ring: f32,
    pub home: Rect,
    /// The "WCS" coordinate-system menu under the cube.
    pub pill: Rect,
    /// Everything that belongs to the cube (the area kept away from the canvas).
    pub bounds: Rect,
    cube_radius: f32,
}

impl Layout {
    pub fn new(canvas: Rect) -> Self {
        let center = pos2(canvas.right() - 92.0, canvas.top() + 82.0);
        let ring = 58.0;
        Layout {
            center,
            ring,
            home: Rect::from_center_size(center + vec2(-ring + 2.0, -ring + 2.0), vec2(20.0, 20.0)),
            pill: Rect::from_center_size(center + vec2(0.0, ring + 26.0), vec2(56.0, 16.0)),
            bounds: Rect::from_center_size(center, vec2(2.0 * (ring + 10.0), 2.0 * (ring + 10.0))),
            cube_radius: 24.0,
        }
    }

    fn part_at(&self, point: Pos2, geometry: &CubeGeometry) -> Option<Part> {
        if self.home.contains(point) {
            return Some(Part::Home);
        }
        if let Some(part) = geometry.part_at(point) {
            return Some(part);
        }
        let distance = point.distance(self.center);
        if (self.ring - 12.0..=self.ring + 12.0).contains(&distance) {
            let offset = point - self.center;
            if offset.y < -35.0 && offset.x.abs() <= 20.0 {
                return Some(Part::CompassN);
            }
            if offset.y > 35.0 && offset.x.abs() <= 20.0 {
                return Some(Part::CompassS);
            }
            if offset.x > 35.0 && offset.y.abs() <= 20.0 {
                return Some(Part::CompassE);
            }
            if offset.x < -35.0 && offset.y.abs() <= 20.0 {
                return Some(Part::CompassW);
            }
            return Some(Part::Ring);
        }
        None
    }

    pub fn blocks_canvas_pointer(canvas: Rect, point: Pos2) -> bool {
        let layout = Self::new(canvas);
        layout.bounds.contains(point) || layout.pill.contains(point)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CubeFace {
    XPositive,
    XNegative,
    YPositive,
    YNegative,
    ZPositive,
    ZNegative,
}

impl CubeFace {
    const ALL: [Self; 6] = [Self::XPositive, Self::XNegative, Self::YPositive, Self::YNegative, Self::ZPositive, Self::ZNegative];

    fn normal(self) -> Vec3 {
        match self {
            Self::XPositive => Vec3::new(1.0, 0.0, 0.0),
            Self::XNegative => Vec3::new(-1.0, 0.0, 0.0),
            Self::YPositive => Vec3::new(0.0, 1.0, 0.0),
            Self::YNegative => Vec3::new(0.0, -1.0, 0.0),
            Self::ZPositive => Vec3::new(0.0, 0.0, 1.0),
            Self::ZNegative => Vec3::new(0.0, 0.0, -1.0),
        }
    }

    fn label(self) -> String {
        match self {
            Self::XPositive => crate::tl!("Right").to_owned(),
            Self::XNegative => crate::tl!("Left").to_owned(),
            Self::YPositive => crate::tl!("Back").to_owned(),
            Self::YNegative => crate::tl!("Front").to_owned(),
            Self::ZPositive => crate::tl!("Top").to_owned(),
            Self::ZNegative => crate::tl!("Bottom").to_owned(),
        }
    }

    fn preset(self) -> &'static str {
        match self {
            Self::XPositive => "right",
            Self::XNegative => "left",
            Self::YPositive => "back",
            Self::YNegative => "front",
            Self::ZPositive => "top",
            Self::ZNegative => "bottom",
        }
    }

    fn ring_abbreviation(self) -> String {
        let horizontal_faces = [Self::YPositive, Self::YNegative, Self::XPositive, Self::XNegative];
        let labels: Vec<String> = horizontal_faces.iter().map(|f| f.label()).collect();
        let target_idx = horizontal_faces.iter().position(|f| *f == self).unwrap_or(0);
        let target_label = &labels[target_idx];

        let initials: Vec<String> = labels.iter().map(|s| s.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default()).collect();

        let target_initial = &initials[target_idx];
        let is_ambiguous = initials.iter().filter(|i| *i == target_initial).count() > 1;

        if is_ambiguous {
            let mut chars = target_label.chars();
            match (chars.next(), chars.next()) {
                (Some(c1), Some(c2)) => {
                    let mut out = c1.to_uppercase().to_string();
                    out.extend(c2.to_lowercase());
                    out
                }
                (Some(c1), None) => c1.to_uppercase().to_string(),
                _ => String::new(),
            }
        } else {
            target_initial.clone()
        }
    }

    fn vertex_signs(self) -> [[i8; 3]; 4] {
        self.vertex_indices().map(corner_signs)
    }

    fn vertex_indices(self) -> [usize; 4] {
        match self {
            Self::XPositive => [4, 6, 7, 5],
            Self::XNegative => [0, 1, 3, 2],
            Self::YPositive => [2, 3, 7, 6],
            Self::YNegative => [0, 4, 5, 1],
            Self::ZPositive => [1, 5, 7, 3],
            Self::ZNegative => [0, 2, 6, 4],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CubeEdge {
    AlongX { y: i8, z: i8 },
    AlongY { x: i8, z: i8 },
    AlongZ { x: i8, y: i8 },
}

impl CubeEdge {
    const ALL: [Self; 12] = [
        Self::AlongX { y: -1, z: -1 },
        Self::AlongX { y: 1, z: -1 },
        Self::AlongX { y: -1, z: 1 },
        Self::AlongX { y: 1, z: 1 },
        Self::AlongY { x: -1, z: -1 },
        Self::AlongY { x: 1, z: -1 },
        Self::AlongY { x: -1, z: 1 },
        Self::AlongY { x: 1, z: 1 },
        Self::AlongZ { x: -1, y: -1 },
        Self::AlongZ { x: 1, y: -1 },
        Self::AlongZ { x: -1, y: 1 },
        Self::AlongZ { x: 1, y: 1 },
    ];

    fn endpoints(self) -> [[i8; 3]; 2] {
        match self {
            Self::AlongX { y, z } => [[-1, y, z], [1, y, z]],
            Self::AlongY { x, z } => [[x, -1, z], [x, 1, z]],
            Self::AlongZ { x, y } => [[x, y, -1], [x, y, 1]],
        }
    }

    fn direction(self) -> Vec3 {
        let [a, b] = self.endpoints();
        (corner_vec(a) + corner_vec(b)).normalized()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CubeCorner {
    At(i8, i8, i8),
}

impl CubeCorner {
    const ALL: [Self; 8] = [
        Self::At(-1, -1, -1),
        Self::At(-1, -1, 1),
        Self::At(-1, 1, -1),
        Self::At(-1, 1, 1),
        Self::At(1, -1, -1),
        Self::At(1, -1, 1),
        Self::At(1, 1, -1),
        Self::At(1, 1, 1),
    ];

    fn signs(self) -> [i8; 3] {
        match self {
            Self::At(x, y, z) => [x, y, z],
        }
    }

    fn direction(self) -> Vec3 {
        corner_vec(self.signs()).normalized()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    Face(CubeFace),
    Edge(CubeEdge),
    Corner(CubeCorner),
    Home,
    CompassN,
    CompassS,
    CompassE,
    CompassW,
    Ring,
}

impl Part {
    fn ring_label(self) -> Option<String> {
        match self {
            Self::CompassN => Some(CubeFace::YPositive.ring_abbreviation()),
            Self::CompassS => Some(CubeFace::YNegative.ring_abbreviation()),
            Self::CompassE => Some(CubeFace::XPositive.ring_abbreviation()),
            Self::CompassW => Some(CubeFace::XNegative.ring_abbreviation()),
            _ => None,
        }
    }

    fn tip(self) -> String {
        match self {
            Part::Face(face) => face.label(),
            Part::Edge(_) => crate::tl!("Select view along cube edge").to_owned(),
            Part::Corner(_) => crate::tl!("Select view from cube corner").to_owned(),
            Part::Home => crate::tl!("Home: SW Isometric View (zoom extents)").to_owned(),
            Part::CompassN => CubeFace::YPositive.label(),
            Part::CompassS => CubeFace::YNegative.label(),
            Part::CompassE => CubeFace::XPositive.label(),
            Part::CompassW => CubeFace::XNegative.label(),
            Part::Ring => crate::tl!("Compass Ring (drag to orbit)").to_owned(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct ProjectedFace {
    face: CubeFace,
    points: [Pos2; 4],
}

#[derive(Clone, Copy, Debug)]
struct ProjectedEdge {
    edge: CubeEdge,
    points: [Pos2; 2],
}

#[derive(Clone, Copy, Debug)]
struct ProjectedCorner {
    corner: CubeCorner,
    point: Pos2,
}

#[derive(Clone, Debug)]
struct CubeGeometry {
    faces: Vec<ProjectedFace>,
    edges: [ProjectedEdge; 12],
    corners: [ProjectedCorner; 8],
    center: Pos2,
    radius: f32,
    camera: OrbitCamera,
}

impl CubeGeometry {
    fn new(layout: &Layout, camera: OrbitCamera) -> Self {
        let project = |signs: [i8; 3]| {
            let point = corner_vec(signs) * f64::from(layout.cube_radius);
            let screen_x = point.dot(camera.right_dir()) as f32;
            let screen_y = point.dot(camera.up_dir()) as f32;
            layout.center + vec2(screen_x, -screen_y)
        };
        let mut faces: Vec<_> = CubeFace::ALL
            .into_iter()
            .filter(|face| face.normal().dot(camera.view_dir()) > 1e-6)
            .map(|face| ProjectedFace { face, points: face.vertex_signs().map(project) })
            .collect();
        faces.sort_by(|a, b| a.face.normal().dot(camera.view_dir()).total_cmp(&b.face.normal().dot(camera.view_dir())));
        let edges = CubeEdge::ALL.map(|edge| {
            let [a, b] = edge.endpoints();
            ProjectedEdge { edge, points: [project(a), project(b)] }
        });
        let corners = CubeCorner::ALL.map(|corner| ProjectedCorner { corner, point: project(corner.signs()) });
        Self { faces, edges, corners, center: layout.center, radius: layout.cube_radius, camera }
    }

    fn part_at(&self, point: Pos2) -> Option<Part> {
        let mut nearest_corner = None;
        for projected in self.corners {
            let distance = projected.point.distance(point);
            if distance <= 6.5
                && let Some(depth) = self.front_depth_at(projected.point)
                && (depth - self.depth_at_corner(projected.corner)).abs() <= 0.02
                && nearest_corner.is_none_or(|(_, best)| distance < best)
            {
                nearest_corner = Some((projected.corner, distance));
            }
        }
        if let Some((corner, _)) = nearest_corner {
            return Some(Part::Corner(corner));
        }

        let mut nearest_edge = None;
        for projected in self.edges {
            let (closest, distance, t) = closest_on_segment(point, projected.points[0], projected.points[1]);
            if distance <= 5.5
                && let Some(depth) = self.front_depth_at(closest)
            {
                let edge_depth = self.depth_at_edge(projected.edge, t);
                if (depth - edge_depth).abs() <= 0.02 && nearest_edge.is_none_or(|(_, best)| distance < best) {
                    nearest_edge = Some((projected.edge, distance));
                }
            }
        }
        if let Some((edge, _)) = nearest_edge {
            return Some(Part::Edge(edge));
        }

        self.front_face_at(point).map(Part::Face)
    }

    fn front_face_at(&self, point: Pos2) -> Option<CubeFace> {
        self.faces
            .iter()
            .filter(|face| point_in_polygon(point, &face.points))
            .max_by(|a, b| self.face_depth_at(a.face, point).total_cmp(&self.face_depth_at(b.face, point)))
            .map(|face| face.face)
    }

    fn front_depth_at(&self, point: Pos2) -> Option<f64> {
        self.faces.iter().filter(|face| point_in_polygon(point, &face.points)).map(|face| self.face_depth_at(face.face, point)).max_by(f64::total_cmp)
    }

    fn face_depth_at(&self, face: CubeFace, point: Pos2) -> f64 {
        let normal = face.normal();
        let right = self.camera.right_dir();
        let up = self.camera.up_dir();
        let screen =
            Vec3::new(f64::from(point.x - self.center.x) / f64::from(self.radius), -f64::from(point.y - self.center.y) / f64::from(self.radius), 0.0);
        let ray_origin = right * screen.x + up * screen.y;
        let plane_offset = 1.0;
        let plane = if normal.x + normal.y + normal.z < 0.0 { -plane_offset } else { plane_offset };
        (plane - normal.dot(ray_origin)) / normal.dot(self.camera.view_dir())
    }

    fn depth_at_corner(&self, corner: CubeCorner) -> f64 {
        corner_vec(corner.signs()).dot(self.camera.view_dir())
    }

    fn depth_at_edge(&self, edge: CubeEdge, t: f32) -> f64 {
        let [a, b] = edge.endpoints();
        (corner_vec(a) * f64::from(1.0 - t) + corner_vec(b) * f64::from(t)).dot(self.camera.view_dir())
    }
}

fn corner_signs(index: usize) -> [i8; 3] {
    [if index & 4 == 0 { -1 } else { 1 }, if index & 2 == 0 { -1 } else { 1 }, if index & 1 == 0 { -1 } else { 1 }]
}

fn corner_vec(signs: [i8; 3]) -> Vec3 {
    Vec3::new(f64::from(signs[0]), f64::from(signs[1]), f64::from(signs[2]))
}

fn closest_on_segment(point: Pos2, a: Pos2, b: Pos2) -> (Pos2, f32, f32) {
    let delta = b - a;
    let length_sq = delta.length_sq();
    let t = if length_sq > 1e-6 { ((point - a).dot(delta) / length_sq).clamp(0.0, 1.0) } else { 0.0 };
    let closest = a + delta * t;
    (closest, closest.distance(point), t)
}

fn point_in_polygon(point: Pos2, polygon: &[Pos2; 4]) -> bool {
    let mut sign = 0.0f32;
    for index in 0..polygon.len() {
        let a = polygon[index];
        let b = polygon[(index + 1) % polygon.len()];
        let cross = (b.x - a.x) * (point.y - a.y) - (b.y - a.y) * (point.x - a.x);
        if cross.abs() <= 1e-3 {
            continue;
        }
        if sign != 0.0 && sign * cross < 0.0 {
            return false;
        }
        sign = cross;
    }
    sign != 0.0
}

fn activate_part(app: &mut CadApp, part: Part) {
    match part {
        Part::Face(face) if face == CubeFace::ZPositive && app.session.camera().is_plan() => {
            let _ = app.run("view.bottom", json!({}));
        }
        Part::Face(face) => {
            let _ = app.run(&format!("view.{}", face.preset()), json!({}));
        }
        Part::Edge(edge) => {
            let direction = edge.direction();
            let _ = app.run("vpoint", json!({"direction": [direction.x, direction.y, direction.z]}));
        }
        Part::Corner(corner) => {
            let direction = corner.direction();
            let _ = app.run("vpoint", json!({"direction": [direction.x, direction.y, direction.z]}));
        }
        Part::Home => {
            let _ = app.run("view.swiso", json!({}));
        }
        Part::CompassN => {
            let _ = app.run("view.back", json!({}));
        }
        Part::CompassS => {
            let _ = app.run("view.front", json!({}));
        }
        Part::CompassE => {
            let _ = app.run("view.right", json!({}));
        }
        Part::CompassW => {
            let _ = app.run("view.left", json!({}));
        }
        Part::Ring => {}
    }
}

/// Paint the camera-oriented ViewCube and handle its pointer input.
/// The canvas paints the WCS pill itself.
pub fn show(app: &mut CadApp, ui: &mut egui::Ui, l: &Layout) {
    let resp = ui.interact(l.bounds, ui.id().with("viewcube"), Sense::click_and_drag());
    let camera = app.session.camera();
    let geometry = CubeGeometry::new(l, camera);
    let part = resp.hover_pos().and_then(|point| l.part_at(point, &geometry));
    let drag_origin_id = ui.id().with("viewcube_drag_origin");
    let (primary_pressed, primary_down, focused, cancelled) = ui.input(|input| {
        (
            input.pointer.button_pressed(egui::PointerButton::Primary),
            input.pointer.primary_down(),
            input.focused,
            input.key_pressed(egui::Key::Escape),
        )
    });
    if primary_pressed
        && resp.hovered()
        && let Some(point) = resp.interact_pointer_pos()
        && let Some(origin) = l.part_at(point, &geometry)
    {
        ui.ctx().data_mut(|data| data.insert_temp(drag_origin_id, origin));
    }
    if !focused || cancelled || resp.drag_stopped() || (!primary_down && !primary_pressed) {
        ui.ctx().data_mut(|data| data.remove::<Part>(drag_origin_id));
    }
    let drag_origin = ui.ctx().data(|data| data.get_temp::<Part>(drag_origin_id));
    let p = ui.painter_at(l.bounds.union(l.pill));
    let ring_col = if part == Some(Part::Ring) { Color32::from_rgb(0x5c, 0x67, 0x7c) } else { Color32::from_rgb(0x48, 0x50, 0x5c) };
    p.circle_stroke(l.center, l.ring, Stroke::new(9.0, ring_col));
    p.circle_stroke(l.center, l.ring + 4.5, Stroke::new(1.0, Color32::from_rgb(0x5c, 0x65, 0x72)));
    let f = egui::FontId::proportional(17.0);
    let lc = Color32::from_rgb(0xc8, 0xcc, 0xd2);
    let l_hot = Color32::from_rgb(0x60, 0xaa, 0xff);
    let c = l.center;
    let ring = l.ring;
    let label_n = Part::CompassN.ring_label().unwrap_or_default();
    let label_s = Part::CompassS.ring_label().unwrap_or_default();
    let label_e = Part::CompassE.ring_label().unwrap_or_default();
    let label_w = Part::CompassW.ring_label().unwrap_or_default();

    p.text(c + vec2(0.0, -ring - 1.0), egui::Align2::CENTER_CENTER, label_n, f.clone(), if part == Some(Part::CompassN) { l_hot } else { lc });
    p.text(c + vec2(0.0, ring + 1.0), egui::Align2::CENTER_CENTER, label_s, f.clone(), if part == Some(Part::CompassS) { l_hot } else { lc });
    p.text(c + vec2(ring + 1.0, 0.0), egui::Align2::CENTER_CENTER, label_e, f.clone(), if part == Some(Part::CompassE) { l_hot } else { lc });
    p.text(c + vec2(-ring - 1.0, 0.0), egui::Align2::CENTER_CENTER, label_w, f, if part == Some(Part::CompassW) { l_hot } else { lc });

    let highlight = Color32::from_rgba_unmultiplied(0x40, 0x90, 0xf0, 0x75);
    for face in &geometry.faces {
        let facing = face.face.normal().dot(camera.view_dir());
        let shade = if facing.is_finite() { (120.0 + facing.clamp(0.0, 1.0) * 56.0).round() as u8 } else { 120 };
        let fill = if part == Some(Part::Face(face.face)) { highlight } else { Color32::from_rgb(shade, shade, shade) };
        p.add(egui::Shape::convex_polygon(face.points.to_vec(), fill, Stroke::new(0.7, Color32::from_rgb(0x35, 0x3a, 0x42))));
        let center = face.points.iter().copied().fold(Pos2::ZERO, |sum, point| sum + point.to_vec2()) / 4.0;
        p.text(center, egui::Align2::CENTER_CENTER, face.face.label(), egui::FontId::proportional(9.5), Color32::from_rgb(0x22, 0x27, 0x30));
    }
    for edge in geometry.edges {
        let midpoint = edge.points[0].lerp(edge.points[1], 0.5);
        if geometry.front_depth_at(midpoint).is_some_and(|depth| (depth - geometry.depth_at_edge(edge.edge, 0.5)).abs() <= 0.02) {
            let stroke =
                if part == Some(Part::Edge(edge.edge)) { Stroke::new(3.0, l_hot) } else { Stroke::new(1.5, Color32::from_rgb(0x28, 0x2d, 0x35)) };
            p.line_segment(edge.points, stroke);
        }
    }
    for corner in geometry.corners {
        if geometry.front_depth_at(corner.point).is_some_and(|depth| (depth - geometry.depth_at_corner(corner.corner)).abs() <= 0.02) {
            let fill = if part == Some(Part::Corner(corner.corner)) { l_hot } else { Color32::from_rgb(0x34, 0x3a, 0x44) };
            p.circle_filled(corner.point, 2.7, fill);
            p.circle_stroke(corner.point, 2.7, Stroke::new(0.7, Color32::from_rgb(0xd0, 0xd4, 0xdc)));
        }
    }

    let view_name = app.session.camera().view_name();
    p.text(c + vec2(0.0, -40.0), egui::Align2::CENTER_CENTER, view_name, egui::FontId::proportional(9.5), Color32::from_rgb(0x30, 0x34, 0x3a));
    paint_home(&p, l.home, part == Some(Part::Home));

    let resp = match part {
        Some(part) => resp.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(part.tip()),
        None => resp,
    };
    let pressed_on = resp.interact_pointer_pos().and_then(|point| l.part_at(point, &geometry));
    if resp.dragged() && drag_origin.is_some_and(|origin| origin != Part::Home) {
        let d = resp.drag_delta();
        let _ = app.session.orbit_step(f64::from(d.x), f64::from(d.y));
    }
    if resp.clicked()
        && let Some(part) = pressed_on
    {
        activate_part(app, part);
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
        frame_with_focus(app, ctx, events, true);
    }

    fn frame_with_focus(app: &mut CadApp, ctx: &egui::Context, events: Vec<egui::Event>, focused: bool) {
        let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1400.0, 900.0))), focused, events, ..Default::default() };
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

    fn camera(preset: &str) -> OrbitCamera {
        let mut camera = OrbitCamera::default();
        camera.set_preset(preset);
        camera
    }

    fn projected_center(face: ProjectedFace) -> Pos2 {
        face.points.iter().copied().fold(Pos2::ZERO, |sum, point| sum + point.to_vec2()) / 4.0
    }

    #[test]
    fn cube_orientation_matches_each_principal_camera_view() {
        let layout = Layout::new(Rect::from_min_size(pos2(200.0, 80.0), vec2(1000.0, 800.0)));
        for (preset, face) in [
            ("top", CubeFace::ZPositive),
            ("bottom", CubeFace::ZNegative),
            ("front", CubeFace::YNegative),
            ("back", CubeFace::YPositive),
            ("left", CubeFace::XNegative),
            ("right", CubeFace::XPositive),
        ] {
            let camera = camera(preset);
            let geometry = CubeGeometry::new(&layout, camera);
            assert_eq!(geometry.faces.len(), 1, "{preset}");
            assert_eq!(geometry.faces[0].face, face, "{preset} visible side");
            assert_eq!(layout.part_at(layout.center, &geometry), Some(Part::Face(face)), "{preset} center hit");

            let center = projected_center(geometry.faces[0]);
            assert!(center.distance(layout.center) < 0.01, "{preset} face center should project to widget center");

            let x_axis = camera.right_dir();
            let y_axis = camera.up_dir();
            let face_corners = geometry.faces[0].points;
            let rightmost = face_corners.iter().copied().max_by(|a, b| a.x.total_cmp(&b.x)).unwrap_or(Pos2::ZERO);
            let leftmost = face_corners.iter().copied().min_by(|a, b| a.x.total_cmp(&b.x)).unwrap_or(Pos2::ZERO);
            let topmost = face_corners.iter().copied().min_by(|a, b| a.y.total_cmp(&b.y)).unwrap_or(Pos2::ZERO);
            assert!(rightmost.x > layout.center.x && leftmost.x < layout.center.x, "{preset} camera-right axis {x_axis:?}");
            assert!(topmost.y < layout.center.y, "{preset} camera-up axis {y_axis:?}");
        }
    }

    #[test]
    fn cube_tracks_isometric_arbitrary_and_orbited_camera_orientations() {
        let layout = Layout::new(Rect::from_min_size(pos2(200.0, 80.0), vec2(1000.0, 800.0)));
        for camera in [camera("swiso"), {
            let mut camera = OrbitCamera::default();
            camera.set_view_dir(Vec3::new(1.0, 2.0, 3.0));
            camera
        }] {
            let geometry = CubeGeometry::new(&layout, camera);
            assert_eq!(geometry.faces.len(), 3);
            assert!(geometry.front_face_at(layout.center).is_some());
            assert!(geometry.faces.iter().all(|face| face.face.normal().dot(camera.view_dir()) > 0.0));
        }
        let initial = camera("swiso");
        let first = CubeGeometry::new(&layout, initial);
        let mut orbited = initial;
        orbited.orbit(75.0, -32.0, (1400.0, 900.0));
        let second = CubeGeometry::new(&layout, orbited);
        assert_ne!(first.faces[0].points, second.faces[0].points, "other camera controls must move the cube");
    }

    #[test]
    fn cube_projection_is_independent_of_camera_target_and_zoom() {
        let layout = Layout::new(Rect::from_min_size(pos2(200.0, 80.0), vec2(1000.0, 800.0)));
        let first = camera("swiso");
        let mut second = first;
        second.target = Vec3::new(500.0, -250.0, 73.0);
        second.height = 0.001;
        assert_eq!(
            CubeGeometry::new(&layout, first).faces.iter().map(|f| f.points).collect::<Vec<_>>(),
            CubeGeometry::new(&layout, second).faces.iter().map(|f| f.points).collect::<Vec<_>>()
        );
    }

    #[test]
    fn hit_testing_only_returns_visible_faces_edges_and_corners() {
        let layout = Layout::new(Rect::from_min_size(pos2(200.0, 80.0), vec2(1000.0, 800.0)));
        let camera = camera("swiso");
        let geometry = CubeGeometry::new(&layout, camera);

        for face in &geometry.faces {
            assert_eq!(geometry.part_at(projected_center(*face)), Some(Part::Face(face.face)), "face hit");
        }
        let visible_corner = geometry
            .corners
            .iter()
            .find(|corner| geometry.front_depth_at(corner.point).is_some_and(|depth| (depth - geometry.depth_at_corner(corner.corner)).abs() <= 0.02))
            .copied()
            .expect("isometric cube has a visible corner");
        assert_eq!(geometry.part_at(visible_corner.point), Some(Part::Corner(visible_corner.corner)));

        let visible_edge = geometry
            .edges
            .iter()
            .find(|edge| {
                let midpoint = edge.points[0].lerp(edge.points[1], 0.5);
                edge.points[0].distance(edge.points[1]) > 10.0
                    && geometry.front_depth_at(midpoint).is_some_and(|depth| (depth - geometry.depth_at_edge(edge.edge, 0.5)).abs() <= 0.02)
                    && geometry.corners.iter().all(|corner| corner.point.distance(midpoint) > 6.5)
            })
            .copied()
            .expect("isometric cube has a visible edge");
        let midpoint = visible_edge.points[0].lerp(visible_edge.points[1], 0.5);
        assert_eq!(geometry.part_at(midpoint), Some(Part::Edge(visible_edge.edge)));

        let hidden_corner = geometry
            .corners
            .iter()
            .find(|corner| geometry.front_depth_at(corner.point).is_none_or(|depth| (depth - geometry.depth_at_corner(corner.corner)).abs() > 0.02))
            .copied()
            .expect("isometric cube has a hidden corner");
        assert_ne!(geometry.part_at(hidden_corner.point), Some(Part::Corner(hidden_corner.corner)));
        for edge in &geometry.edges {
            let midpoint = edge.points[0].lerp(edge.points[1], 0.5);
            if geometry.front_depth_at(midpoint).is_none_or(|depth| (depth - geometry.depth_at_edge(edge.edge, 0.5)).abs() > 0.02) {
                assert_ne!(geometry.part_at(midpoint), Some(Part::Edge(edge.edge)), "occluded edge must not hit");
            }
        }
        assert!(layout.bounds.contains_rect(layout.home));
        assert_eq!(layout.part_at(layout.home.center(), &geometry), Some(Part::Home));
        assert_eq!(layout.part_at(layout.center + vec2(0.0, -layout.ring), &geometry), Some(Part::CompassN));
        assert_eq!(layout.part_at(layout.center + vec2(layout.ring, 0.0), &geometry), Some(Part::CompassE));
    }

    #[test]
    fn pointer_area_for_drawn_cube_is_reserved_from_the_canvas() {
        let canvas = Rect::from_min_size(pos2(100.0, 40.0), vec2(1200.0, 800.0));
        let layout = Layout::new(canvas);
        assert!(Layout::blocks_canvas_pointer(canvas, layout.center));
        assert!(Layout::blocks_canvas_pointer(canvas, layout.pill.center()));
        assert!(!Layout::blocks_canvas_pointer(canvas, layout.center + vec2(0.0, layout.ring + 15.0)));
        assert!(!Layout::blocks_canvas_pointer(canvas, pos2(canvas.left() + 100.0, canvas.top() + 100.0)));
    }

    #[test]
    fn viewcube_clicks_reach_the_cube_not_the_drawing() {
        let (mut app, ctx) = app_and_ctx();
        app.session.set_camera(camera("top"));
        let l = Layout::new(canvas(&app));

        click(&mut app, &ctx, l.center, egui::PointerButton::Primary);
        assert_eq!(app.session.camera().view_name(), "BOTTOM", "the top face toggles to Bottom from Plan");
        assert!(app.session.camera().view_dir().near(Vec3::new(0.0, 0.0, -1.0), 1e-6));
        assert!(app.session.pending_window.is_none());
        assert!(app.session.selection().is_empty());

        click(&mut app, &ctx, l.center, egui::PointerButton::Primary);
        assert_eq!(app.session.camera().view_name(), "BOTTOM", "clicking the bottom face keeps the bottom view");

        for (point, preset, expected) in [
            (l.center + vec2(0.0, l.ring), "front", Vec3::new(0.0, -1.0, 0.0)),
            (l.center + vec2(0.0, -l.ring), "back", Vec3::new(0.0, 1.0, 0.0)),
            (l.center + vec2(-l.ring, 0.0), "left", Vec3::new(-1.0, 0.0, 0.0)),
            (l.center + vec2(l.ring, 0.0), "right", Vec3::new(1.0, 0.0, 0.0)),
        ] {
            click(&mut app, &ctx, point, egui::PointerButton::Primary);
            assert_eq!(app.session.camera().view_name(), preset.to_ascii_uppercase());
            assert!(app.session.camera().view_dir().near(expected, 1e-6), "{preset} compass vector");
        }

        click(&mut app, &ctx, l.home.center(), egui::PointerButton::Primary);
        assert_eq!(app.session.camera().view_name(), "SW ISO", "Home button sets SW ISO view");
        assert!(app.session.pending_window.is_none());
        assert!(app.session.selection().is_empty());

        let orbit_start = l.center + vec2(l.ring, 0.0);
        let orbit_mid = orbit_start + vec2(10.0, 6.0);
        let orbit_end = orbit_mid + vec2(10.0, 6.0);
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
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(orbit_mid)]);
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
        let expected_azimuth = 20.0 * std::f64::consts::PI / app.session.viewport_px.0;
        let expected_elevation = -12.0 * std::f64::consts::PI / app.session.viewport_px.1;
        assert!(((after_orbit.azimuth - before_orbit.azimuth) - expected_azimuth).abs() < 1e-8, "orbit applies incremental X motion once");
        assert!(((after_orbit.elevation - before_orbit.elevation) - expected_elevation).abs() < 1e-8, "orbit applies incremental Y motion once");

        // A click in the drawing still reaches the drawing.
        let empty = canvas(&app).center() + vec2(0.0, -300.0);
        click(&mut app, &ctx, empty, egui::PointerButton::Primary);
        assert!(app.canvas.selection_window.is_some(), "an empty-space click starts a screen-space selection window");
    }

    #[test]
    fn viewcube_does_not_submit_points_to_an_active_line_command() {
        let (mut app, ctx) = app_and_ctx();
        let layout = Layout::new(canvas(&app));
        app.start("line");
        let prompt_before = app.session.current_prompt().map(|prompt| prompt.message);
        assert!(prompt_before.is_some());

        click(&mut app, &ctx, layout.center + vec2(0.0, -layout.ring), egui::PointerButton::Primary);

        assert_eq!(app.session.current_prompt().map(|prompt| prompt.message), prompt_before);
        assert_eq!(app.session.running.as_ref().map(|running| running.id.as_str()), Some("line"));
        assert!(app.session.pending_window.is_none());
        assert!(app.session.selection().is_empty());
    }

    #[test]
    fn faces_edges_and_corners_activate_existing_view_commands() {
        let (mut app, _ctx) = app_and_ctx();
        for (face, expected) in [
            (CubeFace::XPositive, Vec3::new(1.0, 0.0, 0.0)),
            (CubeFace::XNegative, Vec3::new(-1.0, 0.0, 0.0)),
            (CubeFace::YPositive, Vec3::new(0.0, 1.0, 0.0)),
            (CubeFace::YNegative, Vec3::new(0.0, -1.0, 0.0)),
            (CubeFace::ZPositive, Vec3::new(0.0, 0.0, 1.0)),
            (CubeFace::ZNegative, Vec3::new(0.0, 0.0, -1.0)),
        ] {
            activate_part(&mut app, Part::Face(face));
            assert!(app.session.camera().view_dir().near(expected, 1e-6), "{face:?} face");
        }

        activate_part(&mut app, Part::Edge(CubeEdge::AlongX { y: -1, z: 1 }));
        assert!(app.session.camera().view_dir().near(Vec3::new(0.0, -1.0, 1.0).normalized(), 1e-6), "edge direction");
        activate_part(&mut app, Part::Corner(CubeCorner::At(1, -1, 1)));
        assert!(app.session.camera().view_dir().near(Vec3::new(1.0, -1.0, 1.0).normalized(), 1e-6), "corner direction");
    }

    #[test]
    fn cancel_and_focus_loss_stop_viewcube_dragging() {
        let (mut app, ctx) = app_and_ctx();
        let layout = Layout::new(canvas(&app));
        let start = layout.center + vec2(0.0, layout.ring);
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(start)]);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::default(),
            }],
        );
        let before_cancel = app.session.camera();
        frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::default() },
                egui::Event::PointerMoved(start + vec2(15.0, 10.0)),
            ],
        );
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(start + vec2(25.0, 20.0))]);
        assert_eq!(app.session.camera(), before_cancel, "Escape clears the active drag origin");

        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            }],
        );
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(start)]);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::default(),
            }],
        );
        let before_focus_loss = app.session.camera();
        frame_with_focus(&mut app, &ctx, vec![egui::Event::PointerMoved(start + vec2(10.0, 10.0))], false);
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(start + vec2(20.0, 20.0))]);
        assert_eq!(app.session.camera(), before_focus_loss, "lost focus clears the active drag origin");
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

    #[test]
    fn ring_labels_and_tooltips_match_cube_faces_in_english_and_spanish() {
        crate::i18n::with_language("en", || {
            assert_eq!(CubeFace::YPositive.label(), "Back");
            assert_eq!(CubeFace::YNegative.label(), "Front");
            assert_eq!(CubeFace::XPositive.label(), "Right");
            assert_eq!(CubeFace::XNegative.label(), "Left");
            assert_eq!(CubeFace::ZPositive.label(), "Top");
            assert_eq!(CubeFace::ZNegative.label(), "Bottom");

            assert_eq!(Part::CompassN.ring_label(), Some("B".into()));
            assert_eq!(Part::CompassS.ring_label(), Some("F".into()));
            assert_eq!(Part::CompassE.ring_label(), Some("R".into()));
            assert_eq!(Part::CompassW.ring_label(), Some("L".into()));

            assert_eq!(Part::CompassN.tip(), "Back");
            assert_eq!(Part::CompassS.tip(), "Front");
            assert_eq!(Part::CompassE.tip(), "Right");
            assert_eq!(Part::CompassW.tip(), "Left");
        });

        crate::i18n::with_language("es", || {
            assert_eq!(CubeFace::YPositive.label(), "Posterior");
            assert_eq!(CubeFace::YNegative.label(), "Frontal");
            assert_eq!(CubeFace::XPositive.label(), "Derecha");
            assert_eq!(CubeFace::XNegative.label(), "Izquierda");
            assert_eq!(CubeFace::ZPositive.label(), "Superior");
            assert_eq!(CubeFace::ZNegative.label(), "Inferior");

            assert_eq!(Part::CompassN.ring_label(), Some("P".into()));
            assert_eq!(Part::CompassS.ring_label(), Some("F".into()));
            assert_eq!(Part::CompassE.ring_label(), Some("D".into()));
            assert_eq!(Part::CompassW.ring_label(), Some("I".into()));

            assert_eq!(Part::CompassN.tip(), "Posterior");
            assert_eq!(Part::CompassS.tip(), "Frontal");
            assert_eq!(Part::CompassE.tip(), "Derecha");
            assert_eq!(Part::CompassW.tip(), "Izquierda");
        });
    }

    #[test]
    fn ring_controls_correspond_to_correct_views() {
        let (mut app, _ctx) = app_and_ctx();
        activate_part(&mut app, Part::CompassN);
        assert!(app.session.camera().view_dir().near(Vec3::new(0.0, 1.0, 0.0), 1e-6), "CompassN activates Back view");
        activate_part(&mut app, Part::CompassS);
        assert!(app.session.camera().view_dir().near(Vec3::new(0.0, -1.0, 0.0), 1e-6), "CompassS activates Front view");
        activate_part(&mut app, Part::CompassE);
        assert!(app.session.camera().view_dir().near(Vec3::new(1.0, 0.0, 0.0), 1e-6), "CompassE activates Right view");
        activate_part(&mut app, Part::CompassW);
        assert!(app.session.camera().view_dir().near(Vec3::new(-1.0, 0.0, 0.0), 1e-6), "CompassW activates Left view");
    }
}
