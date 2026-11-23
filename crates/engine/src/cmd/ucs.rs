//! User Coordinate System (UCS / SCP) commands.
//!
//! Provides interactive and programmatic definition of coordinate systems:
//! - 3-Point UCS: Origin, point on positive X axis, point on positive Y side of XY plane.
//! - 2-Point UCS: Origin and point on positive X axis (Y/Z derived deterministically).
//! - Axis vectors: Specify origin (or keep current) and direction of X, Y, or Z axis.
//! - Local Rotations: Rotate around local X, Y, or Z axis by numeric angle or interactive drag.
//! - Predefined Orientations: Top, Bottom, Front, Back, Left, Right (always defined relative to WCS).
//! - Alignment: Align to entity (Line, Arc, Circle, Face) or 3D face (cycling faces and flips).
//! - Named UCS: Save, restore, rename, delete, and list named coordinate systems.
//! - World / Previous: Restore WCS or previous UCS from history (last 10).
//! - Live continuous preview: Both the candidate UCS triad and the camera view orientation
//!   update in real time before clicking, without cursor/camera feedback oscillation.

use cadcraft_doc::{EntityKind, Line};
use cadcraft_geom::{Axis3D, OrbitCamera, UcsFrame, Vec2, Vec3};
use serde_json::{Value, json};

use super::*;
use crate::{Accept, EngineError, Input, Interactive, Prompt, Result, Session, Step};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("ucs", "User Coordinate System", run_ucs)
            .alias(&["scp"])
            .menu(&["Tools", "New UCS", "3 Points"])
            .params("{option?: string, origin?: [x,y,z], x_axis?: [x,y,z], y_pt?: [x,y,z], angle?: number, name?: string, preset?: string}")
            .noundo()
            .transparent()
            .interactive(|_| Ok(Box::new(UcsM::default()))),
        CommandSpec::new("ucs.3point", "UCS 3 Points", run_ucs_3point)
            .alias(&["scp.3puntos"])
            .menu(&["Tools", "New UCS", "3 Points"])
            .noundo()
            .transparent()
            .interactive(|_| Ok(Box::new(Ucs3PointM::default()))),
        CommandSpec::new("ucs.2point", "UCS 2 Points", run_ucs_2point)
            .alias(&["scp.2puntos"])
            .menu(&["Tools", "New UCS", "2 Points"])
            .noundo()
            .transparent()
            .interactive(|_| Ok(Box::new(Ucs2PointM::default()))),
        CommandSpec::new("ucs.origin", "UCS Origin", run_ucs_origin)
            .alias(&["scp.origen"])
            .menu(&["Tools", "New UCS", "Origin"])
            .params("{origin: [x,y,z]}")
            .noundo()
            .transparent()
            .interactive(|_| Ok(Box::new(UcsOriginM::default()))),
        CommandSpec::new("ucs.xaxis", "UCS X-Axis Vector", run_ucs_xaxis)
            .alias(&["scp.ejex"])
            .menu(&["Tools", "New UCS", "X-Axis Vector"])
            .params("{origin?: [x,y,z], direction: [x,y,z]}")
            .noundo()
            .transparent()
            .interactive(|_| Ok(Box::new(UcsAxisM::new(Axis3D::X)))),
        CommandSpec::new("ucs.yaxis", "UCS Y-Axis Vector", run_ucs_yaxis)
            .alias(&["scp.ejey"])
            .menu(&["Tools", "New UCS", "Y-Axis Vector"])
            .params("{origin?: [x,y,z], direction: [x,y,z]}")
            .noundo()
            .transparent()
            .interactive(|_| Ok(Box::new(UcsAxisM::new(Axis3D::Y)))),
        CommandSpec::new("ucs.zaxis", "UCS Z-Axis Vector", run_ucs_zaxis)
            .alias(&["scp.ejez"])
            .menu(&["Tools", "New UCS", "Z-Axis Vector"])
            .params("{origin?: [x,y,z], direction: [x,y,z]}")
            .noundo()
            .transparent()
            .interactive(|_| Ok(Box::new(UcsAxisM::new(Axis3D::Z)))),
        CommandSpec::new("ucs.x", "Rotate UCS about X", run_ucs_rotate_x)
            .alias(&["scp.x"])
            .menu(&["Tools", "New UCS", "X"])
            .params("{angle?: number}")
            .noundo()
            .transparent()
            .interactive(|_| Ok(Box::new(UcsRotateM::new(Axis3D::X)))),
        CommandSpec::new("ucs.y", "Rotate UCS about Y", run_ucs_rotate_y)
            .alias(&["scp.y"])
            .menu(&["Tools", "New UCS", "Y"])
            .params("{angle?: number}")
            .noundo()
            .transparent()
            .interactive(|_| Ok(Box::new(UcsRotateM::new(Axis3D::Y)))),
        CommandSpec::new("ucs.z", "Rotate UCS about Z", run_ucs_rotate_z)
            .alias(&["scp.z"])
            .menu(&["Tools", "New UCS", "Z"])
            .params("{angle?: number}")
            .noundo()
            .transparent()
            .interactive(|_| Ok(Box::new(UcsRotateM::new(Axis3D::Z)))),
        CommandSpec::new("ucs.view", "UCS View", run_ucs_view)
            .alias(&["scp.vista"])
            .menu(&["Tools", "New UCS", "View"])
            .params("{}")
            .noundo()
            .transparent(),
        CommandSpec::new("ucs.preset", "UCS Preset", run_ucs_preset)
            .alias(&["scp.predefinido"])
            .params("{preset: \"top\"|\"bottom\"|\"front\"|\"back\"|\"left\"|\"right\"}")
            .noundo()
            .transparent(),
        CommandSpec::new("ucs.object", "UCS Object", run_ucs_object)
            .alias(&["scp.objeto"])
            .menu(&["Tools", "New UCS", "Object"])
            .noundo()
            .transparent()
            .interactive(|_| Ok(Box::new(UcsObjectM::default()))),
        CommandSpec::new("ucs.face", "UCS Face", run_ucs_face)
            .alias(&["scp.cara"])
            .menu(&["Tools", "New UCS", "Face"])
            .noundo()
            .transparent()
            .interactive(|_| Ok(Box::new(UcsFaceM::default()))),
        CommandSpec::new("ucs.zobject", "UCS Z-Axis by Object", run_ucs_zobject)
            .alias(&["scp.zejeobjeto"])
            .menu(&["Tools", "New UCS", "Z-Axis by Object"])
            .noundo()
            .transparent()
            .interactive(|_| Ok(Box::new(UcsZObjectM::default()))),
        CommandSpec::new("ucs.save", "Save Named UCS", run_ucs_save)
            .alias(&["scp.guardar"])
            .params("{name: string}")
            .noundo()
            .transparent()
            .interactive(|_| Ok(Box::new(UcsSaveM::default()))),
        CommandSpec::new("ucs.restore", "Restore Named UCS", run_ucs_restore)
            .alias(&["scp.restaurar"])
            .params("{name: string}")
            .noundo()
            .transparent()
            .interactive(|_| Ok(Box::new(UcsRestoreM::default()))),
        CommandSpec::new("ucs.delete", "Delete Named UCS", run_ucs_delete)
            .alias(&["scp.eliminar"])
            .params("{name: string}")
            .noundo()
            .transparent()
            .interactive(|_| Ok(Box::new(UcsDeleteM::default()))),
        CommandSpec::new("ucs.rename", "Rename Named UCS", run_ucs_rename)
            .alias(&["scp.renombrar"])
            .params("{old_name: string, new_name: string}")
            .noundo()
            .transparent()
            .interactive(|_| Ok(Box::new(UcsRenameM::default()))),
        CommandSpec::new("ucs.list", "List Named UCSs", run_ucs_list)
            .alias(&["scp.listar"])
            .params("{}")
            .noundo()
            .transparent(),
        CommandSpec::new("ucs.world", "UCS World", run_ucs_world)
            .alias(&["scp.universal", "scp.mundo"])
            .menu(&["Tools", "New UCS", "World"])
            .params("{}")
            .noundo()
            .transparent(),
        CommandSpec::new("ucs.previous", "UCS Previous", run_ucs_previous)
            .alias(&["scp.previo"])
            .menu(&["Tools", "New UCS", "Previous"])
            .params("{}")
            .noundo()
            .transparent(),
    ]
}

fn apply_confirmed_view(s: &mut Session, initial_cam: OrbitCamera, initial_ucs: UcsFrame, new_ucs: UcsFrame) {
    if s.settings.ucs_follow_view {
        let mut final_cam = initial_cam;
        final_cam.apply_ucs_delta(&initial_cam, &initial_ucs, &new_ucs, new_ucs.origin);
        s.set_camera(final_cam);
    }
}

