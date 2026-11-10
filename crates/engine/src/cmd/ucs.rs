//! User Coordinate System (UCS / SCP) commands.
//!
//! Provides interactive and programmatic definition of coordinate systems:
//! - 3-Point UCS: Origin, point on positive X axis, point on positive Y side of XY plane.
//! - Origin: Move origin preserving axis orientations.
//! - World: Restore the World Coordinate System (WCS).
//! - Previous: Restore the previous UCS from the session history.

use cadcraft_geom::{UcsFrame, Vec2, Vec3};
use serde_json::{Value, json};

use super::*;
use crate::{Accept, EngineError, Input, Interactive, Prompt, Result, Session, Step};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("ucs", "User Coordinate System", run_ucs)
            .alias(&["scp"])
            .menu(&["Tools", "New UCS", "3 Points"])
            .params("{option?: \"world\"|\"prev\"|\"origin\"|\"3point\", origin?: [x,y,z], x_axis?: [x,y,z], y_pt?: [x,y,z]}")
            .noundo()
            .transparent()
            .interactive(|_| Ok(Box::new(UcsM::default()))),
        CommandSpec::new("ucs.3point", "UCS 3 Points", run_ucs_3point)
            .alias(&["scp.3puntos"])
            .menu(&["Tools", "New UCS", "3 Points"])
            .noundo()
            .transparent()
            .interactive(|_| Ok(Box::new(Ucs3PointM::default()))),
        CommandSpec::new("ucs.origin", "UCS Origin", run_ucs_origin)
            .alias(&["scp.origen"])
            .menu(&["Tools", "New UCS", "Origin"])
            .params("{origin: [x,y,z]}")
            .noundo()
            .transparent()
            .interactive(|_| Ok(Box::new(UcsOriginM))),
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
        CommandSpec::new("ucs.view", "UCS View", run_ucs_view)
            .alias(&["scp.vista"])
            .menu(&["Tools", "New UCS", "View"])
            .params("{}")
            .noundo()
            .transparent(),
    ]
}

pub(crate) fn run_ucs(s: &mut Session, p: &Value) -> Result<Value> {
    if let Some(opt) = p.get("option").and_then(Value::as_str) {
        match opt.trim().to_ascii_lowercase().as_str() {
            "w" | "world" | "universal" | "mundo" => return run_ucs_world(s, p),
            "p" | "prev" | "previous" | "previo" => return run_ucs_previous(s, p),
            "v" | "view" | "vista" => return run_ucs_view(s, p),
            "o" | "m" | "origin" | "move" | "origen" => return run_ucs_origin(s, p),
            "3" | "3p" | "3point" => return run_ucs_3point(s, p),
            _ => return Err(EngineError::Other(format!("Unknown UCS option `{opt}`. Options: World, Previous, View, Origin, 3point."))),
        }
    }
    if p.get("origin").is_some() && p.get("x_axis").is_some() && p.get("y_pt").is_some() {
        return run_ucs_3point(s, p);
    }
    if p.get("origin").is_some() {
        return run_ucs_origin(s, p);
    }
    let cur = s.ucs();
    let is_w = cur.is_world();
    let name = if is_w { "World" } else { "Custom" };
    s.echo(format!("Current UCS is {name}. Origin: {:.4}, {:.4}, {:.4}", cur.origin.x, cur.origin.y, cur.origin.z));
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
    s.set_ucs(UcsFrame::WORLD)?;
    s.echo("Restored World Coordinate System (WCS).");
    Ok(json!({ "status": "ok", "ucs": "world" }))
}

pub(crate) fn run_ucs_previous(s: &mut Session, _p: &Value) -> Result<Value> {
    if s.restore_previous_ucs()? {
        let cur = s.ucs();
        s.echo(format!("Restored previous UCS. Origin: {:.4}, {:.4}, {:.4}", cur.origin.x, cur.origin.y, cur.origin.z));
        Ok(json!({ "status": "ok", "restored": true }))
    } else {
        s.echo("No previous UCS saved in history.");
        Ok(json!({ "status": "ok", "restored": false }))
    }
}

