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
            .params("{offset?: number}; setting an offset enables manual mode; without one, reports the active offset and mode")
            .noundo()
            .transparent(),
        CommandSpec::new("plane.origin", "Drawing Plane Origin", run_plane_origin)
            .alias(&["elev.origin", "cota.origen"])
            .menu(&["Tools", "Work Plane", "Set Manual Zero Offset"])
            .params("{}; sets a manual zero offset; use plane.auto to return to automatic mode")
            .noundo()
            .transparent(),
        CommandSpec::new("plane.auto", "Automatic Plane Elevation", run_plane_auto)
            .alias(&["elevation.auto"])
            .menu(&["Tools", "Work Plane", "Automatic Elevation"])
            .params("{}; derives the active plane offset from the view target")
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
        let plane = s.drawing_plane();
        s.set_drawing_plane(plane);
        s.set_drawing_plane_offset(off);
        s.echo(format!("Drawing plane elevation offset set to {off}."));
        return Ok(json!({ "status": "ok", "offset": off }));
    }
    let cur = s.effective_drawing_plane_offset();
    let automatic = !s.drawing_plane_offset_is_manual();
    let mode = if automatic { "automatic" } else { "manual" };
    s.echo(format!("Current {mode} drawing plane elevation offset is {cur}."));
    Ok(json!({ "offset": cur, "automatic": automatic }))
}

pub(crate) fn run_plane_origin(s: &mut Session, _p: &Value) -> Result<Value> {
    let plane = s.drawing_plane();
    s.set_drawing_plane(plane);
    s.set_drawing_plane_offset(0.0);
    s.echo("Drawing plane elevation offset fixed at the origin (0.0).");
    Ok(json!({ "status": "ok", "offset": 0.0, "automatic": false }))
}

pub(crate) fn run_plane_auto(s: &mut Session, _p: &Value) -> Result<Value> {
    s.set_drawing_plane_automatic();
    s.set_drawing_plane_offset_automatic();
    s.echo("Drawing plane and free-point depth returned to automatic mode.");
    Ok(json!({ "status": "ok", "offset": 0.0, "automatic": true }))
}

pub(crate) fn run_plane_pick(s: &mut Session, p: &Value) -> Result<Value> {
    if let Some(value) = p.get("point") {
        let Some(pt) = value.as_array().filter(|a| a.len() == 3) else {
            return Err(bad("plane.pick", "`point` must be an array of three finite numbers"));
        };
        let coordinate = |i: usize| pt.get(i).and_then(Value::as_f64).filter(|v| v.is_finite());
        let (Some(x), Some(y), Some(z)) = (coordinate(0), coordinate(1), coordinate(2)) else {
            return Err(bad("plane.pick", "`point` must be an array of three finite numbers"));
        };
        let off = match s.drawing_plane() {
            DrawingPlane::XY => z,
            DrawingPlane::XZ => y,
            DrawingPlane::YZ => x,
        };
        let plane = s.drawing_plane();
        s.set_drawing_plane(plane);
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
                    DrawingPlane::XY => s.effective_drawing_plane_offset(),
                    DrawingPlane::XZ => p.y,
                    DrawingPlane::YZ => p.x,
                };
                let plane = s.drawing_plane();
                s.set_drawing_plane(plane);
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
