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




#[test]
fn cli_opens_dwt_template_and_draws_lines_at_z_zero() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Templates/EGI-ISO.dwt");
    let dir = std::env::temp_dir().join(format!("cc-cli-dwt-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out_dxf = dir.join("dwt_out.dxf");
    let output = Command::new(env!("CARGO_BIN_EXE_cadcraft-cli"))
        .args([
            "run",
            root.to_str().unwrap(),
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
    let _ = std::fs::remove_dir_all(&dir);
}
