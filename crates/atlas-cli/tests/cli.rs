//! Runs the compiled binary, including a real server on a real socket.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::time::Duration;

use atlas_model::{
    GraphResponse, HealthResponse, HealthStatus, ImpactResponse, OverviewResponse, RepositoryEntry,
};
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
        "analyze", "overview", "graph", "impact", "compare", "timeline", "backtest", "report",
        "serve", "demo",
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
fn report_writes_a_self_contained_page_and_data() {
    let repository = tempfile::tempdir().unwrap();
    atlas_fixtures::generate_main_fixture(repository.path()).unwrap();
    let data = tempfile::tempdir().unwrap();
    let path = repository.path().to_str().unwrap();
    let data_dir = data.path().to_str().unwrap();
    assert!(
        run(&["analyze", path, "--data-dir", data_dir])
            .status
            .success()
    );

    let page = data.path().join("report.html");
    let output = run(&[
        "report",
        path,
        "--data-dir",
        data_dir,
        "--output",
        page.to_str().unwrap(),
    ]);
    assert!(output.status.success(), "{output:?}");
    let html = std::fs::read_to_string(&page).unwrap();
    assert!(html.starts_with("<!DOCTYPE html>"));
    assert!(html.contains("src/billing/invoice.ts"));
    assert!(!html.contains("<script"));
    assert!(!html.contains("http://") && !html.contains("https://"));

    let file = data.path().join("report.json");
    let output = run(&[
        "report",
        path,
        "--data-dir",
        data_dir,
        "--format",
        "json",
        "--output",
        file.to_str().unwrap(),
    ]);
    assert!(output.status.success(), "{output:?}");
    let document: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(document["history"]["commit_count"], 13);
    assert_eq!(
        document["provenance"]["algorithm_version"],
        atlas_model::ALGORITHM_VERSION
    );
}

