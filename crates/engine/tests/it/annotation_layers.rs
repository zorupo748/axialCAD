//! Automatic layer assignment for dimensions and hatches.

use cadcraft_color::Color;
use cadcraft_engine::Session;
use cadcraft_engine::doc::{EntityKind, Handle};
use serde_json::json;

fn doc(s: &Session) -> cadcraft_engine::doc::Drawing {
    s.doc().unwrap().clone()
}

fn dim_layer(s: &Session, h: Handle) -> String {
    match &doc(s).entity(h).unwrap().kind {
        EntityKind::Dimension(_) => doc(s).entity(h).unwrap().common.layer.clone(),
        k => panic!("not a dimension: {k:?}"),
    }
}

fn hatch_layer(s: &Session, h: Handle) -> String {
    match &doc(s).entity(h).unwrap().kind {
        EntityKind::Hatch(_) => doc(s).entity(h).unwrap().common.layer.clone(),
        k => panic!("not a hatch: {k:?}"),
    }
}

fn dim_color(s: &Session, h: Handle) -> Color {
    doc(s).entity(h).unwrap().common.color
}

fn hatch_color(s: &Session, h: Handle) -> Color {
    doc(s).entity(h).unwrap().common.color
}

fn layer_color(s: &Session, name: &str) -> Color {
    doc(s).layer(name).unwrap().color
}

#[test]
fn dimensions_go_to_cotas_layer() {
    let mut s = Session::new();
    let _ = s.new_drawing(false);
    let r = s.execute("dimlinear", &json!({ "p1": [0, 0], "p2": [10, 0], "at": [5, 3] })).unwrap();
    let h = Handle::parse_hex(r["handle"].as_str().unwrap()).unwrap();
    assert_eq!(dim_layer(&s, h), "01 Cotas 0.18");
    assert_eq!(dim_color(&s, h), Color::ByLayer);
    assert_eq!(layer_color(&s, "01 Cotas 0.18"), Color::Index(11));
}

#[test]
fn hatches_go_to_rayados_layer() {
    let mut s = Session::new();
    let _ = s.new_drawing(false);
    s.execute("circle", &json!({ "center": [0, 0], "radius": 5 })).unwrap();
    let c = s.doc().unwrap().model.last().unwrap().handle;
    let r = s.execute("hatch", &json!({ "points": [[0, 0]], "handles": [c.hex()] })).unwrap();
    let h = Handle::parse_hex(r["handles"][0].as_str().unwrap()).unwrap();
    assert_eq!(hatch_layer(&s, h), "01 Rayados 0.18");
    assert_eq!(hatch_color(&s, h), Color::ByLayer);
    assert_eq!(layer_color(&s, "01 Rayados 0.18"), Color::Index(8));
}

#[test]
fn dimclayer_is_customizable() {
    let mut s = Session::new();
    let _ = s.new_drawing(false);
    s.execute("setvar", &json!({ "name": "DIMCLAYER", "value": "MyDimLayer" })).unwrap();
    let r = s.execute("dimlinear", &json!({ "p1": [0, 0], "p2": [10, 0], "at": [5, 3] })).unwrap();
    let h = Handle::parse_hex(r["handle"].as_str().unwrap()).unwrap();
    assert_eq!(dim_layer(&s, h), "MyDimLayer");
    assert_eq!(layer_color(&s, "MyDimLayer"), Color::Index(11));
}

#[test]
fn hatchclayer_is_customizable() {
    let mut s = Session::new();
    let _ = s.new_drawing(false);
    s.execute("setvar", &json!({ "name": "HATCHCLAYER", "value": "MyHatchLayer" })).unwrap();
    s.execute("circle", &json!({ "center": [0, 0], "radius": 5 })).unwrap();
    let c = s.doc().unwrap().model.last().unwrap().handle;
    let r = s.execute("hatch", &json!({ "points": [[0, 0]], "handles": [c.hex()] })).unwrap();
    let h = Handle::parse_hex(r["handles"][0].as_str().unwrap()).unwrap();
    assert_eq!(hatch_layer(&s, h), "MyHatchLayer");
    assert_eq!(layer_color(&s, "MyHatchLayer"), Color::Index(8));
}

#[test]
fn existing_layer_keeps_its_color() {
    let mut s = Session::new();
    let _ = s.new_drawing(false);
    s.execute("layer.new", &json!({ "name": "01 Cotas 0.18", "color": 3 })).unwrap();
    assert_eq!(layer_color(&s, "01 Cotas 0.18"), Color::Index(3));
    let r = s.execute("dimlinear", &json!({ "p1": [0, 0], "p2": [10, 0], "at": [5, 3] })).unwrap();
    let h = Handle::parse_hex(r["handle"].as_str().unwrap()).unwrap();
    assert_eq!(dim_layer(&s, h), "01 Cotas 0.18");
    assert_eq!(layer_color(&s, "01 Cotas 0.18"), Color::Index(3));
}