pub(crate) fn run_ucs(s: &mut Session, p: &Value) -> Result<Value> {
    if let Some(opt) = p.get("option").and_then(Value::as_str) {
        match opt.trim().to_ascii_lowercase().as_str() {
            "w" | "world" | "universal" | "mundo" => return run_ucs_world(s, p),
            "p" | "prev" | "previous" | "previo" => return run_ucs_previous(s, p),
            "v" | "view" | "vista" => return run_ucs_view(s, p),
            "o" | "m" | "origin" | "move" | "origen" => return run_ucs_origin(s, p),
            "2" | "2p" | "2point" => return run_ucs_2point(s, p),
            "3" | "3p" | "3point" => return run_ucs_3point(s, p),
            "x" => return run_ucs_rotate_x(s, p),
            "y" => return run_ucs_rotate_y(s, p),
            "z" => return run_ucs_rotate_z(s, p),
            "za" | "zaxis" => return run_ucs_zaxis(s, p),
            "ob" | "object" => return run_ucs_object(s, p),
            "f" | "face" => return run_ucs_face(s, p),
            "s" | "save" => return run_ucs_save(s, p),
            "r" | "restore" => return run_ucs_restore(s, p),
            "d" | "del" | "delete" => return run_ucs_delete(s, p),
            "ren" | "rename" => return run_ucs_rename(s, p),
            "?" | "l" | "list" => return run_ucs_list(s, p),
            _ => return Err(EngineError::Other(format!("Opción SCP desconocida `{opt}`."))),
        }
    }
    if p.get("origin").is_some() && p.get("x_axis").is_some() && p.get("y_pt").is_some() {
        return run_ucs_3point(s, p);
    }
    if p.get("origin").is_some() && p.get("x_axis").is_some() {
        return run_ucs_2point(s, p);
    }
    if p.get("origin").is_some() {
        return run_ucs_origin(s, p);
    }
    let cur = s.ucs();
    let is_w = cur.is_world();
    let name = if is_w { "World" } else { "Custom" };
    s.echo(format!("El SCP actual es {name}. Origen: {:.4}, {:.4}, {:.4}", cur.origin.x, cur.origin.y, cur.origin.z));
    Ok(json!({
        "status": "ok",
        "world": is_w,
        "origin": [cur.origin.x, cur.origin.y, cur.origin.z],
        "x_axis": [cur.x_axis.x, cur.x_axis.y, cur.x_axis.z],
        "y_axis": [cur.y_axis.x, cur.y_axis.y, cur.y_axis.z],
        "z_axis": [cur.z_axis.x, cur.z_axis.y, cur.z_axis.z],
    }))
}

pub(crate) fn run_ucs_world(s: &mut Session, _p: &Value) -> Result<Value> {
    let initial_cam = s.camera();
    let initial_ucs = s.ucs();
    s.set_ucs(UcsFrame::WORLD)?;
    apply_confirmed_view(s, initial_cam, initial_ucs, UcsFrame::WORLD);
    s.echo("Sistema de Coordenadas Universal (WCS) restaurado.");
    Ok(json!({ "status": "ok", "ucs": "world" }))
}

pub(crate) fn run_ucs_previous(s: &mut Session, _p: &Value) -> Result<Value> {
    let initial_cam = s.camera();
    let initial_ucs = s.ucs();
    if s.restore_previous_ucs()? {
        let cur = s.ucs();
        apply_confirmed_view(s, initial_cam, initial_ucs, cur);
        s.echo(format!("SCP anterior restaurado. Origen: {:.4}, {:.4}, {:.4}", cur.origin.x, cur.origin.y, cur.origin.z));
        Ok(json!({ "status": "ok", "restored": true }))
    } else {
        s.echo("No hay ningún SCP previo guardado en el historial.");
        Ok(json!({ "status": "ok", "restored": false }))
    }
}

pub(crate) fn run_ucs_origin(s: &mut Session, p: &Value) -> Result<Value> {
    let orig = point3_req("ucs.origin", p, "origin")?;
    let cur = s.ucs();
    let new_ucs = cur.with_origin(orig);
    let initial_cam = s.camera();
    s.set_ucs(new_ucs)?;
    apply_confirmed_view(s, initial_cam, cur, new_ucs);
    s.echo(format!("Origen del SCP trasladado a {:.4}, {:.4}, {:.4}.", orig.x, orig.y, orig.z));
    Ok(json!({ "status": "ok", "origin": [orig.x, orig.y, orig.z] }))
}

pub(crate) fn run_ucs_2point(s: &mut Session, p: &Value) -> Result<Value> {
    let o = point3_req("ucs.2point", p, "origin")?;
    let px = point3_opt(p, "x_axis")
        .or_else(|| point3_opt(p, "x_pt"))
        .ok_or_else(|| EngineError::BadParams {
            cmd: "ucs.2point".into(),
            msg: "`x_axis` ([x, y, z]) is required".into(),
        })?;
    let cur = s.ucs();
    let initial_cam = s.camera();
    let ucs = UcsFrame::from_2_points(o, px, &cur).map_err(|e| EngineError::Other(e.to_string()))?;
    s.set_ucs(ucs)?;
    apply_confirmed_view(s, initial_cam, cur, ucs);
    s.echo(format!("SCP definido por 2 puntos. Eje X hacia: {:.4}, {:.4}, {:.4}.", ucs.x_axis.x, ucs.x_axis.y, ucs.x_axis.z));
    Ok(json!({
        "status": "ok",
        "origin": [ucs.origin.x, ucs.origin.y, ucs.origin.z],
        "x_axis": [ucs.x_axis.x, ucs.x_axis.y, ucs.x_axis.z],
        "y_axis": [ucs.y_axis.x, ucs.y_axis.y, ucs.y_axis.z],
        "z_axis": [ucs.z_axis.x, ucs.z_axis.y, ucs.z_axis.z],
    }))
}

pub(crate) fn run_ucs_3point(s: &mut Session, p: &Value) -> Result<Value> {
    let o = point3_req("ucs.3point", p, "origin")?;
    let px = point3_req("ucs.3point", p, "x_axis")?;
    let py = point3_req("ucs.3point", p, "y_pt")?;
    let cur = s.ucs();
    let initial_cam = s.camera();
    let ucs = UcsFrame::from_3_points(o, px, py).map_err(|e| EngineError::Other(e.to_string()))?;
    s.set_ucs(ucs)?;
    apply_confirmed_view(s, initial_cam, cur, ucs);
    s.echo(format!("SCP definido por 3 puntos. Normal (Z): {:.4}, {:.4}, {:.4}.", ucs.z_axis.x, ucs.z_axis.y, ucs.z_axis.z));
    Ok(json!({
        "status": "ok",
        "origin": [ucs.origin.x, ucs.origin.y, ucs.origin.z],
        "x_axis": [ucs.x_axis.x, ucs.x_axis.y, ucs.x_axis.z],
        "y_axis": [ucs.y_axis.x, ucs.y_axis.y, ucs.y_axis.z],
        "z_axis": [ucs.z_axis.x, ucs.z_axis.y, ucs.z_axis.z],
    }))
}

pub(crate) fn run_ucs_xaxis(s: &mut Session, p: &Value) -> Result<Value> {
    run_ucs_axis(s, p, Axis3D::X)
}

pub(crate) fn run_ucs_yaxis(s: &mut Session, p: &Value) -> Result<Value> {
    run_ucs_axis(s, p, Axis3D::Y)
}

pub(crate) fn run_ucs_zaxis(s: &mut Session, p: &Value) -> Result<Value> {
    run_ucs_axis(s, p, Axis3D::Z)
}

fn point3_opt(p: &Value, key: &str) -> Option<Vec3> {
    let a = p.get(key)?.as_array()?;
    if a.len() < 3 {
        return None;
    }
    Some(Vec3::new(a.first()?.as_f64()?, a.get(1)?.as_f64()?, a.get(2)?.as_f64()?))
}

fn str_req(cmd: &str, p: &Value, key: &str) -> Result<String> {
    p.get(key)
        .and_then(Value::as_str)
        .map(|s| s.to_string())
        .ok_or_else(|| EngineError::Other(format!("{cmd}: `{key}` es obligatorio.")))
}

fn run_ucs_axis(s: &mut Session, p: &Value, axis: Axis3D) -> Result<Value> {
    let cur = s.ucs();
    let o = point3_opt(p, "origin").unwrap_or(cur.origin);
    let dir = point3_opt(p, "direction")
        .or_else(|| point3_opt(p, "axis_pt").map(|pt| pt - o))
        .ok_or_else(|| EngineError::BadParams {
            cmd: "ucs.axis".into(),
            msg: "`direction` ([x, y, z]) is required".into(),
        })?;
    let initial_cam = s.camera();
    let ucs = UcsFrame::from_axis_vector(o, dir, axis, &cur).map_err(|e| EngineError::Other(e.to_string()))?;
    s.set_ucs(ucs)?;
    apply_confirmed_view(s, initial_cam, cur, ucs);
    s.echo(format!("Eje {} del SCP definido. Origen: {:.4}, {:.4}, {:.4}.", axis.name(), o.x, o.y, o.z));
    Ok(json!({
        "status": "ok",
        "axis": axis.name(),
        "origin": [ucs.origin.x, ucs.origin.y, ucs.origin.z],
        "x_axis": [ucs.x_axis.x, ucs.x_axis.y, ucs.x_axis.z],
        "y_axis": [ucs.y_axis.x, ucs.y_axis.y, ucs.y_axis.z],
        "z_axis": [ucs.z_axis.x, ucs.z_axis.y, ucs.z_axis.z],
    }))
}

pub(crate) fn run_ucs_rotate_x(s: &mut Session, p: &Value) -> Result<Value> {
    run_ucs_rotate(s, p, Axis3D::X)
}

pub(crate) fn run_ucs_rotate_y(s: &mut Session, p: &Value) -> Result<Value> {
    run_ucs_rotate(s, p, Axis3D::Y)
}

pub(crate) fn run_ucs_rotate_z(s: &mut Session, p: &Value) -> Result<Value> {
    run_ucs_rotate(s, p, Axis3D::Z)
}

