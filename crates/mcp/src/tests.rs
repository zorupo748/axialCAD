//! An agent completes realistic drafting tasks through MCP only, and verifies through MCP only.

use serde_json::{Value, json};

use crate::{Headless, Server};

/// Send one JSON-RPC request to the server and return the parsed reply; fails the test if the
/// server does not answer or answers with invalid JSON.
fn call(s: &mut Server, id: u64, method: &str, params: Value) -> Value {
    let line = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string();
    let r = s.handle_line(&line).expect("reply");
    serde_json::from_str(&r).unwrap()
}

/// Call an MCP tool and return its decoded result: JSON text is parsed, an image is passed
/// through, and a tool error fails the test with the reported message.
fn tool(s: &mut Server, name: &str, args: Value) -> Value {
    let r = call(s, 99, "tools/call", json!({"name": name, "arguments": args}));
    let c = &r["result"]["content"][0];
    assert_ne!(r["result"]["isError"], json!(true), "{name} failed: {c}");
    if c["type"] == "text" { serde_json::from_str(c["text"].as_str().unwrap()).unwrap_or(Value::Null) } else { c.clone() }
}

/// The MCP handshake works: `initialize` names the server, the `initialized` notification is
/// accepted, `tools/list` offers the command line, and an unknown method is -32601.
#[test]
fn lifecycle_and_tools() {
    let mut s = Server::new(Box::new(Headless::default()));
    let init = call(&mut s, 1, "initialize", json!({"protocolVersion": "2025-06-18"}));
    assert_eq!(init["result"]["serverInfo"]["name"], "cadcraft");
    assert!(s.handle_line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_none());
    assert!(s.is_initialized());
    let tools = call(&mut s, 2, "tools/list", json!({}));
    assert!(tools["result"]["tools"].as_array().unwrap().iter().any(|t| t["name"] == "command_line"));
    let bad = call(&mut s, 3, "nope", json!({}));
    assert_eq!(bad["error"]["code"], -32601);
}

/// Guards that an agent can complete a drawing (plate, holes, layer, text) using only the MCP
/// tools, and verify it only through MCP: counts, selection queries, undo and a PNG render.
#[test]
fn agent_draws_a_plate_with_holes() {
    let mut s = Server::new(Box::new(Headless::default()));
    // Task 1: an outline via the command line, AutoCAD style.
    tool(&mut s, "command_line", json!({"text": "rectang 0,0 100,60"}));
    // Task 2: four holes with JSON.
    for (x, y) in [(10, 10), (90, 10), (10, 50), (90, 50)] {
        tool(&mut s, "execute", json!({"command": "circle", "params": {"center": [x, y], "radius": 4}}));
    }
    // Task 3: a layer and a centre line on it.
    tool(&mut s, "execute", json!({"command": "layer.new", "params": {"name": "Center", "color": "red", "current": true}}));
    tool(&mut s, "command_line", json!({"text": "line -5,30 105,30"}));
    tool(&mut s, "command_line", json!({"text": ""}));
    // Task 4: a script with text. TEXT keeps asking for further lines until an empty one, as in
    // AutoCAD, so the script ends it with a blank line (= Enter).
    let r = tool(&mut s, "script", json!({"text": "TEXT 0,-10 5 0 PLATE 100x60\n\n"}));
    assert_eq!(r["state"]["running"], Value::Null, "TEXT must be finished: {r}");
    // Verify only through MCP.
    let d = tool(&mut s, "inspect_drawing", json!({}));
    assert_eq!(d["counts"]["Circle"], 4);
    assert_eq!(d["counts"]["Polyline"], 1);
    assert_eq!(d["counts"]["Line"], 1);
    assert_eq!(d["counts"]["Text"], 1);
    let lines = tool(&mut s, "query_entities", json!({"type": "Line", "layer": "Center"}));
    assert_eq!(lines["count"], 1);
    // Task 5: undo the text, check, then render.
    let u = tool(&mut s, "execute", json!({"command": "undo"}));
    assert_eq!(u["undone"], json!(["Single Line Text"]), "{u}");
    let d = tool(&mut s, "inspect_drawing", json!({"entities": false}));
    assert!(d["counts"].get("Text").is_none());
    assert_eq!(d["counts"]["Circle"], 4);
    assert_eq!(d["counts"]["Polyline"], 1);
    assert_eq!(d["counts"]["Line"], 1);
    let lines = tool(&mut s, "query_entities", json!({"type": "Line", "layer": "Center"}));
    assert_eq!(lines["count"], 1, "the centre line must survive undoing the text");
    let img = tool(&mut s, "render", json!({"width": 320, "height": 200}));
    assert_eq!(img["type"], "image");
    assert!(img["data"].as_str().unwrap().len() > 100);
}

