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
        Prompt::new(
            format!("Specify drawing plane [XY/XZ/YZ] <{cur}>:"),
            Accept { enter: true, text: true, ..Accept::default() },
        )
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