#[test]
fn locked_layer_rejects_dimension() {
    let mut s = Session::new();
    let _ = s.new_drawing(false);
    s.execute("layer.new", &json!({ "name": "01 Cotas 0.18" })).unwrap();
    s.execute("layer.current", &json!({ "name": "01 Cotas 0.18" })).unwrap();
    s.execute("line", &json!({ "points": [[0, 0], [1, 0]] })).unwrap();
    let lh = s.doc().unwrap().model.last().unwrap().handle;
    s.execute("laylck", &json!({ "handles": [lh.hex()] })).unwrap();
    let r = s.execute("dimlinear", &json!({ "p1": [0, 0], "p2": [10, 0], "at": [5, 3] }));
    assert!(r.is_err());
}

#[test]
fn locked_layer_rejects_hatch() {
    let mut s = Session::new();
    let _ = s.new_drawing(false);
    s.execute("layer.new", &json!({ "name": "01 Rayados 0.18" })).unwrap();
    s.execute("layer.current", &json!({ "name": "01 Rayados 0.18" })).unwrap();
    s.execute("line", &json!({ "points": [[0, 0], [1, 0]] })).unwrap();
    let lh = s.doc().unwrap().model.last().unwrap().handle;
    s.execute("laylck", &json!({ "handles": [lh.hex()] })).unwrap();
    s.execute("circle", &json!({ "center": [0, 0], "radius": 5 })).unwrap();
    let c = s.doc().unwrap().model.last().unwrap().handle;
    let r = s.execute("hatch", &json!({ "points": [[0, 0]], "handles": [c.hex()] }));
    assert!(r.is_err());
}

#[test]
fn clayer_is_not_changed_by_dimension() {
    let mut s = Session::new();
    let _ = s.new_drawing(false);
    s.execute("layer.new", &json!({ "name": "Walls", "current": true })).unwrap();
    assert_eq!(s.doc().unwrap().header.str("CLAYER", ""), "Walls");
    let r = s.execute("dimlinear", &json!({ "p1": [0, 0], "p2": [10, 0], "at": [5, 3] })).unwrap();
    let h = Handle::parse_hex(r["handle"].as_str().unwrap()).unwrap();
    assert_eq!(dim_layer(&s, h), "01 Cotas 0.18");
    assert_eq!(s.doc().unwrap().header.str("CLAYER", ""), "Walls");
}

#[test]
fn clayer_is_not_changed_by_hatch() {
    let mut s = Session::new();
    let _ = s.new_drawing(false);
    s.execute("layer.new", &json!({ "name": "Walls", "current": true })).unwrap();
    assert_eq!(s.doc().unwrap().header.str("CLAYER", ""), "Walls");
    s.execute("circle", &json!({ "center": [0, 0], "radius": 5 })).unwrap();
    let c = s.doc().unwrap().model.last().unwrap().handle;
    let r = s.execute("hatch", &json!({ "points": [[0, 0]], "handles": [c.hex()] })).unwrap();
    let h = Handle::parse_hex(r["handles"][0].as_str().unwrap()).unwrap();
    assert_eq!(hatch_layer(&s, h), "01 Rayados 0.18");
    assert_eq!(s.doc().unwrap().header.str("CLAYER", ""), "Walls");
}

#[test]
fn getvar_reports_dimclayer_and_hatchclayer() {
    let mut s = Session::new();
    let _ = s.new_drawing(false);
    let v: serde_json::Value = s.execute("getvar", &json!({ "name": "DIMCLAYER" })).unwrap();
    assert_eq!(v["value"], json!("01 Cotas 0.18"));
    let v: serde_json::Value = s.execute("getvar", &json!({ "name": "HATCHCLAYER" })).unwrap();
    assert_eq!(v["value"], json!("01 Rayados 0.18"));
}