fn run_ucs_rotate(s: &mut Session, p: &Value, axis: Axis3D) -> Result<Value> {
    let deg = p.get("angle").and_then(Value::as_f64).unwrap_or(90.0);
    let rad = deg.to_radians();
    let cur = s.ucs();
    let initial_cam = s.camera();
    let new_ucs = match axis {
        Axis3D::X => cur.rotated_x(rad),
        Axis3D::Y => cur.rotated_y(rad),
        Axis3D::Z => cur.rotated_z(rad),
    };
    s.set_ucs(new_ucs)?;
    apply_confirmed_view(s, initial_cam, cur, new_ucs);
    s.echo(format!("SCP girado {:.2}° alrededor del eje {}.", deg, axis.name()));
    Ok(json!({
        "status": "ok",
        "axis": axis.name(),
        "angle": deg,
        "origin": [new_ucs.origin.x, new_ucs.origin.y, new_ucs.origin.z],
        "x_axis": [new_ucs.x_axis.x, new_ucs.x_axis.y, new_ucs.x_axis.z],
        "y_axis": [new_ucs.y_axis.x, new_ucs.y_axis.y, new_ucs.y_axis.z],
        "z_axis": [new_ucs.z_axis.x, new_ucs.z_axis.y, new_ucs.z_axis.z],
    }))
}

pub(crate) fn run_ucs_view(s: &mut Session, _p: &Value) -> Result<Value> {
    let cam = s.camera();
    let x_axis = cam.right_dir();
    let y_axis = cam.up_dir();
    let z_axis = cam.view_dir();
    let orig = s.ucs().origin;
    let new_ucs = UcsFrame::new(orig, x_axis, y_axis, z_axis);
    s.set_ucs(new_ucs)?;
    s.echo("SCP ajustado a la dirección de la vista actual.");
    Ok(json!({
        "status": "ok",
        "ucs": "view",
        "origin": [orig.x, orig.y, orig.z],
        "x_axis": [x_axis.x, x_axis.y, x_axis.z],
        "y_axis": [y_axis.x, y_axis.y, y_axis.z],
        "z_axis": [z_axis.x, z_axis.y, z_axis.z],
    }))
}

pub(crate) fn run_ucs_preset(s: &mut Session, p: &Value) -> Result<Value> {
    let name = str_req("ucs.preset", p, "preset")
        .or_else(|_| str_req("ucs.preset", p, "name"))?;
    let preset = UcsFrame::preset(&name).ok_or_else(|| {
        EngineError::Other(format!(
            "Orientación predefinida desconocida `{name}`. Opciones respecto a WCS: superior, inferior, frontal, posterior, izquierda, derecha."
        ))
    })?;
    let initial_cam = s.camera();
    let initial_ucs = s.ucs();
    s.set_ucs(preset)?;
    apply_confirmed_view(s, initial_cam, initial_ucs, preset);
    s.echo(format!("SCP establecido a la orientación predefinida `{name}` (referenciada a WCS)."));
    Ok(json!({ "status": "ok", "preset": name }))
}

pub(crate) fn run_ucs_object(s: &mut Session, p: &Value) -> Result<Value> {
    if let Some(h) = p.get("handle").and_then(Value::as_u64) {
        let handle = cadcraft_doc::Handle(h);
        let ent = {
            let doc = s.doc()?;
            doc.model.get(handle).cloned().ok_or_else(|| EngineError::Other("Entidad no encontrada.".into()))?
        };
        let cur = s.ucs();
        let initial_cam = s.camera();
        let new_ucs = match &ent.kind {
            EntityKind::Line(l) => {
                let dir = l.b - l.a;
                UcsFrame::from_axis_vector(l.a, dir, Axis3D::X, &cur)
                    .map_err(|e| EngineError::Other(e.to_string()))?
            }
            EntityKind::Circle(c) => {
                let norm = ent.common.extrusion;
                UcsFrame::from_axis_vector(c.center, norm, Axis3D::Z, &cur)
                    .map_err(|e| EngineError::Other(e.to_string()))?
            }
            EntityKind::Arc(a) => {
                let norm = ent.common.extrusion;
                UcsFrame::from_axis_vector(a.center, norm, Axis3D::Z, &cur)
                    .map_err(|e| EngineError::Other(e.to_string()))?
            }
            EntityKind::Face3d(f) => {
                UcsFrame::align_to_face(&f.corners, false, false)
                    .map_err(|e| EngineError::Other(e.to_string()))?
            }
            _ => return Err(EngineError::Other("Tipo de objeto no soportado para alinear SCP.".into())),
        };
        s.set_ucs(new_ucs)?;
        apply_confirmed_view(s, initial_cam, cur, new_ucs);
        s.echo("SCP alineado con el objeto seleccionado.");
        return Ok(json!({ "status": "ok", "aligned_to": ent.kind.type_name() }));
    }
    s.echo("Especifique un objeto para alinear el SCP.");
    Ok(json!({ "status": "interactive" }))
}

pub(crate) fn run_ucs_face(s: &mut Session, p: &Value) -> Result<Value> {
    if let Some(pts) = p.get("corners").and_then(Value::as_array) {
        let mut corners = Vec::new();
        for val in pts {
            let pt = point3_req("ucs.face", val, "point")?;
            corners.push(pt);
        }
        let flip_x = p.get("flip_x").and_then(Value::as_bool).unwrap_or(false);
        let flip_y = p.get("flip_y").and_then(Value::as_bool).unwrap_or(false);
        let cur = s.ucs();
        let initial_cam = s.camera();
        let new_ucs = UcsFrame::align_to_face(&corners, flip_x, flip_y)
            .map_err(|e| EngineError::Other(e.to_string()))?;
        s.set_ucs(new_ucs)?;
        apply_confirmed_view(s, initial_cam, cur, new_ucs);
        s.echo("SCP alineado con la cara seleccionada.");
        return Ok(json!({ "status": "ok", "face": true }));
    }
    s.echo("Especifique una cara para alinear el SCP.");
    Ok(json!({ "status": "interactive" }))
}

pub(crate) fn run_ucs_zobject(s: &mut Session, p: &Value) -> Result<Value> {
    if let Some(h) = p.get("handle").and_then(Value::as_u64) {
        let handle = cadcraft_doc::Handle(h);
        let ent = {
            let doc = s.doc()?;
            doc.model.get(handle).cloned().ok_or_else(|| EngineError::Other("Entidad no encontrada.".into()))?
        };
        let cur = s.ucs();
        let initial_cam = s.camera();
        let new_ucs = match &ent.kind {
            EntityKind::Line(l) => {
                let dir = (l.b - l.a).normalized();
                UcsFrame::from_axis_vector(cur.origin, dir, Axis3D::Z, &cur)
                    .map_err(|e| EngineError::Other(e.to_string()))?
            }
            EntityKind::Circle(_c) => {
                let norm = ent.common.extrusion;
                UcsFrame::from_axis_vector(cur.origin, norm, Axis3D::Z, &cur)
                    .map_err(|e| EngineError::Other(e.to_string()))?
            }
            EntityKind::Arc(_a) => {
                let norm = ent.common.extrusion;
                UcsFrame::from_axis_vector(cur.origin, norm, Axis3D::Z, &cur)
                    .map_err(|e| EngineError::Other(e.to_string()))?
            }
            EntityKind::Face3d(f) => {
                let v1 = f.corners[1] - f.corners[0];
                let v2 = f.corners[3] - f.corners[0];
                let norm = v1.cross(v2).normalized();
                UcsFrame::from_axis_vector(cur.origin, norm, Axis3D::Z, &cur)
                    .map_err(|e| EngineError::Other(e.to_string()))?
            }
            _ => return Err(EngineError::Other("Tipo de objeto no soportado para definir dirección Z.".into())),
        };
        s.set_ucs(new_ucs)?;
        apply_confirmed_view(s, initial_cam, cur, new_ucs);
        s.echo(format!("Eje Z del SCP alineado con objeto {}.", ent.kind.type_name()));
        return Ok(json!({ "status": "ok", "aligned_to": ent.kind.type_name() }));
    }
    s.echo("Especifique un objeto para definir la dirección del eje Z del SCP.");
    Ok(json!({ "status": "interactive" }))
}

pub(crate) fn run_ucs_save(s: &mut Session, p: &Value) -> Result<Value> {
    let name = str_req("ucs.save", p, "name")?;
    s.save_named_ucs(&name)?;
    s.echo(format!("SCP actual guardado con el nombre `{name}`."));
    Ok(json!({ "status": "ok", "saved": name }))
}

pub(crate) fn run_ucs_restore(s: &mut Session, p: &Value) -> Result<Value> {
    let name = str_req("ucs.restore", p, "name")?;
    if s.restore_named_ucs(&name)? {
        let cur = s.ucs();
        s.echo(format!("SCP `{name}` restaurado. Origen: {:.4}, {:.4}, {:.4}", cur.origin.x, cur.origin.y, cur.origin.z));
        Ok(json!({ "status": "ok", "restored": name }))
    } else {
        Err(EngineError::Other(format!("No existe ningún SCP guardado con el nombre `{name}`.")))
    }
}

pub(crate) fn run_ucs_delete(s: &mut Session, p: &Value) -> Result<Value> {
    let name = str_req("ucs.delete", p, "name")?;
    if s.delete_named_ucs(&name)? {
        s.echo(format!("SCP `{name}` eliminado."));
        Ok(json!({ "status": "ok", "deleted": name }))
    } else {
        Err(EngineError::Other(format!("No existe ningún SCP guardado con el nombre `{name}`.")))
    }
}

pub(crate) fn run_ucs_rename(s: &mut Session, p: &Value) -> Result<Value> {
    let old_name = str_req("ucs.rename", p, "old_name")?;
    let new_name = str_req("ucs.rename", p, "new_name")?;
    if s.rename_named_ucs(&old_name, &new_name)? {
        s.echo(format!("SCP `{old_name}` renombrado a `{new_name}`."));
        Ok(json!({ "status": "ok", "old_name": old_name, "new_name": new_name }))
    } else {
        Err(EngineError::Other(format!("No existe ningún SCP guardado con el nombre `{old_name}`.")))
    }
}

