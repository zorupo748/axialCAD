//! Intersections for direct TRIM picks and direct LINE extension.
use cadcraft_doc::{Entity, EntityKind};
use cadcraft_geom::{Arc, Circle, Line, Mat4, Polyline, Segment, TAU, Vec2, Vec3, circle_circle, intersect_ext, line_circle, line_line_infinite, norm_angle};

use crate::{EngineError, Result, Session};

use super::{CurvePick, Edges, cutting_entities, implied_edge, prim_to_segments, proj_mode, trim_debug_enabled, trim_trace};

const REL_TOL: f64 = 1e-9;
// Linear contact tolerance in drawing units. Use the local extent of the two
// operands, so neither a far-off origin nor unrelated geometry inflates it.
const CONTACT_REL_TOL: f64 = 2e-5;
const CONTACT_MIN_TOL: f64 = 1e-8;
const CONTACT_MAX_TOL: f64 = 1e-2;

pub(crate) fn contact_tolerance(a: f64, b: f64) -> f64 {
    (CONTACT_REL_TOL * a.abs().max(b.abs())).clamp(CONTACT_MIN_TOL, CONTACT_MAX_TOL)
}

fn degenerate() -> EngineError {
    EngineError::Other("Degenerate projection: the object has no usable geometry in the selected projection.".into())
}

#[derive(Clone, Copy)]
struct SpatialLine {
    a: Vec3,
    b: Vec3,
    before: bool,
    after: bool,
}

impl SpatialLine {
    fn finite(a: Vec3, b: Vec3) -> Self {
        Self { a, b, before: false, after: false }
    }

    fn contains(self, t: f64, tol: f64) -> bool {
        t.is_finite() && (self.before || t >= -tol) && (self.after || t <= 1.0 + tol)
    }

    fn param(self, p: Vec3) -> f64 {
        let d = self.b - self.a;
        (p - self.a).dot(d.normalized()) / d.len()
    }
}

#[derive(Clone, Copy)]
struct Round {
    center: Vec3,
    radius: f64,
    x: Vec3,
    y: Vec3,
    normal: Vec3,
    sweep: Option<(f64, f64)>,
}

impl Round {
    fn new(center: Vec3, radius: f64, normal: Vec3, sweep: Option<(f64, f64)>) -> Self {
        let (x, y, normal) = Mat4::ocs_basis(normal);
        Self { center, radius, x, y, normal, sweep }
    }

    fn local(self, p: Vec3) -> Vec2 {
        let p = p - self.center;
        Vec2::new(p.dot(self.x), p.dot(self.y))
    }

    fn point(self, p: Vec2) -> Vec3 {
        self.center + self.x * p.x + self.y * p.y
    }

    fn contains(self, p: Vec3, tol: f64) -> bool {
        let q = self.local(p);
        if !p.is_finite()
            || ((p - self.center).dot(self.normal)).abs() > tol
            || (q.len() - self.radius).abs() > tol
        {
            return false;
        }
        let Some((a, b)) = self.sweep else { return true };
        let ang_tol = (tol / self.radius.max(1e-9)).clamp(1e-9, 1e-2);
        let sweep = cadcraft_geom::ccw_sweep(a, b);
        let d = norm_angle(q.angle() - a);
        d <= sweep + ang_tol || (TAU - d) <= ang_tol
    }

    fn intersect_line(self, line: SpatialLine) -> Vec<Vec3> {
        let direction = line.b - line.a;
        let length = direction.len();
        if !length.is_finite() || length <= 1e-12 || !self.radius.is_finite() {
            return Vec::new();
        }
        let tol = contact_tolerance(length, self.radius);
        let ta = tol / length;
        let height = (line.a - self.center).dot(self.normal);
        let rise = direction.dot(self.normal);
        if height.abs() <= tol && (height + rise).abs() <= tol {
            let local = Line::new(self.local(line.a), self.local(line.b));
            let hits: Vec<_> = line_circle(&local, &Circle::new(Vec2::ZERO, self.radius))
                .into_iter()
                .filter(|&(_, t)| line.contains(t, ta))
                .map(|(_, t)| line.a + direction * t)
                .filter(|&p| self.contains(p, tol))
                .collect();
            if !hits.is_empty() || (!line.before && !line.after) {
                return hits;
            }
        }
        if rise == 0.0 {
            return Vec::new();
        }
        let t = -height / rise;
        let p = line.a + direction * t;
        (line.contains(t, ta) && self.contains(p, tol)).then_some(p).into_iter().collect()
    }

