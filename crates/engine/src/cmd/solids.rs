//! 3D solid modeling and inquiry commands (Milestone 2B).
//!
//! Exposes structured non-interactive JSON commands and interactive modeling / STL workflows:
//! - `solid.box`, `cylinder` / `solid.cylinder`, `sphere` / `solid.sphere`
//! - `extrude`, `dmextrude` (planar closed profiles + exact height), `solid.extrude`
//! - `solid.translate`, `solid.union`, `solid.subtract`, `solid.intersect`
//! - `query.solid`, `query.metrics`
//! - `stlout`, `export.stl` (validated binary STL in millimeters)

use cadcraft_brep::{BrepSolid, MIN_SAFE_MESH_TOLERANCE, TriangleMesh};
use cadcraft_doc::{BooleanKind, Bounds3, Drawing, EntityKind, FeatureOp, Handle, Recipe, Solid3d, Solid3dStatus};
use cadcraft_geom::{Mat4, Polyline, Vec2, Vec3};
use serde_json::{Value, json};

use super::bad;
use super::machines::{SelOutcome, SelectPhase};
use super::{CommandSpec, Result, Session};
use crate::units::{InsUnitInfo, insunits_info, insunits_name, parse_insunits_choice};
use crate::{Accept, Input, Interactive, Prompt, Step};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("solid.box", "Box", run_box).params("{origin?: [x,y,z], size: [dx,dy,dz]}"),
        CommandSpec::new("cylinder", "Cylinder", run_cylinder)
            .menu(&["Draw", "Modeling", "Solids", "Cylinder"])
            .alias(&["cyl"])
            .params("{center?: [x,y,z], radius: f64, height?: f64, axis?: [x,y,z], axis_endpoint?: [x,y,z]}")
            .interactive(|_| Ok(Box::new(CylinderM::default()))),
        CommandSpec::new("solid.cylinder", "Cylinder", run_cylinder)
            .params("{center?: [x,y,z], radius: f64, height?: f64, axis?: [x,y,z], axis_endpoint?: [x,y,z]}"),
        CommandSpec::new("sphere", "Sphere", run_sphere)
            .menu(&["Draw", "Modeling", "Solids", "Sphere"])
            .alias(&["sph"])
            .params("{center?: [x,y,z], radius: f64}")
            .interactive(|_| Ok(Box::new(SphereM::default()))),
        CommandSpec::new("solid.sphere", "Sphere", run_sphere).params("{center?: [x,y,z], radius: f64}"),
        CommandSpec::new("extrude", "Extrude", run_extrude_multi)
            .menu(&["Draw", "Modeling", "Solids", "Extrude"])
            .alias(&["ext"])
            .params("{profile?: string|u64, handles?: [string|u64], height?: f64, direction?: [dx,dy,dz], delete_profile?: bool}")
            .interactive(|_| Ok(Box::new(ExtrudeM::new("EXTRUDE")))),
        CommandSpec::new("dmextrude", "Extrude (Planar)", run_dmextrude)
            .params("{profile?: string|u64, handles?: [string|u64], height?: f64, direction?: [dx,dy,dz], delete_profile?: bool} — planar closed profiles and exact height only")
            .interactive(|_| Ok(Box::new(ExtrudeM::new("DMEXTRUDE")))),
        CommandSpec::new("solid.extrude", "Extrude Profile", run_extrude).params("{profile: string|u64, height?: f64, direction?: [dx,dy,dz]}"),
        CommandSpec::new("solid.translate", "Translate Solid", run_translate).params("{handle: string|u64, delta: [dx,dy,dz]}"),
        CommandSpec::new("solid.union", "Union Solids", run_union)
            .params("{target?: string|u64, tool?: string|u64, handles?: [string|u64], delete_operands?: bool}"),
        CommandSpec::new("solid.subtract", "Subtract Solids", run_subtract)
            .params("{target: string|u64, tool: string|u64, delete_operands?: bool, oversized?: bool}"),
        CommandSpec::new("solid.intersect", "Intersect Solids", run_intersect)
            .params("{target?: string|u64, tool?: string|u64, handles?: [string|u64], delete_operands?: bool}"),
        CommandSpec::new("query.solid", "Query Solid", run_query_solid).params("{handle: string|u64}").noundo(),
        CommandSpec::new("query.metrics", "Query Metrics", run_query_metrics).params("{handle: string|u64}").noundo(),
        CommandSpec::new("stlout", "Export STL...", run_stlout)
            .menu(&["File", "Export STL..."])
            .params("{handles?: [string|u64], handle?: string|u64, path: string, unit?: string|i64, save_units?: bool, tolerance_mm?: f64, tolerance?: f64}")
            .interactive(|_| Ok(Box::new(StlOutM::new())))
            .noundo(),
        CommandSpec::new("export.stl", "Export STL", run_export_stl)
            .params("{handles: [string], path: string, unit?: string|i64, save_units?: bool, tolerance_mm?: f64, tolerance?: f64}")
            .noundo(),
    ]
}

fn parse_vec3(v: &Value) -> Option<Vec3> {
    if let Some(arr) = v.as_array() {
        if arr.len() >= 3 {
            let x = arr.first()?.as_f64()?;
            let y = arr.get(1)?.as_f64()?;
            let z = arr.get(2)?.as_f64()?;
            if x.is_finite() && y.is_finite() && z.is_finite() {
                return Some(Vec3::new(x, y, z));
            }
        }
    } else if let Some(obj) = v.as_object() {
        let x = obj.get("x")?.as_f64()?;
        let y = obj.get("y")?.as_f64()?;
        let z = obj.get("z")?.as_f64()?;
        if x.is_finite() && y.is_finite() && z.is_finite() {
            return Some(Vec3::new(x, y, z));
        }
    }
    None
}

fn parse_handle(v: &Value) -> Option<Handle> {
    if let Some(s) = v.as_str() { Handle::parse_hex(s) } else { v.as_u64().map(Handle) }
}

fn bbox_to_json(b: &Bounds3) -> Value {
    json!({
        "min": [b.min.x, b.min.y, b.min.z],
        "max": [b.max.x, b.max.y, b.max.z]
    })
}

fn run_box(s: &mut Session, p: &Value) -> Result<Value> {
    let origin = p.get("origin").and_then(parse_vec3).unwrap_or(Vec3::ZERO);
    let size = p
        .get("size")
        .and_then(parse_vec3)
        .or_else(|| {
            let dx = p.get("width")?.as_f64()?;
            let dy = p.get("depth").or_else(|| p.get("length"))?.as_f64()?;
            let dz = p.get("height")?.as_f64()?;
            if dx.is_finite() && dy.is_finite() && dz.is_finite() { Some(Vec3::new(dx, dy, dz)) } else { None }
        })
        .ok_or_else(|| bad("solid.box", "`size` [dx, dy, dz] is required"))?;

    if size.x <= 0.0 || size.y <= 0.0 || size.z <= 0.0 {
        return Err(bad("solid.box", "box dimensions must be strictly positive"));
    }

    let mut recipe = Recipe::new();
    recipe.add_feature(1, FeatureOp::Box { origin, size });

    let evaluated = BrepSolid::evaluate_recipe(&recipe).map_err(|e| bad("solid.box", format!("evaluation failed: {e}")))?;
    let bbox = evaluated.bounding_box();
    let faces = evaluated.to_face3d_mesh(0.05).unwrap_or_default();

    let solid = Solid3d::with_faces(recipe, bbox, faces);
    let handle = s.add_entity(EntityKind::Solid3d(solid))?;

    Ok(json!({
        "status": "ok",
        "handle": handle.hex(),
        "bbox": bbox_to_json(&bbox)
    }))
}

fn run_cylinder(s: &mut Session, p: &Value) -> Result<Value> {
    let center = p.get("center").and_then(parse_vec3).unwrap_or(Vec3::ZERO);
    let radius = p
        .get("radius")
        .and_then(Value::as_f64)
        .filter(|r| r.is_finite() && *r > 0.0)
        .ok_or_else(|| bad("cylinder", "`radius` (positive number) is required"))?;

    let (height, axis) = if let Some(end) = p.get("axis_endpoint").and_then(parse_vec3) {
        let delta = end - center;
        let len = delta.len();
        if len <= 1e-12 {
            return Err(bad("cylinder", "`axis_endpoint` cannot coincide with base center"));
        }
        (len, delta.normalized())
    } else {
        let h = p
            .get("height")
            .and_then(Value::as_f64)
            .filter(|h| h.is_finite() && *h > 0.0)
            .ok_or_else(|| bad("cylinder", "`height` (positive number) is required"))?;
        let a = p.get("axis").and_then(parse_vec3).unwrap_or(Vec3::new(0.0, 0.0, 1.0));
        let len_sq = a.x * a.x + a.y * a.y + a.z * a.z;
        if len_sq <= 1e-12 {
            return Err(bad("cylinder", "`axis` vector cannot be zero-length"));
        }
        (h, a.normalized())
    };

    let handle = create_and_add_cylinder(s, center, radius, height, axis)?;
    let ent = s.state()?.doc.entity(handle).cloned().ok_or_else(|| bad("cylinder", "created entity not found"))?;
    let bbox = match &ent.kind {
        EntityKind::Solid3d(sol) => sol.bbox,
        _ => Bounds3::EMPTY,
    };

    Ok(json!({
        "status": "ok",
        "handle": handle.hex(),
        "bbox": bbox_to_json(&bbox)
    }))
}

fn run_sphere(s: &mut Session, p: &Value) -> Result<Value> {
    let center = p.get("center").and_then(parse_vec3).unwrap_or(Vec3::ZERO);
    let radius = p
        .get("radius")
        .and_then(Value::as_f64)
        .filter(|r| r.is_finite() && *r > 0.0)
        .ok_or_else(|| bad("sphere", "`radius` (positive number) is required"))?;

    let handle = create_and_add_sphere(s, center, radius)?;
    let ent = s.state()?.doc.entity(handle).cloned().ok_or_else(|| bad("sphere", "created entity not found"))?;
    let bbox = match &ent.kind {
        EntityKind::Solid3d(sol) => sol.bbox,
        _ => Bounds3::EMPTY,
    };

    Ok(json!({
        "status": "ok",
        "handle": handle.hex(),
        "bbox": bbox_to_json(&bbox)
    }))
}

