use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use tempfile::tempdir;

fn bin_path() -> PathBuf {
    std::env::var("CARGO_BIN_EXE_kimi-agent")
        .map(PathBuf::from)
        .expect("CARGO_BIN_EXE_kimi-agent should be set by cargo test")
}

fn run_cli(args: &[&str]) -> std::process::Output {
    Command::new(bin_path())
        .args(args)
        .output()
        .expect("run kimi-agent")
}

fn write_metadata(share_dir: &Path, work_dir: &Path, session_id: &str) {
    let canonical_work_dir = work_dir
        .canonicalize()
        .expect("canonicalize work dir")
        .to_string_lossy()
        .to_string();
    let payload = serde_json::json!({
        "work_dirs": [
            {
                "path": canonical_work_dir,
                "kaos": "local",
                "last_session_id": session_id,
            }
        ]
    });
    fs::create_dir_all(share_dir).expect("create share dir");
    fs::write(
        share_dir.join("kimi.json"),
        serde_json::to_vec_pretty(&payload).expect("serialize metadata"),
    )
    .expect("write metadata");
}

fn make_session_dir(share_dir: &Path, work_dir: &Path, session_id: &str) {
    let canonical_work_dir = work_dir
        .canonicalize()
        .expect("canonicalize work dir")
        .to_string_lossy()
        .to_string();
    let work_hash = format!("{:x}", md5::compute(canonical_work_dir.as_bytes()));
    let session_dir = share_dir
        .join("sessions")
        .join(work_hash)
        .join(session_id);
    fs::create_dir_all(&session_dir).expect("create session dir");
    fs::write(session_dir.join("context.jsonl"), "").expect("write context file");
    fs::write(session_dir.join("state.json"), "{}").expect("write state file");
}

#[test]
fn io_001_version_writes_stdout_only() {
    let output = run_cli(&["--version"]);
    assert!(output.status.success(), "status: {}", output.status);

    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    let stderr = String::from_utf8(output.stderr).expect("stderr utf8");

    assert!(stdout.starts_with("kimi-agent, version "));
    assert!(stdout.ends_with('\n'));
    assert!(stderr.is_empty(), "stderr should be empty, got: {stderr:?}");
}

#[test]
fn io_002_info_json_writes_valid_json_to_stdout_only() {
    let output = run_cli(&["info", "--json"]);
    assert!(output.status.success(), "status: {}", output.status);

    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    let stderr = String::from_utf8(output.stderr).expect("stderr utf8");

    let payload: Value = serde_json::from_str(stdout.trim()).expect("valid json");
    assert!(payload.get("kimi_cli_version").is_some());
    assert!(payload.get("wire_protocol_version").is_some());
    assert!(payload.get("server_name").is_some());
    assert!(payload.get("server_version").is_some());
    assert!(payload.get("agent_spec_versions").is_some());
    assert!(payload.get("python_version").is_some());
    assert!(stderr.is_empty(), "stderr should be empty, got: {stderr:?}");
}

#[test]
fn io_003_export_prints_output_path_on_stdout() {
    let temp = tempdir().expect("tempdir");
    let share_dir = temp.path().join("share");
    let work_dir = temp.path().join("work");
    fs::create_dir_all(&work_dir).expect("create work dir");

    let session_id = "session-io-003";
    make_session_dir(&share_dir, &work_dir, session_id);
    write_metadata(&share_dir, &work_dir, session_id);

    let output_zip = temp.path().join("out.zip");
    let output = Command::new(bin_path())
        .current_dir(&work_dir)
        .env("KIMI_SHARE_DIR", &share_dir)
        .args([
            "export",
            "--output",
            output_zip.to_str().expect("output zip utf8"),
            session_id,
        ])
        .output()
        .expect("run export command");

    assert!(output.status.success(), "status: {}", output.status);

    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    let stderr = String::from_utf8(output.stderr).expect("stderr utf8");

    assert_eq!(stdout.trim(), output_zip.to_str().expect("output zip utf8"));
    assert!(stderr.is_empty(), "stderr should be empty, got: {stderr:?}");
    assert!(output_zip.exists(), "zip output should exist");
}