pub(crate) fn run_ucs_origin(s: &mut Session, p: &Value) -> Result<Value> {
    let orig = point3_req("ucs", p, "origin")?;
    let cur = s.ucs();
    let new_ucs = cur.with_origin(orig);
    s.set_ucs(new_ucs)?;
    s.echo(format!("UCS origin moved to {:.4}, {:.4}, {:.4}.", orig.x, orig.y, orig.z));
    Ok(json!({ "status": "ok", "origin": [orig.x, orig.y, orig.z] }))
}

pub(crate) fn run_ucs_3point(s: &mut Session, p: &Value) -> Result<Value> {
    let o = point3_req("ucs", p, "origin")?;
    let px = point3_req("ucs", p, "x_axis")?;
    let py = point3_req("ucs", p, "y_pt")?;
    let ucs = UcsFrame::from_3_points(o, px, py).map_err(|e| EngineError::Other(e.to_string()))?;
    s.set_ucs(ucs)?;
    s.echo(format!(
        "UCS defined by 3 points. Normal: {:.4}, {:.4}, {:.4}.",
        ucs.z_axis.x, ucs.z_axis.y, ucs.z_axis.z
    ));
    Ok(json!({
        "status": "ok",
        "origin": [ucs.origin.x, ucs.origin.y, ucs.origin.z],
        "x_axis": [ucs.x_axis.x, ucs.x_axis.y, ucs.x_axis.z],
        "y_axis": [ucs.y_axis.x, ucs.y_axis.y, ucs.y_axis.z],
        "z_axis": [ucs.z_axis.x, ucs.z_axis.y, ucs.z_axis.z],
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
    s.echo("UCS set to current view direction.");
    Ok(json!({
        "status": "ok",
        "ucs": "view",
        "origin": [orig.x, orig.y, orig.z],
        "x_axis": [x_axis.x, x_axis.y, x_axis.z],
        "y_axis": [y_axis.x, y_axis.y, y_axis.z],
        "z_axis": [z_axis.x, z_axis.y, z_axis.z],
    }))
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

    fn prompt(&self, s: &Session) -> Prompt {
        if let Some(sub) = &self.sub {
            return sub.prompt(s);
        }
        let cur = if s.ucs().is_world() { "World" } else { "Custom" };
        Prompt::new(
            format!("Specify origin of UCS or [3point/Move/Previous/View/World] <{cur}>:"),
            Accept { enter: true, point: true, text: true, ..Accept::default() },
        )
        .kw(&["3point", "Move", "Previous", "View", "World"])
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
                    self.sub = Some(Box::new(Ucs3PointM::default()));
                    Ok(Step::Continue)
                }
                "m" | "move" | "o" | "origin" => {
                    self.sub = Some(Box::new(UcsOriginM));
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
                _ => {
                    s.echo("Invalid option. Specify 3point, Move, Previous, View, or World.");
                    Ok(Step::Continue)
                }
            },
            Input::Point(p) => {
                let orig = s.ucs().to_world2(p);
                let new_ucs = s.ucs().with_origin(orig);
                s.set_ucs(new_ucs)?;
                s.echo(format!("UCS origin moved to {:.4}, {:.4}, {:.4}.", orig.x, orig.y, orig.z));
                Ok(Step::Done)
            }
            Input::Point3(p3) => {
                let new_ucs = s.ucs().with_origin(p3);
                s.set_ucs(new_ucs)?;
                s.echo(format!("UCS origin moved to {:.4}, {:.4}, {:.4}.", p3.x, p3.y, p3.z));
                Ok(Step::Done)
            }
            _ => Ok(Step::Continue),
        }
    }

    fn preview(&self, s: &Session, c: Vec2) -> Vec<cadcraft_doc::EntityKind> {
        if let Some(sub) = &self.sub {
            sub.preview(s, c)
        } else {
            Vec::new()
        }
    }
}

pub(crate) struct UcsOriginM;

impl Interactive for UcsOriginM {
    fn name(&self) -> &'static str {
        "UCS.ORIGIN"
    }

    fn prompt(&self, _s: &Session) -> Prompt {
        Prompt::new("Specify new origin point <0,0,0>:", Accept { enter: true, point: true, ..Accept::default() })
    }

    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        match i {
            Input::Enter => Ok(Step::Done),
            Input::Cancel => Ok(Step::Cancel),
            Input::Point(p) => {
                let orig = s.ucs().to_world2(p);
                let new_ucs = s.ucs().with_origin(orig);
                s.set_ucs(new_ucs)?;
                s.echo(format!("UCS origin moved to {:.4}, {:.4}, {:.4}.", orig.x, orig.y, orig.z));
                Ok(Step::Done)
            }
            Input::Point3(p3) => {
                let new_ucs = s.ucs().with_origin(p3);
                s.set_ucs(new_ucs)?;
                s.echo(format!("UCS origin moved to {:.4}, {:.4}, {:.4}.", p3.x, p3.y, p3.z));
                Ok(Step::Done)
            }
            _ => Ok(Step::Continue),
        }
    }
}

