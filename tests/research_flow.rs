use resen::{
    app::App,
    config::{Config, Paths, Secrets},
    data::{DataClient, import_prices},
    domain::ProviderKind,
};

fn fixture(path: &std::path::Path) {
    std::fs::write(path,"Date,Open,High,Low,Close,Volume\n2025-01-02,100,102,99,101,1000\n2025-01-03,101,103,100,102,1200\n").unwrap();
}
#[tokio::test]
async fn imported_history_is_evidence_without_network() {
    let dir = tempfile::tempdir().unwrap();
    let csv = dir.path().join("user-history.csv");
    fixture(&csv);
    let imports = dir.path().join("imports");
    let series = import_prices(&imports, "TEST", &csv).unwrap();
    assert_eq!(series.bars.len(), 2);
    let config = Config {
        data_provider: "csv".into(),
        ..Config::default()
    };
    let client = DataClient::new(config, Secrets::default())
        .unwrap()
        .with_imports(imports);
    let loaded = client.market("TEST").await.unwrap();
    assert_eq!(loaded.bars[1].close, 102.0);
    let source = resen::data::market_source(&loaded);
    assert!(source.url.starts_with("file://"));
    assert!(source.content.contains("2025-01-03"));
    assert!(!source.content.contains("Currency: USD"));
    assert!(client.market("MISSING").await.is_err());
}
#[test]
fn import_updates_watchlist_and_persists_selected_data_source() {
    let dir = tempfile::tempdir().unwrap();
    let csv = dir.path().join("prices.csv");
    fixture(&csv);
    let mut app = App::new(Paths::new(Some(dir.path().join("state"))).unwrap(), true).unwrap();
    app.import_csv("TEST", &csv).unwrap();
    assert_eq!(app.config.data_provider, "csv");
    assert!(!app.demo);
    assert_eq!(app.selected_symbol(), "TEST");
    assert!(app.markets.contains_key("TEST"));
    assert!(app.markets.values().all(|series| !series.demo));
    let (config, _) = app.paths.load().unwrap();
    assert_eq!(config.data_provider, "csv");
}
#[tokio::test]
async fn every_demo_workflow_has_matching_citations_and_unicode_safe_streaming() {
    use resen::domain::{ResearchKind, ResearchRequest, ResearchRun};
    use resen::research::{self, ResearchEvent};
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(Some(dir.path().join("state"))).unwrap();
    for kind in ResearchKind::ALL {
        let symbols = if kind == ResearchKind::Comparison {
            vec!["AAPL".into(), "MSFT".into()]
        } else {
            vec![]
        };
        let question = "研究 the evidence before drawing conclusions".to_string();
        let run = ResearchRun::new(
            ResearchRequest {
                kind,
                symbols,
                question: question.clone(),
                prior: None,
            },
            "offline fixture".into(),
            "offline fixture".into(),
            true,
        );
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            research::run(run, Config::default(), Secrets::default(), &paths, tx),
        )
        .await
        .unwrap()
        .unwrap();
        let mut streamed = String::new();
        let mut completed = None;
        while let Some(event) = rx.recv().await {
            match event {
                ResearchEvent::Delta(text) => streamed.push_str(&text),
                ResearchEvent::Finished(run) => completed = Some(run),
                _ => {}
            }
        }
        let mut run = completed.unwrap();
        assert_eq!(run.status, "complete");
        assert_eq!(run.report, streamed);
        assert!(run.report.contains("DEMO"));
        assert!(!run.sources.is_empty());
        if kind == ResearchKind::Custom {
            assert!(run.report.contains(&question));
        }
        research::validate_citations(&mut run);
        assert!(run.warnings.iter().all(|w| !w.contains("citation")));
    }
}
#[cfg(unix)]
#[test]
fn full_research_cli_uses_imported_evidence_and_isolated_codex_protocol() {
    cli_research_contract(ProviderKind::Codex);
}
#[cfg(unix)]
#[test]
fn full_research_cli_uses_isolated_claude_protocol() {
    cli_research_contract(ProviderKind::Claude);
}
#[cfg(unix)]
#[test]
fn cli_connection_check_requires_auth_without_generating() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(Some(dir.path().join("state"))).unwrap();
    let config = Config {
        provider: ProviderKind::Claude,
        model: String::new(),
        ..Config::default()
    };
    paths.save(&config, &Secrets::default()).unwrap();
    let bin = dir.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    let executable = bin.join("claude");
    std::fs::write(&executable,r#"#!/usr/bin/env python3
import json, os, sys
if sys.argv[1:] == ['--version']:
 print('fixture-claude 1.0')
else:
 assert sys.argv[1:] == ['auth', 'status'], 'A connection check must not generate'
 print(json.dumps({'loggedIn':os.environ['QA_LOGIN'] == 'true','token':'fixture-hidden-credential'}))
"#).unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .unwrap();
    for authenticated in [false, true] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_resen"))
            .args([
                "--data-dir",
                paths.root.to_str().unwrap(),
                "doctor",
                "--online",
            ])
            .env("PATH", &path)
            .env("QA_LOGIN", authenticated.to_string())
            .output()
            .unwrap();
        assert_eq!(output.status.success(), authenticated);
        assert!(!String::from_utf8_lossy(&output.stderr).contains("fixture-hidden-credential"));
    }
}
#[cfg(unix)]
fn cli_research_contract(provider: ProviderKind) {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(Some(dir.path().join("state"))).unwrap();
    let bin = dir.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    let csv = dir.path().join("contract-fixture.csv");
    fixture(&csv);
    import_prices(&paths.root.join("imports"), "TEST", &csv).unwrap();
    let config = Config {
        provider,
        model: String::new(),
        endpoint: String::new(),
        data_provider: "csv".into(),
        setup_complete: true,
        ..Config::default()
    };
    paths.save(&config, &Secrets::default()).unwrap();
    let executable = bin.join(if provider == ProviderKind::Codex {
        "codex"
    } else {
        "claude"
    });
    std::fs::write(&executable,r#"#!/usr/bin/env python3
import json, sys, os
args = sys.argv[1:]
codex = os.path.basename(sys.argv[0]) == 'codex'
if codex:
 assert args[0] == 'exec'
 assert '--ignore-user-config' in args
 assert '--ignore-rules' in args
 assert args[args.index('--sandbox') + 1] == 'read-only'
 assert 'features.shell_tool=false' in args
 assert 'features.unified_exec=false' in args
 assert 'mcp_servers={}' in args
 assert 'web_search="disabled"' in args
 for feature in ['multi_agent', 'multi_agent_v2', 'apps', 'plugins', 'hooks', 'browser_use', 'computer_use', 'in_app_browser', 'code_mode_host', 'skill_search', 'view_image']:
  assert f'features.{feature}=false' in args
else:
 assert '-p' in args
 assert '--safe-mode' in args
 assert args[args.index('--tools') + 1] == ''
 assert '--strict-mcp-config' in args
 assert args[args.index('--setting-sources') + 1] == ''
 assert '--no-session-persistence' in args
 assert '--disable-slash-commands' in args
text = sys.stdin.read()
assert 'UNTRUSTED SOURCE LEDGER' in text
assert 'contract-fixture.csv' in text
assert '2025-01-03' in text
sys.stderr.write('bounded diagnostic\n' * 5000)
memo = '# Fixture memo\n\nThe imported fixture closes at 102 on 2025-01-03 [1]. Currency is not supplied. This tests the research pipeline, not market performance.'
if codex:
 print(json.dumps({'type':'item.completed','item':{'type':'agent_message','text':memo}}))
 print(json.dumps({'type':'turn.completed'}))
else:
 print(json.dumps({'type':'assistant','message':{'content':[{'type':'text','text':memo}]}}))
 print(json.dumps({'type':'result','is_error':False,'result':memo}))
"#).unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .unwrap();
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_resen"));
    command
        .args([
            "--data-dir",
            paths.root.to_str().unwrap(),
            "research",
            "Explain this contract fixture",
            "--symbols",
            "TEST",
            "--kind",
            "custom",
        ])
        .env("PATH", path);
    for key in [
        "ALPHAVANTAGE_API_KEY",
        "STOOQ_API_KEY",
        "BRAVE_API_KEY",
        "TAVILY_API_KEY",
        "FRED_API_KEY",
    ] {
        command.env_remove(key);
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let memo = String::from_utf8(output.stdout).unwrap();
    assert!(memo.contains("Fixture memo"));
    assert!(memo.contains("Source ledger"));
    assert!(memo.contains("contract-fixture.csv"));
    let store = resen::store::Store::open(&paths).unwrap();
    let runs = store.list().unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].status, "complete");
    assert_eq!(runs[0].sources.len(), 1);
    assert!(runs[0].report.contains("[1]"));
}
