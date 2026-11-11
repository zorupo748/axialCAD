use cadcraft_engine::{Input, Session};
use serde_json::{json, Value};

fn temp_script(name: &str, content: &[u8]) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("cadcraft-test-{}-{nanos}-{n}-{name}", std::process::id()));
    std::fs::write(&path, content).unwrap();
    path.to_string_lossy().into_owned()
}

#[test]
fn test_valid_multicommand_script() {
    let scr = b"; Multi-command valid script\ncircle 0,0 10\nline 0,0 20,20 \n";
    let path = temp_script("multi.scr", scr);
    let mut s = Session::new();
    let r = s.execute("script", &json!({ "path": path })).unwrap();
    assert_eq!(r["lines"], 3);
    assert_eq!(s.state().unwrap().doc.model.len(), 2);
    assert!(!s.is_script_running());
}

#[test]
fn test_empty_script_and_crlf_line_endings() {
    // Empty script
    let path_empty = temp_script("empty.scr", b"");
    let mut s = Session::new();
    let r = s.execute("script", &json!({ "path": path_empty })).unwrap();
    assert_eq!(r["lines"], 0);
    assert_eq!(s.state().unwrap().doc.model.len(), 0);

    // CRLF script with trailing spaces and comments
    let scr_crlf = b"; CRLF test\r\ncircle 5,5 2\r\n; next command\r\nline 0,0 10,0 \r\n";
    let path_crlf = temp_script("crlf.scr", scr_crlf);
    let mut s = Session::new();
    let r = s.execute("script", &json!({ "path": path_crlf })).unwrap();
    assert_eq!(r["lines"], 4);
    assert_eq!(s.state().unwrap().doc.model.len(), 2);
    assert!(!s.is_script_running());
}

#[test]
fn test_utf8_with_and_without_bom() {
    // UTF-8 without BOM
    let plain = "circle 0,0 5\n".as_bytes();
    let path_plain = temp_script("nobom.scr", plain);
    let mut s1 = Session::new();
    s1.execute("script", &json!({ "path": path_plain })).unwrap();
    assert_eq!(s1.state().unwrap().doc.model.len(), 1);

    // UTF-8 with BOM (0xEF, 0xBB, 0xBF)
    let mut with_bom = vec![0xEF, 0xBB, 0xBF];
    with_bom.extend_from_slice(b"; with BOM\ncircle 0,0 8\n");
    let path_bom = temp_script("withbom.scr", &with_bom);
    let mut s2 = Session::new();
    s2.execute("script", &json!({ "path": path_bom })).unwrap();
    assert_eq!(s2.state().unwrap().doc.model.len(), 1);
}

#[test]
fn test_invalid_utf8_is_rejected() {
    // Invalid UTF-8 bytes (e.g. 0xFF, 0xFE, 0x80)
    let invalid = vec![0xFF, 0xFE, 0x63, 0x69, 0x72, 0x63, 0x6C, 0x65];
    let path_inv = temp_script("invalid_utf8.scr", &invalid);
    let mut s = Session::new();
    let err = s.execute("script", &json!({ "path": path_inv })).unwrap_err().to_string();
    assert!(err.contains("not valid UTF-8"), "Expected UTF-8 error, got: {err}");
    assert_eq!(s.state().unwrap().doc.model.len(), 0);
}