fn create_and_add_cylinder(s: &mut Session, center: Vec3, radius: f64, height: f64, axis: Vec3) -> Result<Handle> {
    let mut recipe = Recipe::new();
    recipe.add_feature(1, FeatureOp::Cylinder { center, radius, height, axis });

    let evaluated = BrepSolid::evaluate_recipe(&recipe).map_err(|e| bad("cylinder", format!("evaluation failed: {e}")))?;
    let bbox = evaluated.bounding_box();
    let faces = cadcraft_doc::solid3d::cylinder_faces(center, radius, height, axis, 32);

    let solid = Solid3d::with_faces(recipe, bbox, faces);
    s.add_entity(EntityKind::Solid3d(solid))
}

fn create_and_add_sphere(s: &mut Session, center: Vec3, radius: f64) -> Result<Handle> {
    let mut recipe = Recipe::new();
    recipe.add_feature(1, FeatureOp::Sphere { center, radius });

    let r_vec = Vec3::new(radius, radius, radius);
    let bbox = Bounds3::new(center - r_vec, center + r_vec);
    let faces = cadcraft_doc::solid3d::sphere_faces(center, radius, 16, 32);

    let solid = Solid3d::with_faces(recipe, bbox, faces);
    s.add_entity(EntityKind::Solid3d(solid))
}

#[derive(Clone, Debug)]
enum ExtrudeProfile {
    Polygon {
        points: Vec<Vec3>,
        plane_normal: Vec3,
    },
    Circle {
        center: Vec3,
        radius: f64,
        plane_normal: Vec3,
    },
}

impl ExtrudeProfile {
    fn plane_normal(&self) -> Vec3 {
        match self {
            ExtrudeProfile::Polygon { plane_normal, .. } | ExtrudeProfile::Circle { plane_normal, .. } => *plane_normal,
        }
    }

