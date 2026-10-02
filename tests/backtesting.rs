use base64::{Engine, engine::general_purpose::STANDARD};
use resen::backtest::{qc_auth, read_lean_statistics, scaffold};
#[test]
fn quantconnect_auth_matches_independent_sha256_fixture() {
    let value = qc_auth("123", "token", 1700000000);
    let decoded = STANDARD
        .decode(value.strip_prefix("Basic ").unwrap())
        .unwrap();
    assert_eq!(
        String::from_utf8(decoded).unwrap(),
        "123:c252f2e416aba35415549badd29726e9f9c0627a5e203639098e51f7caa5bba8"
    );
}
#[test]
fn lean_scaffold_preserves_existing_projects() {
    let dir = tempfile::tempdir().unwrap();
    let path = scaffold(dir.path(), "python").unwrap();
    let code = std::fs::read_to_string(path.join("main.py")).unwrap();
    assert!(code.contains("class ResenTrend"));
    assert!(code.contains("is_warming_up"));
    assert!(scaffold(dir.path(), "python").is_err());
    let path = scaffold(dir.path(), "csharp").unwrap();
    assert!(path.join("Main.cs").exists());
    let config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path.join("config.json")).unwrap()).unwrap();
    assert_eq!(config["algorithm-language"], "CSharp");
}
#[test]
fn lean_statistics_reader_skips_nonresult_files() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("config.json"), "{}").unwrap();
    std::fs::write(
        dir.path().join("results.json"),
        r#"{"statistics":{"Sharpe Ratio":"1.25","Total Trades":6}}"#,
    )
    .unwrap();
    let stats = read_lean_statistics(dir.path()).unwrap();
    assert_eq!(stats["Sharpe Ratio"], "1.25");
    assert_eq!(stats["Total Trades"], "6");
}
#[cfg(unix)]
#[test]
fn lean_cli_contract_drains_diagnostics_and_parses_results() {
    use resen::config::{Config, Paths, Secrets};
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path().join("workspace");
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::create_dir(&bin).unwrap();
    std::fs::write(workspace.join("lean.json"), "{}").unwrap();
    let project = scaffold(&workspace, "python").unwrap();
    let fake_docker = bin.join("docker");
    std::fs::write(&fake_docker, "#!/bin/sh\necho fixture-docker\n").unwrap();
    std::fs::set_permissions(&fake_docker, std::fs::Permissions::from_mode(0o700)).unwrap();
    let fake_lean = bin.join("lean");
    std::fs::write(&fake_lean,r#"#!/usr/bin/env python3
import json, pathlib, sys
assert sys.argv[1] == 'backtest'
assert '--no-update' in sys.argv
out = pathlib.Path(sys.argv[sys.argv.index('--output') + 1])
out.mkdir(parents=True, exist_ok=True)
sys.stderr.write('do not leak qa-lean-secret\n')
sys.stderr.write(('diagnostic fixture ' * 20 + '\n') * 30000)
sys.stdout.write('Running reviewed fixture\n')
(out / 'results.json').write_text(json.dumps({'statistics': {'Sharpe Ratio': '0.91', 'Total Trades': '8'}}))
"#).unwrap();
    std::fs::set_permissions(&fake_lean, std::fs::Permissions::from_mode(0o700)).unwrap();
    let paths = Paths::new(Some(dir.path().join("state"))).unwrap();
    let config = Config {
        lean_workspace: workspace.to_string_lossy().into(),
        setup_complete: true,
        ..Config::default()
    };
    paths.save(&config, &Secrets::default()).unwrap();
    let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_resen"))
        .args([
            "--data-dir",
            paths.root.to_str().unwrap(),
            "lean-run",
            project.to_str().unwrap(),
        ])
        .env("PATH", path)
        .env("RESEN_MODEL_API_KEY", "qa-lean-secret")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stats: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let diagnostics = String::from_utf8_lossy(&output.stderr);
    assert!(!diagnostics.contains("qa-lean-secret"));
    assert!(diagnostics.contains("[REDACTED]"));
    assert!(diagnostics.contains("Runner log exceeded 8 MB"));
    assert!(output.stderr.len() <= 8_010_000);
    assert_eq!(stats["Sharpe Ratio"], "0.91");
    assert_eq!(stats["Total Trades"], "8");
}