    fn intersect_round(self, other: Self) -> Vec<Vec3> {
        let tol = contact_tolerance(self.radius, other.radius);
        let axis = self.normal.cross(other.normal);
        let offset = (other.center - self.center).dot(other.normal);
        let points: Vec<_> = if axis.len() <= REL_TOL {
            if offset.abs() > tol {
                return Vec::new();
            }
            circle_circle(&Circle::new(Vec2::ZERO, self.radius), &Circle::new(self.local(other.center), other.radius))
                .into_iter().map(|p| self.point(p)).collect()
        } else {
            // Use the point on the planes' intersection closest to the target center.
            let across = axis.cross(self.normal).normalized();
            let a = self.center + across * (offset / across.dot(other.normal));
            let b = a + axis.normalized() * self.radius;
            line_circle(&Line::new(self.local(a), self.local(b)), &Circle::new(Vec2::ZERO, self.radius))
                .into_iter().map(|(p, _)| self.point(p)).collect()
        };
        points.into_iter().filter(|&p| self.contains(p, tol) && other.contains(p, tol)).collect()
    }
}

enum Cutter {
    Line(SpatialLine),
    Round(Round),
}

fn round_cutter(e: &Entity, implied: bool) -> Option<Round> {
    let normal = e.common.extrusion;
    match &e.kind {
        EntityKind::Circle(c) => Some(Round::new(c.center, c.radius, normal, None)),
        EntityKind::Arc(a) => Some(Round::new(a.center, a.radius, normal, (!implied).then_some((a.start, a.end)))),
        _ => None,
    }
}

fn physical_cutters(s: &Session, edges: Edges, entity: &Entity) -> Result<Vec<Cutter>> {
    let mut out = Vec::new();
    for e in cutting_entities(s, edges, entity.handle)? {
        if let Some(round) = round_cutter(e, edges.implied) {
            out.push(Cutter::Round(round));
            continue;
        }
        match &e.kind {
            EntityKind::Line(l) => out.push(Cutter::Line(SpatialLine { a: l.a, b: l.b, before: edges.implied, after: edges.implied })),
            EntityKind::Ray(l) | EntityKind::XLine(l) => out.push(Cutter::Line(SpatialLine {
                a: l.base, b: l.base + l.dir, before: matches!(e.kind, EntityKind::XLine(_)), after: true,
            })),
            EntityKind::Polyline3d(p) => {
                out.extend(p.path_3d().windows(2).filter_map(|pair| {
                    let [a, b] = pair else { return None };
                    Some(Cutter::Line(SpatialLine::finite(*a, *b)))
                }));
            }
            EntityKind::LwPolyline(p) => {
                let normal = e.common.extrusion;
                let lift = |q: Vec2| Mat4::ocs_to_wcs(normal, q.to3(p.elevation));
                let segments = Polyline { vertices: p.vertices.clone(), closed: p.closed }.segments();
                let count = segments.len();
                for (i, segment) in segments.into_iter().enumerate() {
                    let before = edges.implied && !p.closed && i == 0;
                    let after = edges.implied && !p.closed && i + 1 == count;
                    out.push(match segment {
                        Segment::Line(l) => Cutter::Line(SpatialLine { a: lift(l.a), b: lift(l.b), before, after }),
                        Segment::Arc { arc, .. } => Cutter::Round(Round::new(lift(arc.center), arc.radius, normal,
                            if before || after { None } else { Some((arc.start, arc.end)) })),
                    });
                }
            }
            // Reuse the planar cutter approximation for these curves only when their
            // geometry provides a known horizontal WCS plane.
            EntityKind::Ellipse(ellipse)
                if e.common.extrusion.near(Vec3::Z, REL_TOL) && ellipse.major.z == 0.0 =>
            {
                planar_curve_cutters(e, ellipse.center.z, &mut out);
            }
            EntityKind::Spline(_) if e.common.extrusion.near(Vec3::Z, REL_TOL) => {
                // The document spline representation stores points in WCS XY at Z=0.
                planar_curve_cutters(e, 0.0, &mut out);
            }
            _ => {}
        }
    }
    Ok(out)
}

