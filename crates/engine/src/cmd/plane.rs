//! Drawing/construction planes for 2D and 3D wireframe drafting.
//!
//! Controls the active construction plane (XY, XZ, YZ) onto which graphic cursor
//! positions intersect the camera view rays, allowing spatial 3D wireframe drafting
//! without requiring manual coordinate typing for every point.

use cadcraft_geom::DrawingPlane;
use serde_json::{Value, json};

use super::*;
use crate::{Accept, Input, Interactive, Prompt, Result, Session, Step};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("plane", "Drawing Plane", run_plane)
            .alias(&["cplane", "workplane", "ucs"])
            .menu(&["Tools", "Work Plane", "Select Plane"])
            .params("{plane?: \"xy\"|\"xz\"|\"yz\"}; sets the active drawing plane for cursor clicks")
            .noundo()
            .transparent()
            .interactive(|_| Ok(Box::new(PlaneM))),
        CommandSpec::new("plane.xy", "Drawing Plane XY", |s, _| set_plane(s, DrawingPlane::XY))
            .menu(&["Tools", "Work Plane", "XY Plane"])
            .noundo()
            .transparent(),
        CommandSpec::new("plane.xz", "Drawing Plane XZ", |s, _| set_plane(s, DrawingPlane::XZ))
            .menu(&["Tools", "Work Plane", "XZ Plane"])
            .noundo()
            .transparent(),
        CommandSpec::new("plane.yz", "Drawing Plane YZ", |s, _| set_plane(s, DrawingPlane::YZ))
            .menu(&["Tools", "Work Plane", "YZ Plane"])
            .noundo()
            .transparent(),
        CommandSpec::new("plane.offset", "Drawing Plane Offset", run_plane_offset)
            .alias(&["elev", "elevation", "cota"])
            .menu(&["Tools", "Work Plane", "Set Elevation Offset"])
            .params("{offset?: number}; sets or queries the elevation offset of the active drawing plane")
            .noundo()
            .transparent(),
        CommandSpec::new("plane.origin", "Drawing Plane Origin", run_plane_origin)
            .alias(&["elev.origin", "cota.origen"])
            .menu(&["Tools", "Work Plane", "Return to Origin (Z=0)"])
            .params("{}; resets drawing plane elevation offset to 0.0")
            .noundo()
            .transparent(),
        CommandSpec::new("plane.pick", "Pick Plane Elevation", run_plane_pick)
            .alias(&["elev.pick"])
            .menu(&["Tools", "Work Plane", "Pick Elevation Point..."])
            .params("{}; interactively click a reference point to set plane elevation")
            .noundo()
            .transparent()
            .interactive(|_| Ok(Box::new(PlanePickM))),
    ]
}

fn set_plane(s: &mut Session, plane: DrawingPlane) -> Result<Value> {
    s.set_drawing_plane(plane);
    s.echo(format!("Active drawing plane set to {}.", plane.name()));
    Ok(json!({ "status": "ok", "plane": plane.name() }))
}

pub(crate) fn run_plane(s: &mut Session, p: &Value) -> Result<Value> {
    if let Some(name) = p.get("plane").and_then(Value::as_str) {
        if let Some(plane) = DrawingPlane::parse(name) {
            return set_plane(s, plane);
        }
        return Err(EngineError::Other(format!("Unknown drawing plane `{name}`. Options: XY, XZ, YZ.")));
    }
    let cur = s.drawing_plane();
    s.echo(format!("Current drawing plane is {}.", cur.name()));
    Ok(json!({ "plane": cur.name() }))
}

#[derive(Default)]
struct PlaneM;