/// Guards that bad input is reported, not fatal: a refused command and a screenshot without a
/// UI are `isError` results, and malformed JSON is a parse error (-32700).
#[test]
fn errors_are_tool_errors_not_crashes() {
    let mut s = Server::new(Box::new(Headless::default()));
    let r = call(&mut s, 1, "tools/call", json!({"name": "execute", "arguments": {"command": "circle", "params": {"center": "x"}}}));
    assert_eq!(r["result"]["isError"], true);
    let r = call(&mut s, 2, "tools/call", json!({"name": "screenshot", "arguments": {}}));
    assert_eq!(r["result"]["isError"], true);
    assert!(s.handle_line("{not json").unwrap().contains("-32700"));
}

/// Unknown tools and arguments that don't fit a tool's schema are invalid params (-32602), never
/// guessed at; an empty batch is one Invalid Request.
#[test]
fn bad_tool_calls_are_invalid_params() {
    let mut s = Server::new(Box::new(Headless::default()));
    tool(&mut s, "command_line", json!({"text": "circle 0,0 1"}));
    let bad = [
        json!({"name": "nope", "arguments": {}}),
        json!({"name": "command_line", "arguments": {}}),
        json!({"name": "command_line", "arguments": {"text": 5}}),
        json!({"name": "command_line", "arguments": "circle 1,1 1"}),
        json!({"name": "script", "arguments": {}}),
        json!({"name": "query_entities", "arguments": {"window": [-5, -5, 5, 5]}}),
        json!({"name": "query_entities", "arguments": {"window": [[-5, -5]]}}),
        json!({"name": "query_entities", "arguments": {"window": {"min": [-5, -5], "max": [5, 5]}}}),
        json!({"name": "query_entities", "arguments": {"type": 7}}),
        json!({"name": "query_entities", "arguments": {"layer": ["0"]}}),
        json!({"name": "query_entities", "arguments": {"offset": -1}}),
        json!({"name": "query_entities", "arguments": {"selected": "true"}}),
        json!({"name": "query_entities", "arguments": {"selected": 1}}),
    ];
    for (i, params) in bad.into_iter().enumerate() {
        let r = call(&mut s, i as u64, "tools/call", params.clone());
        assert_eq!(r["error"]["code"], -32602, "{params} -> {r}");
    }
    // Nothing was typed: the circle is still the only object and no command is running.
    let d = tool(&mut s, "inspect_drawing", json!({"entities": false}));
    assert_eq!(d["entityCount"], 1, "{d}");
    assert_eq!(s.handle_line("[]").map(|r| serde_json::from_str::<Value>(&r).unwrap()["error"]["code"].clone()), Some(json!(-32600)));
}

/// `open` reads both DXF and DWG, so `tools/list` must advertise both (#1875).
#[test]
fn open_tool_description_names_dxf_and_dwg() {
    let mut s = Server::new(Box::new(Headless::default()));
    let tools = call(&mut s, 1, "tools/list", json!({}));
    let open = tools["result"]["tools"].as_array().unwrap().iter().find(|t| t["name"] == "open").unwrap();
    let desc = open["description"].as_str().unwrap();
    assert!(desc.contains("DXF"), "`open` must still name DXF: {desc}");
    assert!(desc.contains("DWG"), "`open` reads DWG too: {desc}");
}

/// Guards that `query_entities` advertises its `selected` filter and honours it (active-space
/// selection only, both ways) without changing the drawing.
#[test]
fn query_entities_selection_filter_is_discoverable_and_forwarded() {
    let mut s = Server::new(Box::new(Headless::default()));
    let tools = call(&mut s, 1, "tools/list", json!({}));
    let query = tools["result"]["tools"].as_array().unwrap().iter().find(|t| t["name"] == "query_entities").unwrap();
    assert_eq!(query["inputSchema"]["properties"]["selected"]["type"], "boolean");
    tool(&mut s, "execute", json!({"command": "circle", "params": {"center": [0, 0], "radius": 2}}));
    tool(&mut s, "execute", json!({"command": "circle", "params": {"center": [10, 0], "radius": 2}}));
    assert_eq!(tool(&mut s, "query_entities", json!({"selected": true}))["count"], 0);
    tool(&mut s, "execute", json!({"command": "select", "params": {"window": [[-3, -3], [3, 3]]}}));
    let before = tool(&mut s, "inspect_drawing", json!({}));
    let selected = tool(&mut s, "query_entities", json!({"selected": true, "type": "Circle"}));
    assert_eq!(selected["count"], 1);
    assert_eq!(selected["entities"][0]["geometry"]["center"]["x"], 0.0);
    assert_eq!(tool(&mut s, "query_entities", json!({"selected": false}))["count"], 2);
    assert_eq!(tool(&mut s, "inspect_drawing", json!({})), before);
}

