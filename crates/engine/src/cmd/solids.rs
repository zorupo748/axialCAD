//! 3D solid modeling and inquiry commands (Milestone 2B).
//!
//! Exposes structured non-interactive JSON commands for AI/JARVIS control:
//! - `solid.box`
//! - `solid.cylinder`
//! - `solid.translate`
//! - `query.solid`
//! - `query.metrics`
//! - `export.stl`

use std::fs::File;
use std::path::Path;

use cadcraft_brep::{BrepSolid, MIN_SAFE_MESH_TOLERANCE, TriangleMesh};
use cadcraft_doc::{Bounds3, EntityKind, FeatureOp, Handle, Recipe, Solid3d, Solid3dStatus};
use cadcraft_geom::Vec3;
use serde_json::{Value, json};

use super::bad;
use super::{CommandSpec, Result, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("solid.box", "Box", run_box)
            .params("{origin?: [x,y,z], size: [dx,dy,dz]}"),
        CommandSpec::new("solid.cylinder", "Cylinder", run_cylinder)
            .params("{center?: [x,y,z], radius: f64, height: f64, axis?: [x,y,z]}"),
        CommandSpec::new("solid.extrude", "Extrude Profile", run_extrude)
            .params("{profile: string|u64, height: f64}"),
        CommandSpec::new("solid.translate", "Translate Solid", run_translate)
            .params("{handle: string|u64, delta: [dx,dy,dz]}"),
        CommandSpec::new("query.solid", "Query Solid", run_query_solid)
            .params("{handle: string|u64}")
            .noundo(),
        CommandSpec::new("query.metrics", "Query Metrics", run_query_metrics)
            .params("{handle: string|u64}")
            .noundo(),
        CommandSpec::new("export.stl", "Export STL", run_export_stl)
            .params("{handles: [string], path: string, tolerance?: f64}")
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
    if let Some(s) = v.as_str() {
        Handle::parse_hex(s)
    } else if let Some(n) = v.as_u64() {
        Some(Handle(n))
    } else {
        None
    }
}

fn bbox_to_json(b: &Bounds3) -> Value {
    json!({
        "min": [b.min.x, b.min.y, b.min.z],
        "max": [b.max.x, b.max.y, b.max.z]
    })
}