#[test]
fn report_needs_an_analysis_first() {
    let repository = tempfile::tempdir().unwrap();
    atlas_fixtures::generate_main_fixture(repository.path()).unwrap();
    let data = tempfile::tempdir().unwrap();
    let output = run(&[
        "report",
        repository.path().to_str().unwrap(),
        "--data-dir",
        data.path().to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("analyzed")
    );
}

#[test]
fn demo_prepares_real_results_and_can_run_again() {
    let data = tempfile::tempdir().unwrap();
    let data_dir = data.path().to_str().unwrap();
    for _ in 0..2 {
        let output = run(&["demo", "--prepare-only", "--data-dir", data_dir]);
        assert!(output.status.success(), "{output:?}");
    }
    let repository = data.path().join("demo/product");
    let demo_data = data.path().join("demo");
    let output = run(&[
        "overview",
        repository.to_str().unwrap(),
        "--format",
        "json",
        "--data-dir",
        demo_data.to_str().unwrap(),
    ]);
    assert!(output.status.success(), "{output:?}");
    let overview: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(overview["history"]["commit_count"], 180);
    assert!(!data.path().join("codeatlas.sqlite").exists());
}

#[test]
fn graph_by_directory_shows_groups() {
    let repository = tempfile::tempdir().unwrap();
    atlas_fixtures::generate_main_fixture(repository.path()).unwrap();
    let data = tempfile::tempdir().unwrap();
    let path = repository.path().to_str().unwrap();
    let data_dir = data.path().to_str().unwrap();
    assert!(
        run(&["analyze", path, "--data-dir", data_dir])
            .status
            .success()
    );
    let output = run(&["graph", path, "--by-directory", "--data-dir", data_dir]);
    assert!(output.status.success(), "{output:?}");
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("src/"), "{text}");
    assert!(text.contains("imports"), "{text}");
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

#[test]
fn graph_reads_a_commit_without_any_frontend() {
    let (repo, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    atlas_fixtures::generate_main_fixture(repo.path()).unwrap();
    assert!(analyze_json(repo.path(), data.path()).status.success());

    let output = codeatlas()
        .arg("graph")
        .arg(repo.path())
        .args(["--format", "json", "--data-dir"])
        .arg(data.path())
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let graph: GraphResponse = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(graph.total_node_count, 8);
    assert_eq!(graph.total_edge_count, 4);
}

#[test]
fn serve_refuses_a_web_directory_without_a_dashboard() {
    let (web, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let output = codeatlas()
        .args(["serve", "--port", "0", "--web-dir"])
        .arg(web.path())
        .arg("--data-dir")
        .arg(data.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("index.html")
    );
}

fn impact(repo: &Path, data: &Path, args: &[&str]) -> Output {
    codeatlas()
        .arg("impact")
        .arg(repo)
        .args(args)
        .arg("--data-dir")
        .arg(data)
        .output()
        .unwrap()
}

#[test]
fn impact_ranks_files_with_their_evidence() {
    let (repo, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    atlas_fixtures::generate_main_fixture(repo.path()).unwrap();
    assert!(analyze_json(repo.path(), data.path()).status.success());

    let args = ["--file", "src/auth/session.ts", "--format", "json"];
    let first = impact(repo.path(), data.path(), &args);
    assert!(first.status.success(), "{first:?}");
    let parsed: ImpactResponse = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(
        parsed.recommendations[0].path.as_str(),
        "src/billing/invoice.ts"
    );

    let second = impact(repo.path(), data.path(), &args);
    assert_eq!(first.stdout, second.stdout, "rankings must be reproducible");

    let text = impact(repo.path(), data.path(), &["--file", "src/auth/session.ts"]);
    let text = String::from_utf8(text.stdout).unwrap();
    assert!(text.contains("not a probability"), "{text}");
    assert!(text.contains("3 of 4"), "{text}");
}

#[test]
fn impact_rejects_a_file_missing_at_the_cutoff() {
    let (repo, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    atlas_fixtures::generate_main_fixture(repo.path()).unwrap();
    assert!(analyze_json(repo.path(), data.path()).status.success());
    let output = impact(repo.path(), data.path(), &["--file", "src/nope.ts"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("does not exist")
    );
}

fn compare(repo: &Path, data: &Path, args: &[&str]) -> Output {
    codeatlas()
        .arg("compare")
        .arg(repo)
        .args(args)
        .arg("--data-dir")
        .arg(data)
        .output()
        .unwrap()
}

#[test]
fn compare_diffs_two_commits() {
    let (repo, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    atlas_fixtures::generate_main_fixture(repo.path()).unwrap();
    assert!(analyze_json(repo.path(), data.path()).status.success());

    let args = ["--base", "HEAD~2", "--target", "HEAD~1", "--format", "json"];
    let first = compare(repo.path(), data.path(), &args);
    assert!(first.status.success(), "{first:?}");
    let parsed: atlas_model::CompareResponse = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(parsed.added_files[0].path.as_str(), "tests/auth.test.ts");
    assert_eq!(
        first.stdout,
        compare(repo.path(), data.path(), &args).stdout,
        "comparisons must be reproducible"
    );

    let args = ["--base", "HEAD~3", "--target", "HEAD~2"];
    let text = compare(repo.path(), data.path(), &args);
    let stdout = String::from_utf8(text.stdout).unwrap();
    assert!(
        stdout.contains("- cycle  src/auth/session.ts, src/auth/token.ts"),
        "{stdout}"
    );
    assert!(
        stdout.contains("- import src/auth/token.ts -> src/auth/session.ts"),
        "{stdout}"
    );
    assert!(stdout.contains("not in what runs"), "{stdout}");
}

#[test]
fn a_repository_with_no_typescript_says_so() {
    let (repo, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let mut fixture = atlas_fixtures::FixtureRepo::init(repo.path()).unwrap();
    fixture
        .change()
        .unwrap()
        .write("main.py", "print(1)\n")
        .commit("Python")
        .unwrap();

    let output = codeatlas()
        .arg("analyze")
        .arg(repo.path())
        .arg("--data-dir")
        .arg(data.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("no TS or TSX files"), "{stdout}");
    assert!(!stdout.contains("Next:"), "{stdout}");

    let output = backtest(repo.path(), data.path(), &[]);
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("nothing to evaluate"), "{stdout}");
}

#[test]
fn analyze_says_what_to_do_next_and_warns_when_results_go_stale() {
    let (repo, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let mut fixture = atlas_fixtures::FixtureRepo::init(repo.path()).unwrap();
    fixture
        .change()
        .unwrap()
        .write("a.ts", "export const a = 1;\n")
        .commit("One")
        .unwrap();
    let args = |command: &str| {
        let mut built = codeatlas();
        built
            .arg(command)
            .arg(repo.path())
            .arg("--data-dir")
            .arg(data.path());
        built.output().unwrap()
    };
    let first = String::from_utf8(args("analyze").stdout).unwrap();
    assert!(first.contains("Next: codeatlas serve"), "{first}");

    fixture
        .change()
        .unwrap()
        .append("a.ts", "export const b = 2;\n")
        .commit("Two")
        .unwrap();
    let later = String::from_utf8(args("overview").stdout).unwrap();
    assert!(later.contains("Run `codeatlas analyze`"), "{later}");

    let stderr = String::from_utf8(args("graph").stderr).unwrap();
    assert!(stderr.contains("Run `codeatlas analyze`"), "{stderr}");
}

#[test]
fn impact_accepts_paths_as_shells_give_them() {
    let (repo, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    atlas_fixtures::generate_main_fixture(repo.path()).unwrap();
    assert!(analyze_json(repo.path(), data.path()).status.success());

    let dotted = impact(
        repo.path(),
        data.path(),
        &["--file", "./src/auth/session.ts"],
    );
    assert!(dotted.status.success(), "{dotted:?}");

    let full = repo
        .path()
        .canonicalize()
        .unwrap()
        .join("src/auth/session.ts");
    let full = impact(
        repo.path(),
        data.path(),
        &["--file", full.to_str().unwrap()],
    );
    assert!(full.status.success(), "{full:?}");

    let missing = impact(repo.path(), data.path(), &["--file", "src/nope.ts"]);
    let stderr = String::from_utf8(missing.stderr).unwrap();
    assert!(
        stderr.contains("relative to the repository root"),
        "{stderr}"
    );
}

#[test]
fn a_bad_path_says_what_is_wrong() {
    let data = tempfile::tempdir().unwrap();
    let file = tempfile::NamedTempFile::new().unwrap();
    let error = |path: &Path| {
        let output = codeatlas()
            .arg("analyze")
            .arg(path)
            .arg("--data-dir")
            .arg(data.path())
            .output()
            .unwrap();
        String::from_utf8(output.stderr).unwrap()
    };
    assert!(error(Path::new("/no/such/place")).contains("no such folder"));
    assert!(error(file.path()).contains("it is a file"));
    assert!(error(data.path()).contains("contains .git"));
}

#[test]
fn compare_rejects_an_unknown_commit() {
    let (repo, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    atlas_fixtures::generate_main_fixture(repo.path()).unwrap();
    assert!(analyze_json(repo.path(), data.path()).status.success());

    let args = ["--base", "HEAD", "--target", "no-such-ref"];
    let output = compare(repo.path(), data.path(), &args);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
}

fn backtest(repo: &Path, data: &Path, args: &[&str]) -> Output {
    codeatlas()
        .arg("backtest")
        .arg(repo)
        .args(args)
        .arg("--data-dir")
        .arg(data)
        .output()
        .unwrap()
}

#[test]
fn backtest_reports_rankers_and_says_what_it_measures() {
    let (repo, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    atlas_fixtures::generate_main_fixture(repo.path()).unwrap();
    assert!(analyze_json(repo.path(), data.path()).status.success());

    let args = ["--warmup", "4", "--format", "json"];
    let first = backtest(repo.path(), data.path(), &args);
    assert!(first.status.success(), "{first:?}");
    let parsed: atlas_model::BacktestResult = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(parsed.warmup, 4);
    assert_eq!(parsed.rankers.len(), 5);
    assert_eq!(
        first.stdout,
        backtest(repo.path(), data.path(), &args).stdout,
        "backtests must be reproducible"
    );

    let text = backtest(repo.path(), data.path(), &["--warmup", "4"]);
    let stdout = String::from_utf8(text.stdout).unwrap();
    assert!(stdout.contains("co-edit prediction"), "{stdout}");
    assert!(stdout.contains("Change Impact"), "{stdout}");
}

#[test]
fn backtest_explains_too_little_history() {
    let (repo, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    atlas_fixtures::generate_main_fixture(repo.path()).unwrap();
    assert!(analyze_json(repo.path(), data.path()).status.success());

    let output = backtest(repo.path(), data.path(), &[]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("No commit could be evaluated"), "{stdout}");
}