pub(crate) fn run_ucs_list(s: &mut Session, _p: &Value) -> Result<Value> {
    let list = s.named_ucss()?;
    if list.is_empty() {
        s.echo("No hay SCP guardados en este dibujo.");
    } else {
        s.echo(format!("SCP guardados ({}):", list.len()));
        for u in &list {
            s.echo(format!("  - {}: Origen=({:.3}, {:.3}, {:.3})", u.name, u.origin.x, u.origin.y, u.origin.z));
        }
    }
    let names: Vec<String> = list.into_iter().map(|u| u.name).collect();
    Ok(json!({ "status": "ok", "ucss": names.clone(), "names": names }))
}

// -----------------------------------------------------------------------------
// Interactive machines
// -----------------------------------------------------------------------------

#[derive(Default)]
pub(crate) struct UcsM {
    sub: Option<Box<dyn Interactive>>,
}

impl Interactive for UcsM {
    fn name(&self) -> &'static str {
        "UCS"
    }

    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        if let Some(sub) = &mut self.sub {
            sub.begin(s)
        } else {
            Ok(Step::Continue)
        }
    }

    fn prompt(&self, s: &Session) -> Prompt {
        if let Some(sub) = &self.sub {
            return sub.prompt(s);
        }
        let cur = if s.ucs().is_world() { "World" } else { "Custom" };
        Prompt::new(
            format!("Specify origin of UCS or [Face/NAmed/OBject/Previous/View/World/X/Y/Z/ZAxis/2point/3point] <{cur}>:"),
            Accept { enter: true, point: true, text: true, ..Accept::default() },
        )
        .kw(&[
            "Face", "NAmed", "OBject", "Previous", "View", "World", "X", "Y", "Z", "ZAxis", "2point", "3point",
        ])
        .default(cur)
    }

    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        if let Some(sub) = &mut self.sub {
            let res = sub.input(s, i)?;
            if matches!(res, Step::Done | Step::Cancel) {
                self.sub = None;
            }
            return Ok(res);
        }
        match i {
            Input::Enter => {
                let _ = run_ucs_world(s, &json!({}))?;
                Ok(Step::Done)
            }
            Input::Cancel => Ok(Step::Cancel),
            Input::Keyword(k) | Input::Text(k) => match k.trim().to_ascii_lowercase().as_str() {
                "3" | "3p" | "3point" => {
                    let mut m = Box::new(Ucs3PointM::default());
                    let _ = m.begin(s)?;
                    self.sub = Some(m);
                    Ok(Step::Continue)
                }
                "2" | "2p" | "2point" => {
                    let mut m = Box::new(Ucs2PointM::default());
                    let _ = m.begin(s)?;
                    self.sub = Some(m);
                    Ok(Step::Continue)
                }
                "f" | "face" => {
                    let mut m = Box::new(UcsFaceM::default());
                    let _ = m.begin(s)?;
                    self.sub = Some(m);
                    Ok(Step::Continue)
                }
                "ob" | "object" => {
                    let mut m = Box::new(UcsObjectM::default());
                    let _ = m.begin(s)?;
                    self.sub = Some(m);
                    Ok(Step::Continue)
                }
                "x" => {
                    let mut m = Box::new(UcsRotateM::new(Axis3D::X));
                    let _ = m.begin(s)?;
                    self.sub = Some(m);
                    Ok(Step::Continue)
                }
                "y" => {
                    let mut m = Box::new(UcsRotateM::new(Axis3D::Y));
                    let _ = m.begin(s)?;
                    self.sub = Some(m);
                    Ok(Step::Continue)
                }
                "z" => {
                    let mut m = Box::new(UcsRotateM::new(Axis3D::Z));
                    let _ = m.begin(s)?;
                    self.sub = Some(m);
                    Ok(Step::Continue)
                }
                "za" | "zaxis" => {
                    let mut m = Box::new(UcsAxisM::new(Axis3D::Z));
                    let _ = m.begin(s)?;
                    self.sub = Some(m);
                    Ok(Step::Continue)
                }
                "m" | "move" | "o" | "origin" => {
                    let mut m = Box::new(UcsOriginM::default());
                    let _ = m.begin(s)?;
                    self.sub = Some(m);
                    Ok(Step::Continue)
                }
                "w" | "world" => {
                    let _ = run_ucs_world(s, &json!({}))?;
                    Ok(Step::Done)
                }
                "p" | "prev" | "previous" => {
                    let _ = run_ucs_previous(s, &json!({}))?;
                    Ok(Step::Done)
                }
                "v" | "view" | "vista" => {
                    let _ = run_ucs_view(s, &json!({}))?;
                    Ok(Step::Done)
                }
                "na" | "named" => {
                    s.echo("Opciones de SCP con nombre: [Save/Restore/Delete/Rename/?]");
                    Ok(Step::Continue)
                }
                _ => {
                    s.echo("Opción no válida. Especifique 3point, 2point, Move, Face, Object, Previous, View, World, X, Y, Z, o ZAxis.");
                    Ok(Step::Continue)
                }
            },
            Input::Point(p) => {
                let orig = s.ucs().to_world2(p);
                let cur = s.ucs();
                let initial_cam = s.camera();
                let new_ucs = cur.with_origin(orig);
                s.set_ucs(new_ucs)?;
                apply_confirmed_view(s, initial_cam, cur, new_ucs);
                s.echo(format!("Origen del SCP trasladado a {:.4}, {:.4}, {:.4}.", orig.x, orig.y, orig.z));
                Ok(Step::Done)
            }
            Input::Point3(p3) => {
                let cur = s.ucs();
                let initial_cam = s.camera();
                let new_ucs = cur.with_origin(p3);
                s.set_ucs(new_ucs)?;
                apply_confirmed_view(s, initial_cam, cur, new_ucs);
                s.echo(format!("Origen del SCP trasladado a {:.4}, {:.4}, {:.4}.", p3.x, p3.y, p3.z));
                Ok(Step::Done)
            }
            _ => Ok(Step::Continue),
        }
    }

    fn preview(&self, s: &Session, c: Vec2) -> Vec<EntityKind> {
        if let Some(sub) = &self.sub {
            sub.preview(s, c)
        } else {
            Vec::new()
        }
    }

    fn preview_ucs(&self, s: &Session) -> Option<UcsFrame> {
        self.sub.as_ref()?.preview_ucs(s)
    }

    fn preview_camera(&self, s: &Session) -> Option<OrbitCamera> {
        self.sub.as_ref()?.preview_camera(s)
    }
}

// -----------------------------------------------------------------------------
// UcsOriginM
// -----------------------------------------------------------------------------

#[derive(Default)]
pub(crate) struct UcsOriginM {
    pub initial_cam: Option<OrbitCamera>,
    pub initial_ucs: Option<UcsFrame>,
}

impl Interactive for UcsOriginM {
    fn name(&self) -> &'static str {
        "UCS.ORIGIN"
    }

    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        self.initial_cam = Some(s.camera());
        self.initial_ucs = Some(s.ucs());
        Ok(Step::Continue)
    }

    fn prompt(&self, s: &Session) -> Prompt {
        let o = s.ucs().origin;
        Prompt::new(
            format!("Specify new origin point <{:.4},{:.4},{:.4}>:", o.x, o.y, o.z),
            Accept { enter: true, point: true, ..Accept::default() },
        )
        .base3_opt(Some(o))
    }

    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        let init_cam = self.initial_cam.unwrap_or_else(|| s.camera());
        let init_ucs = self.initial_ucs.unwrap_or_else(|| s.ucs());
        match i {
            Input::Enter => Ok(Step::Done),
            Input::Cancel => {
                s.set_camera(init_cam);
                s.set_ucs(init_ucs)?;
                Ok(Step::Cancel)
            }
            Input::Point(p) => {
                let orig = s.ucs().to_world2(p);
                let new_ucs = init_ucs.with_origin(orig);
                s.set_ucs(new_ucs)?;
                apply_confirmed_view(s, init_cam, init_ucs, new_ucs);
                s.echo(format!("Origen del SCP trasladado a {:.4}, {:.4}, {:.4}.", orig.x, orig.y, orig.z));
                Ok(Step::Done)
            }
            Input::Point3(p3) => {
                let new_ucs = init_ucs.with_origin(p3);
                s.set_ucs(new_ucs)?;
                apply_confirmed_view(s, init_cam, init_ucs, new_ucs);
                s.echo(format!("Origen del SCP trasladado a {:.4}, {:.4}, {:.4}.", p3.x, p3.y, p3.z));
                Ok(Step::Done)
            }
            _ => Ok(Step::Continue),
        }
    }

    fn preview_ucs(&self, s: &Session) -> Option<UcsFrame> {
        let init = self.initial_ucs.unwrap_or_else(|| s.ucs());
        Some(init.with_origin(s.cursor3))
    }

    fn preview_camera(&self, _s: &Session) -> Option<OrbitCamera> {
        // Preserves camera framing without jumping while picking origin
        None
    }

    fn preview(&self, s: &Session, _c: Vec2) -> Vec<EntityKind> {
        let cur = s.cursor3;
        let init = self.initial_ucs.unwrap_or_else(|| s.ucs());
        build_triad_preview(cur, init.x_axis, init.y_axis, init.z_axis, 2.0)
    }
}

// -----------------------------------------------------------------------------
// Ucs2PointM
// -----------------------------------------------------------------------------

#[derive(Default)]
pub(crate) struct Ucs2PointM {
    pub initial_cam: Option<OrbitCamera>,
    pub initial_ucs: Option<UcsFrame>,
    pub origin: Option<Vec3>,
}