/// `count` is every match while `returned` is the page; a window matches geometry, not bounding boxes.
#[test]
fn query_entities_pages_and_windows_by_geometry() {
    let mut s = Server::new(Box::new(Headless::default()));
    for c in ["circle 0,0 1", "circle 100,100 1", "circle 200,0 1"] {
        tool(&mut s, "command_line", json!({"text": c}));
    }
    // An arc whose bounding box covers the window but whose curve stays outside it.
    tool(&mut s, "execute", json!({"command": "arc", "params": {"p1": [40, 0], "p2": [50, 10], "p3": [60, 0]}}));
    let r = tool(&mut s, "query_entities", json!({"type": "Circle", "limit": 1, "offset": 1}));
    assert_eq!((r["count"].clone(), r["returned"].clone()), (json!(3), json!(1)), "{r}");
    assert_eq!(r["entities"][0]["geometry"]["center"]["x"], 100.0, "{r}");
    let r = tool(&mut s, "query_entities", json!({"window": [[45, 1], [55, 5]]}));
    assert_eq!(r["count"], 0, "the arc passes above this window: {r}");
    let r = tool(&mut s, "query_entities", json!({"window": [[-5, -5], [55, 5]]}));
    assert_eq!(r["count"], 2, "the circle at 0,0 and the arc crossing the window: {r}");
    let r = tool(&mut s, "query_entities", json!({"window": [[-5, -5], [55, 5]], "crossing": false}));
    assert_eq!(r["count"], 1, "only the circle is entirely inside: {r}");
}

/// Guards that the `list_commands` filter matches ids, labels and aliases but not parameter
/// docs, so searching `circle` does not return QSELECT merely for mentioning circles.
#[test]
fn list_commands_filter_matches_names_not_docs() {
    let mut s = Server::new(Box::new(Headless::default()));
    let r = tool(&mut s, "list_commands", json!({"filter": "circle"}));
    let ids: Vec<&str> = r.as_array().unwrap().iter().filter_map(|c| c["id"].as_str()).collect();
    assert!(ids.contains(&"circle"), "{ids:?}");
    assert!(!ids.contains(&"qselect"), "QSELECT only mentions circles in its docs: {ids:?}");
    let r = tool(&mut s, "list_commands", json!({"filter": "pl"}));
    assert!(r.as_array().unwrap().iter().any(|c| c["id"] == "pline"), "aliases and ids match");
}

/// Guards that `find_text` returns TEXT and MTEXT with their placement, strips MTEXT formatting,
/// and filters by text, layer and window; a windowed render keeps that area's aspect ratio.
#[test]
fn find_text_reads_texts_and_render_shows_one_area() {
    let mut s = Server::new(Box::new(Headless::default()));
    call(&mut s, 1, "initialize", json!({"protocolVersion": "2025-06-18"}));
    tool(&mut s, "execute", json!({"command": "layer.new", "params": {"name": "Texto"}}));
    tool(&mut s, "execute", json!({"command": "text", "params": {"at": [1, 1], "height": 0.15, "text": "RECAMARA 1"}}));
    tool(&mut s, "execute", json!({"command": "text", "params": {"at": [50, 1], "height": 0.15, "text": "COCINA"}}));
    tool(
        &mut s,
        "execute",
        json!({"command": "mtext", "params": {"at": [1, 5], "height": 0.2, "width": 5, "text": "{\\fArial|b1;PLANTA} BAJA\\PNIVEL 0.00"}}),
    );
    tool(&mut s, "execute", json!({"command": "rectang", "params": {"p1": [0, 0], "p2": [10, 5]}}));

    let all = tool(&mut s, "find_text", json!({}));
    assert_eq!(all["count"], 3);
    let r = tool(&mut s, "find_text", json!({"contains": "recamara"}));
    assert_eq!(r["count"], 1);
    assert_eq!(r["texts"][0]["text"], "RECAMARA 1");
    assert_eq!(r["texts"][0]["at"], json!([1.0, 1.0]));
    assert_eq!(r["texts"][0]["height"], 0.15);
    // Multiline text comes back readable, without its formatting codes.
    let m = tool(&mut s, "find_text", json!({"contains": "planta"}));
    let t = m["texts"][0]["text"].as_str().unwrap();
    assert!(t.contains("PLANTA BAJA") && t.contains("NIVEL 0.00") && !t.contains('\\') && !t.contains('{'), "{t}");
    // By area: only what is inside the window.
    let w = tool(&mut s, "find_text", json!({"window": [[-1, -1], [20, 20]]}));
    assert_eq!(w["count"], 2);
    assert_eq!(tool(&mut s, "find_text", json!({"layer": "nope"}))["count"], 0);

    // A render of one area takes the area's shape.
    let r = call(&mut s, 9, "tools/call", json!({"name": "render", "arguments": {"window": [[0, 0], [10, 5]]}}));
    let content = r["result"]["content"].as_array().expect("content array");
    assert_eq!(content.len(), 2, "render must return both ImageContent and metadata TextContent: {r}");
    let img_block = &content[0];
    assert_eq!(img_block["type"], "image", "{r}");
    assert_eq!(img_block["mimeType"], "image/png");
    let png = cadcraft_engine::cmd::file::base64_decode(img_block["data"].as_str().unwrap()).unwrap();
    let (w, h) = (u32::from_be_bytes(png[16..20].try_into().unwrap()), u32::from_be_bytes(png[20..24].try_into().unwrap()));
    assert_eq!((w, h), (1600, 800));

    let txt_block = &content[1];
    assert_eq!(txt_block["type"], "text");
    let txt = txt_block["text"].as_str().unwrap();
    assert!(txt.contains("MIME: image/png"), "{txt}");
    assert!(txt.contains("Dimensions: 1600x800"), "{txt}");
    assert!(txt.contains(&format!("Size: {} bytes", png.len())), "{txt}");
    assert!(txt.contains("SHA-256: "), "{txt}");
}

