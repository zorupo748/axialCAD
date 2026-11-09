use cadcraft_engine::Session;
use serde_json::json;

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
fn test_autolisp_expressions_rejected() {
    let lisp_expressions = [
        "(+ 1 2)\n",
        "(defun c:mytest () (princ))\n",
        "!myvar\n",
    ];

    for expr in lisp_expressions {
        let scr = format!("circle 0,0 5\n{expr}circle 0,0 10\n");
        let path = temp_script("lisp_expr.scr", scr.as_bytes());
        let mut s = Session::new();
        let err = s.execute("script", &json!({ "path": path })).unwrap_err().to_string();
        assert!(err.contains("AutoLISP expressions are not allowed in scripts"), "Expected LISP rejection: {err}");
        assert_eq!(s.state().unwrap().doc.model.len(), 1);
        assert!(!s.is_script_running());
    }
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