fn planar_curve_cutters(entity: &Entity, z: f64, out: &mut Vec<Cutter>) {
    out.extend(entity.kind.prims().iter().flat_map(prim_to_segments).filter_map(|segment| {
        let Segment::Line(line) = segment else { return None };
        Some(Cutter::Line(SpatialLine::finite(line.a.to3(z), line.b.to3(z))))
    }));
}

fn line_contact(target: SpatialLine, cutter: SpatialLine) -> Option<f64> {
    let a = target.b - target.a;
    let b = cutter.b - cutter.a;
    let (la, lb) = (a.len(), b.len());
    if !la.is_finite() || !lb.is_finite() || la <= 1e-12 || lb <= 1e-12 {
        return None;
    }
    let (u, v) = (a / la, b / lb);
    let cross = u.cross(v);
    let denom = cross.dot(cross);
    if denom <= REL_TOL * REL_TOL {
        return None;
    }
    let offset = cutter.a - target.a;
    let (t, w) = (offset.cross(v).dot(cross) / (denom * la), offset.cross(u).dot(cross) / (denom * lb));
    let tol = contact_tolerance(la, lb);
    (target.contains(t, tol / la) && cutter.contains(w, tol / lb) && (a * t - offset - b * w).len() <= tol).then_some(t)
}

fn projection(s: &Session, mode: i64) -> impl Fn(Vec3) -> Vec2 {
    let (x, y) = if mode == 1 {
        let ucs = s.ucs();
        (ucs.x_axis, ucs.y_axis)
    } else {
        let camera = s.camera();
        (camera.right_dir(), camera.up_dir())
    };
    move |v| Vec2::new(v.dot(x), v.dot(y))
}

/// For JSON and typed picks, a 2D point is in WCS XY. A vertical line needs its WCS Z explicitly.
fn line_pick(line: SpatialLine, pick: CurvePick) -> Result<Vec3> {
    if let Some(world) = pick.world {
        return Ok(world);
    }
    let xy = Line::new(line.a.xy(), line.b.xy());
    if xy.a.dist(xy.b) <= 1e-12 {
        return Err(EngineError::Other("Specify a 3D pick [x,y,z] for a vertical line.".into()));
    }
    Ok(line.a + (line.b - line.a) * xy.param_of(pick.xy))
}

