//! Runs the compiled binary, including a real server on a real socket.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::time::Duration;

use atlas_model::{HealthResponse, HealthStatus, OverviewResponse, RepositoryEntry};
use serde_json::Value;

fn codeatlas() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_codeatlas"));
    command
        .env_remove("CODEATLAS_DATA_DIR")
        .env_remove("RUST_LOG");
    command
}

fn run(args: &[&str]) -> Output {
    codeatlas().args(args).output().unwrap()
}

/// Kills the server even when an assertion fails.
struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn start_server(data_dir: &std::path::Path) -> (Server, String) {
    let mut child = codeatlas()
        .args(["serve", "--port", "0", "--data-dir"])
        .arg(data_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let started: Value = serde_json::from_str(&line).unwrap();
    let address = started["url"]
        .as_str()
        .unwrap()
        .trim_start_matches("http://")
        .to_owned();
    (Server(child), address)
}

fn get(address: &str, path: &str, host: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let (head, body) = response.split_once("\r\n\r\n").unwrap();
    let status = head.split(' ').nth(1).unwrap().parse().unwrap();
    (status, body.to_owned())
}

#[test]
fn help_documents_the_whole_interface_and_its_safety_constraints() {
    let output = run(&["--help"]);
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    for command in [
        "analyze", "overview", "impact", "compare", "backtest", "report", "serve", "demo",
    ] {
        assert!(help.contains(command), "missing {command}");
    }
    assert!(help.contains("never modifies"));
}

#[test]
fn version_matches_the_package() {
    let output = run(&["--version"]);
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        format!("codeatlas {}", atlas_model::APP_VERSION)
    );
}

#[test]
fn unbuilt_commands_fail_and_name_their_phase() {
    let cases: [(&[&str], &str); 5] = [
        (&["impact", "/nonexistent", "--file", "a.ts"], "Phase 3"),
        (
            &["compare", "/nonexistent", "--base", "a", "--target", "b"],
            "Phase 4",
        ),
        (&["backtest", "/nonexistent"], "Phase 5"),
        (&["report", "/nonexistent"], "Phase 6"),
        (&["demo"], "Phase 6"),
    ];
    for (args, phase) in cases {
        let output = run(args);
        assert_eq!(output.status.code(), Some(1), "{args:?}");
        assert!(output.stdout.is_empty(), "{args:?}");
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.contains("not implemented"), "{stderr}");
        assert!(stderr.contains(phase), "{stderr}");
    }
}

#[test]
fn invalid_usage_is_a_usage_error() {
    assert_eq!(run(&["no-such-command"]).status.code(), Some(2));
    assert_eq!(run(&["impact", "/repo"]).status.code(), Some(2));
}

#[test]
fn serve_refuses_non_loopback_hosts() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let output = codeatlas()
        .args(["serve", "--host", "0.0.0.0", "--data-dir"])
        .arg(&data_dir)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("loopback")
    );
    assert!(!data_dir.exists());
}

#[test]
fn config_errors_are_reported() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[server]\ntypo = 1\n").unwrap();
    let output = codeatlas()
        .arg("--config")
        .arg(&path)
        .args(["serve", "--port", "0"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8(output.stderr).unwrap().contains("typo"));
}

#[test]
fn health_endpoint_works_over_a_real_socket() {
    let dir = tempfile::tempdir().unwrap();
    let (_server, address) = start_server(dir.path());

    let (status, body) = get(&address, "/api/v1/health", &address);
    assert_eq!(status, 200, "{body}");
    let health: HealthResponse = serde_json::from_str(&body).unwrap();
    assert_eq!(health.status, HealthStatus::Ok);
    assert_eq!(
        health.database.schema_version,
        Some(atlas_storage::latest_version())
    );

    assert!(dir.path().join("codeatlas.sqlite").exists());
}

#[test]
fn real_server_rejects_rebinding_hosts() {
    let dir = tempfile::tempdir().unwrap();
    let (_server, address) = start_server(dir.path());
    let (status, body) = get(&address, "/api/v1/health", "evil.example");
    assert_eq!(status, 403, "{body}");
    assert!(body.contains("forbidden_host"), "{body}");
}

/// The server migrates on startup, so a second start exercises the upgrade path.
#[test]
fn restarting_on_an_existing_database_works() {
    let dir = tempfile::tempdir().unwrap();
    {
        let (_server, address) = start_server(dir.path());
        assert_eq!(get(&address, "/api/v1/health", &address).0, 200);
    }
    let (_server, address) = start_server(dir.path());
    assert_eq!(get(&address, "/api/v1/health", &address).0, 200);
}

fn analyze_json(repo: &Path, data_dir: &Path) -> Output {
    codeatlas()
        .arg("analyze")
        .arg(repo)
        .args(["--format", "json", "--data-dir"])
        .arg(data_dir)
        .output()
        .unwrap()
}

#[test]
fn analyze_then_overview_reports_real_history_and_reruns_identically() {
    let (repo, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    atlas_fixtures::generate_main_fixture(repo.path()).unwrap();

    let first = analyze_json(repo.path(), data.path());
    assert!(first.status.success(), "{first:?}");
    let parsed: OverviewResponse = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(parsed.history.commit_count, 13);
    assert_eq!(parsed.top_churn[0].path.as_str(), "src/billing/invoice.ts");

    let second = analyze_json(repo.path(), data.path());
    assert!(String::from_utf8_lossy(&second.stderr).contains("reused"));
    assert_eq!(first.stdout, second.stdout);

    let overview = codeatlas()
        .arg("overview")
        .arg(repo.path())
        .args(["--format", "json", "--data-dir"])
        .arg(data.path())
        .output()
        .unwrap();
    assert_eq!(overview.stdout, first.stdout);
}

#[test]
fn text_output_neutralizes_terminal_escapes_in_paths() {
    let (repo, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let mut fixture = atlas_fixtures::FixtureRepo::init(repo.path()).unwrap();
    fixture
        .change()
        .unwrap()
        .write("evil\u{1b}[2J.ts", "x\n")
        .commit("Subject \u{1b}[31mred")
        .unwrap();

    let output = codeatlas()
        .arg("analyze")
        .arg(repo.path())
        .arg("--data-dir")
        .arg(data.path())
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(!output.stdout.contains(&0x1b));
}

#[test]
fn overview_before_analysis_says_what_to_do() {
    let (repo, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    atlas_fixtures::generate_main_fixture(repo.path()).unwrap();
    let output = codeatlas()
        .arg("overview")
        .arg(repo.path())
        .arg("--data-dir")
        .arg(data.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("codeatlas analyze")
    );
}

#[test]
fn analyzing_something_that_is_not_a_repository_fails() {
    let (dir, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let output = analyze_json(dir.path(), data.path());
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
}

#[test]
fn serve_registers_a_repository_argument() {
    let (repo, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    atlas_fixtures::generate_main_fixture(repo.path()).unwrap();
    let mut child = codeatlas()
        .arg("serve")
        .arg(repo.path())
        .args(["--port", "0", "--data-dir"])
        .arg(data.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let _server = Server(child);
    let started: Value = serde_json::from_str(&line).unwrap();
    let address = started["url"]
        .as_str()
        .unwrap()
        .trim_start_matches("http://");

    let (status, body) = get(address, "/api/v1/repositories", address);
    assert_eq!(status, 200, "{body}");
    let listed: Vec<RepositoryEntry> = serde_json::from_str(&body).unwrap();
    assert_eq!(listed.len(), 1);
}