impl Interactive for Ucs2PointM {
    fn name(&self) -> &'static str {
        "UCS.2POINT"
    }

    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        self.initial_cam = Some(s.camera());
        self.initial_ucs = Some(s.ucs());
        Ok(Step::Continue)
    }

    fn prompt(&self, s: &Session) -> Prompt {
        if self.origin.is_none() {
            let o = s.ucs().origin;
            Prompt::new(
                format!("Specify new origin point <{:.4},{:.4},{:.4}>:", o.x, o.y, o.z),
                Accept { enter: true, point: true, ..Accept::default() },
            )
            .base3_opt(Some(o))
        } else {
            let orig = self.origin.unwrap_or(Vec3::ZERO);
            Prompt::new("Specify point on positive X-axis:", Accept { point: true, ..Accept::default() })
                .base3(orig)
        }
    }

    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        let init_cam = self.initial_cam.unwrap_or_else(|| s.camera());
        let init_ucs = self.initial_ucs.unwrap_or_else(|| s.ucs());
        let pt = match i {
            Input::Enter if self.origin.is_none() => s.ucs().origin,
            Input::Cancel => {
                s.set_camera(init_cam);
                s.set_ucs(init_ucs)?;
                return Ok(Step::Cancel);
            }
            Input::Point(p) => s.ucs().to_world2(p),
            Input::Point3(p3) => p3,
            _ => return Ok(Step::Continue),
        };

        if self.origin.is_none() {
            self.origin = Some(pt);
            s.echo(format!("Origen establecido en {:.4}, {:.4}, {:.4}.", pt.x, pt.y, pt.z));
            Ok(Step::Continue)
        } else {
            let orig = self.origin.unwrap_or(Vec3::ZERO);
            match UcsFrame::from_2_points(orig, pt, &init_ucs) {
                Ok(new_ucs) => {
                    s.set_ucs(new_ucs)?;
                    apply_confirmed_view(s, init_cam, init_ucs, new_ucs);
                    s.echo(format!(
                        "Nuevo SCP definido por 2 puntos. Eje X hacia: {:.4}, {:.4}, {:.4}.",
                        new_ucs.x_axis.x, new_ucs.x_axis.y, new_ucs.x_axis.z
                    ));
                    Ok(Step::Done)
                }
                Err(err) => {
                    s.echo(err);
                    Ok(Step::Continue)
                }
            }
        }
    }

    fn preview_ucs(&self, s: &Session) -> Option<UcsFrame> {
        let init = self.initial_ucs.unwrap_or_else(|| s.ucs());
        if let Some(orig) = self.origin {
            if orig.dist(s.cursor3) > cadcraft_geom::EPS {
                UcsFrame::from_2_points(orig, s.cursor3, &init).ok()
            } else {
                Some(init.with_origin(orig))
            }
        } else {
            Some(init.with_origin(s.cursor3))
        }
    }

    fn preview_camera(&self, s: &Session) -> Option<OrbitCamera> {
        if !s.settings.ucs_follow_view || self.origin.is_none() {
            return None;
        }
        let init_cam = self.initial_cam.unwrap_or_else(|| s.camera());
        let init_ucs = self.initial_ucs.unwrap_or_else(|| s.ucs());
        let cand = self.preview_ucs(s)?;
        let pivot = self.origin.unwrap_or(cand.origin);
        let mut cam = init_cam;
        cam.apply_ucs_delta(&init_cam, &init_ucs, &cand, pivot);
        Some(cam)
    }

    fn preview(&self, s: &Session, _c: Vec2) -> Vec<EntityKind> {
        let cur = s.cursor3;
        let mut list = Vec::new();
        if let Some(orig) = self.origin {
            list.push(EntityKind::Line(Line { a: orig, b: cur }));
            if let Some(cand) = self.preview_ucs(s) {
                list.extend(build_triad_preview(orig, cand.x_axis, cand.y_axis, cand.z_axis, orig.dist(cur).max(1.0)));
            }
        } else {
            let init = self.initial_ucs.unwrap_or_else(|| s.ucs());
            list.extend(build_triad_preview(cur, init.x_axis, init.y_axis, init.z_axis, 2.0));
        }
        list
    }
}

// -----------------------------------------------------------------------------
// Ucs3PointM
// -----------------------------------------------------------------------------

#[derive(Default)]
pub(crate) struct Ucs3PointM {
    pub initial_cam: Option<OrbitCamera>,
    pub initial_ucs: Option<UcsFrame>,
    pub origin: Option<Vec3>,
    pub pt_x: Option<Vec3>,
}

impl Interactive for Ucs3PointM {
    fn name(&self) -> &'static str {
        "UCS.3POINT"
    }

    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        self.initial_cam = Some(s.camera());
        self.initial_ucs = Some(s.ucs());
        Ok(Step::Continue)
    }

    fn prompt(&self, s: &Session) -> Prompt {
        if self.origin.is_none() {
            let o = s.ucs().origin;
            Prompt::new(
                format!("Specify new origin point <{:.4},{:.4},{:.4}>:", o.x, o.y, o.z),
                Accept { enter: true, point: true, ..Accept::default() },
            )
            .base3_opt(Some(o))
        } else if self.pt_x.is_none() {
            let orig = self.origin.unwrap_or(Vec3::ZERO);
            Prompt::new("Specify point on X-axis:", Accept { point: true, ..Accept::default() })
                .base3(orig)
        } else {
            let orig = self.origin.unwrap_or(Vec3::ZERO);
            Prompt::new("Specify point on the XY plane with positive Y value:", Accept { point: true, ..Accept::default() })
                .base3(orig)
        }
    }

    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        let init_cam = self.initial_cam.unwrap_or_else(|| s.camera());
        let init_ucs = self.initial_ucs.unwrap_or_else(|| s.ucs());
        let pt = match i {
            Input::Enter if self.origin.is_none() => s.ucs().origin,
            Input::Cancel => {
                s.set_camera(init_cam);
                s.set_ucs(init_ucs)?;
                return Ok(Step::Cancel);
            }
            Input::Point(p) => s.ucs().to_world2(p),
            Input::Point3(p3) => p3,
            _ => return Ok(Step::Continue),
        };

        if self.origin.is_none() {
            self.origin = Some(pt);
            s.echo(format!("Origen establecido en {:.4}, {:.4}, {:.4}.", pt.x, pt.y, pt.z));
            Ok(Step::Continue)
        } else if self.pt_x.is_none() {
            let orig = self.origin.unwrap_or(Vec3::ZERO);
            if orig.dist(pt) < cadcraft_geom::EPS {
                s.echo("El punto del eje X no puede coincidir con el origen.");
                return Ok(Step::Continue);
            }
            self.pt_x = Some(pt);
            s.echo(format!("Punto del eje X establecido en {:.4}, {:.4}, {:.4}.", pt.x, pt.y, pt.z));
            Ok(Step::Continue)
        } else {
            let orig = self.origin.unwrap_or(Vec3::ZERO);
            let px = self.pt_x.unwrap_or(Vec3::ZERO);
            match UcsFrame::from_3_points(orig, px, pt) {
                Ok(new_ucs) => {
                    s.set_ucs(new_ucs)?;
                    apply_confirmed_view(s, init_cam, init_ucs, new_ucs);
                    s.echo(format!(
                        "Nuevo SCP definido por 3 puntos. Normal (Z): {:.4}, {:.4}, {:.4}.",
                        new_ucs.z_axis.x, new_ucs.z_axis.y, new_ucs.z_axis.z
                    ));
                    Ok(Step::Done)
                }
                Err(err_msg) => {
                    s.echo(err_msg);
                    Ok(Step::Continue)
                }
            }
        }
    }

    fn preview_ucs(&self, s: &Session) -> Option<UcsFrame> {
        let init = self.initial_ucs.unwrap_or_else(|| s.ucs());
        if let Some(orig) = self.origin {
            if let Some(px) = self.pt_x {
                if orig.dist(s.cursor3) > cadcraft_geom::EPS {
                    UcsFrame::from_3_points(orig, px, s.cursor3).ok()
                } else {
                    UcsFrame::from_2_points(orig, px, &init).ok()
                }
            } else if orig.dist(s.cursor3) > cadcraft_geom::EPS {
                UcsFrame::from_2_points(orig, s.cursor3, &init).ok()
            } else {
                Some(init.with_origin(orig))
            }
        } else {
            Some(init.with_origin(s.cursor3))
        }
    }

    fn preview_camera(&self, s: &Session) -> Option<OrbitCamera> {
        if !s.settings.ucs_follow_view || self.origin.is_none() {
            return None;
        }
        let init_cam = self.initial_cam.unwrap_or_else(|| s.camera());
        let init_ucs = self.initial_ucs.unwrap_or_else(|| s.ucs());
        let cand = self.preview_ucs(s)?;
        let pivot = self.origin.unwrap_or(cand.origin);
        let mut cam = init_cam;
        cam.apply_ucs_delta(&init_cam, &init_ucs, &cand, pivot);
        Some(cam)
    }

    fn preview(&self, s: &Session, _c: Vec2) -> Vec<EntityKind> {
        let cur_3d = s.cursor3;
        let mut list = Vec::new();
        if let Some(orig) = self.origin {
            if self.pt_x.is_none() {
                list.push(EntityKind::Line(Line { a: orig, b: cur_3d }));
                if let Some(cand) = self.preview_ucs(s) {
                    list.extend(build_triad_preview(orig, cand.x_axis, cand.y_axis, cand.z_axis, orig.dist(cur_3d).max(1.0)));
                }
            } else if let Some(px) = self.pt_x {
                list.push(EntityKind::Line(Line { a: orig, b: px }));
                list.push(EntityKind::Line(Line { a: orig, b: cur_3d }));
                if let Some(cand) = self.preview_ucs(s) {
                    list.extend(build_triad_preview(orig, cand.x_axis, cand.y_axis, cand.z_axis, orig.dist(px).max(1.0)));
                }
            }
        } else {
            let init = self.initial_ucs.unwrap_or_else(|| s.ucs());
            list.extend(build_triad_preview(cur_3d, init.x_axis, init.y_axis, init.z_axis, 2.0));
        }
        list
    }
}

