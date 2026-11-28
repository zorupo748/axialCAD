//! MCP tool definitions and dispatch.

use serde_json::{Value, json};

use crate::backend::Backend;

/// The allowed tools in read-only mode.
pub fn is_allowed_in_read_only(name: &str) -> bool {
    matches!(
        name,
        "inspect_drawing"
            | "query_entities"
            | "find_text"
            | "list_commands"
            | "render"
            | "screenshot"
            | "ui_inspect"
    )
}

/// The `tools/list` payload: one definition (name, description, input schema) per MCP tool,
/// including the UI-only tools when `has_ui` says a window is available. In `read_only` mode,
/// mutating tools are excluded and disk paths in render/screenshot schemas are omitted.
pub fn tool_definitions(has_ui: bool, read_only: bool) -> Value {
    let readonly_annotations = json!({
        "readOnlyHint": true,
        "destructiveHint": false,
        "openWorldHint": false
    });

    let render_schema = if read_only {
        json!({"type": "object", "properties": {
            "width": {"type": "integer", "minimum": 16},
            "height": {"type": "integer", "minimum": 16},
            "fit": {"type": "boolean"},
            "window": {"type": "array", "items": {"type": "array", "items": {"type": "number"}, "minItems": 2, "maxItems": 2}, "minItems": 2, "maxItems": 2}
        }})
    } else {
        json!({"type": "object", "properties": {
            "width": {"type": "integer", "minimum": 16},
            "height": {"type": "integer", "minimum": 16},
            "fit": {"type": "boolean"},
            "window": {"type": "array", "items": {"type": "array", "items": {"type": "number"}, "minItems": 2, "maxItems": 2}, "minItems": 2, "maxItems": 2},
            "path": {"type": "string"}
        }})
    };

    let mut tools = vec![
        json!({"name": "command_line", "description": "Type one line at the axialCAD command line, exactly as a user would at an AutoCAD-style prompt. Starts commands by name or alias (LINE, L, CIRCLE, C, PLINE, RECTANG, MOVE, TRIM, OFFSET, ZOOM…) and answers the active prompt: points `x,y`, relative `@dx,dy`, polar `@dist<angle`, distances, keywords (e.g. `c` for Close), empty text = Enter. Spaces act as Enter, so `circle 0,0 5` works in one call. Returns the new prompt and command-line output, and `error` when the line was refused.",
            "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]}}),
        json!({"name": "script", "description": "Run a multi-line script (like an AutoCAD .scr file): one or more inputs per line, blank line = Enter.",
            "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]}}),
        json!({"name": "execute", "description": "Run any command with JSON parameters (no prompts or dialogs). Use list_commands to find ids and parameter docs. Examples: line {points:[[0,0],[10,0]]}, circle {center:[0,0], radius:5}, layer.new {name:\"Walls\", color:\"red\", current:true}, move {handles:[\"10A\"], delta:[5,0]}, zoom {mode:\"extents\"}.",
            "inputSchema": {"type": "object", "properties": {"command": {"type": "string"}, "params": {"type": "object"}}, "required": ["command"]}}),
        json!({"name": "list_commands", "description": "List commands with ids, labels, menu paths, aliases and parameter docs. Optional filter: a substring of the command id, label or an alias.",
            "inputSchema": {"type": "object", "properties": {"filter": {"type": "string"}}},
            "annotations": readonly_annotations.clone()}),
        json!({"name": "inspect_drawing", "description": "Summary of the active drawing: entity counts, extents, layers, styles, blocks, layouts, selection, undo history and (optionally) entities with handles and geometry.",
            "inputSchema": {"type": "object", "properties": {"entities": {"type": "boolean"}, "limit": {"type": "integer"}}},
            "annotations": readonly_annotations.clone()}),
        json!({"name": "query_entities", "description": "Entities filtered by type (Line, Circle, Polyline…), layer, current selection (selected:true; active space only) and/or a window [[x0,y0],[x1,y1]] (objects whose geometry is inside or crosses it; crossing:false = entirely inside). Returns count (all matches), returned (this page) and entities; page with offset and limit (default 500).",
            "inputSchema": {"type": "object", "properties": {"type": {"type": "string"}, "layer": {"type": "string"}, "selected": {"type": "boolean"},
                "window": {"type": "array", "items": {"type": "array", "items": {"type": "number"}, "minItems": 2, "maxItems": 2}, "minItems": 2, "maxItems": 2},
                "crossing": {"type": "boolean"}, "limit": {"type": "integer", "minimum": 0}, "offset": {"type": "integer", "minimum": 0}}},
            "annotations": readonly_annotations.clone()}),
        json!({"name": "render", "description": "Render the active drawing to a PNG and return it as an image: fitted to the extents by default, or just the area `window` [[x0,y0],[x1,y1]] (drawing units), e.g. one sheet or one room of a large drawing. With a window and no size the image is 1600 px wide with the window's shape.",
            "inputSchema": render_schema}),
        json!({"name": "find_text", "description": "Read the drawing's texts: TEXT, MTEXT (formatting removed), block attributes and multileaders, each with its text, insertion point `at`, height, rotation, style, layer and handle. Filter by `contains` (any case), `layer` and/or `window` [[x0,y0],[x1,y1]] around the insertion points; page with offset and limit (default 200). Use it to find room names, sheet titles and notes without reading all the geometry.",
            "inputSchema": {"type": "object", "properties": {"contains": {"type": "string"}, "layer": {"type": "string"},
                "window": {"type": "array", "items": {"type": "array", "items": {"type": "number"}, "minItems": 2, "maxItems": 2}, "minItems": 2, "maxItems": 2},
                "limit": {"type": "integer", "minimum": 0}, "offset": {"type": "integer", "minimum": 0}}},
            "annotations": readonly_annotations.clone()}),
        json!({"name": "new_drawing", "description": "Create a new drawing (imperial by default, or metric).",
            "inputSchema": {"type": "object", "properties": {"metric": {"type": "boolean"}}}}),
        json!({"name": "open", "description": "Open a DXF or DWG file.", "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}}),
        json!({"name": "save", "description": "Save the drawing (DXF or DWG by extension). A .svg, .png or .pdf path exports instead: the drawing keeps its name and unsaved changes.", "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}}}}),
        json!({"name": "cancel", "description": "Press Escape: cancel the running command (or clear the selection).", "inputSchema": {"type": "object", "properties": {}}}),
    ];
    if read_only {
        if let Some(tool) = tools.iter_mut().find(|t| t["name"] == "render") {
            tool["annotations"] = readonly_annotations.clone();
        }
    }
    if has_ui {
        let screenshot_schema = if read_only {
            json!({"type": "object", "properties": {}})
        } else {
            json!({"type": "object", "properties": {"path": {"type": "string"}}})
        };
        tools.push(json!({"name": "screenshot", "description": "Screenshot the axialCAD window (PNG).", "inputSchema": screenshot_schema}));
        if read_only {
            if let Some(tool) = tools.last_mut() {
                tool["annotations"] = readonly_annotations.clone();
            }
        }
        tools.push(json!({"name": "ui_inspect", "description": "UI state: panels, canvas rect, view, cursor and snap, performance.", "inputSchema": {"type": "object", "properties": {}},
            "annotations": readonly_annotations.clone()}));
        tools.push(json!({"name": "ui_click", "description": "Click in the drawing area at world coordinates (or screen with space:'screen'). Right button = Enter.", "inputSchema": {"type": "object", "properties": {"x": {"type": "number"}, "y": {"type": "number"}, "space": {"type": "string"}, "button": {"type": "string"}}, "required": ["x", "y"]}}));
    }
    if read_only {
        tools.retain(|t| t.get("name").and_then(Value::as_str).is_some_and(is_allowed_in_read_only));
    }
    json!({ "tools": tools })
}

/// Wrap a JSON value as a successful MCP result containing pretty-printed text.
fn text(v: &Value) -> Value {
    json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(v).unwrap_or_default()}]})
}

/// Wrap an error message as an MCP tool error result (`isError`), not a protocol error.
fn err_text(e: &str) -> Value {
    json!({"content": [{"type": "text", "text": e}], "isError": true})
}

/// Whether `v` fits the subset of JSON Schema the tool definitions use: `type`, `items`,
/// `minItems`, `maxItems` and `minimum`.
fn fits(v: &Value, schema: &Value) -> bool {
    let typed = match schema["type"].as_str() {
        Some("string") => v.is_string(),
        // JSON Schema integers include numbers such as 2.0.
        Some("integer") => v.as_f64().is_some_and(|f| f.fract() == 0.0),
        Some("number") => v.is_number(),
        Some("boolean") => v.is_boolean(),
        Some("array") => v.is_array(),
        Some("object") => v.is_object(),
        _ => true,
    };
    let len = v.as_array().map(Vec::len);
    typed
        && schema["minimum"].as_f64().is_none_or(|m| v.as_f64().is_none_or(|f| f >= m))
        && schema["minItems"].as_u64().is_none_or(|m| len.is_none_or(|n| n as u64 >= m))
        && schema["maxItems"].as_u64().is_none_or(|m| len.is_none_or(|n| n as u64 <= m))
        && (schema["items"].is_null() || v.as_array().is_none_or(|a| a.iter().all(|i| fits(i, &schema["items"]))))
}

/// Arguments checked against the tool's input schema: an object, required properties present and
/// each given property fitting its schema (`null` counts as not given). `Err` is the
/// invalid-params message.
fn check_args(name: &str, args: &Value, has_ui: bool, read_only: bool) -> Result<Value, String> {
    if read_only {
        if !is_allowed_in_read_only(name) {
            return Err(format!("tool `{name}` is disabled in read-only mode"));
        }
        if !has_ui && (name == "ui_inspect" || name == "screenshot") {
            return Err(format!("tool `{name}` is only available when a UI is present"));
        }
    }
    let defs = tool_definitions(has_ui || !read_only, read_only);
    let schema = defs["tools"].as_array().and_then(|t| t.iter().find(|t| t["name"] == name)).map(|t| t["inputSchema"].clone());
    let Some(schema) = schema else { return Err(format!("unknown tool `{name}`")) };
    let a = match args {
        Value::Null => json!({}),
        Value::Object(_) => args.clone(),
        _ => return Err(format!("`arguments` of `{name}` must be an object")),
    };
    if read_only {
        if a.get("path").is_some_and(|p| !p.is_null()) {
            return Err(format!("`path` parameter is disabled in read-only mode for tool `{name}`"));
        }
        if let Some(obj) = a.as_object() {
            let empty_map = serde_json::Map::new();
            let schema_props = schema.get("properties").and_then(Value::as_object).unwrap_or(&empty_map);
            for key in obj.keys() {
                if !schema_props.contains_key(key) {
                    return Err(format!("unknown property `{key}` for tool `{name}` in read-only mode"));
                }
            }
        }
    }
    for key in schema["required"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        if a.get(key).is_none_or(Value::is_null) {
            return Err(format!("`{name}` needs `{key}`"));
        }
    }
    for (key, prop) in schema["properties"].as_object().into_iter().flatten() {
        if let Some(v) = a.get(key).filter(|v| !v.is_null())
            && !fits(v, prop)
        {
            return Err(format!("`{key}` of `{name}` must match {prop}"));
        }
    }
    Ok(a)
}

/// Whether a command-catalog entry's id, label or one of its aliases contains `f` (lower case).
fn command_matches(c: &Value, f: &str) -> bool {
    let has = |s: &str| s.to_ascii_lowercase().contains(f);
    ["id", "label"].iter().any(|k| c[*k].as_str().is_some_and(has))
        || c["aliases"].as_array().is_some_and(|a| a.iter().filter_map(Value::as_str).any(has))
}

/// Run a tool. `Err` is a protocol error (unknown tool, or arguments that don't fit its input
/// schema), answered with JSON-RPC invalid params; a failing tool is an `isError` result.
pub fn call_tool(b: &mut dyn Backend, name: &str, args: &Value, read_only: bool) -> Result<Value, String> {
    let a = check_args(name, args, b.has_ui(), read_only)?;
    let r = match name {
        "command_line" => b.call("cmdline.input", a),
        "script" => b.call("cmdline.script", a),
        "execute" => b.call(
            "engine.execute",
            json!({"command": a.get("command").cloned().unwrap_or(Value::Null), "params": a.get("params").cloned().unwrap_or(json!({}))}),
        ),
        "list_commands" => b.call("engine.commands", json!({})).map(|v| {
            let f = a.get("filter").and_then(Value::as_str).map(str::to_ascii_lowercase);
            match (v, f) {
                (Value::Array(items), Some(f)) => Value::Array(items.into_iter().filter(|c| command_matches(c, &f)).collect()),
                (v, _) => v,
            }
        }),
        "inspect_drawing" => b.call("drawing.inspect", a),
        "query_entities" => b.call("engine.execute", json!({"command": "entities", "params": a})),
        "find_text" => b.call("engine.execute", json!({"command": "texts", "params": a})),
        "new_drawing" => b.call("engine.execute", json!({"command": "new", "params": a})),
        "open" => b.call("app.open", a),
        "save" => b.call("app.save", a),
        "cancel" => b.call("cmdline.key", json!({"key": "escape"})),
        "screenshot" => {
            return Ok(match b.call("ui.screenshot", a) {
                Ok(v) => match v.get("pngBase64").and_then(Value::as_str) {
                    Some(data) => json!({"content": [{"type": "image", "data": data, "mimeType": "image/png"}]}),
                    None => text(&v),
                },
                Err(e) => err_text(&e),
            });
        }
        "ui_inspect" => b.call("ui.inspect", json!({})),
        "ui_click" => b.call("ui.pointer", a),
        "render" => {
            return Ok(match b.call("ui.render", a) {
                Ok(v) => match v.get("pngBase64").and_then(Value::as_str) {
                    Some(data) => {
                        let mut content = vec![json!({"type": "image", "data": data, "mimeType": "image/png"})];
                        if let Some(png_bytes) = cadcraft_engine::cmd::file::base64_decode(data) {
                            let dims_str = match parse_png_dimensions(&png_bytes) {
                                Some((w, h)) => format!("{w}x{h}"),
                                None => "unavailable".to_string(),
                            };
                            let sha = sha256_hex(&png_bytes);
                            let bytes_len = png_bytes.len();
                            let meta_text = format!(
                                "MIME: image/png\nDimensions: {dims_str}\nSize: {bytes_len} bytes\nSHA-256: {sha}"
                            );
                            content.push(json!({"type": "text", "text": meta_text}));
                        } else {
                            content.push(json!({
                                "type": "text",
                                "text": "MIME: image/png\nMetadata: unavailable (invalid base64 payload)"
                            }));
                        }
                        json!({"content": content})
                    }
                    None => text(&v),
                },
                Err(e) => err_text(&e),
            });
        }
        other => return Err(format!("unknown tool `{other}`")),
    };
    Ok(match r {
        Ok(v) => text(&v),
        Err(e) => err_text(&e),
    })
}

/// Reads width and height from a PNG signature and IHDR chunk.
///
/// Limits: This reads only the 8-byte signature and the first chunk (IHDR) header;
/// it does NOT validate subsequent chunks (IDAT, IEND) or verify the IHDR CRC.
///
/// It checks:
/// 1. 8-byte PNG signature: `[137, 80, 78, 71, 13, 10, 26, 10]`.
/// 2. Declared IHDR data length in bytes 8..12 equals 13 (`0x0000000d`).
/// 3. Chunk type in bytes 12..16 equals `b"IHDR"`.
/// 4. Buffer contains the full chunk including its 4-byte CRC space (at least 33 bytes: 8 signature + 4 length + 4 type + 13 data + 4 CRC).
/// 5. Width and height are non-zero and within valid PNG range (1..=i32::MAX as u32).
pub(crate) fn parse_png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    const SIGNATURE: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
    const MIN_PNG_HEADER_LEN: usize = 8 + 4 + 4 + 13 + 4; // 33 bytes
    if bytes.len() < MIN_PNG_HEADER_LEN {
        return None;
    }
    if bytes.get(0..8)? != SIGNATURE {
        return None;
    }
    // Length of IHDR data must be exactly 13 bytes
    let ihdr_len = u32::from_be_bytes(bytes.get(8..12)?.try_into().ok()?);
    if ihdr_len != 13 {
        return None;
    }
    // Chunk type must be IHDR
    if bytes.get(12..16)? != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes(bytes.get(16..20)?.try_into().ok()?);
    let height = u32::from_be_bytes(bytes.get(20..24)?.try_into().ok()?);
    // PNG specification requires width and height to be non-zero and up to 2^31 - 1
    const MAX_DIMENSION: u32 = 0x7fff_ffff;
    if width == 0 || width > MAX_DIMENSION || height == 0 || height > MAX_DIMENSION {
        return None;
    }
    Some((width, height))
}

/// The SHA-256 digest of `bytes` as 64 lowercase hexadecimal characters (FIPS 180-4).
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    // Padding: a single 1 bit, zeros, then the message length in bits as a big-endian u64.
    let bit_len = u64::try_from(bytes.len()).unwrap_or(0).wrapping_mul(8);
    let mut padded = bytes.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for block in padded.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, word) in block.chunks_exact(4).take(16).enumerate() {
            let candidate = [word[0], word[1], word[2], word[3]];
            w[i] = u32::from_be_bytes(candidate);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut hh = h[7];

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut out = String::with_capacity(64);
    for word in h {
        use std::fmt::Write;
        let _ = write!(out, "{word:08x}");
    }
    out
}
