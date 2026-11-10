//! 3D solid modeling and inquiry commands (Milestone 2B).
//!
//! Exposes structured non-interactive JSON commands for AI/JARVIS control:
//! - `solid.box`
//! - `solid.cylinder`
//! - `solid.extrude`
//! - `solid.translate`
//! - `solid.union`
//! - `solid.subtract`
//! - `solid.intersect`
//! - `query.solid`
//! - `query.metrics`
//! - `export.stl`

use std::fs::File;
use std::path::Path;

use cadcraft_brep::{BrepSolid, MIN_SAFE_MESH_TOLERANCE, TriangleMesh};
use cadcraft_doc::{BooleanKind, Bounds3, EntityKind, FeatureOp, Handle, Recipe, Solid3d, Solid3dStatus};
use cadcraft_geom::{Vec2, Vec3};
use serde_json::{Value, json};

use super::bad;
use super::{CommandSpec, Result, Session};
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
        CommandSpec::new("export.stl", "Export STL", run_export_stl).params("{handles: [string], path: string, tolerance?: f64}").noundo(),
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

fn run_extrude(s: &mut Session, p: &Value) -> Result<Value> {
    let profile_handle = p.get("profile").and_then(parse_handle).ok_or_else(|| bad("solid.extrude", "`profile` handle is required"))?;

    let height_opt = p.get("height").and_then(Value::as_f64);
    let dir_opt = p.get("direction").and_then(parse_vec3);
    let direction = match (height_opt, dir_opt) {
        (Some(h), None) => {
            if !h.is_finite() || h.abs() <= 1e-9 {
                return Err(bad("solid.extrude", "`height` must be a finite, non-zero number"));
            }
            Vec3::new(0.0, 0.0, h)
        }
        (None, Some(d)) => {
            if !d.is_finite() || d.len() <= 1e-9 {
                return Err(bad("solid.extrude", "`direction` must be a finite, non-zero vector"));
            }
            d
        }
        (Some(h), Some(d)) => {
            if !d.is_finite() || d.len() <= 1e-9 {
                return Err(bad("solid.extrude", "`direction` must be a finite, non-zero vector"));
            }
            if !h.is_finite() || h.abs() <= 1e-9 {
                return Err(bad("solid.extrude", "`height` must be a finite, non-zero number"));
            }
            d.normalized() * h
        }
        (None, None) => {
            return Err(bad("solid.extrude", "either `height` or `direction` is required"));
        }
    };

    // Polyline plane is XY (at z = poly.elevation); extrusion direction must not be coplanar with it
    if direction.z.abs() <= 1e-9 {
        return Err(bad("solid.extrude", "extrusion direction cannot be coplanar with polyline plane (Z component must be non-zero)"));
    }

    let doc = s.doc()?;
    let entity =
        doc.entity(profile_handle).ok_or_else(|| bad("solid.extrude", format!("profile entity with handle {profile_handle:?} not found")))?;

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
        if let Some(last) = distinct_pts.last()
            && (last.x - v.p.x).abs() <= 1e-9
            && (last.y - v.p.y).abs() <= 1e-9
        {
            continue;
        }
        distinct_pts.push(v.p);
    }

    if distinct_pts.len() > 1
        && let (Some(first), Some(last)) = (distinct_pts.first(), distinct_pts.last())
        && (first.x - last.x).abs() <= 1e-9
        && (first.y - last.y).abs() <= 1e-9
    {
        distinct_pts.pop();
    }

    if distinct_pts.len() < 3 {
        return Err(bad("solid.extrude", "profile polyline must have at least 3 distinct vertices"));
    }

    // Validate simple (non-self-intersecting) polygon
    if let Err(err_msg) = cadcraft_geom::validate_simple_polygon(&distinct_pts) {
        return Err(bad("solid.extrude", format!("cannot extrude self-intersecting polyline: {err_msg}")));
    }

    let area = cadcraft_geom::shoelace(&distinct_pts);
    if area.abs() <= 1e-9 {
        return Err(bad("solid.extrude", "profile polyline has degenerate zero area"));
    }

    // Ensure CCW orientation in XY plane so that face normal points +Z
    if area < 0.0 {
        distinct_pts.reverse();
    }

    let z = poly.elevation;
    let profile: Vec<Vec3> = distinct_pts.into_iter().map(|p| Vec3::new(p.x, p.y, z)).collect();

    let mut recipe = Recipe::new();
    recipe.add_feature(1, FeatureOp::Extrusion { profile, direction });

    let evaluated = BrepSolid::evaluate_recipe(&recipe).map_err(|e| bad("solid.extrude", format!("evaluation failed: {e}")))?;
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

fn run_export_stl(s: &mut Session, p: &Value) -> Result<Value> {
    let path_str = p.get("path").and_then(Value::as_str).ok_or_else(|| bad("export.stl", "`path` is required"))?;
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

        let evaluated = BrepSolid::evaluate_recipe(&solid.recipe).map_err(|e| bad("export.stl", format!("solid {h:?} evaluation failed: {e}")))?;
        let mesh = evaluated.triangulate(tolerance).map_err(|e| bad("export.stl", format!("solid {h:?} triangulation failed: {e}")))?;

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
    combined_mesh.write_stl_binary(&mut file).map_err(|e| bad("export.stl", format!("failed writing STL data: {e}")))?;

    Ok(json!({
        "status": "ok",
        "path": path_str,
        "solids_count": solids_count,
        "triangles_count": combined_mesh.triangles.len()
    }))
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
}

impl Interactive for CylinderM {
    fn name(&self) -> &'static str {
        "CYLINDER"
    }

    fn prompt(&self, _s: &Session) -> Prompt {
        match self.stage {
            CylStage::Center => Prompt::new("Specify center point of base or [3P/2P/Ttr/Elliptical]:", Accept::POINT)
                .kw(&["3P", "2P", "Ttr", "Elliptical"]),
            CylStage::Radius => Prompt::new("Specify base radius or [Diameter]:", Accept::POINT_OR_NUMBER)
                .kw(&["Diameter"]),
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
                    Input::Text(t) => {
                        let r = crate::units::parse_distance(&t)
                            .ok_or_else(|| crate::EngineError::Other("Requires a distance.".into()))?;
                        if r <= 0.0 || !r.is_finite() {
                            return Err(crate::EngineError::Other("Radius must be positive and nonzero.".into()));
                        }
                        self.radius = Some(r);
                        self.stage = CylStage::HeightOrAxis;
                        Ok(Step::Continue)
                    }
                    Input::Point3(p3) => {
                        let center = self.center.unwrap_or(Vec3::ZERO);
                        let r = (p3 - center).len();
                        if r <= 1e-9 || !r.is_finite() {
                            return Err(crate::EngineError::Other("Radius must be positive and nonzero.".into()));
                        }
                        self.radius = Some(r);
                        self.stage = CylStage::HeightOrAxis;
                        Ok(Step::Continue)
                    }
                    Input::Point(p) => {
                        let center = self.center.unwrap_or(Vec3::ZERO);
                        let p3 = s.drawing_plane().embed(p, s.effective_drawing_plane_offset());
                        let r = (p3 - center).len();
                        if r <= 1e-9 || !r.is_finite() {
                            return Err(crate::EngineError::Other("Radius must be positive and nonzero.".into()));
                        }
                        self.radius = Some(r);
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
                        let axis = if h >= 0.0 { Vec3::new(0.0, 0.0, 1.0) } else { Vec3::new(0.0, 0.0, -1.0) };
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