pub(super) fn line_hits(s: &Session, entity: &Entity, line: &cadcraft_doc::Line, pick: CurvePick, edges: Edges, extend: bool) -> Result<(f64, Vec<f64>)> {
    let mut target = SpatialLine::finite(line.a, line.b);
    let length = (line.b - line.a).len();
    if !length.is_finite() || length <= 1e-12 {
        return Err(degenerate());
    }
    let mode = proj_mode(s);
    let vector = projection(s, mode);
    let project = |p| vector(p - line.a);
    let projected = Line::new(Vec2::ZERO, vector(line.b - line.a));
    let projected_len = projected.b.len();
    if mode != 0 && (!projected_len.is_finite() || projected_len <= (length * REL_TOL).max(1e-12)) {
        return Err(degenerate());
    }
    let world = line_pick(target, pick)?;
    let tp = if mode == 0 { target.param(world) } else { projected.param_of(project(world)) };
    if !tp.is_finite() {
        return Err(degenerate());
    }
    target.before = extend && tp <= 0.5;
    target.after = extend && tp > 0.5;

    let hits = if mode == 0 {
        physical_cutters(s, edges, entity)?.into_iter().flat_map(|c| match c {
            Cutter::Line(line) => line_contact(target, line).into_iter().collect(),
            Cutter::Round(round) => round.intersect_line(target).into_iter().map(|p| target.param(p)).collect::<Vec<_>>(),
        }).collect()
    } else {
        projected_cutters(s, edges, entity, &project, &vector)?.iter().flat_map(|c| match c {
            Cut2::Line(l, before, after) => line_line_infinite(projected.a, projected.b, l.a, l.b)
                .filter(|&(_, _, u)| (*before || u >= -REL_TOL) && (*after || u <= 1.0 + REL_TOL))
                .map(|(_, t, _)| t).into_iter().collect(),
            Cut2::Arc(arc) => line_circle(&projected, &Circle::new(arc.center, arc.radius)).into_iter()
                .filter(|(p, _)| arc.contains_angle(arc.center.angle_to(*p))).map(|(_, t)| t).collect::<Vec<_>>(),
        }).collect()
    };
    Ok((tp, hits))
}

enum Cut2 {
    Line(Line, bool, bool),
    Arc(Arc),
}

fn projected_cutters(
    s: &Session,
    edges: Edges,
    target: &Entity,
    project: &impl Fn(Vec3) -> Vec2,
    vector: &impl Fn(Vec3) -> Vec2,
) -> Result<Vec<Cut2>> {
    let mut out = Vec::new();
    for e in cutting_entities(s, edges, target.handle)? {
        if let EntityKind::Line(l) = &e.kind {
            let (a, b) = (project(l.a), project(l.b));
            if a.is_finite() && b.is_finite() && a.dist(b) > 1e-12 {
                out.push(Cut2::Line(Line::new(a, b), edges.implied, edges.implied));
            }
        } else if let EntityKind::Polyline3d(p) = &e.kind {
            for pair in p.path_3d().windows(2) {
                let [a, b] = pair else { continue };
                let (a, b) = (project(*a), project(*b));
                if a.is_finite() && b.is_finite() && a.dist(b) > 1e-12 {
                    out.push(Cut2::Line(Line::new(a, b), false, false));
                }
            }
        } else if let Some(round) = round_cutter(e, edges.implied) {
            let center = project(round.center);
            let x = vector(round.x * round.radius);
            let y = vector(round.y * round.radius);
            let (start, end) = round.sweep.unwrap_or((0.0, TAU));
            if x.len() > 1e-12 && (x.len() - y.len()).abs() <= REL_TOL * x.len() && x.dot(y).abs() <= REL_TOL * x.len2() {
                let angle = |t: f64| (x * t.cos() + y * t.sin()).angle();
                let (a, b) = if x.cross(y) >= 0.0 { (angle(start), angle(end)) } else { (angle(end), angle(start)) };
                let arc = if round.sweep.is_none() { Arc::new(center, x.len(), 0.0, 0.0) } else { Arc::new(center, x.len(), a, b) };
                out.push(Cut2::Arc(arc));
            } else {
                // A circle in a different plane projects to an ellipse. Approximate only that
                // cutter, keeping the target curve and its parameters exact.
                let point = |t: f64| center + x * t.cos() + y * t.sin();
                let sweep = round.sweep.map_or(TAU, |(a, b)| cadcraft_geom::ccw_sweep(a, b));
                for i in 0..256 {
                    let a = point(start + sweep * f64::from(i) / 256.0);
                    let b = point(start + sweep * f64::from(i + 1) / 256.0);
                    if a.is_finite() && b.is_finite() && a.dist(b) > 1e-12 {
                        out.push(Cut2::Line(Line::new(a, b), false, false));
                    }
                }
            }
        } else {
            // Lightweight polyline segments are stored in OCS. Other cutter types
            // retain their legacy planar representation in projected modes.
            let transform = match &e.kind {
                EntityKind::LwPolyline(p) => Some((Mat4::ocs(e.common.extrusion), p.elevation)),
                _ => None,
            };
            let project_point = |p: Vec2| project(transform.map_or_else(|| p.to3(0.0), |(ocs, z)| ocs.apply(p.to3(z))));
            let segments = if edges.implied { implied_edge(&e.kind) } else { None }
                .unwrap_or_else(|| e.kind.prims().iter().flat_map(prim_to_segments).collect());
            for segment in segments {
                let count = if matches!(segment, Segment::Line(_)) { 1 } else { 128 };
                for i in 0..count {
                    let a = project_point(segment.at(f64::from(i) / f64::from(count)));
                    let b = project_point(segment.at(f64::from(i + 1) / f64::from(count)));
                    if a.is_finite() && b.is_finite() && a.dist(b) > 1e-12 {
                        out.push(Cut2::Line(Line::new(a, b), false, false));
                    }
                }
            }
        }
    }
    Ok(out)
}