#[test]
fn undo_removes_new_layer_and_dimension() {
    let mut s = Session::new();
    let _ = s.new_drawing(false);
    s.execute("setvar", &json!({ "name": "DIMCLAYER", "value": "TempDimLayer" })).unwrap();
    let before = s.doc().unwrap().model.len();
    let r = s.execute("dimlinear", &json!({ "p1": [0, 0], "p2": [10, 0], "at": [5, 3] })).unwrap();
    let h = Handle::parse_hex(r["handle"].as_str().unwrap()).unwrap();
    assert_eq!(dim_layer(&s, h), "TempDimLayer");
    assert!(s.doc().unwrap().layer("TempDimLayer").is_some());
    assert_eq!(s.doc().unwrap().model.len(), before + 1);
    s.execute("undo", &json!({})).unwrap();
    assert!(s.doc().unwrap().layer("TempDimLayer").is_none());
    assert_eq!(s.doc().unwrap().model.len(), before);
    assert!(s.doc().unwrap().entity(h).is_none());
    s.execute("redo", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().model.len(), before + 1);
    assert!(s.doc().unwrap().layer("TempDimLayer").is_some());
    assert!(s.doc().unwrap().entity(h).is_some());
}

#[test]
fn undo_removes_new_layer_and_hatch() {
    let mut s = Session::new();
    let _ = s.new_drawing(false);
    s.execute("setvar", &json!({ "name": "HATCHCLAYER", "value": "TempHatchLayer" })).unwrap();
    s.execute("circle", &json!({ "center": [0, 0], "radius": 5 })).unwrap();
    let c = s.doc().unwrap().model.last().unwrap().handle;
    let r = s.execute("hatch", &json!({ "points": [[0, 0]], "handles": [c.hex()] })).unwrap();
    let h = Handle::parse_hex(r["handles"][0].as_str().unwrap()).unwrap();
    assert_eq!(hatch_layer(&s, h), "TempHatchLayer");
    assert!(s.doc().unwrap().layer("TempHatchLayer").is_some());
    s.execute("undo", &json!({})).unwrap();
    assert!(s.doc().unwrap().layer("TempHatchLayer").is_none());
    assert!(s.doc().unwrap().entity(h).is_none());
}

#[test]
fn redo_restores_new_layer_and_hatch() {
    let mut s = Session::new();
    let _ = s.new_drawing(false);
    s.execute("setvar", &json!({ "name": "HATCHCLAYER", "value": "TempHatchLayer2" })).unwrap();
    s.execute("circle", &json!({ "center": [0, 0], "radius": 5 })).unwrap();
    let c = s.doc().unwrap().model.last().unwrap().handle;
    let before = s.doc().unwrap().model.len();
    let r = s.execute("hatch", &json!({ "points": [[0, 0]], "handles": [c.hex()] })).unwrap();
    let h = Handle::parse_hex(r["handles"][0].as_str().unwrap()).unwrap();
    assert!(s.doc().unwrap().layer("TempHatchLayer2").is_some());
    assert_eq!(s.doc().unwrap().model.len(), before + 1);
    s.execute("undo", &json!({})).unwrap();
    assert!(s.doc().unwrap().layer("TempHatchLayer2").is_none());
    assert!(s.doc().unwrap().entity(h).is_none());
    assert_eq!(s.doc().unwrap().model.len(), before);
    s.execute("redo", &json!({})).unwrap();
    assert!(s.doc().unwrap().layer("TempHatchLayer2").is_some());
    assert!(s.doc().unwrap().entity(h).is_some());
    assert_eq!(s.doc().unwrap().model.len(), before + 1);
}

#[test]
fn off_layer_rejects_dimension_without_clayer_fallback() {
    let mut s = Session::new();
    let _ = s.new_drawing(false);
    s.execute("layer.new", &json!({ "name": "01 Cotas 0.18" })).unwrap();
    s.execute("layer.current", &json!({ "name": "01 Cotas 0.18" })).unwrap();
    s.execute("line", &json!({ "points": [[0, 0], [1, 0]] })).unwrap();
    let lh = s.doc().unwrap().model.last().unwrap().handle;
    s.execute("layoff", &json!({ "handles": [lh.hex()] })).unwrap();
    let r = s.execute("dimlinear", &json!({ "p1": [0, 0], "p2": [10, 0], "at": [5, 3] }));
    assert!(r.is_err());
}

#[test]
fn off_layer_rejects_hatch_without_clayer_fallback() {
    let mut s = Session::new();
    let _ = s.new_drawing(false);
    s.execute("layer.new", &json!({ "name": "01 Rayados 0.18" })).unwrap();
    s.execute("layer.current", &json!({ "name": "01 Rayados 0.18" })).unwrap();
    s.execute("line", &json!({ "points": [[0, 0], [1, 0]] })).unwrap();
    let lh = s.doc().unwrap().model.last().unwrap().handle;
    s.execute("layoff", &json!({ "handles": [lh.hex()] })).unwrap();
    s.execute("circle", &json!({ "center": [0, 0], "radius": 5 })).unwrap();
    let c = s.doc().unwrap().model.last().unwrap().handle;
    let r = s.execute("hatch", &json!({ "points": [[0, 0]], "handles": [c.hex()] }));
    assert!(r.is_err());
}