    fn reference_point(&self) -> Vec3 {
        match self {
            ExtrudeProfile::Polygon { points, .. } => points.first().copied().unwrap_or(Vec3::ZERO),
            ExtrudeProfile::Circle { center, .. } => *center,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum DirectionSpec {
    Height(f64),
    Vector(Vec3),
}

fn newell_normal_3d(pts: &[Vec3]) -> Option<Vec3> {
    let n = pts.len();
    if n < 3 {
        return None;
    }
    let mut normal = Vec3::ZERO;
    for i in 0..n {
        let p0 = *pts.get(i)?;
        let p1 = *pts.get((i + 1) % n)?;
        normal.x += (p0.y - p1.y) * (p0.z + p1.z);
        normal.y += (p0.z - p1.z) * (p0.x + p1.x);
        normal.z += (p0.x - p1.x) * (p0.y + p1.y);
    }
    (normal.len() > 1e-9).then(|| normal.normalized())
}

fn clean_2d_closed_ring(raw: &[Vec2], cmd: &'static str) -> Result<Vec<Vec2>> {
    let mut distinct: Vec<Vec2> = Vec::new();
    for p in raw {
        if !p.x.is_finite() || !p.y.is_finite() {
            return Err(bad(cmd, "polyline vertex coordinates must be finite"));
        }
        if let Some(last) = distinct.last()
            && (last.x - p.x).abs() <= 1e-9
            && (last.y - p.y).abs() <= 1e-9
        {
            continue;
        }
        distinct.push(*p);
    }
    if distinct.len() > 1
        && let (Some(first), Some(last)) = (distinct.first(), distinct.last())
        && (first.x - last.x).abs() <= 1e-9
        && (first.y - last.y).abs() <= 1e-9
    {
        distinct.pop();
    }
    if distinct.len() < 3 {
        return Err(bad(cmd, "profile polyline must have at least 3 distinct vertices"));
    }
    if let Err(err_msg) = cadcraft_geom::validate_simple_polygon(&distinct) {
        return Err(bad(cmd, format!("cannot extrude self-intersecting polyline: {err_msg}")));
    }
    if cadcraft_geom::shoelace(&distinct).abs() <= 1e-9 {
        return Err(bad(cmd, "profile polyline has degenerate zero area"));
    }
    Ok(distinct)
}

fn extract_extrude_profile(s: &Session, profile_handle: Handle, cmd: &'static str) -> Result<ExtrudeProfile> {
    let doc = s.doc()?;
    let entity =
        doc.entity(profile_handle).ok_or_else(|| bad(cmd, format!("profile entity with handle {} not found", profile_handle.hex())))?;
    let extrusion = if entity.common.extrusion.is_finite() && entity.common.extrusion.len() > 1e-9 {
        entity.common.extrusion.normalized()
    } else {
        Vec3::Z
    };

    match &entity.kind {
        EntityKind::LwPolyline(poly) => {
            if !poly.closed {
                return Err(bad(cmd, "profile polyline must be closed"));
            }
            if !poly.elevation.is_finite() {
                return Err(bad(cmd, "profile elevation must be finite"));
            }
            if poly.vertices.iter().any(|v| !v.p.x.is_finite() || !v.p.y.is_finite() || !v.bulge.is_finite()) {
                return Err(bad(cmd, "polyline vertex coordinates must be finite"));
            }
            let raw_2d: Vec<Vec2> = if poly.vertices.iter().any(|v| v.bulge.abs() > 1e-9) {
                let geom_poly = Polyline { vertices: poly.vertices.clone(), closed: true };
                let b = geom_poly.bounds();
                let diag = if b.is_empty() { 10.0 } else { b.min.dist(b.max) };
                let tol = (diag * 1e-3).clamp(1e-4, 0.01);
                geom_poly.tessellate(tol)
            } else {
                poly.vertices.iter().map(|v| v.p).collect()
            };
            let distinct = clean_2d_closed_ring(&raw_2d, cmd)?;
            let ocs = Mat4::ocs(extrusion);
            let points = distinct.into_iter().map(|p| ocs.apply(Vec3::new(p.x, p.y, poly.elevation))).collect();
            Ok(ExtrudeProfile::Polygon { points, plane_normal: extrusion })
        }
        EntityKind::Polyline3d(p3) => {
            if !p3.closed {
                return Err(bad(cmd, "3D polyline profile must be closed"));
            }
            let mut distinct = Vec::new();
            for p in &p3.points {
                if !p.is_finite() {
                    return Err(bad(cmd, "3D polyline vertex coordinates must be finite"));
                }
                if distinct.last().is_some_and(|l: &Vec3| l.near(*p, 1e-9)) {
                    continue;
                }
                distinct.push(*p);
            }
            if distinct.len() > 1
                && let (Some(first), Some(last)) = (distinct.first(), distinct.last())
                && first.near(*last, 1e-9)
            {
                distinct.pop();
            }
            if distinct.len() < 3 {
                return Err(bad(cmd, "3D polyline profile must have at least 3 distinct vertices"));
            }
            let Some(mut normal) = newell_normal_3d(&distinct) else {
                return Err(bad(cmd, "3D polyline profile has degenerate zero area"));
            };
            let p_base = distinct.first().copied().unwrap_or(Vec3::ZERO);
            for (i, p) in distinct.iter().enumerate() {
                let dist = (*p - p_base).dot(normal).abs();
                if dist > 1e-4 {
                    return Err(bad(cmd, format!("3D polyline vertex {i} is not coplanar (deviation {dist:.6})")));
                }
            }
            // Align default height direction with active UCS Z when non-orthogonal.
            if normal.dot(s.ucs().z_axis) < -1e-6 {
                normal = -normal;
            }
            Ok(ExtrudeProfile::Polygon { points: distinct, plane_normal: normal })
        }
        EntityKind::Circle(c) => {
            if !c.center.is_finite() || !c.radius.is_finite() || c.radius <= 1e-9 {
                return Err(bad(cmd, "circle profile must have finite center and positive radius"));
            }
            Ok(ExtrudeProfile::Circle { center: c.center, radius: c.radius, plane_normal: extrusion })
        }
        EntityKind::Region(r) => {
            if r.loops.len() != 1 {
                return Err(bad(cmd, "extruding regions with islands or multiple loops is not supported yet"));
            }
            let Some(loop0) = r.loops.first() else {
                return Err(bad(cmd, "region has no boundary loop"));
            };
            let raw_2d: Vec<Vec2> = if loop0.vertices.iter().any(|v| v.bulge.abs() > 1e-9) {
                let geom_poly = Polyline { vertices: loop0.vertices.clone(), closed: true };
                let b = geom_poly.bounds();
                let diag = if b.is_empty() { 10.0 } else { b.min.dist(b.max) };
                let tol = (diag * 1e-3).clamp(1e-4, 0.01);
                geom_poly.tessellate(tol)
            } else {
                loop0.vertices.iter().map(|v| v.p).collect()
            };
            let distinct = clean_2d_closed_ring(&raw_2d, cmd)?;
            let ocs = Mat4::ocs(extrusion);
            let points = distinct.into_iter().map(|p| ocs.apply(Vec3::new(p.x, p.y, 0.0))).collect();
            Ok(ExtrudeProfile::Polygon { points, plane_normal: extrusion })
        }
        _ => Err(bad(cmd, "profile entity must be a closed planar polyline, circle, or single-loop region")),
    }
}

fn parse_direction_spec(p: &Value, cmd: &'static str) -> Result<DirectionSpec> {
    if p.get("taper").is_some_and(|v| !v.is_null() && v.as_f64() != Some(0.0))
        || p.get("angle").is_some_and(|v| !v.is_null() && v.as_f64() != Some(0.0))
        || p.get("path").is_some_and(|v| !v.is_null())
    {
        return Err(bad(cmd, "tapered and path extrusions are not supported yet; provide height or direction"));
    }
    let height_opt = match p.get("height") {
        None | Some(Value::Null) => None,
        Some(v) => Some(v.as_f64().ok_or_else(|| bad(cmd, "`height` must be a finite, non-zero number"))?),
    };
    let dir_opt = match p.get("direction") {
        None | Some(Value::Null) => None,
        Some(v) => Some(parse_vec3(v).ok_or_else(|| bad(cmd, "`direction` must be a finite 3D vector [dx, dy, dz]"))?),
    };

    match (height_opt, dir_opt) {
        (Some(h), None) => {
            if !h.is_finite() || h.abs() <= 1e-9 {
                return Err(bad(cmd, "`height` must be a finite, non-zero number"));
            }
            Ok(DirectionSpec::Height(h))
        }
        (None, Some(d)) => {
            if !d.is_finite() || d.len() <= 1e-9 {
                return Err(bad(cmd, "`direction` must be a finite, non-zero vector"));
            }
            Ok(DirectionSpec::Vector(d))
        }
        (Some(h), Some(d)) => {
            if !d.is_finite() || d.len() <= 1e-9 {
                return Err(bad(cmd, "`direction` must be a finite, non-zero vector"));
            }
            if !h.is_finite() || h.abs() <= 1e-9 {
                return Err(bad(cmd, "`height` must be a finite, non-zero number"));
            }
            Ok(DirectionSpec::Vector(d.normalized() * h))
        }
        (None, None) => Err(bad(cmd, "either `height` or `direction` is required")),
    }
}

fn extrude_profile_to_solid(s: &mut Session, profile: ExtrudeProfile, spec: DirectionSpec, cmd: &'static str) -> Result<(Handle, Bounds3)> {
    let normal = profile.plane_normal();
    let direction = match spec {
        DirectionSpec::Height(h) => normal * h,
        DirectionSpec::Vector(d) => d,
    };

    if !direction.is_finite() || direction.len() <= 1e-9 {
        return Err(bad(cmd, "extrusion direction must be finite and non-zero"));
    }
    if normal.dot(direction).abs() <= 1e-9 * direction.len() {
        return Err(bad(cmd, "extrusion direction cannot be coplanar with profile plane"));
    }

    let mut recipe = Recipe::new();
    match profile {
        ExtrudeProfile::Polygon { points, .. } => {
            recipe.add_feature(1, FeatureOp::Extrusion { profile: points, direction });
        }
        ExtrudeProfile::Circle { center, radius, plane_normal } => {
            let dir_unit = direction.normalized();
            if dir_unit.cross(plane_normal).len() > 1e-6 {
                return Err(bad(cmd, "oblique (non-perpendicular) extrusion of Circle is not supported yet"));
            }
            recipe.add_feature(1, FeatureOp::Cylinder { center, radius, height: direction.len(), axis: dir_unit });
        }
    }

    let evaluated = BrepSolid::evaluate_recipe(&recipe).map_err(|e| bad(cmd, format!("evaluation failed: {e}")))?;
    let bbox = evaluated.bounding_box();
    let faces = evaluated.to_face3d_mesh(0.05).unwrap_or_default();

    let solid = Solid3d::with_faces(recipe, bbox, faces);
    let handle = s.add_entity(EntityKind::Solid3d(solid))?;
    Ok((handle, bbox))
}

fn run_extrude(s: &mut Session, p: &Value) -> Result<Value> {
    let profile_handle = p.get("profile").and_then(parse_handle).ok_or_else(|| bad("solid.extrude", "`profile` handle is required"))?;
    let spec = parse_direction_spec(p, "solid.extrude")?;
    let profile = extract_extrude_profile(s, profile_handle, "solid.extrude")?;
    let (handle, bbox) = extrude_profile_to_solid(s, profile, spec, "solid.extrude")?;

    Ok(json!({
        "status": "ok",
        "handle": handle.hex(),
        "bbox": bbox_to_json(&bbox)
    }))
}

fn collect_profile_handles(s: &Session, p: &Value, cmd: &'static str) -> Result<Vec<Handle>> {
    let mut handles = Vec::new();
    if let Some(arr) = p.get("handles").and_then(Value::as_array) {
        for v in arr {
            let h = parse_handle(v).ok_or_else(|| bad(cmd, format!("invalid handle `{v}` in `handles`")))?;
            if !handles.contains(&h) {
                handles.push(h);
            }
        }
    }
    if let Some(v) = p.get("profile") {
        let h = parse_handle(v).ok_or_else(|| bad(cmd, "`profile` must be a valid entity handle"))?;
        if !handles.contains(&h) {
            handles.push(h);
        }
    }
    if handles.is_empty() {
        for h in s.selection() {
            if !handles.contains(&h) {
                handles.push(h);
            }
        }
    }
    if handles.is_empty() {
        return Err(bad(cmd, "at least one closed profile `profile` or `handles` (or preselected entity) is required"));
    }
    Ok(handles)
}

fn extrude_handles_atomic(s: &mut Session, handles: &[Handle], spec: DirectionSpec, delete_profile: bool, cmd: &'static str) -> Result<Value> {
    if handles.is_empty() {
        return Err(bad(cmd, "no profile entities selected"));
    }
    let mut profiles = Vec::with_capacity(handles.len());
    for h in handles {
        profiles.push((*h, extract_extrude_profile(s, *h, cmd)?));
    }

    let doc_backup = s.doc()?.clone();
    let mut created = Vec::with_capacity(profiles.len());
    let mut first_bbox = Bounds3::EMPTY;
    for (src_h, profile) in profiles {
        match extrude_profile_to_solid(s, profile, spec, cmd) {
            Ok((solid_h, bbox)) => {
                if created.is_empty() {
                    first_bbox = bbox;
                }
                created.push((src_h, solid_h));
            }
            Err(e) => {
                *s.doc_mut()? = doc_backup;
                return Err(e);
            }
        }
    }

    if delete_profile {
        let doc_mut = s.doc_mut()?;
        for (src_h, _) in &created {
            doc_mut.remove_entity(*src_h);
        }
    }

    if cmd == "extrude" && !s.settings.vscurrent.eq_ignore_ascii_case("Conceptual") {
        s.settings.vscurrent = "Conceptual".into();
    }

    let out_handles: Vec<String> = created.iter().map(|(_, h)| h.hex()).collect();
    let first_handle = out_handles.first().cloned().unwrap_or_default();
    Ok(json!({
        "status": "ok",
        "handle": first_handle,
        "handles": out_handles,
        "bbox": bbox_to_json(&first_bbox)
    }))
}

fn run_extrude_cmd(s: &mut Session, p: &Value, cmd: &'static str) -> Result<Value> {
    let handles = collect_profile_handles(s, p, cmd)?;
    let spec = parse_direction_spec(p, cmd)?;
    let delete_profile = p.get("delete_profile").and_then(Value::as_bool).unwrap_or(false);
    extrude_handles_atomic(s, &handles, spec, delete_profile, cmd)
}

fn run_extrude_multi(s: &mut Session, p: &Value) -> Result<Value> {
    run_extrude_cmd(s, p, "extrude")
}

fn run_dmextrude(s: &mut Session, p: &Value) -> Result<Value> {
    run_extrude_cmd(s, p, "dmextrude")
}

fn run_translate(s: &mut Session, p: &Value) -> Result<Value> {
    let handle = p.get("handle").and_then(parse_handle).ok_or_else(|| bad("solid.translate", "`handle` is required"))?;
    let delta = p.get("delta").and_then(parse_vec3).ok_or_else(|| bad("solid.translate", "`delta` [dx, dy, dz] is required"))?;

    let mut new_bbox = Bounds3::EMPTY;
    s.doc_mut()?.modify_entity(handle, |e| {
        let EntityKind::Solid3d(solid) = &mut e.kind else { return };
        solid.translate(delta);
        if let Ok(eval) = BrepSolid::evaluate_recipe(&solid.recipe) {
            solid.bbox = eval.bounding_box();
            solid.faces = eval.to_face3d_mesh(0.05).unwrap_or_default();
            solid.status = Solid3dStatus::Evaluated;
            new_bbox = solid.bbox;
        } else {
            solid.status = Solid3dStatus::Failed("re-evaluation failed after translation".into());
        }
    })?;

    Ok(json!({
        "status": "ok",
        "handle": handle.hex(),
        "bbox": bbox_to_json(&new_bbox)
    }))
}

fn run_query_solid(s: &mut Session, p: &Value) -> Result<Value> {
    let handle = p.get("handle").and_then(parse_handle).ok_or_else(|| bad("query.solid", "`handle` is required"))?;

    let doc = s.doc()?;
    let ent = doc.entity(handle).ok_or_else(|| bad("query.solid", format!("entity {handle:?} not found")))?;
    let EntityKind::Solid3d(solid) = &ent.kind else {
        return Err(bad("query.solid", format!("entity {handle:?} is not a 3D Solid")));
    };

    let status_str = match &solid.status {
        Solid3dStatus::Evaluated => "evaluated",
        Solid3dStatus::Pending => "pending",
        Solid3dStatus::Failed(err) => err.as_str(),
    };

    Ok(json!({
        "status": "ok",
        "handle": handle.hex(),
        "type": "Solid3d",
        "layer": ent.common.layer,
        "schema_version": solid.schema_version,
        "evaluated": matches!(solid.status, Solid3dStatus::Evaluated),
        "evaluation_status": status_str,
        "bbox": bbox_to_json(&solid.bbox),
        "features_count": solid.recipe.features.len()
    }))
}

fn run_query_metrics(s: &mut Session, p: &Value) -> Result<Value> {
    let handle = p.get("handle").and_then(parse_handle).ok_or_else(|| bad("query.metrics", "`handle` is required"))?;

    let doc = s.doc()?;
    let ent = doc.entity(handle).ok_or_else(|| bad("query.metrics", format!("entity {handle:?} not found")))?;
    let EntityKind::Solid3d(solid) = &ent.kind else {
        return Err(bad("query.metrics", format!("entity {handle:?} is not a 3D Solid")));
    };

    let evaluated = BrepSolid::evaluate_recipe(&solid.recipe).map_err(|e| bad("query.metrics", format!("failed to evaluate solid: {e}")))?;

    let volume = evaluated.volume().map_err(|e| bad("query.metrics", format!("failed volume calculation: {e}")))?;
    let bbox = evaluated.bounding_box();
    let boundaries = evaluated.boundaries_count();

    Ok(json!({
        "status": "ok",
        "handle": handle.hex(),
        "volume": volume,
        "bbox": bbox_to_json(&bbox),
        "boundaries": boundaries
    }))
}

const DEFAULT_STL_TOLERANCE_MM: f64 = 0.05;
const STL_TOPOLOGY_MERGE_TOL_MM: f64 = 1e-5;

#[derive(Clone, Copy, Debug)]
struct StlUnitChoice {
    info: InsUnitInfo,
    save_in_doc: bool,
}

fn parse_unit_value(v: &Value, cmd: &'static str) -> Result<InsUnitInfo> {
    let parsed = if let Some(s) = v.as_str() {
        parse_insunits_choice(s)
    } else if let Some(code) = v.as_i64() {
        insunits_info(code)
    } else {
        None
    };
    parsed.ok_or_else(|| {
        bad(
            cmd,
            "unknown or unitless STL unit; specify a physical unit such as \"mm\", \"cm\", \"m\", \"in\", or \"ft\"",
        )
    })
}

fn resolve_stl_unit(doc: &Drawing, p: &Value, cmd: &'static str) -> Result<StlUnitChoice> {
    let explicit = p.get("unit").or_else(|| p.get("insunits")).filter(|v| !v.is_null());
    let save_in_doc = p.get("save_units").and_then(Value::as_bool).unwrap_or(false);

    if let Some(v) = explicit {
        let info = parse_unit_value(v, cmd)?;
        return Ok(StlUnitChoice { info, save_in_doc });
    }

    let doc_code = doc.header.i64("INSUNITS", 0);
    if let Some(info) = insunits_info(doc_code) {
        return Ok(StlUnitChoice { info, save_in_doc: false });
    }

    let name = insunits_name(doc_code);
    Err(bad(
        cmd,
        format!(
            "drawing has no known physical units (INSUNITS={doc_code} \"{name}\"); specify an explicit unit \
             (\"mm\", \"cm\", \"m\", \"in\", \"ft\") or set INSUNITS via UNITS before exporting STL"
        ),
    ))
}

fn resolve_stl_tolerance(p: &Value, unit: InsUnitInfo, cmd: &'static str) -> Result<f64> {
    if let Some(v) = p.get("tolerance_mm").filter(|v| !v.is_null()) {
        let mm = v.as_f64().filter(|x| x.is_finite() && *x > 0.0).ok_or_else(|| bad(cmd, "`tolerance_mm` must be positive and finite"))?;
        return Ok((mm / unit.mm_per_unit).max(MIN_SAFE_MESH_TOLERANCE));
    }
    if let Some(v) = p.get("tolerance").filter(|v| !v.is_null()) {
        let tol = v.as_f64().filter(|x| x.is_finite() && *x > 0.0).ok_or_else(|| bad(cmd, "`tolerance` must be positive and finite"))?;
        return Ok(tol.max(MIN_SAFE_MESH_TOLERANCE));
    }
    Ok((DEFAULT_STL_TOLERANCE_MM / unit.mm_per_unit).max(MIN_SAFE_MESH_TOLERANCE))
}

fn collect_stl_handles(s: &Session, p: &Value, cmd: &'static str, allow_all_model_solids: bool) -> Result<(Vec<Handle>, usize)> {
    let doc = s.doc()?;
    let mut explicit = Vec::new();
    let mut saw_explicit_key = false;

    if let Some(arr) = p.get("handles").and_then(Value::as_array) {
        saw_explicit_key = true;
        for v in arr {
            let h = parse_handle(v).ok_or_else(|| bad(cmd, format!("invalid handle `{v}` in `handles`")))?;
            if !explicit.contains(&h) {
                explicit.push(h);
            }
        }
    } else if let Some(v) = p.get("handle").filter(|v| !v.is_null()) {
        saw_explicit_key = true;
        let h = parse_handle(v).ok_or_else(|| bad(cmd, format!("invalid handle `{v}`")))?;
        explicit.push(h);
    }

    if saw_explicit_key {
        if explicit.is_empty() {
            return Err(bad(cmd, "at least one valid solid `handle` or `handles` array is required"));
        }
        for h in &explicit {
            let ent = doc.entity(*h).ok_or_else(|| bad(cmd, format!("entity {h:?} not found")))?;
            if !matches!(ent.kind, EntityKind::Solid3d(_)) {
                return Err(bad(cmd, format!("entity {h:?} is not a 3D Solid")));
            }
        }
        return Ok((explicit, 0));
    }

    if !allow_all_model_solids && cmd == "export.stl" {
        return Err(bad(cmd, "at least one valid solid `handle` or `handles` array is required"));
    }

    let sel = s.selection();
    if !sel.is_empty() {
        let mut solids = Vec::new();
        let mut skipped = 0usize;
        for h in sel {
            match doc.entity(h).map(|e| &e.kind) {
                Some(EntityKind::Solid3d(_)) => {
                    if !solids.contains(&h) {
                        solids.push(h);
                    }
                }
                Some(_) => skipped = skipped.saturating_add(1),
                None => {}
            }
        }
        if solids.is_empty() {
            return Err(bad(cmd, "selected objects contain no 3D solids to export to STL"));
        }
        return Ok((solids, skipped));
    }

    if allow_all_model_solids {
        let space = s.space();
        let solids: Vec<Handle> = doc
            .space(&space)
            .map(|st| st.iter().filter_map(|e| matches!(e.kind, EntityKind::Solid3d(_)).then_some(e.handle)).collect())
            .unwrap_or_default();
        if solids.is_empty() {
            return Err(bad(cmd, "no 3D solids found in current space to export to STL"));
        }
        return Ok((solids, 0));
    }

    Err(bad(cmd, "select one or more 3D solids to export to STL"))
}

fn build_validated_stl_bytes(doc: &Drawing, handles: &[Handle], unit: InsUnitInfo, model_tol: f64, cmd: &'static str) -> Result<(Vec<u8>, Value)> {
    let mut combined_mesh = TriangleMesh::default();
    let mut solids_count: usize = 0;

    for h in handles {
        let ent = doc.entity(*h).ok_or_else(|| bad(cmd, format!("entity {h:?} not found")))?;
        let EntityKind::Solid3d(solid) = &ent.kind else {
            return Err(bad(cmd, format!("entity {h:?} is not a 3D Solid")));
        };

        let evaluated = BrepSolid::evaluate_recipe(&solid.recipe).map_err(|e| bad(cmd, format!("solid {} evaluation failed: {e}", h.hex())))?;
        let raw_mesh = evaluated.triangulate(model_tol).map_err(|e| bad(cmd, format!("solid {} triangulation failed: {e}", h.hex())))?;
        let mm_mesh = raw_mesh.scaled(unit.mm_per_unit).map_err(|e| bad(cmd, format!("solid {} scaling failed: {e}", h.hex())))?;
        mm_mesh
            .validate_for_stl(STL_TOPOLOGY_MERGE_TOL_MM)
            .map_err(|e| bad(cmd, format!("solid {} STL mesh invalid: {e}", h.hex())))?;

        let base_vertex = combined_mesh.positions.len();
        combined_mesh.positions.extend(mm_mesh.positions);
        combined_mesh.normals.extend(mm_mesh.normals);
        for tri in mm_mesh.triangles {
            combined_mesh.triangles.push([
                tri[0].saturating_add(base_vertex),
                tri[1].saturating_add(base_vertex),
                tri[2].saturating_add(base_vertex),
            ]);
        }
        solids_count = solids_count.saturating_add(1);
    }

    let report = combined_mesh
        .validate_for_stl(STL_TOPOLOGY_MERGE_TOL_MM)
        .map_err(|e| bad(cmd, format!("combined STL mesh invalid: {e}")))?;
    let bbox_mm = combined_mesh.bounding_box();
    let bytes = combined_mesh.to_stl_binary_bytes().map_err(|e| bad(cmd, format!("failed serializing binary STL: {e}")))?;

    let meta = json!({
        "status": "ok",
        "solids_count": solids_count,
        "triangles_count": report.triangle_count,
        "unit": unit.name,
        "insunits": unit.code,
        "mm_per_unit": unit.mm_per_unit,
        "model_tolerance": model_tol,
        "volume_mm3": report.signed_volume,
        "bbox_mm": {
            "min": [bbox_mm.min.x, bbox_mm.min.y, bbox_mm.min.z],
            "max": [bbox_mm.max.x, bbox_mm.max.y, bbox_mm.max.z]
        },
        "topology": {
            "open_edges": report.open_edges,
            "non_manifold_edges": report.non_manifold_edges,
            "degenerate_faces": report.degenerate_faces,
            "consistent_orientation": report.consistent_orientation
        }
    });
    Ok((bytes, meta))
}

pub(super) fn persist_stl_unit_if_requested(s: &mut Session, p: &Value) -> Result<()> {
    let unit_choice = resolve_stl_unit(s.doc()?, p, "stl")?;
    if unit_choice.save_in_doc {
        s.doc_mut()?.header.set_i64("INSUNITS", unit_choice.info.code);
    }
    Ok(())
}

pub(super) fn export_stl_for_export(s: &Session, p: &Value) -> Result<(Vec<u8>, Value)> {
    let doc = s.doc()?;
    let unit_choice = resolve_stl_unit(doc, p, "export")?;
    let model_tol = resolve_stl_tolerance(p, unit_choice.info, "export")?;
    let (handles, skipped) = collect_stl_handles(s, p, "export", true)?;
    let (bytes, mut meta) = build_validated_stl_bytes(doc, &handles, unit_choice.info, model_tol, "export")?;
    if let Some(m) = meta.as_object_mut() {
        m.insert("skipped_non_solids".into(), json!(skipped));
        m.insert("unit_saved".into(), json!(unit_choice.save_in_doc));
    }
    Ok((bytes, meta))
}

fn execute_stl_write(s: &mut Session, handles: &[Handle], unit_choice: StlUnitChoice, model_tol: f64, path_str: &str, cmd: &'static str) -> Result<Value> {
    let (bytes, mut meta) = build_validated_stl_bytes(s.doc()?, handles, unit_choice.info, model_tol, cmd)?;
    super::exchange::write_file(cmd, path_str, &bytes)?;
    if unit_choice.save_in_doc {
        s.doc_mut()?.header.set_i64("INSUNITS", unit_choice.info.code);
    }
    if let Some(m) = meta.as_object_mut() {
        m.insert("path".into(), json!(path_str));
        m.insert("bytes".into(), json!(bytes.len()));
        m.insert("unit_saved".into(), json!(unit_choice.save_in_doc));
    }
    Ok(meta)
}

fn run_stl_command(s: &mut Session, p: &Value, cmd: &'static str, allow_selection: bool) -> Result<Value> {
    let path_str = p.get("path").and_then(Value::as_str).filter(|t| !t.trim().is_empty()).ok_or_else(|| bad(cmd, "`path` is required"))?;
    let (handles, skipped) = collect_stl_handles(s, p, cmd, allow_selection && p.get("all").and_then(Value::as_bool) == Some(true))?;
    let unit_choice = resolve_stl_unit(s.doc()?, p, cmd)?;
    let model_tol = resolve_stl_tolerance(p, unit_choice.info, cmd)?;
    let mut out = execute_stl_write(s, &handles, unit_choice, model_tol, path_str, cmd)?;
    if let Some(m) = out.as_object_mut() {
        m.insert("skipped_non_solids".into(), json!(skipped));
    }
    Ok(out)
}

fn run_stlout(s: &mut Session, p: &Value) -> Result<Value> {
    run_stl_command(s, p, "stlout", true)
}

fn run_export_stl(s: &mut Session, p: &Value) -> Result<Value> {
    run_stl_command(s, p, "export.stl", false)
}

// ---------------- 3D Boolean Operations ----------------

struct BooleanParams {
    target: Handle,
    tool: Handle,
    delete_operands: bool,
    oversized: bool,
}

fn parse_boolean_params(cmd: &str, p: &Value) -> Result<BooleanParams> {
    let mut target = None;
    if let Some(v) = p.get("target") {
        target = Some(parse_handle(v).ok_or_else(|| bad(cmd, "invalid `target` handle"))?);
    }
    if let Some(v) = p.get("from") {
        let from_h = parse_handle(v).ok_or_else(|| bad(cmd, "invalid `from` handle"))?;
        if let Some(t) = target {
            if t != from_h {
                return Err(bad(cmd, "conflicting `target` and `from` operand handles"));
            }
        } else {
            target = Some(from_h);
        }
    }

    let mut tool = None;
    if let Some(v) = p.get("tool") {
        tool = Some(parse_handle(v).ok_or_else(|| bad(cmd, "invalid `tool` handle"))?);
    }
    if let Some(v) = p.get("tools").or_else(|| p.get("remove")) {
        let tools_h = if let Some(arr) = v.as_array() {
            if arr.is_empty() {
                return Err(bad(cmd, "`tools` array cannot be empty"));
            }
            if let Some(first) = arr.first() {
                parse_handle(first).ok_or_else(|| bad(cmd, "invalid handle in `tools` array"))?
            } else {
                return Err(bad(cmd, "`tools` array cannot be empty"));
            }
        } else {
            parse_handle(v).ok_or_else(|| bad(cmd, "invalid `tool` handle"))?
        };

        if let Some(t) = tool {
            if t != tools_h {
                return Err(bad(cmd, "conflicting `tool` and `tools` operand handles"));
            }
        } else {
            tool = Some(tools_h);
        }
    }

    if let Some(v) = p.get("handles") {
        if let Some(arr) = v.as_array() {
            if arr.len() != 2 {
                return Err(bad(cmd, "`handles` array must contain exactly 2 solid handles (target and tool)"));
            }
            let h0 = if let Some(first) = arr.first() {
                parse_handle(first).ok_or_else(|| bad(cmd, "invalid target handle in `handles`"))?
            } else {
                return Err(bad(cmd, "`handles` array must contain exactly 2 solid handles"));
            };
            let h1 = if let Some(second) = arr.get(1) {
                parse_handle(second).ok_or_else(|| bad(cmd, "invalid tool handle in `handles`"))?
            } else {
                return Err(bad(cmd, "`handles` array must contain exactly 2 solid handles"));
            };

            if let Some(t) = target {
                if t != h0 {
                    return Err(bad(cmd, "conflicting target handle specified in `target` and `handles`"));
                }
            } else {
                target = Some(h0);
            }

            if let Some(t) = tool {
                if t != h1 {
                    return Err(bad(cmd, "conflicting tool handle specified in `tool` and `handles`"));
                }
            } else {
                tool = Some(h1);
            }
        } else {
            return Err(bad(cmd, "`handles` parameter must be an array"));
        }
    }

    let target = target.ok_or_else(|| bad(cmd, "missing `target` operand handle (or `handles` array)"))?;
    let tool = tool.ok_or_else(|| bad(cmd, "missing `tool` operand handle (or `handles` array)"))?;

    if target == tool {
        return Err(bad(cmd, "target and tool must be distinct entities"));
    }

    let delete_operands = p.get("delete_operands").and_then(Value::as_bool).unwrap_or(false);

    let oversized_opt = p.get("oversized").and_then(Value::as_bool);
    let oversize_through_opt = p.get("oversize_through").and_then(Value::as_bool);
    let oversized = match (oversized_opt, oversize_through_opt) {
        (Some(a), Some(b)) if a != b => {
            return Err(bad(cmd, "conflicting values for `oversized` and `oversize_through`"));
        }
        (Some(a), _) => a,
        (None, Some(b)) => b,
        (None, None) => false,
    };

    Ok(BooleanParams { target, tool, delete_operands, oversized })
}

fn merge_recipes_for_boolean(cmd: &str, target_recipe: &Recipe, tool_recipe: &Recipe, kind: BooleanKind, oversized: bool) -> Result<Recipe> {
    if target_recipe.features.is_empty() {
        return Err(bad(cmd, "target solid recipe contains no features"));
    }
    if tool_recipe.features.is_empty() {
        return Err(bad(cmd, "tool solid recipe contains no features"));
    }

    let target_last_id = target_recipe.features.last().map(|f| f.id).ok_or_else(|| bad(cmd, "target solid recipe is empty"))?;
    let tool_last_id = tool_recipe.features.last().map(|f| f.id).ok_or_else(|| bad(cmd, "tool solid recipe is empty"))?;

    let mut combined = Recipe::new();
    let mut next_id: u64 = 1;
    let mut target_map = std::collections::HashMap::new();

    for feature in &target_recipe.features {
        let new_id = next_id;
        next_id = next_id.saturating_add(1);
        target_map.insert(feature.id, new_id);

        let remapped_op = match &feature.op {
            FeatureOp::Box { origin, size } => FeatureOp::Box { origin: *origin, size: *size },
            FeatureOp::Cylinder { center, radius, height, axis } => {
                FeatureOp::Cylinder { center: *center, radius: *radius, height: *height, axis: *axis }
            }
            FeatureOp::Sphere { center, radius } => FeatureOp::Sphere { center: *center, radius: *radius },
            FeatureOp::Extrusion { profile, direction } => FeatureOp::Extrusion { profile: profile.clone(), direction: *direction },
            FeatureOp::Translation { target_id, delta } => {
                let mapped_target = target_map
                    .get(target_id)
                    .copied()
                    .ok_or_else(|| bad(cmd, format!("invalid recipe: reference target_id {target_id} not found")))?;
                FeatureOp::Translation { target_id: mapped_target, delta: *delta }
            }
            FeatureOp::Boolean { kind, target_id, tool_id, oversized } => {
                let mapped_target = target_map
                    .get(target_id)
                    .copied()
                    .ok_or_else(|| bad(cmd, format!("invalid recipe: reference target_id {target_id} not found")))?;
                let mapped_tool =
                    target_map.get(tool_id).copied().ok_or_else(|| bad(cmd, format!("invalid recipe: reference tool_id {tool_id} not found")))?;
                FeatureOp::Boolean { kind: *kind, target_id: mapped_target, tool_id: mapped_tool, oversized: *oversized }
            }
        };
        combined.add_feature(new_id, remapped_op);
    }

    let final_target_id = target_map.get(&target_last_id).copied().ok_or_else(|| bad(cmd, "failed to map target feature id"))?;

    let mut tool_map = std::collections::HashMap::new();

    for feature in &tool_recipe.features {
        let new_id = next_id;
        next_id = next_id.saturating_add(1);
        tool_map.insert(feature.id, new_id);

        let remapped_op = match &feature.op {
            FeatureOp::Box { origin, size } => FeatureOp::Box { origin: *origin, size: *size },
            FeatureOp::Cylinder { center, radius, height, axis } => {
                FeatureOp::Cylinder { center: *center, radius: *radius, height: *height, axis: *axis }
            }
            FeatureOp::Sphere { center, radius } => FeatureOp::Sphere { center: *center, radius: *radius },
            FeatureOp::Extrusion { profile, direction } => FeatureOp::Extrusion { profile: profile.clone(), direction: *direction },
            FeatureOp::Translation { target_id, delta } => {
                let mapped_target =
                    tool_map.get(target_id).copied().ok_or_else(|| bad(cmd, format!("invalid recipe: reference target_id {target_id} not found")))?;
                FeatureOp::Translation { target_id: mapped_target, delta: *delta }
            }
            FeatureOp::Boolean { kind, target_id, tool_id, oversized } => {
                let mapped_target =
                    tool_map.get(target_id).copied().ok_or_else(|| bad(cmd, format!("invalid recipe: reference target_id {target_id} not found")))?;
                let mapped_tool =
                    tool_map.get(tool_id).copied().ok_or_else(|| bad(cmd, format!("invalid recipe: reference tool_id {tool_id} not found")))?;
                FeatureOp::Boolean { kind: *kind, target_id: mapped_target, tool_id: mapped_tool, oversized: *oversized }
            }
        };
        combined.add_feature(new_id, remapped_op);
    }

    let final_tool_id = tool_map.get(&tool_last_id).copied().ok_or_else(|| bad(cmd, "failed to map tool feature id"))?;

    let boolean_id = next_id;
    combined.add_feature(boolean_id, FeatureOp::Boolean { kind, target_id: final_target_id, tool_id: final_tool_id, oversized });

    Ok(combined)
}

fn run_boolean_op(s: &mut Session, p: &Value, cmd: &'static str, kind: BooleanKind) -> Result<Value> {
    let params = parse_boolean_params(cmd, p)?;

    let (target_recipe, tool_recipe) = {
        let doc = s.doc()?;
        let target_ent = doc.entity(params.target).ok_or_else(|| bad(cmd, format!("target entity with handle {} not found", params.target.hex())))?;
        let EntityKind::Solid3d(target_solid) = &target_ent.kind else {
            return Err(bad(cmd, format!("target entity with handle {} is not a 3D Solid", params.target.hex())));
        };

        let tool_ent = doc.entity(params.tool).ok_or_else(|| bad(cmd, format!("tool entity with handle {} not found", params.tool.hex())))?;
        let EntityKind::Solid3d(tool_solid) = &tool_ent.kind else {
            return Err(bad(cmd, format!("tool entity with handle {} is not a 3D Solid", params.tool.hex())));
        };

        (target_solid.recipe.clone(), tool_solid.recipe.clone())
    };

    let combined_recipe = merge_recipes_for_boolean(cmd, &target_recipe, &tool_recipe, kind, params.oversized)?;

    let evaluated = BrepSolid::evaluate_recipe(&combined_recipe).map_err(|e| bad(cmd, format!("boolean evaluation failed: {e}")))?;

    if evaluated.is_empty() {
        return Err(bad(cmd, "boolean operation produced an empty solid (no common volume)"));
    }

    let bbox = evaluated.bounding_box();
    let faces = evaluated.to_face3d_mesh(0.05).unwrap_or_default();

    let solid = Solid3d::with_faces(combined_recipe, bbox, faces);

    // Snapshot document before modifying to guarantee complete atomicity on any mutation failure
    let doc_backup = s.doc()?.clone();

    let handle = match s.add_entity(EntityKind::Solid3d(solid)) {
        Ok(h) => h,
        Err(e) => {
            *s.doc_mut()? = doc_backup;
            return Err(e);
        }
    };

    if params.delete_operands {
        let rm_target = s.doc_mut()?.remove_entity(params.target);
        let rm_tool = s.doc_mut()?.remove_entity(params.tool);
        if rm_target.is_none() || rm_tool.is_none() {
            // Revert changes if operand deletion failed
            *s.doc_mut()? = doc_backup;
            return Err(bad(cmd, "failed to remove operand entities cleanly; document restored"));
        }
    }

    let mut resp = json!({
        "status": "ok",
        "handle": handle.hex(),
        "bbox": bbox_to_json(&bbox),
        "target": params.target.hex(),
        "tool": params.tool.hex(),
    });
    if params.delete_operands {
        resp["deleted"] = json!([params.target.hex(), params.tool.hex()]);
    }

    Ok(resp)
}

fn run_union(s: &mut Session, p: &Value) -> Result<Value> {
    run_boolean_op(s, p, "solid.union", BooleanKind::Union)
}

fn run_subtract(s: &mut Session, p: &Value) -> Result<Value> {
    run_boolean_op(s, p, "solid.subtract", BooleanKind::Difference)
}

fn run_intersect(s: &mut Session, p: &Value) -> Result<Value> {
    run_boolean_op(s, p, "solid.intersect", BooleanKind::Intersection)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CylStage {
    Center,
    Radius,
    HeightOrAxis,
    AxisEndpoint,
}

impl Default for CylStage {
    fn default() -> Self {
        CylStage::Center
    }
}

#[derive(Default)]
pub struct CylinderM {
    stage: CylStage,
    center: Option<Vec3>,
    radius: Option<f64>,
    diameter_mode: bool,
}

impl Interactive for CylinderM {
    fn name(&self) -> &'static str {
        "CYLINDER"
    }

    fn prompt(&self, _s: &Session) -> Prompt {
        match self.stage {
            CylStage::Center => Prompt::new("Specify center point of base or [3P/2P/Ttr/Elliptical]:", Accept::POINT)
                .kw(&["3P", "2P", "Ttr", "Elliptical"]),
            CylStage::Radius => {
                if self.diameter_mode {
                    Prompt::new("Specify diameter of base:", Accept::POINT_OR_NUMBER)
                } else {
                    Prompt::new("Specify base radius or [Diameter]:", Accept::POINT_OR_NUMBER).kw(&["Diameter"])
                }
            }
            CylStage::HeightOrAxis => Prompt::new("Specify height or [2Point/Axis endpoint]:", Accept::POINT_OR_NUMBER)
                .kw(&["2Point", "Axis endpoint"]),
            CylStage::AxisEndpoint => Prompt::new("Specify axis endpoint:", Accept::POINT),
        }
    }

    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        match self.stage {
            CylStage::Center => {
                let pt = match i {
                    Input::Point3(p3) => p3,
                    Input::Point(p) => s.drawing_plane().embed(p, s.effective_drawing_plane_offset()),
                    Input::Cancel => return Ok(Step::Done),
                    _ => return Ok(Step::Continue),
                };
                self.center = Some(pt);
                self.stage = CylStage::Radius;
                Ok(Step::Continue)
            }
            CylStage::Radius => {
                match i {
                    Input::Keyword(k) if !self.diameter_mode && k.trim().eq_ignore_ascii_case("Diameter") => {
                        self.diameter_mode = true;
                        Ok(Step::Continue)
                    }
                    Input::Text(t) => {
                        let upper = t.trim().to_ascii_uppercase();
                        if !self.diameter_mode && (upper == "D" || upper == "DIAMETER") {
                            self.diameter_mode = true;
                            return Ok(Step::Continue);
                        }
                        let v = crate::units::parse_distance(&t)
                            .ok_or_else(|| crate::EngineError::Other("Requires a distance.".into()))?;
                        let r = if self.diameter_mode { v * 0.5 } else { v };
                        if r <= 0.0 || !r.is_finite() {
                            return Err(crate::EngineError::Other("Radius must be positive and nonzero.".into()));
                        }
                        self.radius = Some(r);
                        self.diameter_mode = false;
                        self.stage = CylStage::HeightOrAxis;
                        Ok(Step::Continue)
                    }
                    Input::Point3(p3) => {
                        let center = self.center.unwrap_or(Vec3::ZERO);
                        let d = (p3 - center).len();
                        let r = if self.diameter_mode { d * 0.5 } else { d };
                        if r <= 1e-9 || !r.is_finite() {
                            return Err(crate::EngineError::Other("Radius must be positive and nonzero.".into()));
                        }
                        self.radius = Some(r);
                        self.diameter_mode = false;
                        self.stage = CylStage::HeightOrAxis;
                        Ok(Step::Continue)
                    }
                    Input::Point(p) => {
                        let center = self.center.unwrap_or(Vec3::ZERO);
                        let p3 = s.drawing_plane().embed(p, s.effective_drawing_plane_offset());
                        let d = (p3 - center).len();
                        let r = if self.diameter_mode { d * 0.5 } else { d };
                        if r <= 1e-9 || !r.is_finite() {
                            return Err(crate::EngineError::Other("Radius must be positive and nonzero.".into()));
                        }
                        self.radius = Some(r);
                        self.diameter_mode = false;
                        self.stage = CylStage::HeightOrAxis;
                        Ok(Step::Continue)
                    }
                    Input::Cancel => Ok(Step::Done),
                    _ => Ok(Step::Continue),
                }
            }
            CylStage::HeightOrAxis => {
                match i {
                    Input::Keyword(k) => {
                        let upper = k.trim().to_ascii_uppercase();
                        if upper == "A" || upper == "_A" || upper.starts_with("AXIS") {
                            self.stage = CylStage::AxisEndpoint;
                            Ok(Step::Continue)
                        } else {
                            Err(crate::EngineError::Other("Invalid option keyword.".into()))
                        }
                    }
                    Input::Text(t) => {
                        let upper = t.trim().to_ascii_uppercase();
                        if upper == "A" || upper == "_A" || upper.starts_with("AXIS") {
                            self.stage = CylStage::AxisEndpoint;
                            return Ok(Step::Continue);
                        }
                        let h = crate::units::parse_distance(&t)
                            .ok_or_else(|| crate::EngineError::Other("Requires a distance.".into()))?;
                        if h.abs() <= 1e-9 || !h.is_finite() {
                            return Err(crate::EngineError::Other("Height must be nonzero.".into()));
                        }
                        let center = self.center.unwrap_or(Vec3::ZERO);
                        let radius = self.radius.unwrap_or(1.0);
                        let height = h.abs();
                        let ucs_z = s.ucs().z_axis;
                        let axis = if h >= 0.0 { ucs_z } else { -ucs_z };
                        create_and_add_cylinder(s, center, radius, height, axis)?;
                        Ok(Step::Done)
                    }
                    Input::Point3(p3) => {
                        let center = self.center.unwrap_or(Vec3::ZERO);
                        let h = (p3 - center).len();
                        if h <= 1e-9 || !h.is_finite() {
                            return Err(crate::EngineError::Other("Height must be nonzero.".into()));
                        }
                        let radius = self.radius.unwrap_or(1.0);
                        let axis = (p3 - center).normalized();
                        create_and_add_cylinder(s, center, radius, h, axis)?;
                        Ok(Step::Done)
                    }
                    Input::Point(p) => {
                        let center = self.center.unwrap_or(Vec3::ZERO);
                        let p3 = s.drawing_plane().embed(p, s.effective_drawing_plane_offset());
                        let h = (p3 - center).len();
                        if h <= 1e-9 || !h.is_finite() {
                            return Err(crate::EngineError::Other("Height must be nonzero.".into()));
                        }
                        let radius = self.radius.unwrap_or(1.0);
                        let axis = (p3 - center).normalized();
                        create_and_add_cylinder(s, center, radius, h, axis)?;
                        Ok(Step::Done)
                    }
                    Input::Cancel => Ok(Step::Done),
                    _ => Ok(Step::Continue),
                }
            }
            CylStage::AxisEndpoint => {
                match i {
                    Input::Point3(p3) => {
                        let center = self.center.unwrap_or(Vec3::ZERO);
                        let delta = p3 - center;
                        let len = delta.len();
                        if len <= 1e-9 || !len.is_finite() {
                            return Err(crate::EngineError::Other("Axis endpoint cannot coincide with the base center.".into()));
                        }
                        let radius = self.radius.unwrap_or(1.0);
                        let axis = delta.normalized();
                        create_and_add_cylinder(s, center, radius, len, axis)?;
                        Ok(Step::Done)
                    }
                    Input::Point(p) => {
                        let center = self.center.unwrap_or(Vec3::ZERO);
                        let p3 = s.drawing_plane().embed(p, s.effective_drawing_plane_offset());
                        let delta = p3 - center;
                        let len = delta.len();
                        if len <= 1e-9 || !len.is_finite() {
                            return Err(crate::EngineError::Other("Axis endpoint cannot coincide with the base center.".into()));
                        }
                        let radius = self.radius.unwrap_or(1.0);
                        let axis = delta.normalized();
                        create_and_add_cylinder(s, center, radius, len, axis)?;
                        Ok(Step::Done)
                    }
                    Input::Cancel => Ok(Step::Done),
                    _ => Ok(Step::Continue),
                }
            }
        }
    }

    fn preview(&self, _s: &Session, _c: Vec2) -> Vec<EntityKind> {
        Vec::new()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExtrudeStage {
    Select,
    HeightOrDirection,
    DirectionStart,
    DirectionEnd,
}

struct ExtrudeM {
    cmd_name: &'static str,
    cmd_id: &'static str,
    sel: SelectPhase,
    stage: ExtrudeStage,
    profiles: Vec<Handle>,
    ref_point: Vec3,
    ref_normal: Vec3,
    dir_start: Option<Vec3>,
}

impl ExtrudeM {
    fn new(cmd_name: &'static str) -> Self {
        let cmd_id = if cmd_name == "DMEXTRUDE" { "dmextrude" } else { "extrude" };
        ExtrudeM {
            cmd_name,
            cmd_id,
            sel: SelectPhase::default(),
            stage: ExtrudeStage::Select,
            profiles: Vec::new(),
            ref_point: Vec3::ZERO,
            ref_normal: Vec3::Z,
            dir_start: None,
        }
    }

    fn accept_profiles(&mut self, s: &mut Session, raw: Vec<Handle>) -> Result<()> {
        let doc = s.doc()?;
        let mut valid = Vec::new();
        let mut skipped = 0usize;
        for h in raw {
            if let Some(e) = doc.entity(h) {
                match &e.kind {
                    EntityKind::LwPolyline(_) | EntityKind::Polyline3d(_) | EntityKind::Circle(_) | EntityKind::Region(_) => {
                        if !valid.contains(&h) {
                            valid.push(h);
                        }
                    }
                    _ => skipped = skipped.saturating_add(1),
                }
            }
        }
        if skipped > 0 {
            s.echo(format!("{skipped} non-extrudable object(s) ignored."));
        }
        let Some(&first_h) = valid.first() else {
            return Err(crate::EngineError::Other("Select one or more closed planar profiles (polyline, circle, or region) to extrude.".into()));
        };
        let first_profile = extract_extrude_profile(s, first_h, self.cmd_id)?;
        for &h in valid.iter().skip(1) {
            let _ = extract_extrude_profile(s, h, self.cmd_id)?;
        }
        self.ref_point = first_profile.reference_point();
        self.ref_normal = first_profile.plane_normal();
        self.profiles = valid;
        self.stage = ExtrudeStage::HeightOrDirection;
        Ok(())
    }
}

impl Interactive for ExtrudeM {
    fn name(&self) -> &'static str {
        self.cmd_name
    }

    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        self.sel = SelectPhase::begin(s);
        if self.sel.done {
            let hs = std::mem::take(&mut self.sel.picked);
            if hs.is_empty() {
                return Ok(Step::Done);
            }
            self.accept_profiles(s, hs)?;
        }
        Ok(Step::Continue)
    }

    fn prompt(&self, _s: &Session) -> Prompt {
        match self.stage {
            ExtrudeStage::Select => self.sel.prompt(),
            ExtrudeStage::HeightOrDirection => {
                Prompt::new("Specify height of extrusion or [Direction]:", Accept::POINT_OR_NUMBER).kw(&["Direction"])
            }
            ExtrudeStage::DirectionStart => Prompt::new("Specify start point of direction:", Accept::POINT),
            ExtrudeStage::DirectionEnd => Prompt::new("Specify end point of direction:", Accept::POINT),
        }
    }

    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        match self.stage {
            ExtrudeStage::Select => match self.sel.feed(s, &i)? {
                SelOutcome::More => Ok(Step::Continue),
                SelOutcome::Empty => Ok(Step::Done),
                SelOutcome::Done(hs) => {
                    self.accept_profiles(s, hs)?;
                    Ok(Step::Continue)
                }
            },
            ExtrudeStage::HeightOrDirection => match i {
                Input::Cancel => Ok(Step::Done),
                Input::Keyword(k) => {
                    if k.trim().eq_ignore_ascii_case("Direction") || k.trim().eq_ignore_ascii_case("D") {
                        self.stage = ExtrudeStage::DirectionStart;
                        Ok(Step::Continue)
                    } else {
                        Err(crate::EngineError::Other(format!(
                            "Option `{}` is not supported; specify exact height or Direction.",
                            k.trim()
                        )))
                    }
                }
                Input::Text(t) => {
                    let upper = t.trim().to_ascii_uppercase();
                    if upper == "D" || upper == "DIRECTION" {
                        self.stage = ExtrudeStage::DirectionStart;
                        return Ok(Step::Continue);
                    }
                    let h = crate::units::parse_distance(&t).ok_or_else(|| crate::EngineError::Other("Requires a distance.".into()))?;
                    if !h.is_finite() || h.abs() <= 1e-9 {
                        return Err(crate::EngineError::Other("Extrusion height must be finite and non-zero.".into()));
                    }
                    let res = extrude_handles_atomic(s, &self.profiles, DirectionSpec::Height(h), false, self.cmd_id)?;
                    if let Some(h) = res.get("handle").and_then(Value::as_str) {
                        s.echo(format!("Extruded {} solid(s) (first handle {h}).", self.profiles.len()));
                    }
                    s.set_selection(Vec::new());
                    Ok(Step::Done)
                }
                Input::Point3(p3) => {
                    let proj = (p3 - self.ref_point).dot(self.ref_normal);
                    let h = if proj.abs() > 1e-6 { proj } else { (p3 - self.ref_point).len() };
                    if !h.is_finite() || h.abs() <= 1e-9 {
                        return Err(crate::EngineError::Other("Extrusion height must be non-zero.".into()));
                    }
                    extrude_handles_atomic(s, &self.profiles, DirectionSpec::Height(h), false, self.cmd_id)?;
                    s.set_selection(Vec::new());
                    Ok(Step::Done)
                }
                Input::Point(p) => {
                    let p3 = s.drawing_plane().embed(p, s.effective_drawing_plane_offset());
                    let proj = (p3 - self.ref_point).dot(self.ref_normal);
                    let h = if proj.abs() > 1e-6 { proj } else { (p3 - self.ref_point).len() };
                    if !h.is_finite() || h.abs() <= 1e-9 {
                        return Err(crate::EngineError::Other("Extrusion height must be non-zero.".into()));
                    }
                    extrude_handles_atomic(s, &self.profiles, DirectionSpec::Height(h), false, self.cmd_id)?;
                    s.set_selection(Vec::new());
                    Ok(Step::Done)
                }
                _ => Ok(Step::Continue),
            },
            ExtrudeStage::DirectionStart => match i {
                Input::Cancel => Ok(Step::Done),
                Input::Point3(p3) => {
                    self.dir_start = Some(p3);
                    self.stage = ExtrudeStage::DirectionEnd;
                    Ok(Step::Continue)
                }
                Input::Point(p) => {
                    self.dir_start = Some(s.drawing_plane().embed(p, s.effective_drawing_plane_offset()));
                    self.stage = ExtrudeStage::DirectionEnd;
                    Ok(Step::Continue)
                }
                _ => Ok(Step::Continue),
            },
            ExtrudeStage::DirectionEnd => match i {
                Input::Cancel => Ok(Step::Done),
                Input::Point3(p3) => {
                    let start = self.dir_start.unwrap_or(Vec3::ZERO);
                    let dir = p3 - start;
                    extrude_handles_atomic(s, &self.profiles, DirectionSpec::Vector(dir), false, self.cmd_id)?;
                    s.set_selection(Vec::new());
                    Ok(Step::Done)
                }
                Input::Point(p) => {
                    let start = self.dir_start.unwrap_or(Vec3::ZERO);
                    let p3 = s.drawing_plane().embed(p, s.effective_drawing_plane_offset());
                    let dir = p3 - start;
                    extrude_handles_atomic(s, &self.profiles, DirectionSpec::Vector(dir), false, self.cmd_id)?;
                    s.set_selection(Vec::new());
                    Ok(Step::Done)
                }
                _ => Ok(Step::Continue),
            },
        }
    }

    fn preview(&self, _s: &Session, _c: Vec2) -> Vec<EntityKind> {
        Vec::new()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StlStage {
    Select,
    AskUnit,
    AskSaveUnit,
    Path,
}

struct StlOutM {
    sel: SelectPhase,
    stage: StlStage,
    solids: Vec<Handle>,
    chosen_unit: Option<InsUnitInfo>,
    save_unit: bool,
}

impl StlOutM {
    fn new() -> Self {
        StlOutM {
            sel: SelectPhase::default(),
            stage: StlStage::Select,
            solids: Vec::new(),
            chosen_unit: None,
            save_unit: false,
        }
    }

    fn accept_solids(&mut self, s: &mut Session, raw: Vec<Handle>) -> Result<()> {
        let (solids, skipped, insunits) = {
            let doc = s.doc()?;
            let mut solids = Vec::new();
            let mut skipped = 0usize;
            for h in raw {
                match doc.entity(h).map(|e| &e.kind) {
                    Some(EntityKind::Solid3d(_)) => {
                        if !solids.contains(&h) {
                            solids.push(h);
                        }
                    }
                    Some(_) => skipped = skipped.saturating_add(1),
                    None => {}
                }
            }
            (solids, skipped, doc.header.i64("INSUNITS", 0))
        };
        if skipped > 0 {
            s.echo(format!("Skipped {skipped} non-solid object(s)."));
        }
        if solids.is_empty() {
            return Err(crate::EngineError::Other("Select at least one valid 3D Solid to export to STL.".into()));
        }
        self.solids = solids;

        if let Some(info) = insunits_info(insunits) {
            self.chosen_unit = Some(info);
            self.stage = StlStage::Path;
        } else {
            s.echo(format!(
                "Drawing units are unset (INSUNITS={insunits} \"{}\"). Choose the drawing unit before exporting STL.",
                insunits_name(insunits)
            ));
            self.stage = StlStage::AskUnit;
        }
        Ok(())
    }
}

impl Interactive for StlOutM {
    fn name(&self) -> &'static str {
        "STLOUT"
    }

    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        self.sel = SelectPhase::begin(s);
        if self.sel.done {
            let hs = std::mem::take(&mut self.sel.picked);
            if hs.is_empty() {
                return Ok(Step::Done);
            }
            self.accept_solids(s, hs)?;
        }
        Ok(Step::Continue)
    }

    fn prompt(&self, s: &Session) -> Prompt {
        match self.stage {
            StlStage::Select => self.sel.prompt(),
            StlStage::AskUnit => Prompt::new(
                "Drawing units are unset. Specify unit for STL export [Millimeters/Centimeters/Meters/Inches/Feet]:",
                Accept::TEXT,
            )
            .kw(&["Millimeters", "Centimeters", "Meters", "Inches", "Feet"]),
            StlStage::AskSaveUnit => Prompt::new("Save chosen unit into drawing (INSUNITS)? [Yes/No] <Yes>:", Accept::TEXT).kw(&["Yes", "No"]),
            StlStage::Path => {
                let title = s.state().map(|st| st.title.clone()).unwrap_or_default();
                let stem = std::path::Path::new(&title)
                    .file_stem()
                    .map(|n| n.to_string_lossy().to_string())
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| "Drawing".into());
                Prompt::new("Enter binary STL output file path (.stl):", Accept::TEXT)
                    .picker(crate::prompt::Picker::SaveFile { default_name: format!("{stem}.stl"), extension: "stl".into() })
            }
        }
    }

    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        match self.stage {
            StlStage::Select => match self.sel.feed(s, &i)? {
                SelOutcome::More => Ok(Step::Continue),
                SelOutcome::Empty => Ok(Step::Done),
                SelOutcome::Done(hs) => {
                    self.accept_solids(s, hs)?;
                    Ok(Step::Continue)
                }
            },
            StlStage::AskUnit => match i {
                Input::Cancel => Ok(Step::Done),
                Input::Text(t) | Input::Keyword(t) => {
                    let info = parse_insunits_choice(&t).ok_or_else(|| {
                        crate::EngineError::Other(
                            "Invalid or unitless choice; enter Millimeters (mm), Centimeters (cm), Meters (m), Inches (in), or Feet (ft).".into(),
                        )
                    })?;
                    self.chosen_unit = Some(info);
                    self.stage = StlStage::AskSaveUnit;
                    Ok(Step::Continue)
                }
                _ => Ok(Step::Continue),
            },
            StlStage::AskSaveUnit => match i {
                Input::Cancel => Ok(Step::Done),
                Input::Enter => {
                    self.save_unit = true;
                    self.stage = StlStage::Path;
                    Ok(Step::Continue)
                }
                Input::Text(t) | Input::Keyword(t) => {
                    let ans = t.trim().to_ascii_lowercase();
                    self.save_unit = match ans.as_str() {
                        "" | "y" | "yes" | "s" | "si" | "sí" => true,
                        "n" | "no" => false,
                        _ => return Err(crate::EngineError::Other("Please answer Yes or No.".into())),
                    };
                    self.stage = StlStage::Path;
                    Ok(Step::Continue)
                }
                _ => Ok(Step::Continue),
            },
            StlStage::Path => match i {
                Input::Cancel => Ok(Step::Done),
                Input::Text(raw_path) => {
                    let path_str = raw_path.trim().trim_matches('"');
                    if path_str.is_empty() {
                        return Err(crate::EngineError::Other("STL file path cannot be empty.".into()));
                    }
                    let unit = self
                        .chosen_unit
                        .ok_or_else(|| crate::EngineError::Other("Drawing units must be resolved before STL export.".into()))?;
                    let model_tol = (DEFAULT_STL_TOLERANCE_MM / unit.mm_per_unit).max(MIN_SAFE_MESH_TOLERANCE);
                    let out = execute_stl_write(
                        s,
                        &self.solids,
                        StlUnitChoice { info: unit, save_in_doc: self.save_unit },
                        model_tol,
                        path_str,
                        "stlout",
                    )?;
                    let tris = out["triangles_count"].as_u64().unwrap_or(0);
                    s.echo(format!(
                        "Exported {} solid(s) ({tris} triangles) in millimeters (source: {}) to {path_str}",
                        self.solids.len(),
                        unit.name
                    ));
                    Ok(Step::Done)
                }
                _ => Ok(Step::Continue),
            },
        }
    }

    fn preview(&self, _s: &Session, _c: Vec2) -> Vec<EntityKind> {
        Vec::new()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SphStage {
    Center,
    Radius,
}

impl Default for SphStage {
    fn default() -> Self {
        SphStage::Center
    }
}

#[derive(Default)]
pub struct SphereM {
    stage: SphStage,
    center: Option<Vec3>,
}

impl Interactive for SphereM {
    fn name(&self) -> &'static str {
        "SPHERE"
    }

    fn prompt(&self, _s: &Session) -> Prompt {
        match self.stage {
            SphStage::Center => Prompt::new("Specify center point or [3P/2P/Ttr]:", Accept::POINT).kw(&["3P", "2P", "Ttr"]),
            SphStage::Radius => Prompt::new("Specify radius or [Diameter]:", Accept::POINT_OR_NUMBER).kw(&["Diameter"]),
        }
    }

    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        match self.stage {
            SphStage::Center => {
                let pt = match i {
                    Input::Point3(p3) => p3,
                    Input::Point(p) => s.drawing_plane().embed(p, s.effective_drawing_plane_offset()),
                    Input::Cancel => return Ok(Step::Done),
                    _ => return Ok(Step::Continue),
                };
                self.center = Some(pt);
                self.stage = SphStage::Radius;
                Ok(Step::Continue)
            }
            SphStage::Radius => match i {
                Input::Text(t) => {
                    let r = crate::units::parse_distance(&t).ok_or_else(|| crate::EngineError::Other("Requires a distance.".into()))?;
                    if r <= 0.0 || !r.is_finite() {
                        return Err(crate::EngineError::Other("Radius must be positive and nonzero.".into()));
                    }
                    create_and_add_sphere(s, self.center.unwrap_or(Vec3::ZERO), r)?;
                    Ok(Step::Done)
                }
                Input::Point3(p3) => {
                    let center = self.center.unwrap_or(Vec3::ZERO);
                    let r = (p3 - center).len();
                    if r <= 1e-9 || !r.is_finite() {
                        return Err(crate::EngineError::Other("Radius must be positive and nonzero.".into()));
                    }
                    create_and_add_sphere(s, center, r)?;
                    Ok(Step::Done)
                }
                Input::Point(p) => {
                    let center = self.center.unwrap_or(Vec3::ZERO);
                    let p3 = s.drawing_plane().embed(p, s.effective_drawing_plane_offset());
                    let r = (p3 - center).len();
                    if r <= 1e-9 || !r.is_finite() {
                        return Err(crate::EngineError::Other("Radius must be positive and nonzero.".into()));
                    }
                    create_and_add_sphere(s, center, r)?;
                    Ok(Step::Done)
                }
                Input::Cancel => Ok(Step::Done),
                _ => Ok(Step::Continue),
            },
        }
    }

    fn preview(&self, _s: &Session, _c: Vec2) -> Vec<EntityKind> {
        Vec::new()
    }
}