/// Guards a lazy Remote bridge: it serves tools without the app, reports how to start it
/// (`--control`), and reaches the app once that app starts listening on the port.
#[test]
fn a_lazy_bridge_starts_without_the_app_and_says_how_to_open_it() {
    // Nothing listens on a port that was just freed.
    let addr = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().to_string()
    };
    let mut s = Server::new(Box::new(crate::Remote::lazy(&addr, false)));
    let init = call(&mut s, 1, "initialize", json!({"protocolVersion": "2025-06-18"}));
    assert_eq!(init["result"]["serverInfo"]["name"], "cadcraft");
    let tools = call(&mut s, 2, "tools/list", json!({}));
    assert!(tools["result"]["tools"].as_array().unwrap().iter().any(|t| t["name"] == "screenshot"));
    let r = call(&mut s, 3, "tools/call", json!({"name": "command_line", "arguments": {"text": "line 0,0 1,1"}}));
    assert_eq!(r["result"]["isError"], json!(true));
    let msg = r["result"]["content"][0]["text"].as_str().unwrap();
    assert!(msg.contains("not answering") && msg.contains("--control"), "{msg}");

    // Once the app listens, the next call reaches it.
    let listener = std::net::TcpListener::bind(&addr).unwrap();
    let app = std::thread::spawn(move || {
        use std::io::{BufRead, Write};
        let (sock, _) = listener.accept().unwrap();
        let mut line = String::new();
        std::io::BufReader::new(sock.try_clone().unwrap()).read_line(&mut line).unwrap();
        let id = serde_json::from_str::<Value>(&line).unwrap()["id"].clone();
        writeln!(&sock, "{}", json!({"id": id, "ok": true, "result": {"prompt": "Command:"}})).unwrap();
    });
    let r = call(&mut s, 4, "tools/call", json!({"name": "command_line", "arguments": {"text": ""}}));
    assert_ne!(r["result"]["isError"], json!(true), "{r}");
    app.join().unwrap();
}

use std::sync::{Arc, Mutex};

struct SpyBackend {
    calls: Arc<Mutex<Vec<(String, Value)>>>,
    has_ui: bool,
}

impl SpyBackend {
    fn new(has_ui: bool) -> (Self, Arc<Mutex<Vec<(String, Value)>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        (Self { calls: Arc::clone(&calls), has_ui }, calls)
    }
}

impl crate::backend::Backend for SpyBackend {
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.calls.lock().unwrap().push((method.to_string(), params));
        Ok(json!({"ok": true}))
    }
    fn has_ui(&self) -> bool {
        self.has_ui
    }
    fn describe(&self) -> String {
        "spy".to_string()
    }
}