pub(super) fn round_hits(s: &Session, entity: &Entity, center: Vec3, radius: f64, pick: CurvePick, edges: Edges) -> Result<(Vec2, Vec<f64>)> {
    let mode = proj_mode(s);
    let debug = trim_debug_enabled();
    let world = pick.world.unwrap_or_else(|| pick.xy.to3(center.z));
    let local_pick = (world - center).xy();
    if mode == 0 {
        let round = Round::new(center, radius, entity.common.extrusion, None);
        let cutters = physical_cutters(s, edges, entity)?;
        let count = cutters.len();
        let hits: Vec<_> = cutters
            .into_iter()
            .enumerate()
            .flat_map(|(i, c)| match c {
                Cutter::Line(line) => {
                    let contacts = round.intersect_line(line);
                    if debug && i < 8 {
                        let tolerance = contact_tolerance((line.b - line.a).len(), radius);
                        trim_trace(format_args!(
                            "round_cutter index={i} a={:?} b={:?} tolerance={tolerance} contacts={}",
                            line.a,
                            line.b,
                            contacts.len()
                        ));
                    }
                    contacts
                }
                Cutter::Round(other) => round.intersect_round(other),
            })
            .map(|p| norm_angle(round.local(p).angle()))
            .collect();
        if debug {
            trim_trace(format_args!("round_hits projmode=0 center={center:?} radius={radius} cutters={count} hits={}", hits.len()));
        }
        return Ok((local_pick, hits));
    }

    let project = projection(s, mode);
    let x = project(Vec3::X);
    let y = project(Vec3::Y);
    let det = x.cross(y);
    if !det.is_finite() || det.abs() <= REL_TOL {
        return Err(degenerate());
    }
    // Undo the target plane's affine projection so its circle stays circular in the solver.
    let vector = |v| {
        let q = project(v);
        Vec2::new(q.cross(y) / det, x.cross(q) / det)
    };
    let flatten = |p| vector(p - center);
    let circle = Circle::new(Vec2::ZERO, radius);
    let full = Segment::Arc { arc: Arc::new(Vec2::ZERO, radius, 0.0, 0.0), ccw: true };
    let hits: Vec<_> = projected_cutters(s, edges, entity, &flatten, &vector)?
        .iter()
        .flat_map(|c| match c {
            Cut2::Line(l, before, after) => line_circle(l, &circle)
                .into_iter()
                .filter(|&(_, t)| (*before || t >= -REL_TOL) && (*after || t <= 1.0 + REL_TOL))
                .map(|(p, _)| p)
                .collect(),
            Cut2::Arc(arc) => intersect_ext(&full, &Segment::Arc { arc: *arc, ccw: true }, false),
        })
        .map(|p| norm_angle(p.angle()))
        .collect();
    if debug {
        trim_trace(format_args!("round_hits projmode={mode} center={center:?} radius={radius} hits={}", hits.len()));
    }
    Ok((flatten(world), hits))
}