#[derive(Default)]
pub(crate) struct Ucs3PointM {
    pub origin: Option<Vec3>,
    pub pt_x: Option<Vec3>,
}

impl Interactive for Ucs3PointM {
    fn name(&self) -> &'static str {
        "UCS.3POINT"
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
        let pt = match i {
            Input::Enter if self.origin.is_none() => s.ucs().origin,
            Input::Cancel => return Ok(Step::Cancel),
            Input::Point(p) => s.ucs().to_world2(p),
            Input::Point3(p3) => p3,
            _ => return Ok(Step::Continue),
        };

        if self.origin.is_none() {
            self.origin = Some(pt);
            s.echo(format!("Origin set to {:.4}, {:.4}, {:.4}.", pt.x, pt.y, pt.z));
            Ok(Step::Continue)
        } else if self.pt_x.is_none() {
            let orig = self.origin.unwrap_or(Vec3::ZERO);
            if orig.dist(pt) < cadcraft_geom::EPS {
                s.echo("El punto del eje X no puede coincidir con el origen.");
                return Ok(Step::Continue);
            }
            self.pt_x = Some(pt);
            s.echo(format!("X-axis point set to {:.4}, {:.4}, {:.4}.", pt.x, pt.y, pt.z));
            Ok(Step::Continue)
        } else {
            let orig = self.origin.unwrap_or(Vec3::ZERO);
            let px = self.pt_x.unwrap_or(Vec3::ZERO);
            match UcsFrame::from_3_points(orig, px, pt) {
                Ok(new_ucs) => {
                    s.set_ucs(new_ucs)?;
                    s.echo(format!(
                        "New UCS defined. Normal (Z): {:.4}, {:.4}, {:.4}.",
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

    fn preview(&self, s: &Session, _c: Vec2) -> Vec<cadcraft_doc::EntityKind> {
        let cur_3d = s.cursor3;
        let mut list = Vec::new();
        if let Some(orig) = self.origin {
            if self.pt_x.is_none() {
                // Rubber band and line towards candidate X
                list.push(cadcraft_doc::EntityKind::Line(cadcraft_doc::Line { a: orig, b: cur_3d }));
            } else if let Some(px) = self.pt_x {
                // Confirmed X axis line
                list.push(cadcraft_doc::EntityKind::Line(cadcraft_doc::Line { a: orig, b: px }));
                // Line towards candidate Y point
                list.push(cadcraft_doc::EntityKind::Line(cadcraft_doc::Line { a: orig, b: cur_3d }));
                // Candidate orthogonal Y line
                if let Ok(cand) = UcsFrame::from_3_points(orig, px, cur_3d) {
                    let y_len = orig.dist(cur_3d).max(1.0);
                    let py = orig + cand.y_axis * y_len;
                    list.push(cadcraft_doc::EntityKind::Line(cadcraft_doc::Line { a: orig, b: py }));
                }
            }
        }
        list
    }
}