/// Catalog filtering in read-only mode with and without UI.
#[test]
fn read_only_catalog_filtering_with_and_without_ui() {
    // 1. Headless read-only
    let mut s_headless = Server::new(Box::new(Headless::default())).read_only(true);
    let init = call(&mut s_headless, 1, "initialize", json!({"protocolVersion": "2025-06-18"}));
    assert!(init["result"]["instructions"].as_str().unwrap().contains("read-only"));
    let tools_res = call(&mut s_headless, 2, "tools/list", json!({}));
    let tools: Vec<&str> = tools_res["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(tools, vec!["list_commands", "inspect_drawing", "query_entities", "render", "find_text"]);

    // 2. UI backend read-only
    let (spy_ui, _) = SpyBackend::new(true);
    let mut s_ui = Server::new(Box::new(spy_ui)).read_only(true);
    let tools_ui_res = call(&mut s_ui, 3, "tools/list", json!({}));
    let tools_ui: Vec<&str> = tools_ui_res["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(tools_ui, vec!["list_commands", "inspect_drawing", "query_entities", "render", "find_text", "screenshot", "ui_inspect"]);

    // Verify schemas omit `path` in read-only
    let render_tool = tools_res["result"]["tools"].as_array().unwrap().iter().find(|t| t["name"] == "render").unwrap();
    assert!(render_tool["inputSchema"]["properties"].get("path").is_none());
    let screenshot_tool = tools_ui_res["result"]["tools"].as_array().unwrap().iter().find(|t| t["name"] == "screenshot").unwrap();
    assert!(screenshot_tool["inputSchema"]["properties"].get("path").is_none());
}

/// Allowed queries are functional in read-only mode.
#[test]
fn read_only_allowed_queries_are_functional() {
    let mut s = Server::new(Box::new(Headless::default())).read_only(true);
    // inspect_drawing
    let inspect = tool(&mut s, "inspect_drawing", json!({}));
    assert_eq!(inspect["entityCount"], 0);

    // list_commands
    let cmds = tool(&mut s, "list_commands", json!({"filter": "circle"}));
    assert!(cmds.as_array().unwrap().iter().any(|c| c["id"] == "circle"));

    // query_entities
    let entities = tool(&mut s, "query_entities", json!({}));
    assert_eq!(entities["count"], 0);

    // find_text
    let texts = tool(&mut s, "find_text", json!({}));
    assert_eq!(texts["count"], 0);

    // render without path returns an image in memory (base64)
    let render = call(&mut s, 5, "tools/call", json!({"name": "render", "arguments": {"width": 320, "height": 200}}));
    assert_eq!(render["result"]["content"][0]["type"], "image");
    assert!(render["result"]["content"][0]["data"].as_str().unwrap().len() > 50);
}

/// Direct calls to forbidden mutating tools are rejected before reaching the backend.
#[test]
fn read_only_blocks_mutating_tools_without_backend_invocation() {
    let forbidden = [
        ("execute", json!({"command": "circle", "params": {"center": [0, 0], "radius": 5}})),
        ("command_line", json!({"text": "circle 0,0 5"})),
        ("script", json!({"text": "line 0,0 10,10\n"})),
        ("new_drawing", json!({})),
        ("open", json!({"path": "test.dxf"})),
        ("save", json!({"path": "test.dxf"})),
        ("cancel", json!({})),
        ("ui_click", json!({"x": 100, "y": 100})),
        ("arbitrary_tool", json!({})),
    ];

    let (spy, spy_calls) = SpyBackend::new(true);
    let mut s = Server::new(Box::new(spy)).read_only(true);

    for (i, (name, args)) in forbidden.into_iter().enumerate() {
        let r = call(&mut s, 100 + i as u64, "tools/call", json!({"name": name, "arguments": args}));
        assert_eq!(r["error"]["code"], -32602, "expected -32602 for tool `{name}`: {r}");
        let msg = r["error"]["message"].as_str().unwrap();
        assert!(
            msg.contains("disabled in read-only mode") || msg.contains("unknown tool"),
            "unexpected error message for `{name}`: {msg}"
        );
    }

    // Crucial security evidence: the backend received ZERO invocations.
    assert_eq!(
        spy_calls.lock().unwrap().len(),
        0,
        "backend must not be invoked for any denied tool"
    );
}

/// render and screenshot with path are rejected before reaching backend; smuggling parameters are blocked.
#[test]
fn read_only_blocks_file_paths_and_unknown_properties_before_backend() {
    let (spy, spy_calls) = SpyBackend::new(true);
    let mut s = Server::new(Box::new(spy)).read_only(true);

    // render with path
    let r1 = call(&mut s, 201, "tools/call", json!({"name": "render", "arguments": {"path": "/tmp/forbidden.png"}}));
    assert_eq!(r1["error"]["code"], -32602);
    assert!(r1["error"]["message"].as_str().unwrap().contains("path"));

    // screenshot with path
    let r2 = call(&mut s, 202, "tools/call", json!({"name": "screenshot", "arguments": {"path": "/tmp/forbidden.png"}}));
    assert_eq!(r2["error"]["code"], -32602);
    assert!(r2["error"]["message"].as_str().unwrap().contains("path"));

    // render with smuggled unknown property
    let r3 = call(&mut s, 203, "tools/call", json!({"name": "render", "arguments": {"dest": "/tmp/evil.png"}}));
    assert_eq!(r3["error"]["code"], -32602);
    assert!(r3["error"]["message"].as_str().unwrap().contains("unknown property `dest`"));

    // screenshot in headless read-only mode is rejected before backend
    let (spy_headless, spy_h_calls) = SpyBackend::new(false);
    let mut s_headless = Server::new(Box::new(spy_headless)).read_only(true);
    let r4 = call(&mut s_headless, 204, "tools/call", json!({"name": "screenshot", "arguments": {}}));
    assert_eq!(r4["error"]["code"], -32602);
    assert!(r4["error"]["message"].as_str().unwrap().contains("UI is present"));

    // Zero backend calls made
    assert_eq!(spy_calls.lock().unwrap().len(), 0);
    assert_eq!(spy_h_calls.lock().unwrap().len(), 0);
}

/// Resources (cadcraft://drawing and cadcraft://commands) work safely and cannot bypass read-only.
#[test]
fn read_only_resources_safety() {
    let mut s = Server::new(Box::new(Headless::default())).read_only(true);
    let res_list = call(&mut s, 301, "resources/list", json!({}));
    let uris: Vec<&str> = res_list["result"]["resources"].as_array().unwrap().iter().map(|r| r["uri"].as_str().unwrap()).collect();
    assert_eq!(uris, vec!["cadcraft://drawing", "cadcraft://commands"]);

    let r_drawing = call(&mut s, 302, "resources/read", json!({"uri": "cadcraft://drawing"}));
    assert!(r_drawing["result"]["contents"][0]["text"].as_str().unwrap().contains("entityCount"));

    let r_commands = call(&mut s, 303, "resources/read", json!({"uri": "cadcraft://commands"}));
    assert!(r_commands["result"]["contents"][0]["text"].as_str().unwrap().contains("LINE"));

    // Arbitrary / unknown resource URIs are rejected (MCP ResourceNotFound code -32002)
    let r_unknown = call(&mut s, 304, "resources/read", json!({"uri": "file:///etc/passwd"}));
    assert_eq!(r_unknown["error"]["code"], -32002);
}

/// Backward compatibility: when read_only is false, mutating tools and path parameters work normally.
#[test]
fn backwards_compatibility_without_read_only_is_preserved() {
    let mut s = Server::new(Box::new(Headless::default()));
    assert!(!s.is_read_only());

    // tool catalog contains mutating tools
    let tools = call(&mut s, 401, "tools/list", json!({}));
    let names: Vec<&str> = tools["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"command_line"));
    assert!(names.contains(&"execute"));
    assert!(names.contains(&"open"));
    assert!(names.contains(&"save"));

    // mutating tools execute normally
    let line_res = tool(&mut s, "command_line", json!({"text": "circle 0,0 10"}));
    assert_ne!(line_res["isError"], json!(true));

    let inspect = tool(&mut s, "inspect_drawing", json!({}));
    assert_eq!(inspect["entityCount"], 1);

    // render inputSchema allows `path`
    let render_tool = tools["result"]["tools"].as_array().unwrap().iter().find(|t| t["name"] == "render").unwrap();
    assert!(render_tool["inputSchema"]["properties"].get("path").is_some());
}

/// In read-only mode, all 7 tools declare readOnlyHint: true, destructiveHint: false, openWorldHint: false.
/// In full mode, tools that write to disk (render/screenshot with path) do NOT declare readOnlyHint: true.
#[test]
fn read_only_tools_include_annotations() {
    let defs_ro = crate::tools::tool_definitions(true, true);
    let tools_ro = defs_ro["tools"].as_array().unwrap();
    assert_eq!(tools_ro.len(), 7);
    for t in tools_ro {
        let name = t["name"].as_str().unwrap();
        let ann = &t["annotations"];
        assert_eq!(
            ann["readOnlyHint"],
            json!(true),
            "tool `{name}` must have readOnlyHint: true in read-only mode"
        );
        assert_eq!(
            ann["destructiveHint"],
            json!(false),
            "tool `{name}` must have destructiveHint: false in read-only mode"
        );
        assert_eq!(
            ann["openWorldHint"],
            json!(false),
            "tool `{name}` must have openWorldHint: false in read-only mode"
        );
    }

    // In full mode (read_only = false), render and screenshot allow path and must not declare readOnlyHint: true
    let defs_full = crate::tools::tool_definitions(true, false);
    let tools_full = defs_full["tools"].as_array().unwrap();
    let render_full = tools_full.iter().find(|t| t["name"] == "render").unwrap();
    assert_ne!(render_full["annotations"]["readOnlyHint"], json!(true));
    let shot_full = tools_full.iter().find(|t| t["name"] == "screenshot").unwrap();
    assert_ne!(shot_full["annotations"]["readOnlyHint"], json!(true));
}

/// Tests sha256_hex against standard FIPS 180-4 and boundary vectors.
#[test]
fn sha256_canonical_vectors() {
    use crate::tools::sha256_hex;

    // 1. Empty string
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );

    // 2. Standard NIST vector "abc"
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );

    // 3. Multi-block entry (448 bits / 56 bytes limit and multi-block input)
    assert_eq!(
        sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );

    // 4. Boundary vector: exactly 55 bytes (fits in single block with 0x80 and 8-byte length)
    let b55 = b"1234567890123456789012345678901234567890123456789012345";
    assert_eq!(b55.len(), 55);
    assert_eq!(
        sha256_hex(b55),
        "03c3a70e99ed5eeccd80f73771fcf1ece643d939d9ecc76f25544b0233f708e9"
    );

    // 5. Boundary vector: exactly 56 bytes (0x80 overflows single block, requires 2 blocks)
    let b56 = b"12345678901234567890123456789012345678901234567890123456";
    assert_eq!(b56.len(), 56);
    assert_eq!(
        sha256_hex(b56),
        "0be66ce72c2467e793202906000672306661791622e0ca9adf4a8955b2ed189c"
    );

    // 6. Boundary vector: exactly 63 bytes
    let b63 = [b'a'; 63];
    assert_eq!(
        sha256_hex(&b63),
        "7d3e74a05d7db15bce4ad9ec0658ea98e3f06eeecf16b4c6fff2da457ddc2f34"
    );

    // 7. Boundary vector: exactly 64 bytes (full single block, padding fills second block)
    let b64 = [b'a'; 64];
    assert_eq!(
        sha256_hex(&b64),
        "ffe054fe7ae0cb6dc65c3af9b61d5209f439851db43d0ba5997337df154668eb"
    );
}

/// Tests parse_png_dimensions on valid header, truncation, invalid signatures, non-IHDR, and bad sizes.
#[test]
fn parse_png_dimensions_failure_modes() {
    use crate::tools::parse_png_dimensions;

    // Helper: builds a minimal 33-byte PNG header buffer
    let make_header = |sig: [u8; 8], len: u32, chunk_type: &[u8; 4], w: u32, h: u32| {
        let mut b = Vec::new();
        b.extend_from_slice(&sig);
        b.extend_from_slice(&len.to_be_bytes());
        b.extend_from_slice(chunk_type);
        b.extend_from_slice(&w.to_be_bytes());
        b.extend_from_slice(&h.to_be_bytes());
        b.extend_from_slice(&[8, 6, 0, 0, 0]); // bit_depth, color_type, compression, filter, interlace
        b.extend_from_slice(&[0x11, 0x22, 0x33, 0x44]); // 4-byte CRC space
        b
    };

    let valid_sig = [137, 80, 78, 71, 13, 10, 26, 10];

    // 1. Valid PNG header
    let valid = make_header(valid_sig, 13, b"IHDR", 1024, 768);
    assert_eq!(parse_png_dimensions(&valid), Some((1024, 768)));

    // 2. Truncated data (<33 bytes)
    assert_eq!(parse_png_dimensions(&valid[..32]), None);
    assert_eq!(parse_png_dimensions(&valid[..20]), None);
    assert_eq!(parse_png_dimensions(&[]), None);

    // 3. Incorrect signature
    let mut bad_sig = valid.clone();
    bad_sig[0] = 0;
    assert_eq!(parse_png_dimensions(&bad_sig), None);

    // 4. First chunk not IHDR
    let not_ihdr = make_header(valid_sig, 13, b"PLTE", 100, 100);
    assert_eq!(parse_png_dimensions(&not_ihdr), None);

    // 5. Declared IHDR length not equal to 13
    let bad_len = make_header(valid_sig, 12, b"IHDR", 100, 100);
    assert_eq!(parse_png_dimensions(&bad_len), None);
    let bad_len2 = make_header(valid_sig, 14, b"IHDR", 100, 100);
    assert_eq!(parse_png_dimensions(&bad_len2), None);

    // 6. Width zero or height zero
    let zero_w = make_header(valid_sig, 13, b"IHDR", 0, 500);
    assert_eq!(parse_png_dimensions(&zero_w), None);
    let zero_h = make_header(valid_sig, 13, b"IHDR", 500, 0);
    assert_eq!(parse_png_dimensions(&zero_h), None);

    // 7. Dimension out of PNG range (> 2^31 - 1)
    let huge_w = make_header(valid_sig, 13, b"IHDR", 0x8000_0000, 100);
    assert_eq!(parse_png_dimensions(&huge_w), None);
}

/// Tests that render metadata handles invalid base64, unavailable dimensions, and backend errors.
#[test]
fn render_metadata_resilience() {
    struct MockBackend {
        reply: Result<Value, String>,
    }
    impl crate::backend::Backend for MockBackend {
        fn has_ui(&self) -> bool { true }
        fn describe(&self) -> String { "mock".to_string() }
        fn call(&mut self, _method: &str, _params: Value) -> Result<Value, String> {
            self.reply.clone()
        }
    }

    // 1. Backend error must remain an error and not report fake success
    let mut s_err = Server::new(Box::new(MockBackend { reply: Err("render device failed".into()) }));
    let r_err = call(&mut s_err, 1, "tools/call", json!({"name": "render", "arguments": {}}));
    assert_eq!(r_err["result"]["isError"], true);
    assert_eq!(r_err["result"]["content"][0]["text"], "render device failed");

    // 2. Invalid base64 payload -> reports metadata unavailable, keeps ImageContent intact
    let mut s_bad_b64 = Server::new(Box::new(MockBackend {
        reply: Ok(json!({"pngBase64": "!!!not_valid_base64!!!"})),
    }));
    let r_b64 = call(&mut s_bad_b64, 2, "tools/call", json!({"name": "render", "arguments": {}}));
    let content_b64 = r_b64["result"]["content"].as_array().unwrap();
    assert_eq!(content_b64.len(), 2);
    assert_eq!(content_b64[0]["type"], "image");
    assert_eq!(content_b64[0]["data"], "!!!not_valid_base64!!!");
    assert_eq!(content_b64[1]["type"], "text");
    assert!(content_b64[1]["text"].as_str().unwrap().contains("Metadata: unavailable"));

    // 3. Valid base64 but corrupt PNG header -> Dimensions: unavailable, exact size and precalculated SHA-256 reported
    let corrupt_payload = b"not a png at all, just some raw random bytes for testing!";
    let corrupt_b64 = cadcraft_engine::cmd::file::base64_encode(corrupt_payload);
    let mut s_corrupt = Server::new(Box::new(MockBackend {
        reply: Ok(json!({"pngBase64": corrupt_b64})),
    }));
    let r_corrupt = call(&mut s_corrupt, 3, "tools/call", json!({"name": "render", "arguments": {}}));
    let content_c = r_corrupt["result"]["content"].as_array().unwrap();
    assert_eq!(content_c.len(), 2);
    assert_eq!(content_c[0]["type"], "image");
    assert_eq!(content_c[0]["data"], corrupt_b64);
    let txt_c = content_c[1]["text"].as_str().unwrap();
    assert!(txt_c.contains("Dimensions: unavailable"), "{txt_c}");
    assert!(txt_c.contains("Size: 57 bytes"), "{txt_c}");
    assert_eq!(txt_c, "MIME: image/png\nDimensions: unavailable\nSize: 57 bytes\nSHA-256: 9c29075484b0a3ce8cd98404926ac67bd1f633334bb1d8183716115644681723");

    // 4. Known valid sample -> ImageContent intact, verified dimensions, exact precalculated SHA-256 and size
    let mut valid_png_sample = Vec::new();
    valid_png_sample.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]); // signature
    valid_png_sample.extend_from_slice(&13u32.to_be_bytes()); // length 13
    valid_png_sample.extend_from_slice(b"IHDR"); // type
    valid_png_sample.extend_from_slice(&800u32.to_be_bytes()); // width 800
    valid_png_sample.extend_from_slice(&600u32.to_be_bytes()); // height 600
    valid_png_sample.extend_from_slice(&[8, 6, 0, 0, 0]); // data tail
    valid_png_sample.extend_from_slice(&[0x11, 0x22, 0x33, 0x44]); // CRC space
    assert_eq!(valid_png_sample.len(), 33);
    let valid_b64 = cadcraft_engine::cmd::file::base64_encode(&valid_png_sample);
    let mut s_valid = Server::new(Box::new(MockBackend {
        reply: Ok(json!({"pngBase64": valid_b64})),
    }));
    let r_valid = call(&mut s_valid, 4, "tools/call", json!({"name": "render", "arguments": {}}));
    let content_v = r_valid["result"]["content"].as_array().unwrap();
    assert_eq!(content_v.len(), 2);
    // ImageContent intact
    assert_eq!(content_v[0]["type"], "image");
    assert_eq!(content_v[0]["mimeType"], "image/png");
    assert_eq!(content_v[0]["data"], valid_b64);
    // TextContent exact and verified
    assert_eq!(content_v[1]["type"], "text");
    assert_eq!(
        content_v[1]["text"],
        "MIME: image/png\nDimensions: 800x600\nSize: 33 bytes\nSHA-256: eb15527d06ff05e018598472c6cb9a2cc140bb1dfe1e155f8f0db040311537f8"
    );
}