impl Interactive for PlaneM {
    fn name(&self) -> &'static str {
        "PLANE"
    }

    fn prompt(&self, s: &Session) -> Prompt {
        let cur = s.drawing_plane().name();
        Prompt::new(format!("Specify drawing plane [XY/XZ/YZ] <{cur}>:"), Accept { enter: true, text: true, ..Accept::default() })
            .kw(&["XY", "XZ", "YZ"])
            .default(cur)
    }

    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        match i {
            Input::Enter => {
                let cur = s.drawing_plane();
                s.echo(format!("Active drawing plane is {}.", cur.name()));
                Ok(Step::Done)
            }
            Input::Cancel => Ok(Step::Cancel),
            Input::Keyword(k) | Input::Text(k) => {
                let trimmed = k.trim();
                if trimmed.is_empty() {
                    let cur = s.drawing_plane();
                    s.echo(format!("Active drawing plane is {}.", cur.name()));
                    return Ok(Step::Done);
                }
                if let Some(plane) = DrawingPlane::parse(trimmed) {
                    let _ = set_plane(s, plane)?;
                    Ok(Step::Done)
                } else {
                    s.echo("Invalid plane option. Specify XY, XZ, or YZ.");
                    Ok(Step::Continue)
                }
            }
            _ => Ok(Step::Continue),
        }
    }
}

pub(crate) fn run_plane_offset(s: &mut Session, p: &Value) -> Result<Value> {
    if let Some(off) = p.get("offset").and_then(Value::as_f64) {
        if !off.is_finite() {
            return Err(EngineError::Other("Elevation offset must be finite.".into()));
        }
        s.set_drawing_plane_offset(off);
        s.echo(format!("Drawing plane elevation offset set to {off}."));
        return Ok(json!({ "status": "ok", "offset": off }));
    }
    let cur = s.drawing_plane_offset();
    s.echo(format!("Current drawing plane elevation offset is {cur}."));
    Ok(json!({ "offset": cur }))
}

pub(crate) fn run_plane_origin(s: &mut Session, _p: &Value) -> Result<Value> {
    s.set_drawing_plane_offset(0.0);
    s.echo("Drawing plane elevation offset reset to origin (0.0).");
    Ok(json!({ "status": "ok", "offset": 0.0 }))
}

pub(crate) fn run_plane_pick(s: &mut Session, p: &Value) -> Result<Value> {
    if let Some(pt) = p.get("point").and_then(Value::as_array).filter(|a| a.len() == 3) {
        let x = pt.first().and_then(Value::as_f64).unwrap_or(0.0);
        let y = pt.get(1).and_then(Value::as_f64).unwrap_or(0.0);
        let z = pt.get(2).and_then(Value::as_f64).unwrap_or(0.0);
        let off = match s.drawing_plane() {
            DrawingPlane::XY => z,
            DrawingPlane::XZ => y,
            DrawingPlane::YZ => x,
        };
        s.set_drawing_plane_offset(off);
        s.echo(format!("Drawing plane elevation offset set to {off}."));
        return Ok(json!({ "status": "ok", "offset": off }));
    }
    s.start("plane.pick")?;
    Ok(json!({ "status": "interactive" }))
}

#[derive(Default)]
struct PlanePickM;

impl Interactive for PlanePickM {
    fn name(&self) -> &'static str {
        "PLANE.PICK"
    }

    fn prompt(&self, _s: &Session) -> Prompt {
        Prompt::new("Specify reference point for plane elevation:", Accept::POINT)
    }

    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        match i {
            Input::Point(p) => {
                let off = match s.drawing_plane() {
                    DrawingPlane::XY => s.settings.drawing_plane_offset,
                    DrawingPlane::XZ => p.y,
                    DrawingPlane::YZ => p.x,
                };
                s.set_drawing_plane_offset(off);
                s.echo(format!("Drawing plane elevation offset set to {off}."));
                Ok(Step::Done)
            }
            Input::Point3(p3) => {
                let off = match s.drawing_plane() {
                    DrawingPlane::XY => p3.z,
                    DrawingPlane::XZ => p3.y,
                    DrawingPlane::YZ => p3.x,
                };
                s.set_drawing_plane_offset(off);
                s.echo(format!("Drawing plane elevation offset set to {off}."));
                Ok(Step::Done)
            }
            Input::Cancel => Ok(Step::Cancel),
            Input::Enter => Ok(Step::Done),
            _ => Ok(Step::Continue),
        }
    }
}