// -----------------------------------------------------------------------------
// UcsAxisM (for X-Axis, Y-Axis, Z-Axis Vector)
// -----------------------------------------------------------------------------

pub(crate) struct UcsAxisM {
    pub axis: Axis3D,
    pub initial_cam: Option<OrbitCamera>,
    pub initial_ucs: Option<UcsFrame>,
    pub origin: Option<Vec3>,
}

impl UcsAxisM {
    pub fn new(axis: Axis3D) -> Self {
        Self { axis, initial_cam: None, initial_ucs: None, origin: None }
    }
}

impl Interactive for UcsAxisM {
    fn name(&self) -> &'static str {
        match self.axis {
            Axis3D::X => "UCS.XAXIS",
            Axis3D::Y => "UCS.YAXIS",
            Axis3D::Z => "UCS.ZAXIS",
        }
    }

    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        self.initial_cam = Some(s.camera());
        self.initial_ucs = Some(s.ucs());
        Ok(Step::Continue)
    }

    fn prompt(&self, s: &Session) -> Prompt {
        if self.origin.is_none() {
            let o = s.ucs().origin;
            Prompt::new(
                format!("Specify new origin for {}-axis or press ENTER to keep current <{:.4},{:.4},{:.4}>:", self.axis.name(), o.x, o.y, o.z),
                Accept { enter: true, point: true, ..Accept::default() },
            )
            .base3_opt(Some(o))
        } else {
            let orig = self.origin.unwrap_or(Vec3::ZERO);
            Prompt::new(
                format!("Specify point on positive {}-axis:", self.axis.name()),
                Accept { point: true, ..Accept::default() },
            )
            .base3(orig)
        }
    }

    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        let init_cam = self.initial_cam.unwrap_or_else(|| s.camera());
        let init_ucs = self.initial_ucs.unwrap_or_else(|| s.ucs());
        let pt = match i {
            Input::Enter if self.origin.is_none() => s.ucs().origin,
            Input::Cancel => {
                s.set_camera(init_cam);
                s.set_ucs(init_ucs)?;
                return Ok(Step::Cancel);
            }
            Input::Point(p) => s.ucs().to_world2(p),
            Input::Point3(p3) => p3,
            _ => return Ok(Step::Continue),
        };

        if self.origin.is_none() {
            self.origin = Some(pt);
            s.echo(format!("Origen para eje {} establecido en {:.4}, {:.4}, {:.4}.", self.axis.name(), pt.x, pt.y, pt.z));
            Ok(Step::Continue)
        } else {
            let orig = self.origin.unwrap_or(Vec3::ZERO);
            let dir = pt - orig;
            match UcsFrame::from_axis_vector(orig, dir, self.axis, &init_ucs) {
                Ok(new_ucs) => {
                    s.set_ucs(new_ucs)?;
                    apply_confirmed_view(s, init_cam, init_ucs, new_ucs);
                    s.echo(format!("Dirección del eje {} del SCP confirmada.", self.axis.name()));
                    Ok(Step::Done)
                }
                Err(err) => {
                    s.echo(err);
                    Ok(Step::Continue)
                }
            }
        }
    }

    fn preview_ucs(&self, s: &Session) -> Option<UcsFrame> {
        let init = self.initial_ucs.unwrap_or_else(|| s.ucs());
        let orig = self.origin.unwrap_or(init.origin);
        let dir = s.cursor3 - orig;
        if dir.len() > cadcraft_geom::EPS {
            UcsFrame::from_axis_vector(orig, dir, self.axis, &init).ok()
        } else {
            Some(init.with_origin(orig))
        }
    }

    fn preview_camera(&self, s: &Session) -> Option<OrbitCamera> {
        if !s.settings.ucs_follow_view || self.origin.is_none() {
            return None;
        }
        let init_cam = self.initial_cam.unwrap_or_else(|| s.camera());
        let init_ucs = self.initial_ucs.unwrap_or_else(|| s.ucs());
        let cand = self.preview_ucs(s)?;
        let pivot = self.origin.unwrap_or(cand.origin);
        let mut cam = init_cam;
        cam.apply_ucs_delta(&init_cam, &init_ucs, &cand, pivot);
        Some(cam)
    }

    fn preview(&self, s: &Session, _c: Vec2) -> Vec<EntityKind> {
        let cur = s.cursor3;
        let mut list = Vec::new();
        if let Some(orig) = self.origin {
            list.push(EntityKind::Line(Line { a: orig, b: cur }));
            if let Some(cand) = self.preview_ucs(s) {
                list.extend(build_triad_preview(orig, cand.x_axis, cand.y_axis, cand.z_axis, orig.dist(cur).max(1.0)));
            }
        }
        list
    }
}

// -----------------------------------------------------------------------------
// UcsRotateM (local X, Y, Z rotation)
// -----------------------------------------------------------------------------

pub(crate) struct UcsRotateM {
    pub axis: Axis3D,
    pub initial_cam: Option<OrbitCamera>,
    pub initial_ucs: Option<UcsFrame>,
}

impl UcsRotateM {
    pub fn new(axis: Axis3D) -> Self {
        Self { axis, initial_cam: None, initial_ucs: None }
    }
}

impl Interactive for UcsRotateM {
    fn name(&self) -> &'static str {
        match self.axis {
            Axis3D::X => "UCS.ROTATE_X",
            Axis3D::Y => "UCS.ROTATE_Y",
            Axis3D::Z => "UCS.ROTATE_Z",
        }
    }

    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        self.initial_cam = Some(s.camera());
        self.initial_ucs = Some(s.ucs());
        Ok(Step::Continue)
    }

    fn prompt(&self, _s: &Session) -> Prompt {
        Prompt::new(
            format!("Specify rotation angle about {} axis <90>:", self.axis.name()),
            Accept { enter: true, point: true, text: true, ..Accept::default() },
        )
        .default("90")
    }

    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        let init_cam = self.initial_cam.unwrap_or_else(|| s.camera());
        let init_ucs = self.initial_ucs.unwrap_or_else(|| s.ucs());
        match i {
            Input::Enter => {
                let new_ucs = match self.axis {
                    Axis3D::X => init_ucs.rotated_x(std::f64::consts::FRAC_PI_2),
                    Axis3D::Y => init_ucs.rotated_y(std::f64::consts::FRAC_PI_2),
                    Axis3D::Z => init_ucs.rotated_z(std::f64::consts::FRAC_PI_2),
                };
                s.set_ucs(new_ucs)?;
                apply_confirmed_view(s, init_cam, init_ucs, new_ucs);
                s.echo(format!("SCP girado 90° alrededor del eje {}.", self.axis.name()));
                Ok(Step::Done)
            }
            Input::Cancel => {
                s.set_camera(init_cam);
                s.set_ucs(init_ucs)?;
                Ok(Step::Cancel)
            }
            Input::Keyword(txt) | Input::Text(txt) => {
                let deg: f64 = txt.trim().parse().unwrap_or(90.0);
                let rad = deg.to_radians();
                let new_ucs = match self.axis {
                    Axis3D::X => init_ucs.rotated_x(rad),
                    Axis3D::Y => init_ucs.rotated_y(rad),
                    Axis3D::Z => init_ucs.rotated_z(rad),
                };
                s.set_ucs(new_ucs)?;
                apply_confirmed_view(s, init_cam, init_ucs, new_ucs);
                s.echo(format!("SCP girado {:.2}° alrededor del eje {}.", deg, self.axis.name()));
                Ok(Step::Done)
            }
            Input::Point(p) => {
                let world_pt = s.ucs().to_world2(p);
                let cand = self.calc_rotated_from_pt(world_pt, init_ucs);
                s.set_ucs(cand)?;
                apply_confirmed_view(s, init_cam, init_ucs, cand);
                s.echo(format!("SCP girado interactivamente alrededor del eje {}.", self.axis.name()));
                Ok(Step::Done)
            }
            Input::Point3(p3) => {
                let cand = self.calc_rotated_from_pt(p3, init_ucs);
                s.set_ucs(cand)?;
                apply_confirmed_view(s, init_cam, init_ucs, cand);
                s.echo(format!("SCP girado interactivamente alrededor del eje {}.", self.axis.name()));
                Ok(Step::Done)
            }
            _ => Ok(Step::Continue),
        }
    }

    fn preview_ucs(&self, s: &Session) -> Option<UcsFrame> {
        let init = self.initial_ucs.unwrap_or_else(|| s.ucs());
        Some(self.calc_rotated_from_pt(s.cursor3, init))
    }

    fn preview_camera(&self, s: &Session) -> Option<OrbitCamera> {
        if !s.settings.ucs_follow_view {
            return None;
        }
        let init_cam = self.initial_cam.unwrap_or_else(|| s.camera());
        let init_ucs = self.initial_ucs.unwrap_or_else(|| s.ucs());
        let cand = self.preview_ucs(s)?;
        let mut cam = init_cam;
        cam.apply_ucs_delta(&init_cam, &init_ucs, &cand, init_ucs.origin);
        Some(cam)
    }

    fn preview(&self, s: &Session, _c: Vec2) -> Vec<EntityKind> {
        if let Some(cand) = self.preview_ucs(s) {
            build_triad_preview(cand.origin, cand.x_axis, cand.y_axis, cand.z_axis, 3.0)
        } else {
            Vec::new()
        }
    }
}