#[test]
fn frozen_layer_rejects_dimension_without_clayer_fallback() {
    let mut s = Session::new();
    let _ = s.new_drawing(false);
    s.execute("layer.new", &json!({ "name": "01 Cotas 0.18" })).unwrap();
    s.execute("line", &json!({ "points": [[0, 0], [1, 0]] })).unwrap();
    let lh = s.doc().unwrap().model.last().unwrap().handle;
    s.execute("laymch", &json!({ "handles": [lh.hex()], "layer": "01 Cotas 0.18" })).unwrap();
    s.execute("layfrz", &json!({ "handles": [lh.hex()] })).unwrap();
    let r = s.execute("dimlinear", &json!({ "p1": [0, 0], "p2": [10, 0], "at": [5, 3] }));
    assert!(r.is_err());
}

#[test]
fn frozen_layer_rejects_hatch_without_clayer_fallback() {
    let mut s = Session::new();
    let _ = s.new_drawing(false);
    s.execute("layer.new", &json!({ "name": "01 Rayados 0.18" })).unwrap();
    s.execute("line", &json!({ "points": [[0, 0], [1, 0]] })).unwrap();
    let lh = s.doc().unwrap().model.last().unwrap().handle;
    s.execute("laymch", &json!({ "handles": [lh.hex()], "layer": "01 Rayados 0.18" })).unwrap();
    s.execute("layfrz", &json!({ "handles": [lh.hex()] })).unwrap();
    s.execute("circle", &json!({ "center": [0, 0], "radius": 5 })).unwrap();
    let c = s.doc().unwrap().model.last().unwrap().handle;
    let r = s.execute("hatch", &json!({ "points": [[0, 0]], "handles": [c.hex()] }));
    assert!(r.is_err());
}

#[test]
fn clayer_and_cecolor_are_preserved_for_dimension() {
    let mut s = Session::new();
    let _ = s.new_drawing(false);
    s.execute("layer.new", &json!({ "name": "Walls", "current": true })).unwrap();
    assert_eq!(s.doc().unwrap().header.str("CLAYER", ""), "Walls");
    s.execute("setvar", &json!({ "name": "CECOLOR", "value": 1 })).unwrap();
    let r = s.execute("dimlinear", &json!({ "p1": [0, 0], "p2": [10, 0], "at": [5, 3] })).unwrap();
    let h = Handle::parse_hex(r["handle"].as_str().unwrap()).unwrap();
    assert_eq!(dim_layer(&s, h), "01 Cotas 0.18");
    assert_eq!(dim_color(&s, h), Color::ByLayer);
    assert_eq!(s.doc().unwrap().header.str("CLAYER", ""), "Walls");
}

#[test]
fn clayer_and_cecolor_are_preserved_for_hatch() {
    let mut s = Session::new();
    let _ = s.new_drawing(false);
    s.execute("layer.new", &json!({ "name": "Walls", "current": true })).unwrap();
    assert_eq!(s.doc().unwrap().header.str("CLAYER", ""), "Walls");
    s.execute("setvar", &json!({ "name": "CECOLOR", "value": 1 })).unwrap();
    s.execute("circle", &json!({ "center": [0, 0], "radius": 5 })).unwrap();
    let c = s.doc().unwrap().model.last().unwrap().handle;
    let r = s.execute("hatch", &json!({ "points": [[0, 0]], "handles": [c.hex()] })).unwrap();
    let h = Handle::parse_hex(r["handles"][0].as_str().unwrap()).unwrap();
    assert_eq!(hatch_layer(&s, h), "01 Rayados 0.18");
    assert_eq!(hatch_color(&s, h), Color::ByLayer);
    assert_eq!(s.doc().unwrap().header.str("CLAYER", ""), "Walls");
}

#[test]
fn other_entity_types_use_clayer_not_annotation_layer() {
    let mut s = Session::new();
    let _ = s.new_drawing(false);
    s.execute("line", &json!({ "points": [[0, 0], [10, 0]] })).unwrap();
    let lh = s.doc().unwrap().model.last().unwrap().handle;
    assert_eq!(doc(&s).entity(lh).unwrap().common.layer, "0");
    s.execute("circle", &json!({ "center": [0, 0], "radius": 5 })).unwrap();
    let ch = s.doc().unwrap().model.last().unwrap().handle;
    assert_eq!(doc(&s).entity(ch).unwrap().common.layer, "0");
}