#[test]
fn test_script_path_with_spaces() {
    let dir = std::env::temp_dir().join(format!("cadcraft test spaces {}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("my script file.scr");
    std::fs::write(&path, b"circle 0,0 12\n").unwrap();
    let path_str = path.to_string_lossy().to_string();

    let mut s = Session::new();
    let r = s.execute("script", &json!({ "path": path_str })).unwrap();
    assert_eq!(r["lines"], 1);
    assert_eq!(s.state().unwrap().doc.model.len(), 1);

    // Also test typed SCRIPT with quotes around path with spaces
    let mut s2 = Session::new();
    s2.cmdline("script").unwrap();
    assert!(s2.running.is_some());
    s2.cmdline(&format!("\"{path_str}\"")).unwrap();
    assert!(s2.running.is_none());
    assert_eq!(s2.state().unwrap().doc.model.len(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_quoted_arguments_inside_script() {
    // A script with quoted argument containing spaces, ending with an empty line to commit TEXT
    let scr = b"circle 0,0 10\ntext 0,0 2.5 0 \"Label With Spaces\"\n\n";
    let path = temp_script("quoted_args.scr", scr);
    let mut s = Session::new();
    s.execute("script", &json!({ "path": path })).unwrap();
    assert_eq!(s.state().unwrap().doc.model.len(), 2);
}

#[test]
fn test_error_mid_sequence_halts_and_reports_line() {
    // Line 1: valid circle
    // Line 2: comment
    // Line 3: invalid command
    // Line 4: another circle that must NOT be executed
    let scr = b"circle 0,0 10\n; comment line\nunknown_bogus_cmd\ncircle 0,0 20\n";
    let path = temp_script("error_line.scr", scr);
    let mut s = Session::new();
    let err = s.execute("script", &json!({ "path": path })).unwrap_err().to_string();

    assert!(err.contains("line 3"), "Error should report physical line 3: {err}");
    assert!(err.contains("unknown_bogus_cmd"), "Error should report line content: {err}");
    assert!(err.contains("partial changes may have been applied"), "Error should note partial changes: {err}");

    // Line 1's circle remains in document (partial changes retained)
    assert_eq!(s.state().unwrap().doc.model.len(), 1);
    assert!(!s.is_script_running());
    assert!(s.running.is_none(), "Session must be left ready without active command");
}

#[test]
fn test_eof_with_pending_command() {
    // Line 1: line command with first point and second point, but no closing Enter/space
    let scr = b"line 0,0 10,10\n";
    let path = temp_script("eof_pending.scr", scr);
    let mut s = Session::new();
    let err = s.execute("script", &json!({ "path": path })).unwrap_err().to_string();

    assert!(err.contains("ended at EOF with command 'line' awaiting input"), "Expected EOF pending error: {err}");
    assert!(err.contains("line 1"), "Expected line 1: {err}");
    assert!(!s.is_script_running());
    assert!(s.running.is_none(), "Pending command must be cleanly cancelled");
}

#[test]
fn test_restricted_commands_rejected() {
    let forbidden = [
        "qsave",
        "save",
        "_save",
        ".saveas",
        "saveas",
        "export",
        "exportpdf",
        "publish",
        "wblock",
        "new",
        "_qnew",
        "open",
        "close",
        "closeall",
        "quit",
        "exit",
        "_exit",
        "script",
        "scr",
        "_scr",
        "appload",
        "ap",
    ];

    for cmd_name in forbidden {
        let scr = format!("circle 0,0 5\n{cmd_name}\ncircle 0,0 10\n");
        let path = temp_script(&format!("forbidden_{cmd_name}.scr"), scr.as_bytes());
        let mut s = Session::new();
        let err = s.execute("script", &json!({ "path": path })).unwrap_err().to_string();
        assert!(
            err.contains("restricted in scripts") || err.contains("already running"),
            "Command '{cmd_name}' should be rejected as restricted, got: {err}"
        );
        assert_eq!(s.state().unwrap().doc.model.len(), 1, "Only first circle should exist for {cmd_name}");
        assert!(!s.is_script_running());
    }
}

#[test]
fn test_autolisp_user_function_and_wait_execution() {
    // 1. Definition and call of user function in SCR
    // 2. The same wait function under a different name ("mi_espera", not "esp")
    // 3. Continuation persistence across slices and script order: circle line runs only after completion
    let scr = b"; AutoLISP user function and wait test\n\
(defun mi_espera (ms / t0) (setq t0 (getvar \"MILLISECS\")) (while (< (- (getvar \"MILLISECS\") t0) ms)))\n\
(mi_espera 30)\n\
circle 0,0 15\n";
    let path = temp_script("lisp_wait.scr", scr);
    let mut s = Session::new();
    let r = s.execute("script", &json!({ "path": path })).unwrap();
    assert_eq!(r["lines"], 4);
    assert_eq!(s.state().unwrap().doc.model.len(), 1, "Circle must be executed after mi_espera finishes");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_millisecs_numeric_and_readonly() {
    let mut s = Session::new();
    let r = s.execute("getvar", &json!({"name": "MILLISECS"})).unwrap();
    let val = r["value"].as_i64().expect("MILLISECS must be an integer");
    assert!(val >= 0, "MILLISECS must be non-negative");

    // Must be read-only
    let err = s.execute("setvar", &json!({"name": "MILLISECS", "value": 100})).unwrap_err().to_string();
    assert!(err.contains("read-only"), "MILLISECS must be rejected as read-only: {err}");
}

#[test]
fn test_lisp_script_security_restrictions() {
    let mut s = Session::new();

    // 1. Direct forbidden command from LISP in script
    let scr1 = b"; Forbidden command via LISP\n(command \"quit\")\ncircle 0,0 10\n";
    let path1 = temp_script("lisp_sec1.scr", scr1);
    let err1 = s.execute("script", &json!({ "path": path1 })).unwrap_err().to_string();
    assert!(err1.contains("restricted in scripts"), "Expected restriction: {err1}");
    assert_eq!(s.doc().unwrap().model.len(), 0);
    let _ = std::fs::remove_file(&path1);

    // 2. User function calling forbidden command
    let scr2 = b"; User function calling forbidden command\n(defun peligro () (command \"save\"))\n(peligro)\ncircle 0,0 10\n";
    let path2 = temp_script("lisp_sec2.scr", scr2);
    let err2 = s.execute("script", &json!({ "path": path2 })).unwrap_err().to_string();
    assert!(err2.contains("restricted in scripts"), "Expected restriction: {err2}");
    assert_eq!(s.doc().unwrap().model.len(), 0);
    let _ = std::fs::remove_file(&path2);

    // 3. File write attempt from script
    let scr3 = b"; File write from script\n(open \"/tmp/cadcraft_test_bad.txt\" \"w\")\ncircle 0,0 10\n";
    let path3 = temp_script("lisp_sec3.scr", scr3);
    let err3 = s.execute("script", &json!({ "path": path3 })).unwrap_err().to_string();
    assert!(err3.contains("file writing is restricted in scripts"), "Expected file restriction: {err3}");
    assert_eq!(s.doc().unwrap().model.len(), 0);
    let _ = std::fs::remove_file(&path3);
}

#[test]
fn test_script_escape_cancels_infinite_lisp_loop() {
    let mut s = Session::new();
    let scr = b"(while t)\ncircle 0,0 20\n";
    s.start_script_file("infinite_loop.scr", std::str::from_utf8(scr).unwrap()).unwrap();

    // Step once to launch the while loop
    let step = s.step_script();
    assert!(matches!(step, cadcraft_engine::ScriptStep::Progress { .. }));
    assert!(s.lisp_running(), "LISP must be running the while loop");

    // User presses Escape
    let cancel = s.cancel_script().expect("Cancel script must return Cancelled step");
    assert!(matches!(cancel, cadcraft_engine::ScriptStep::Cancelled { .. }));
    assert!(!s.lisp_running(), "LISP must be aborted");
    assert!(!s.is_script_running(), "Script must be stopped");
    assert_eq!(s.doc().unwrap().model.len(), 0, "Subsequent circle must not be created");
}

#[test]
fn test_parentheses_in_text_command_treated_as_literal_text() {
    let mut s = Session::new();
    let scr = b"; Text with parentheses\n_TEXT\n0,0\n1.5\n0\n(synthetic text with parentheses)\n\n";
    let path = temp_script("text_parens.scr", scr);
    let r = s.execute("script", &json!({ "path": path })).unwrap();
    assert_eq!(r["lines"], 7);
    let doc = s.doc().unwrap();
    assert_eq!(doc.model.len(), 1);
    let ent = doc.model.iter().next().unwrap();
    if let cadcraft_doc::EntityKind::Text(t) = &ent.kind {
        assert_eq!(t.value, "(synthetic text with parentheses)");
    } else {
        panic!("Expected text entity");
    }
    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_lisp_runtime_error_reports_script_line_and_context() {
    let mut s = Session::new();
    let scr = b"; LISP division by zero on line 2\n(/ 10 0)\ncircle 0,0 10\n";
    let path = temp_script("lisp_div0.scr", scr);
    let err = s.execute("script", &json!({ "path": path })).unwrap_err().to_string();
    assert!(err.contains("line 2"), "Error must report script line 2: {err}");
    assert!(err.contains("divide by zero") || err.contains("division by zero") || err.contains("error"), "Expected div0: {err}");
    assert_eq!(s.doc().unwrap().model.len(), 0, "Subsequent circle must not be created");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_script_clayer_on_following_line() {
    let mut s = Session::new();
    s.execute("layer.new", &json!({ "name": "SUELO" })).unwrap();
    let scr = b"; CLAYER test\nCLAYER\nSUELO\ncircle 0,0 5\n";
    let path = temp_script("clayer.scr", scr);
    let r = s.execute("script", &json!({ "path": path })).unwrap();
    assert_eq!(r["lines"], 4);
    assert_eq!(s.doc().unwrap().header.str("CLAYER", ""), "SUELO");
    let c = s.doc().unwrap().model.iter().last().unwrap();
    assert_eq!(c.common.layer, "SUELO");
}

#[test]
fn test_script_clayer_nonexistent_fails_and_halts() {
    let scr = b"; non existent layer\nCLAYER\nNONEXISTENT\ncircle 0,0 5\n";
    let path = temp_script("clayer_err.scr", scr);
    let mut s = Session::new();
    let err = s.execute("script", &json!({ "path": path })).unwrap_err().to_string();
    assert!(err.contains("cannot find layer `NONEXISTENT`") || err.contains("CLAYER"), "got: {err}");
    // circle on line 4 was NOT executed
    assert_eq!(s.state().unwrap().doc.model.len(), 0);
}

#[test]
fn test_synthetic_multicommand_workflow_script() {
    let scr = b"; Synthetic workflow script\n_LAYER\n_M\nL_WORKFLOW\n_C\n1\n\n\ncircle 0,0 15\nline 0,0 30,30 \n";
    let path = temp_script("workflow.scr", scr);
    let mut s = Session::new();
    let r = s.execute("script", &json!({ "path": path })).unwrap();
    assert!(r.get("lines").is_some(), "Script must report line execution count");
    let doc = s.doc().unwrap();
    assert!(doc.layer("L_WORKFLOW").is_some(), "Layer L_WORKFLOW created by script");
    assert!(doc.model.len() >= 2, "Entities created by workflow script");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_script_dynmode_consumes_value_and_proceeds() {
    let scr = b"; DYNMODE test\n_OSMODE\n0\n_DYNMODE\n0\ncircle 0,0 5\n";
    let path = temp_script("dynmode_off.scr", scr);
    let mut s = Session::new();
    s.settings.dynmode = true;
    let r = s.execute("script", &json!({ "path": path })).unwrap();
    assert_eq!(r["lines"], 6);
    assert_eq!(s.settings.dynmode, false, "DYNMODE 0 must turn dynmode off");
    assert_eq!(s.doc().unwrap().model.len(), 1, "Circle following DYNMODE must be executed");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_script_dynmode_on_and_transparent_toggle() {
    let mut s = Session::new();
    s.settings.dynmode = false;
    let scr = b"; DYNMODE 3\n_DYNMODE\n3\n";
    let path = temp_script("dynmode_on.scr", scr);
    s.execute("script", &json!({ "path": path })).unwrap();
    assert_eq!(s.settings.dynmode, true, "DYNMODE 3 must turn dynmode on");
    let _ = std::fs::remove_file(&path);

    // Verify F12 transparent toggle with empty params
    s.execute("dynmode", &json!({})).unwrap();
    assert_eq!(s.settings.dynmode, false, "Empty params toggles dynmode");
    s.execute("dynmode", &json!({})).unwrap();
    assert_eq!(s.settings.dynmode, true, "Empty params toggles dynmode back");
    s.execute("dynmode", &Value::Null).unwrap();
    assert_eq!(s.settings.dynmode, false, "Null params toggles dynmode");
}

#[test]
fn test_dynmode_strict_validation_and_no_unwanted_state_change() {
    let mut s = Session::new();
    s.settings.dynmode = true;

    // Invalid parameters via programmatic execute: must return Err and NOT toggle state
    assert!(s.execute("dynmode", &json!({"bogus": "value"})).is_err());
    assert_eq!(s.settings.dynmode, true, "State must remain true after bad object");

    assert!(s.execute("dynmode", &json!("invalid_string")).is_err());
    assert_eq!(s.settings.dynmode, true, "State must remain true after bad string");

    // Out-of-range value (> 3 or < -3): must return Err and NOT change state
    assert!(s.execute("dynmode", &json!({"value": 4})).is_err());
    assert_eq!(s.settings.dynmode, true, "State must remain true after out-of-range 4");

    assert!(s.execute("dynmode", &json!({"value": -4})).is_err());
    assert_eq!(s.settings.dynmode, true, "State must remain true after out-of-range -4");

    assert!(s.execute("dynmode", &json!(4)).is_err());
    assert_eq!(s.settings.dynmode, true);

    // Interactive console: Enter accepts default without state change
    s.start("dynmode").unwrap();
    assert!(s.running.is_some());
    s.input(Input::Enter).unwrap();
    assert!(s.running.is_none());
    assert_eq!(s.settings.dynmode, true, "Enter must preserve true");

    // Interactive console: Cancel cancels without state change
    s.start("dynmode").unwrap();
    assert!(s.running.is_some());
    s.input(Input::Cancel).unwrap();
    assert!(s.running.is_none());
    assert_eq!(s.settings.dynmode, true, "Cancel must preserve true");

    // Interactive console: Out-of-range interactive input reprompts with error message without state change
    s.start("dynmode").unwrap();
    s.input(Input::Text("5".into())).unwrap();
    assert_eq!(s.settings.dynmode, true, "Out-of-range interactive text must not change state");
    assert!(s.running.is_some(), "Interactive console stays active and reprompts on invalid input");
    assert!(s.log.last().is_some_and(|msg| msg.contains("between -3 and 3")));
    s.input(Input::Cancel).unwrap();

    // Interactive console: invalid string reprompts with error message without state change
    s.start("dynmode").unwrap();
    s.input(Input::Text("not_a_number".into())).unwrap();
    assert_eq!(s.settings.dynmode, true, "Invalid text must not change state");
    assert!(s.running.is_some(), "Interactive console stays active and reprompts on invalid input");
    assert!(s.log.last().is_some_and(|msg| msg.contains("between -3 and 3")));
    s.input(Input::Cancel).unwrap();

    // Interactive console: valid ON/OFF
    s.start("dynmode").unwrap();
    s.input(Input::Keyword("OFF".into())).unwrap();
    assert_eq!(s.settings.dynmode, false, "OFF must set false");

    s.start("dynmode").unwrap();
    s.input(Input::Keyword("ON".into())).unwrap();
    assert_eq!(s.settings.dynmode, true, "ON must set true");

    // SETVAR / GETVAR coherence
    s.execute("setvar", &json!({"name": "DYNMODE", "value": 0})).unwrap();
    assert_eq!(s.settings.dynmode, false);
    let r = s.execute("getvar", &json!({"name": "DYNMODE"})).unwrap();
    assert_eq!(r["value"], 0);

    s.execute("setvar", &json!({"name": "DYNMODE", "value": 3})).unwrap();
    assert_eq!(s.settings.dynmode, true);
    let r = s.execute("getvar", &json!({"name": "DYNMODE"})).unwrap();
    assert_eq!(r["value"], 1);

    // SETVAR out of range: must fail and leave dynmode true
    assert!(s.execute("setvar", &json!({"name": "DYNMODE", "value": 4})).is_err());
    assert_eq!(s.settings.dynmode, true, "SETVAR with 4 must fail and keep dynmode true");

    assert!(s.execute("setvar", &json!({"name": "DYNMODE", "value": -4})).is_err());
    assert_eq!(s.settings.dynmode, true, "SETVAR with -4 must fail and keep dynmode true");

    // Script execution with invalid DYNMODE halts on that line without executing following commands
    let scr = b"; Bad dynmode script\n_DYNMODE\n4\ncircle 0,0 5\n";
    let path = temp_script("dynmode_err.scr", scr);
    let err = s.execute("script", &json!({ "path": path })).unwrap_err().to_string();
    assert!(err.contains("between -3 and 3"), "Script error must report range: {err}");
    assert_eq!(s.doc().unwrap().model.len(), 0, "Circle following bad DYNMODE must not be created");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_text_literal_command_names_not_inferred_as_commands() {
    let mut s = Session::new();
    // Script enters literal command names at TEXT prompt: CIRCLE, LINE, ERASE, _UCS
    let scr = b"; Literal command names as text
_TEXT
0,0
1.0
0
CIRCLE
_TEXT
0,2
1.0
0
LINE
_TEXT
0,4
1.0
0
ERASE
_TEXT
0,6
1.0
0
_UCS
";
    let path = temp_script("text_literal_cmds.scr", scr);
    let r = s.execute("script", &json!({ "path": path })).unwrap();
    assert_eq!(r["lines"], 21);
    let doc = s.doc().unwrap();
    assert_eq!(doc.model.len(), 4, "Must have created exactly 4 text entities");

    let values: Vec<String> = doc.model.iter().filter_map(|e| {
        if let cadcraft_doc::EntityKind::Text(t) = &e.kind {
            Some(t.value.clone())
        } else {
            None
        }
    }).collect();

    assert_eq!(values, vec!["CIRCLE", "LINE", "ERASE", "_UCS"]);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_text_style_fixed_vs_variable_height() {
    let mut s = Session::new();
    // 1. Variable height style (Standard default: height = 0.0)
    // Sequence: point, height, rotation, text
    let scr_var = b"; Variable height text
_TEXT
0,0
2.75
0
VarHeightText
";
    let path_var = temp_script("text_var_ht.scr", scr_var);
    s.execute("script", &json!({ "path": path_var })).unwrap();
    assert_eq!(s.doc().unwrap().model.len(), 1);
    if let cadcraft_doc::EntityKind::Text(t) = &s.doc().unwrap().model.iter().next().unwrap().kind {
        assert_eq!(t.value, "VarHeightText");
        assert!((t.height - 2.75).abs() < 1e-4);
    } else {
        panic!("Expected text entity");
    }
    let _ = std::fs::remove_file(&path_var);

    // 2. Fixed height style: define style with fixed height 4.2
    s.execute("style", &json!({ "name": "FIXED_HT", "font": "txt", "height": 4.2 })).unwrap();
    s.execute("setvar", &json!({ "name": "TEXTSTYLE", "value": "FIXED_HT" })).unwrap();

    // Sequence for fixed height: point, rotation, text (NO height prompt!)
    let scr_fixed = b"; Fixed height text (skips height prompt)
_TEXT
10,10
0
FixedHeightText
";
    let path_fixed = temp_script("text_fixed_ht.scr", scr_fixed);
    s.execute("script", &json!({ "path": path_fixed })).unwrap();
    assert_eq!(s.doc().unwrap().model.len(), 2);
    let last = s.doc().unwrap().model.iter().last().unwrap();
    if let cadcraft_doc::EntityKind::Text(t) = &last.kind {
        assert_eq!(t.value, "FixedHeightText");
        assert!((t.height - 4.2).abs() < 1e-4, "Height must match style's fixed height");
    } else {
        panic!("Expected text entity");
    }
    let _ = std::fs::remove_file(&path_fixed);
}

#[test]
fn test_empty_line_semantics_cases_a_b_c_d_e() {
    let mut s = Session::new();

    // Case (a): Empty line consumed by active command
    let scr_a = b"; Case a: empty line ends active line command
line 0,0 10,10

";
    let path_a = temp_script("empty_a.scr", scr_a);
    s.execute("script", &json!({ "path": path_a })).unwrap();
    assert_eq!(s.doc().unwrap().model.len(), 1);
    assert!(s.running.is_none());
    let _ = std::fs::remove_file(&path_a);

    // Case (e) & (b): Script repeating a command via blank line at prompt level
    // Line 1: LINE 0,0 10,0
    // Line 2: empty line (ends first LINE)
    // Line 3: empty line at prompt level -> repeats LINE
    // Line 4: 20,0 30,0 (drawn by repeated LINE)
    // Line 5: empty line (ends repeated LINE)
    let scr_e = b"; Case e: repeating command via empty line
line 0,0 10,0


20,0 30,0

";
    let path_e = temp_script("empty_e.scr", scr_e);
    s.execute("script", &json!({ "path": path_e })).unwrap();
    assert_eq!(s.doc().unwrap().model.len(), 3, "Original line + 2 new lines = 3");
    assert!(s.running.is_none());
    let _ = std::fs::remove_file(&path_e);

    // Case (c): Trailing empty lines at EOF (must not cause EofPending error)
    let scr_c = b"; Case c: trailing empty lines at EOF
circle 5,5 2



";
    let path_c = temp_script("empty_c.scr", scr_c);
    s.execute("script", &json!({ "path": path_c })).unwrap();
    assert_eq!(s.doc().unwrap().model.len(), 4, "3 lines + 1 circle = 4");
    assert!(s.running.is_none());
    let _ = std::fs::remove_file(&path_c);

    // Case (d): Multiple consecutive empty lines between commands
    // Line 1: line 10,10 20,20
    // Line 2: empty line (ends first line)
    // Line 3: empty line at prompt level (repeats line)
    // Line 4: empty line (ends repeated line without adding entities)
    // Line 5: circle 25,25 4
    let scr_d = b"; Case d: multiple empty lines between commands
line 10,10 20,20



circle 25,25 4

";
    let path_d = temp_script("empty_d.scr", scr_d);
    s.execute("script", &json!({ "path": path_d })).unwrap();
    assert_eq!(s.doc().unwrap().model.len(), 6, "4 previous + 2 new circles = 6");
    assert!(s.running.is_none());
    let _ = std::fs::remove_file(&path_d);
}

#[test]
fn test_lisp_custom_wait_function_renamed() {
    let mut s = Session::new();
    // Test wait function with completely different name: 'mi_espera'
    let scr = b"(defun mi_espera (ms / t0) (setq t0 (getvar \"MILLISECS\")) (while (< (- (getvar \"MILLISECS\") t0) ms)))
(progn (redraw) (mi_espera 50))
circle 0,0 10
";
    let path = temp_script("lisp_renamed_wait.scr", scr);
    let r = s.execute("script", &json!({ "path": path })).unwrap();
    assert_eq!(r["lines"], 3);
    assert_eq!(s.doc().unwrap().model.len(), 1, "Circle drawn after custom wait function");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_script_3dpoly_preserves_xyz_and_closes_cleanly() {
    use cadcraft_engine::doc::EntityKind;
    use cadcraft_geom::Vec3;

    // Case 1: SCR execution with _3DPOLY, non-zero variable Z, _C close, and immediate next command
    let scr = b"; Synthetic 3DPOLY XYZ + Close + subsequent command
_-LAYER
_M
SYNTH_LAYER
_C
3
SYNTH_LAYER

_3DPOLY
10.0,20.0,5.5
30.0,20.0,15.25
30.0,40.0,25.75
10.0,40.0,16.0
_C
_CLAYER
0
_LINE
0,0,0
0,0,12.0

";
    let path = temp_script("poly3d_z.scr", scr);
    let mut s = Session::new();
    let r = s.execute("script", &json!({ "path": path })).unwrap();
    assert_eq!(r["lines"], 20);
    assert!(s.running.is_none(), "No command should remain active after script completes");
    let _ = std::fs::remove_file(&path);

    let doc = s.doc().unwrap();
    assert_eq!(doc.model.len(), 2, "Expected 1 Polyline3d + 1 Line");

    let poly_ent = doc
        .model
        .iter()
        .find(|e| matches!(e.kind, EntityKind::Polyline3d(_)))
        .expect("Polyline3d must exist");
    assert_eq!(poly_ent.common.layer, "SYNTH_LAYER");
    let EntityKind::Polyline3d(pl) = &poly_ent.kind else { unreachable!() };
    assert!(pl.closed, "Polyline3d must be closed by _C");
    assert_eq!(
        pl.points,
        vec![
            Vec3::new(10.0, 20.0, 5.5),
            Vec3::new(30.0, 20.0, 15.25),
            Vec3::new(30.0, 40.0, 25.75),
            Vec3::new(10.0, 40.0, 16.0),
        ]
    );

    let line_ent = doc
        .model
        .iter()
        .find(|e| matches!(e.kind, EntityKind::Line(_)))
        .expect("Subsequent Line must exist");
    assert_eq!(line_ent.common.layer, "0", "_CLAYER 0 after _C must be consumed properly");
    let EntityKind::Line(ln) = &line_ent.kind else { unreachable!() };
    assert_eq!(ln.a, Vec3::new(0.0, 0.0, 0.0));
    assert_eq!(ln.b, Vec3::new(0.0, 0.0, 12.0));

    // Case 2: Interactive cmdline sequence (with Undo)
    let mut s2 = Session::new();
    s2.cmdline("_3DPOLY").unwrap();
    s2.cmdline("1,2,3").unwrap();
    s2.cmdline("4,5,9").unwrap();
    s2.cmdline("99,99,99").unwrap();
    s2.cmdline("_U").unwrap();
    s2.cmdline("7,8,-4.5").unwrap();
    s2.cmdline("_C").unwrap();
    assert!(s2.running.is_none());
    let doc2 = s2.doc().unwrap();
    let EntityKind::Polyline3d(pl2) = &doc2.model.iter().next().unwrap().kind else {
        panic!("Expected Polyline3d");
    };
    assert!(pl2.closed);
    assert_eq!(
        pl2.points,
        vec![
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::new(4.0, 5.0, 9.0),
            Vec3::new(7.0, 8.0, -4.5),
        ]
    );
}