impl UcsRotateM {
    fn calc_rotated_from_pt(&self, pt: Vec3, init: UcsFrame) -> UcsFrame {
        let d = pt - init.origin;
        if d.len() < cadcraft_geom::EPS {
            return init;
        }
        let (u, v) = match self.axis {
            Axis3D::X => (d.dot(init.y_axis), d.dot(init.z_axis)),
            Axis3D::Y => (d.dot(init.z_axis), d.dot(init.x_axis)),
            Axis3D::Z => (d.dot(init.x_axis), d.dot(init.y_axis)),
        };
        let angle = v.atan2(u);
        match self.axis {
            Axis3D::X => init.rotated_x(angle),
            Axis3D::Y => init.rotated_y(angle),
            Axis3D::Z => init.rotated_z(angle),
        }
    }
}

// -----------------------------------------------------------------------------
// UcsObjectM (align to clicked entity)
// -----------------------------------------------------------------------------

#[derive(Default)]
pub(crate) struct UcsObjectM {
    pub initial_cam: Option<OrbitCamera>,
    pub initial_ucs: Option<UcsFrame>,
}

impl Interactive for UcsObjectM {
    fn name(&self) -> &'static str {
        "UCS.OBJECT"
    }

    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        self.initial_cam = Some(s.camera());
        self.initial_ucs = Some(s.ucs());
        Ok(Step::Continue)
    }

    fn prompt(&self, _s: &Session) -> Prompt {
        Prompt::new("Select object to align UCS:", Accept { point: true, ..Accept::default() })
    }

    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        let init_cam = self.initial_cam.unwrap_or_else(|| s.camera());
        let init_ucs = self.initial_ucs.unwrap_or_else(|| s.ucs());
        let pt = match i {
            Input::Cancel => {
                s.set_camera(init_cam);
                s.set_ucs(init_ucs)?;
                return Ok(Step::Cancel);
            }
            Input::Point(p) => s.ucs().to_world2(p),
            Input::Point3(p3) => p3,
            _ => return Ok(Step::Continue),
        };

        let mut closest_ucs = None;
        let mut min_dist = f64::MAX;
        let doc = s.doc()?;
        for ent in doc.model.iter() {
            match &ent.kind {
                EntityKind::Line(l) => {
                    let d = l.a.dist(pt).min(l.b.dist(pt));
                    if d < min_dist {
                        min_dist = d;
                        let dir = l.b - l.a;
                        if let Ok(u) = UcsFrame::from_axis_vector(l.a, dir, Axis3D::X, &init_ucs) {
                            closest_ucs = Some((u, "Line"));
                        }
                    }
                }
                EntityKind::Circle(c) => {
                    let d = c.center.dist(pt);
                    if d < min_dist {
                        min_dist = d;
                        let norm = ent.common.extrusion;
                        if let Ok(u) = UcsFrame::from_axis_vector(c.center, norm, Axis3D::Z, &init_ucs) {
                            closest_ucs = Some((u, "Circle"));
                        }
                    }
                }
                EntityKind::Arc(a) => {
                    let d = a.center.dist(pt);
                    if d < min_dist {
                        min_dist = d;
                        let norm = ent.common.extrusion;
                        if let Ok(u) = UcsFrame::from_axis_vector(a.center, norm, Axis3D::Z, &init_ucs) {
                            closest_ucs = Some((u, "Arc"));
                        }
                    }
                }
                EntityKind::Face3d(f) => {
                    let d = f.corners[0].dist(pt);
                    if d < min_dist {
                        min_dist = d;
                        if let Ok(u) = UcsFrame::align_to_face(&f.corners, false, false) {
                            closest_ucs = Some((u, "3DFace"));
                        }
                    }
                }
                _ => {}
            }
        }

        if let Some((new_ucs, name)) = closest_ucs {
            s.set_ucs(new_ucs)?;
            apply_confirmed_view(s, init_cam, init_ucs, new_ucs);
            s.echo(format!("SCP alineado con objeto {name}."));
            Ok(Step::Done)
        } else {
            s.echo("No se encontró ningún objeto seleccionable en ese punto.");
            Ok(Step::Continue)
        }
    }
}

// -----------------------------------------------------------------------------
// UcsFaceM (align to face with cycle Next/FlipX/FlipY/Accept)
// -----------------------------------------------------------------------------

#[derive(Default)]
pub(crate) struct UcsFaceM {
    pub initial_cam: Option<OrbitCamera>,
    pub initial_ucs: Option<UcsFrame>,
    pub faces: Vec<[Vec3; 4]>,
    pub face_idx: usize,
    pub flip_x: bool,
    pub flip_y: bool,
    pub selected: bool,
}

impl Interactive for UcsFaceM {
    fn name(&self) -> &'static str {
        "UCS.FACE"
    }

    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        self.initial_cam = Some(s.camera());
        self.initial_ucs = Some(s.ucs());
        // Collect faces from Face3d and Solid3d in the drawing
        if let Ok(doc) = s.doc() {
            for ent in doc.model.iter() {
                match &ent.kind {
                    EntityKind::Face3d(f) => self.faces.push(f.corners),
                    EntityKind::Solid3d(solid) => {
                        for f in &solid.faces {
                            self.faces.push(f.corners);
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(Step::Continue)
    }

    fn prompt(&self, _s: &Session) -> Prompt {
        if !self.selected {
            Prompt::new("Select face of solid or 3DFace object:", Accept { point: true, ..Accept::default() })
        } else {
            Prompt::new(
                "Enter an option [Next face/Xflip/Yflip/Accept] <Accept>:",
                Accept { enter: true, point: true, text: true, ..Accept::default() },
            )
            .kw(&["Next", "Xflip", "Yflip", "Accept"])
            .default("Accept")
        }
    }

    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        let init_cam = self.initial_cam.unwrap_or_else(|| s.camera());
        let init_ucs = self.initial_ucs.unwrap_or_else(|| s.ucs());
        if !self.selected {
            let pt = match i {
                Input::Cancel => {
                    s.set_camera(init_cam);
                    s.set_ucs(init_ucs)?;
                    return Ok(Step::Cancel);
                }
                Input::Point(p) => s.ucs().to_world2(p),
                Input::Point3(p3) => p3,
                _ => return Ok(Step::Continue),
            };
            if self.faces.is_empty() {
                s.echo("No hay objetos con caras 3D en el dibujo.");
                return Ok(Step::Cancel);
            }
            // Pick face closest to pt
            let mut best_i = 0;
            let mut min_d = f64::MAX;
            for (idx, f) in self.faces.iter().enumerate() {
                let center = (f[0] + f[1] + f[2] + f[3]) * 0.25;
                let d = center.dist(pt);
                if d < min_d {
                    min_d = d;
                    best_i = idx;
                }
            }
            self.face_idx = best_i;
            self.selected = true;
            s.echo("Cara seleccionada. [Next face/Xflip/Yflip/Accept] <Accept>:");
            Ok(Step::Continue)
        } else {
            match i {
                Input::Enter => {
                    if let Some(cand) = self.preview_ucs(s) {
                        s.set_ucs(cand)?;
                        apply_confirmed_view(s, init_cam, init_ucs, cand);
                        s.echo("Alineación con cara confirmada.");
                    }
                    Ok(Step::Done)
                }
                Input::Cancel => {
                    s.set_camera(init_cam);
                    s.set_ucs(init_ucs)?;
                    Ok(Step::Cancel)
                }
                Input::Keyword(k) | Input::Text(k) => match k.trim().to_ascii_lowercase().as_str() {
                    "n" | "next" => {
                        if !self.faces.is_empty() {
                            self.face_idx = (self.face_idx + 1) % self.faces.len();
                            s.echo(format!("Siguiente cara ({}/{}).", self.face_idx + 1, self.faces.len()));
                        }
                        Ok(Step::Continue)
                    }
                    "x" | "xflip" => {
                        self.flip_x = !self.flip_x;
                        s.echo(format!("Eje X invertido: {}.", self.flip_x));
                        Ok(Step::Continue)
                    }
                    "y" | "yflip" => {
                        self.flip_y = !self.flip_y;
                        s.echo(format!("Eje Y invertido: {}.", self.flip_y));
                        Ok(Step::Continue)
                    }
                    "a" | "accept" => {
                        if let Some(cand) = self.preview_ucs(s) {
                            s.set_ucs(cand)?;
                            apply_confirmed_view(s, init_cam, init_ucs, cand);
                            s.echo("Alineación con cara confirmada.");
                        }
                        Ok(Step::Done)
                    }
                    _ => Ok(Step::Continue),
                },
                _ => Ok(Step::Continue),
            }
        }
    }

    fn preview_ucs(&self, _s: &Session) -> Option<UcsFrame> {
        if !self.selected || self.face_idx >= self.faces.len() {
            return None;
        }
        let corners = self.faces.get(self.face_idx)?;
        UcsFrame::align_to_face(&corners[..], self.flip_x, self.flip_y).ok()
    }

    fn preview_camera(&self, s: &Session) -> Option<OrbitCamera> {
        if !s.settings.ucs_follow_view || !self.selected {
            return None;
        }
        let init_cam = self.initial_cam.unwrap_or_else(|| s.camera());
        let init_ucs = self.initial_ucs.unwrap_or_else(|| s.ucs());
        let cand = self.preview_ucs(s)?;
        let mut cam = init_cam;
        cam.apply_ucs_delta(&init_cam, &init_ucs, &cand, cand.origin);
        Some(cam)
    }

    fn preview(&self, s: &Session, _c: Vec2) -> Vec<EntityKind> {
        if let Some(cand) = self.preview_ucs(s) {
            let mut list = build_triad_preview(cand.origin, cand.x_axis, cand.y_axis, cand.z_axis, 3.0);
            if let Some(f) = self.faces.get(self.face_idx) {
                list.push(EntityKind::Line(Line { a: f[0], b: f[1] }));
                list.push(EntityKind::Line(Line { a: f[1], b: f[2] }));
                list.push(EntityKind::Line(Line { a: f[2], b: f[3] }));
                list.push(EntityKind::Line(Line { a: f[3], b: f[0] }));
            }
            list
        } else {
            Vec::new()
        }
    }
}

// -----------------------------------------------------------------------------
// UcsZObjectM (define Z axis direction by object)
// -----------------------------------------------------------------------------

#[derive(Default)]
pub(crate) struct UcsZObjectM {
    pub initial_cam: Option<OrbitCamera>,
    pub initial_ucs: Option<UcsFrame>,
}

impl Interactive for UcsZObjectM {
    fn name(&self) -> &'static str {
        "UCS.ZOBJECT"
    }

    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        self.initial_cam = Some(s.camera());
        self.initial_ucs = Some(s.ucs());
        Ok(Step::Continue)
    }

    fn prompt(&self, _s: &Session) -> Prompt {
        Prompt::new("Select object to define Z-axis direction:", Accept { point: true, ..Accept::default() })
    }

    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        let init_cam = self.initial_cam.unwrap_or_else(|| s.camera());
        let init_ucs = self.initial_ucs.unwrap_or_else(|| s.ucs());
        let pt = match i {
            Input::Cancel => {
                s.set_camera(init_cam);
                s.set_ucs(init_ucs)?;
                return Ok(Step::Cancel);
            }
            Input::Point(p) => s.ucs().to_world2(p),
            Input::Point3(p3) => p3,
            _ => return Ok(Step::Continue),
        };

        let mut closest_z = None;
        let mut min_dist = f64::MAX;
        let doc = s.doc()?;
        for ent in doc.model.iter() {
            match &ent.kind {
                EntityKind::Line(l) => {
                    let d = l.a.dist(pt).min(l.b.dist(pt));
                    if d < min_dist {
                        min_dist = d;
                        let dir = (l.b - l.a).normalized();
                        if let Ok(u) = UcsFrame::from_axis_vector(init_ucs.origin, dir, Axis3D::Z, &init_ucs) {
                            closest_z = Some((u, "Line"));
                        }
                    }
                }
                EntityKind::Circle(c) => {
                    let d = c.center.dist(pt);
                    if d < min_dist {
                        min_dist = d;
                        let norm = ent.common.extrusion;
                        if let Ok(u) = UcsFrame::from_axis_vector(init_ucs.origin, norm, Axis3D::Z, &init_ucs) {
                            closest_z = Some((u, "Circle"));
                        }
                    }
                }
                EntityKind::Arc(a) => {
                    let d = a.center.dist(pt);
                    if d < min_dist {
                        min_dist = d;
                        let norm = ent.common.extrusion;
                        if let Ok(u) = UcsFrame::from_axis_vector(init_ucs.origin, norm, Axis3D::Z, &init_ucs) {
                            closest_z = Some((u, "Arc"));
                        }
                    }
                }
                EntityKind::Face3d(f) => {
                    let d = f.corners[0].dist(pt);
                    if d < min_dist {
                        min_dist = d;
                        let v1 = f.corners[1] - f.corners[0];
                        let v2 = f.corners[3] - f.corners[0];
                        let norm = v1.cross(v2).normalized();
                        if let Ok(u) = UcsFrame::from_axis_vector(init_ucs.origin, norm, Axis3D::Z, &init_ucs) {
                            closest_z = Some((u, "3DFace"));
                        }
                    }
                }
                _ => {}
            }
        }

        if let Some((new_ucs, name)) = closest_z {
            s.set_ucs(new_ucs)?;
            apply_confirmed_view(s, init_cam, init_ucs, new_ucs);
            s.echo(format!("Eje Z del SCP alineado con objeto {name}."));
            Ok(Step::Done)
        } else {
            s.echo("No se encontró ningún objeto seleccionable en ese punto.");
            Ok(Step::Continue)
        }
    }
}