fn run_box(s: &mut Session, p: &Value) -> Result<Value> {
    let origin = p.get("origin").and_then(parse_vec3).unwrap_or(Vec3::ZERO);
    let size = p.get("size").and_then(parse_vec3).or_else(|| {
        let dx = p.get("width")?.as_f64()?;
        let dy = p.get("depth").or_else(|| p.get("length"))?.as_f64()?;
        let dz = p.get("height")?.as_f64()?;
        if dx.is_finite() && dy.is_finite() && dz.is_finite() {
            Some(Vec3::new(dx, dy, dz))
        } else {
            None
        }
    }).ok_or_else(|| bad("solid.box", "`size` [dx, dy, dz] is required"))?;

    if size.x <= 0.0 || size.y <= 0.0 || size.z <= 0.0 {
        return Err(bad("solid.box", "box dimensions must be strictly positive"));
    }

    let mut recipe = Recipe::new();
    recipe.add_feature(1, FeatureOp::Box { origin, size });

    let evaluated = BrepSolid::evaluate_recipe(&recipe)
        .map_err(|e| bad("solid.box", format!("evaluation failed: {e}")))?;
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
    let radius = p.get("radius").and_then(Value::as_f64).filter(|r| r.is_finite() && *r > 0.0)
        .ok_or_else(|| bad("solid.cylinder", "`radius` (positive number) is required"))?;
    let height = p.get("height").and_then(Value::as_f64).filter(|h| h.is_finite() && *h > 0.0)
        .ok_or_else(|| bad("solid.cylinder", "`height` (positive number) is required"))?;
    let axis = p.get("axis").and_then(parse_vec3).unwrap_or(Vec3::new(0.0, 0.0, 1.0));

    let len_sq = axis.x * axis.x + axis.y * axis.y + axis.z * axis.z;
    if len_sq <= 1e-12 {
        return Err(bad("solid.cylinder", "`axis` vector cannot be zero-length"));
    }

    let mut recipe = Recipe::new();
    recipe.add_feature(1, FeatureOp::Cylinder { center, radius, height, axis });

    let evaluated = BrepSolid::evaluate_recipe(&recipe)
        .map_err(|e| bad("solid.cylinder", format!("evaluation failed: {e}")))?;
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

fn run_extrude(s: &mut Session, p: &Value) -> Result<Value> {
    let profile_handle = p.get("profile")
        .and_then(parse_handle)
        .ok_or_else(|| bad("solid.extrude", "`profile` handle is required"))?;
    let height = p.get("height")
        .and_then(Value::as_f64)
        .ok_or_else(|| bad("solid.extrude", "`height` is required"))?;

    if !height.is_finite() || height.abs() <= 1e-9 {
        return Err(bad("solid.extrude", "`height` must be a finite, non-zero number"));
    }

    let doc = s.doc()?;
    let entity = doc.entity(profile_handle)
        .ok_or_else(|| bad("solid.extrude", format!("profile entity with handle {profile_handle:?} not found")))?;

    let EntityKind::LwPolyline(poly) = &entity.kind else {
        return Err(bad("solid.extrude", "profile entity must be a closed 2D polyline (LwPolyline)"));
    };

    if !poly.closed {
        return Err(bad("solid.extrude", "profile polyline must be closed"));
    }

    if poly.vertices.iter().any(|v| v.bulge.abs() > 1e-9) {
        return Err(bad("solid.extrude", "polylines with arc segments (bulges) are not supported in this version"));
    }

    let mut distinct_pts: Vec<cadcraft_geom::Vec2> = Vec::new();
    for v in &poly.vertices {
        if !v.p.x.is_finite() || !v.p.y.is_finite() {
            return Err(bad("solid.extrude", "polyline vertex coordinates must be finite"));
        }
        if let Some(last) = distinct_pts.last() {
            if (last.x - v.p.x).abs() <= 1e-9 && (last.y - v.p.y).abs() <= 1e-9 {
                continue;
            }
        }
        distinct_pts.push(v.p);
    }

    if distinct_pts.len() > 1 {
        if let (Some(first), Some(last)) = (distinct_pts.first(), distinct_pts.last()) {
            if (first.x - last.x).abs() <= 1e-9 && (first.y - last.y).abs() <= 1e-9 {
                distinct_pts.pop();
            }
        }
    }

    if distinct_pts.len() < 3 {
        return Err(bad("solid.extrude", "profile polyline must have at least 3 distinct vertices"));
    }

    let mut signed_area2 = 0.0;
    let n = distinct_pts.len();
    for i in 0..n {
        let p0 = distinct_pts[i];
        let p1 = distinct_pts[(i + 1) % n];
        signed_area2 += p0.x * p1.y - p1.x * p0.y;
    }
    if signed_area2.abs() <= 1e-9 {
        return Err(bad("solid.extrude", "profile polyline has degenerate zero area"));
    }

    // Ensure CCW orientation in XY plane so that face normal points +Z
    if signed_area2 < 0.0 {
        distinct_pts.reverse();
    }

    let z = poly.elevation;
    let profile: Vec<Vec3> = distinct_pts.into_iter().map(|p| Vec3::new(p.x, p.y, z)).collect();
    let direction = Vec3::new(0.0, 0.0, height);

    let mut recipe = Recipe::new();
    recipe.add_feature(1, FeatureOp::Extrusion { profile, direction });

    let evaluated = BrepSolid::evaluate_recipe(&recipe)
        .map_err(|e| bad("solid.extrude", format!("evaluation failed: {e}")))?;
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

fn run_translate(s: &mut Session, p: &Value) -> Result<Value> {
    let handle = p.get("handle").and_then(parse_handle)
        .ok_or_else(|| bad("solid.translate", "`handle` is required"))?;
    let delta = p.get("delta").and_then(parse_vec3)
        .ok_or_else(|| bad("solid.translate", "`delta` [dx, dy, dz] is required"))?;

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
    let handle = p.get("handle").and_then(parse_handle)
        .ok_or_else(|| bad("query.solid", "`handle` is required"))?;

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
    let handle = p.get("handle").and_then(parse_handle)
        .ok_or_else(|| bad("query.metrics", "`handle` is required"))?;

    let doc = s.doc()?;
    let ent = doc.entity(handle).ok_or_else(|| bad("query.metrics", format!("entity {handle:?} not found")))?;
    let EntityKind::Solid3d(solid) = &ent.kind else {
        return Err(bad("query.metrics", format!("entity {handle:?} is not a 3D Solid")));
    };

    let evaluated = BrepSolid::evaluate_recipe(&solid.recipe)
        .map_err(|e| bad("query.metrics", format!("failed to evaluate solid: {e}")))?;

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

fn run_export_stl(s: &mut Session, p: &Value) -> Result<Value> {
    let path_str = p.get("path").and_then(Value::as_str)
        .ok_or_else(|| bad("export.stl", "`path` is required"))?;
    let path = Path::new(path_str);

    let tolerance = p.get("tolerance").and_then(Value::as_f64).unwrap_or(0.01).max(MIN_SAFE_MESH_TOLERANCE);

    let mut handles = Vec::new();
    if let Some(arr) = p.get("handles").and_then(Value::as_array) {
        for v in arr {
            if let Some(h) = parse_handle(v) {
                handles.push(h);
            }
        }
    } else if let Some(h) = p.get("handle").and_then(parse_handle) {
        handles.push(h);
    }

    if handles.is_empty() {
        return Err(bad("export.stl", "at least one valid solid `handle` or `handles` array is required"));
    }

    let doc = s.doc()?;
    let mut combined_mesh = TriangleMesh::default();
    let mut solids_count: usize = 0;

    for h in &handles {
        let ent = doc.entity(*h).ok_or_else(|| bad("export.stl", format!("entity {h:?} not found")))?;
        let EntityKind::Solid3d(solid) = &ent.kind else {
            return Err(bad("export.stl", format!("entity {h:?} is not a 3D Solid")));
        };

        let evaluated = BrepSolid::evaluate_recipe(&solid.recipe)
            .map_err(|e| bad("export.stl", format!("solid {h:?} evaluation failed: {e}")))?;
        let mesh = evaluated.triangulate(tolerance)
            .map_err(|e| bad("export.stl", format!("solid {h:?} triangulation failed: {e}")))?;

        let base_vertex = combined_mesh.positions.len();
        combined_mesh.positions.extend(mesh.positions);
        combined_mesh.normals.extend(mesh.normals);
        for tri in mesh.triangles {
            combined_mesh.triangles.push([
                tri[0].saturating_add(base_vertex),
                tri[1].saturating_add(base_vertex),
                tri[2].saturating_add(base_vertex),
            ]);
        }
        solids_count = solids_count.saturating_add(1);
    }

    let mut file = File::create(path).map_err(|e| bad("export.stl", format!("failed to create output file: {e}")))?;
    combined_mesh.write_stl_binary(&mut file)
        .map_err(|e| bad("export.stl", format!("failed writing STL data: {e}")))?;

    Ok(json!({
        "status": "ok",
        "path": path_str,
        "solids_count": solids_count,
        "triangles_count": combined_mesh.triangles.len()
    }))
}
