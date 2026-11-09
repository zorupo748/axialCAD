//! `cadcraft-cli` against a stdout nobody reads any more (#537).

use std::io::Read;
use std::process::{Command, Output, Stdio};

/// `cadcraft-cli commands | head` panicked with "failed printing to stdout: Broken pipe (os error
/// 32)" and exit status 101: `commands` prints a few thousand lines, which no pipe buffer holds,
/// so the reader is gone before the last write.
#[test]
fn closed_stdout_ends_quietly() {
    for args in [&["commands"][..], &["commands", "ellipse"], &["--version"]] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_cadcraft-cli")).args(args).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        let mut first = [0u8; 10];
        child.stdout.as_mut().unwrap().read_exact(&mut first).unwrap();
        drop(child.stdout.take());
        let out = child.wait_with_output().unwrap();
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success() && err.is_empty(), "{args:?}: {:?}: {err}", out.status);
    }
}

/// The CLI keeps working once stdout is gone: `run` still executes its commands and still saves,
/// so `cadcraft-cli run … --save out.dxf | head` keeps the drawing.
fn run_with_closed_stdout(args: &[&str]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_cadcraft-cli")).args(args).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    drop(child.stdout.take());
    child.wait_with_output().unwrap()
}

#[test]
fn closed_stdout_still_runs_the_commands_and_saves() {
    let dir = std::env::temp_dir().join(format!("cc-cli-pipe-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let drawing = dir.join("piped.dxf");
    let out = run_with_closed_stdout(&[
        "run",
        "--metric",
        "--cmd",
        "ellipse {\"center\":[0,0],\"major\":[10,0],\"ratio\":0.5}",
        "--cmd",
        "drawing.inspect {}",
        "--save",
        drawing.to_str().unwrap(),
    ]);
    assert!(out.status.success(), "{:?}: {}", out.status, String::from_utf8_lossy(&out.stderr));
    let saved = std::fs::read_to_string(&drawing).unwrap();
    assert!(saved.contains("ELLIPSE"), "the drawing is saved all the same: {}", &saved[..saved.len().min(200)]);

    // A failing command is still a failure, whether or not anyone reads stdout.
    let out = run_with_closed_stdout(&["run", "--metric", "--cmd", "no.such.command {}"]);
    assert_eq!(out.status.code(), Some(1));
    let _ = std::fs::remove_dir_all(&dir);
}




#[test]
fn cli_export_dwg_rejects_unexportable_entities_and_preserves_file() {
    let dir = std::env::temp_dir().join(format!("cc-cli-dwg-safe-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src_dxf = dir.join("input.dxf");
    let target_dwg = dir.join("output.dwg");

    // Create a DXF containing an unexportable entity
    let dxf_content =
        "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\n3DSOLID\n5\n100\n8\n0\n0\nENDSEC\n0\nEOF\n";
    std::fs::write(&src_dxf, dxf_content).unwrap();

    // Pre-create the target DWG with existing data
    let original_bytes = b"precious pre-existing dwg bytes";
    std::fs::write(&target_dwg, original_bytes).unwrap();

    let output =
        Command::new(env!("CARGO_BIN_EXE_cadcraft-cli")).args(["convert", src_dxf.to_str().unwrap(), target_dwg.to_str().unwrap()]).output().unwrap();

    assert!(!output.status.success(), "CLI convert must fail on unexportable data");
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(err.contains("Cannot save to DWG without losing data"), "{err}");
    assert!(err.contains("3DSOLID"), "{err}");

    // Target file must be preserved intact!
    let current_bytes = std::fs::read(&target_dwg).unwrap();
    assert_eq!(current_bytes, original_bytes, "Pre-existing target file must not be overwritten or truncated");

    let _ = std::fs::remove_dir_all(&dir);
}





fn unique_cli_temp_dir(prefix: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create unique temp dir");
    dir
}

fn create_synthetic_cli_template(dir: &std::path::Path) -> (std::path::PathBuf, String, String) {
    use cadcraft_engine::Session;
    use serde_json::json;
    let mut s = Session::new();
    s.new_drawing(true);
    s.execute("layout.new", &json!({ "name": "Formato A4 vertical" })).unwrap();
    s.execute("layout.set", &json!({ "name": "Formato A4 vertical" })).unwrap();
    let res1 = s.execute("text", &json!({ "at": [10, 10], "height": 3.5, "text": "1:1" })).unwrap();
    let h1 = res1["handle"].as_str().unwrap().to_string();
    let res2 = s.execute("text", &json!({ "at": [10, 20], "height": 5.0, "text": "TITULO PREVIO" })).unwrap();
    let h2 = res2["handle"].as_str().unwrap().to_string();
    s.execute("layout.set", &json!({ "name": "Model" })).unwrap();

    let path = dir.join("synthetic_cli_template.dwt");
    let d = s.doc().unwrap();
    let bytes = cadcraft_io::write(d, "template.dxf").unwrap();
    std::fs::write(&path, bytes).unwrap();
    (path, h1, h2)
}

#[test]
fn cli_opens_dwt_template_and_draws_lines_at_z_zero() {
    let dir = unique_cli_temp_dir("cc-cli-dwt");
    let (template_path, _, _) = create_synthetic_cli_template(&dir);
    let out_dxf = dir.join("dwt_out.dxf");
    let output = Command::new(env!("CARGO_BIN_EXE_cadcraft-cli"))
        .args([
            "run",
            template_path.to_str().unwrap(),
            "--cmd",
            "view.swiso {}",
            "--cmd",
            "line {\"points\":[[10,20],[30,40],[50,60]]}",
            "--save",
            out_dxf.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}: {}", output.status, String::from_utf8_lossy(&output.stderr));
    let content = std::fs::read_to_string(&out_dxf).unwrap();
    assert!(content.contains("LINE"), "Saved DXF must contain the drawn LINE entities");

    // Explicitly verify Z coordinates of drawn lines:
    let dxf_bytes = std::fs::read(&out_dxf).unwrap();
    let doc = cadcraft_io::read(&dxf_bytes, "dwt_out.dxf").unwrap();
    let lines: Vec<_> = doc.model.iter().filter_map(|e| if let cadcraft_engine::doc::EntityKind::Line(l) = &e.kind { Some(l.clone()) } else { None }).collect();
    assert_eq!(lines.len(), 2, "Must contain exactly 2 line segments");
    for line in &lines {
        assert_eq!(line.a.z, 0.0, "Start point Z must be exactly 0.0");
        assert_eq!(line.b.z, 0.0, "End point Z must be exactly 0.0");
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cli_dwt_template_edits_cajetin_title_and_scale_and_preserves_on_reload() {
    let dir = unique_cli_temp_dir("cc-cli-dwt-edit");
    let (template_path, h_scale, h_title) = create_synthetic_cli_template(&dir);
    let out_dxf = dir.join("dwt_edited.dxf");
    let output = Command::new(env!("CARGO_BIN_EXE_cadcraft-cli"))
        .args([
            "run",
            template_path.to_str().unwrap(),
            "--cmd",
            "layout.set {\"name\":\"Formato A4 vertical\"}",
            "--cmd",
            &format!("textedit {{\"handle\":\"{}\",\"text\":\"1:5\"}}", h_scale),
            "--cmd",
            &format!("textedit {{\"handle\":\"{}\",\"text\":\"NUEVO TITULO AUDITORIA\"}}", h_title),
            "--save",
            out_dxf.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}: {}", output.status, String::from_utf8_lossy(&output.stderr));

    let inspect_output = Command::new(env!("CARGO_BIN_EXE_cadcraft-cli"))
        .args([
            "run",
            out_dxf.to_str().unwrap(),
            "--cmd",
            "layout.set {\"name\":\"Formato A4 vertical\"}",
            "--cmd",
            "drawing.inspect {\"entities\":true}",
        ])
        .output()
        .unwrap();
    assert!(inspect_output.status.success());
    let stdout = String::from_utf8_lossy(&inspect_output.stdout);
    assert!(stdout.contains("1:5"), "Saved DXF must contain edited scale 1:5");
    assert!(stdout.contains("NUEVO TITULO AUDITORIA"), "Saved DXF must contain edited title");
    let _ = std::fs::remove_dir_all(&dir);
}