// -----------------------------------------------------------------------------
// Interactive machines for Named UCS: Save, Restore, Delete, Rename
// -----------------------------------------------------------------------------

#[derive(Default)]
pub(crate) struct UcsSaveM;

impl Interactive for UcsSaveM {
    fn name(&self) -> &'static str {
        "UCS.SAVE"
    }

    fn begin(&mut self, _s: &mut Session) -> Result<Step> {
        Ok(Step::Continue)
    }

    fn prompt(&self, _s: &Session) -> Prompt {
        Prompt::new("Enter name to save current UCS:", Accept { enter: false, text: true, ..Accept::default() })
    }

    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        match i {
            Input::Cancel => Ok(Step::Cancel),
            Input::Text(name) | Input::Keyword(name) => {
                let name = name.trim();
                if name.is_empty() {
                    return Ok(Step::Continue);
                }
                run_ucs_save(s, &json!({ "name": name }))?;
                Ok(Step::Done)
            }
            _ => Ok(Step::Continue),
        }
    }
}

#[derive(Default)]
pub(crate) struct UcsRestoreM;

impl Interactive for UcsRestoreM {
    fn name(&self) -> &'static str {
        "UCS.RESTORE"
    }

    fn begin(&mut self, _s: &mut Session) -> Result<Step> {
        Ok(Step::Continue)
    }

    fn prompt(&self, s: &Session) -> Prompt {
        let names = s.named_ucss().unwrap_or_default();
        let prompt_str = if names.is_empty() {
            "Enter name of UCS to restore [? to list]:".to_string()
        } else {
            let list_str = names.iter().map(|u| u.name.as_str()).collect::<Vec<_>>().join("/");
            format!("Enter name of UCS to restore [{list_str}] [?]:")
        };
        Prompt::new(prompt_str, Accept { enter: false, text: true, ..Accept::default() })
    }

    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        match i {
            Input::Cancel => Ok(Step::Cancel),
            Input::Text(name) | Input::Keyword(name) => {
                let name = name.trim();
                if name == "?" {
                    let _ = run_ucs_list(s, &json!({}))?;
                    return Ok(Step::Continue);
                }
                if name.is_empty() {
                    return Ok(Step::Continue);
                }
                match run_ucs_restore(s, &json!({ "name": name })) {
                    Ok(_) => Ok(Step::Done),
                    Err(e) => {
                        s.echo(e.to_string());
                        Ok(Step::Continue)
                    }
                }
            }
            _ => Ok(Step::Continue),
        }
    }
}

#[derive(Default)]
pub(crate) struct UcsDeleteM;

impl Interactive for UcsDeleteM {
    fn name(&self) -> &'static str {
        "UCS.DELETE"
    }

    fn begin(&mut self, _s: &mut Session) -> Result<Step> {
        Ok(Step::Continue)
    }

    fn prompt(&self, _s: &Session) -> Prompt {
        Prompt::new("Enter name of UCS to delete:", Accept { enter: false, text: true, ..Accept::default() })
    }

    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        match i {
            Input::Cancel => Ok(Step::Cancel),
            Input::Text(name) | Input::Keyword(name) => {
                let name = name.trim();
                if name.is_empty() {
                    return Ok(Step::Continue);
                }
                match run_ucs_delete(s, &json!({ "name": name })) {
                    Ok(_) => Ok(Step::Done),
                    Err(e) => {
                        s.echo(e.to_string());
                        Ok(Step::Continue)
                    }
                }
            }
            _ => Ok(Step::Continue),
        }
    }
}

#[derive(Default)]
pub(crate) struct UcsRenameM {
    pub old_name: Option<String>,
}

impl Interactive for UcsRenameM {
    fn name(&self) -> &'static str {
        "UCS.RENAME"
    }

    fn begin(&mut self, _s: &mut Session) -> Result<Step> {
        Ok(Step::Continue)
    }

    fn prompt(&self, _s: &Session) -> Prompt {
        if self.old_name.is_none() {
            Prompt::new("Enter old name of UCS to rename:", Accept { enter: false, text: true, ..Accept::default() })
        } else {
            Prompt::new("Enter new name for UCS:", Accept { enter: false, text: true, ..Accept::default() })
        }
    }

    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        match i {
            Input::Cancel => Ok(Step::Cancel),
            Input::Text(name) | Input::Keyword(name) => {
                let name = name.trim();
                if name.is_empty() {
                    return Ok(Step::Continue);
                }
                if self.old_name.is_none() {
                    self.old_name = Some(name.to_string());
                    Ok(Step::Continue)
                } else {
                    let old = self.old_name.take().unwrap_or_default();
                    match run_ucs_rename(s, &json!({ "old_name": old, "new_name": name })) {
                        Ok(_) => Ok(Step::Done),
                        Err(e) => {
                            s.echo(e.to_string());
                            Ok(Step::Continue)
                        }
                    }
                }
            }
            _ => Ok(Step::Continue),
        }
    }
}

fn build_triad_preview(origin: Vec3, x: Vec3, y: Vec3, z: Vec3, len: f64) -> Vec<EntityKind> {
    vec![
        EntityKind::Line(Line { a: origin, b: origin + x * len }),
        EntityKind::Line(Line { a: origin, b: origin + y * (len * 0.8) }),
        EntityKind::Line(Line { a: origin, b: origin + z * (len * 0.6) }),
    ]
}
