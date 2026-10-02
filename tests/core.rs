use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use resen::{
    app::{App, Input, Modal, Page, SettingsForm},
    backtest::{StrategyParams, simulate},
    config::{Config, Paths, Secrets, atomic_write, validate_endpoint},
    data::{parse_alpha_bars, parse_stooq},
    domain::{
        Bar, MarketSeries, ProviderKind, ResearchKind, ResearchRequest, ResearchRun, parse_symbols,
    },
    store::Store,
};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}
fn app() -> (tempfile::TempDir, App) {
    let tmp = tempfile::tempdir().unwrap();
    let app = App::new(Paths::new(Some(tmp.path().to_path_buf())).unwrap(), true).unwrap();
    (tmp, app)
}
fn request() -> ResearchRequest {
    ResearchRequest {
        kind: ResearchKind::Company,
        symbols: vec!["AAPL".into()],
        question: "Assess business durability".into(),
        prior: None,
    }
}

#[test]
fn symbol_validation_and_deduplication() {
    assert_eq!(
        parse_symbols("$aapl, MSFT aapl brk-b ^spx").unwrap(),
        ["AAPL", "MSFT", "BRK-B", "^SPX"]
    );
    for value in ["../../secrets", "AAPL;ls", "a/b", "😀", "A B C D E F G"] {
        assert!(parse_symbols(value).is_err(), "{value}");
    }
}
#[test]
fn endpoint_transport_and_credentials() {
    for value in [
        "https://api.example.com/v1",
        "http://localhost:11434/v1",
        "http://127.0.0.1:8000/v1",
        "http://[::1]:8000/v1",
    ] {
        assert!(validate_endpoint(value).is_ok(), "{value}");
    }
    for value in [
        "http://example.com/v1",
        "https://user:secret@example.com",
        "https://example.com/?key=x",
        "file:///tmp/a",
        "https://example.com/#token",
        "not a url",
    ] {
        assert!(validate_endpoint(value).is_err(), "{value}");
    }
}
#[test]
fn config_and_keys_roundtrip_with_private_permissions() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = Paths::new(Some(tmp.path().join("state"))).unwrap();
    let config = Config {
        setup_complete: true,
        ..Config::default()
    };
    let mut secrets = Secrets::default();
    secrets.set("RESEN_MODEL_API_KEY", "secret-contract-fixture".into());
    paths.save(&config, &secrets).unwrap();
    let (c, s) = paths.load().unwrap();
    assert!(c.setup_complete);
    assert_eq!(
        s.get("RESEN_MODEL_API_KEY").as_deref(),
        Some("secret-contract-fixture")
    );
    assert!(
        !std::fs::read_to_string(paths.root.join("config.toml"))
            .unwrap()
            .contains("secret-contract-fixture")
    );
    assert_eq!(
        s.redact("oops secret-contract-fixture\x1b[2J"),
        "oops [REDACTED][2J"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(paths.root.join("credentials.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(&paths.root).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
}
#[test]
fn redaction_scans_original_input_and_holds_overlapping_keys() {
    let mut secrets = Secrets::default();
    for (name, value) in [
        ("RESEN_MODEL_API_KEY", "R"),
        ("OLLAMA_API_KEY", "E"),
        ("FRED_API_KEY", "D"),
    ] {
        secrets.set(name, value.into());
    }
    assert_eq!(secrets.redact("RED"), "[REDACTED][REDACTED][REDACTED]");
    assert_eq!(
        secrets.redact(&secrets.redact("RED")),
        secrets.redact("RED")
    );
    let mut stream = secrets.stream_redactor();
    assert_eq!(stream.push("R"), "[REDACTED]");
    assert_eq!(stream.push("ED"), "[REDACTED][REDACTED]");
    assert!(stream.finish().is_empty());
    let mut overlapping = Secrets::default();
    overlapping.set("RESEN_MODEL_API_KEY", "abc".into());
    overlapping.set("OLLAMA_API_KEY", "abcdef".into());
    let mut stream = overlapping.stream_redactor();
    assert_eq!(stream.push("xabc"), "x");
    assert_eq!(stream.push("defz"), "[REDACTED]z");
    assert!(stream.finish().is_empty());
}
#[test]
fn atomic_write_replaces_complete_file() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("report.md");
    atomic_write(&p, b"old").unwrap();
    atomic_write(&p, b"new\ncomplete").unwrap();
    assert_eq!(std::fs::read(&p).unwrap(), b"new\ncomplete");
    assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 1);
}
#[test]
fn unicode_input_preserves_boundaries() {
    let mut input = Input::new("a€🦀z");
    input.key(key(KeyCode::Left));
    input.key(key(KeyCode::Backspace));
    assert_eq!(input.text, "a€z");
    input.key(key(KeyCode::Home));
    input.key(key(KeyCode::Delete));
    assert_eq!(input.text, "€z");
    input.insert("研究");
    assert_eq!(input.text, "研究€z");
    input.key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL));
    input.key(key(KeyCode::Right));
    assert_eq!(input.cursor, 3);
}
#[test]
fn input_paste_is_bounded_and_control_safe() {
    let mut input = Input::default();
    input.insert(&"🦀".repeat(6000));
    assert!(input.text.len() <= 16000);
    assert!(input.text.is_char_boundary(input.cursor));
    let mut input = Input::default();
    input.insert("hello\x1b\x00\nworld");
    assert_eq!(input.text, "hello\nworld");
}
#[test]
fn even_short_saved_keys_are_redacted() {
    let mut secrets = Secrets::default();
    secrets.set("RESEN_MODEL_API_KEY", "abc".into());
    assert_eq!(secrets.redact("key abc"), "key [REDACTED]");
    let mut stream = secrets.stream_redactor();
    let output = stream.push("key a") + &stream.push("bc") + &stream.finish();
    assert_eq!(output, "key [REDACTED]");
}
#[test]
fn invalid_persisted_config_is_rejected_before_startup() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = Paths::new(Some(tmp.path().to_path_buf())).unwrap();
    for watchlist in [
        vec![],
        vec!["NVDA".into(), "NVDA".into()],
        vec!["nvda".into()],
    ] {
        let config = Config {
            watchlist,
            ..Config::default()
        };
        std::fs::write(
            paths.root.join("config.toml"),
            toml::to_string(&config).unwrap(),
        )
        .unwrap();
        assert!(paths.load().is_err());
    }
    std::fs::write(
        paths.root.join("config.toml"),
        toml::to_string(&Config::default()).unwrap(),
    )
    .unwrap();
    std::fs::write(
        paths.root.join("credentials.json"),
        r#"{"values":{"RESEN_MODEL_API_KEY":123456789}}"#,
    )
    .unwrap();
    let error = match paths.load() {
        Err(error) => error.to_string(),
        Ok(_) => panic!("Invalid secrets accepted"),
    };
    assert!(!error.contains("123456789"));
}
#[test]
fn store_recovers_partial_research_and_export_provenance() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = Paths::new(Some(tmp.path().to_path_buf())).unwrap();
    let store = Store::open(&paths).unwrap();
    let mut run = ResearchRun::new(request(), "fixture".into(), "model".into(), false);
    run.report = "Partial insight".into();
    store.save(&run).unwrap();
    assert_eq!(store.recover_interrupted().unwrap(), 1);
    let restored = store.get(&run.id).unwrap();
    assert_eq!(restored.status, "interrupted");
    assert_eq!(restored.report, "Partial insight");
    assert!(restored.markdown().contains("Source ledger"));
    assert_eq!(store.recover_interrupted().unwrap(), 0);
}
#[test]
fn csv_parsing_rejects_bad_prices_duplicates_and_html() {
    let valid =
        b"Date,Open,High,Low,Close,Volume\n2025-01-03,11,12,10,11,200\n2025-01-02,10,11,9,10,100\n";
    let bars = parse_stooq(valid).unwrap();
    assert_eq!(bars[0].date, "2025-01-02");
    for bytes in [
        b"No data".as_slice(),
        b"<html>rate limited</html>",
        b"Date,Open,High,Low,Close,Volume\n2025-01-01,NaN,2,1,1,10\n",
        b"Date,Open,High,Low,Close,Volume\n2025-1-1,1,2,1,1,10\n",
        b"Date,Open,High,Low,Close,Volume\n2099-01-01,1,2,1,1,10\n",
        b"Date,Open,High,Low,Close,Volume\n2025-01-01,1,2,1,1,10\n2025-01-01,1,2,1,1,10\n",
    ] {
        assert!(parse_stooq(bytes).is_err());
    }
}
#[test]
fn alpha_vantage_http_200_rate_limit_is_an_error() {
    assert!(parse_alpha_bars(&serde_json::json!({"Information":"Rate limit reached"})).is_err());
    assert!(parse_alpha_bars(&serde_json::json!({"Note":"API limit"})).is_err());
}
#[test]
fn macro_evidence_keeps_units_frequency_vintage_and_missing_values() {
    let metadata = serde_json::json!({"seriess":[{"id":"UNRATE","title":"Unemployment Rate","units":"Percent","frequency":"Monthly","seasonal_adjustment":"Seasonally Adjusted","last_updated":"2026-09-04"}]});
    let data = serde_json::json!({"observations":[{"date":"2026-08-01","value":"."}]});
    let source = resen::data::fred_source("UNRATE", &metadata, &data).unwrap();
    for text in [
        "Percent",
        "Monthly",
        "Seasonally Adjusted",
        "not point-in-time",
        "missing, not zero",
    ] {
        assert!(source.content.contains(text));
    }
    assert_eq!(source.as_of.as_deref(), Some("2026-08-01"));
    assert!(resen::data::fred_source("GDP", &metadata, &data).is_err());
    assert!(resen::data::fred_source("UNRATE", &serde_json::json!({}), &data).is_err());
}
#[test]
fn sec_facts_preserve_overlapping_periods_units_and_restatement_provenance() {
    let row = |start: &str, end: &str, value: f64, form: &str, filed: &str| serde_json::json!({"start":start,"end":end,"val":value,"accn":"000001-25-000002","fy":2025,"fp":"Q2","form":form,"filed":filed});
    let data = serde_json::json!({"cik":1,"entityName":"Fixture Company","facts":{"us-gaap":{"Revenues":{"label":"Revenues","units":{"USD":[row("2025-01-01","2025-06-30",200.0,"10-Q","2025-07-20"),row("2025-04-01","2025-06-30",110.0,"10-Q/A","2025-08-10")],"CAD":[row("2025-04-01","2025-06-30",150.0,"10-Q","2025-07-20")]}}},"ifrs-full":{"ProfitLoss":{"label":"Profit or loss","units":{"USD":[row("2025-01-01","2025-06-30",-10.0,"20-F","2025-07-20")]}}}}});
    let source = resen::data::sec_facts_source("TEST", 1, &data).unwrap();
    for text in [
        "2025-01-01",
        "2025-04-01",
        "USD",
        "CAD",
        "10-Q/A",
        "000001-25-000002",
        "ifrs-full",
        "-10.0",
        "Do not sum",
    ] {
        assert!(source.content.contains(text), "Missing {text}");
    }
    assert_eq!(source.as_of.as_deref(), Some("2025-06-30"));
    assert!(resen::data::sec_facts_source("TEST", 2, &data).is_err());
    assert!(
        resen::data::sec_facts_source("TEST", 1, &serde_json::json!({"cik":1,"facts":{}})).is_err()
    );
}
fn series(prices: impl Iterator<Item = f64>) -> MarketSeries {
    let start = chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
    let bars = prices
        .enumerate()
        .map(|(i, p)| Bar {
            date: (start + chrono::Duration::days(i as i64)).to_string(),
            open: p,
            high: p,
            low: p,
            close: p,
            volume: 1000.0,
        })
        .collect();
    MarketSeries {
        symbol: "TEST".into(),
        bars,
        source: "fixture".into(),
        retrieved_at: chrono::Utc::now(),
        demo: false,
    }
}
fn params() -> StrategyParams {
    StrategyParams {
        fast: 2,
        slow: 4,
        initial_cash: 1000.0,
        cost_bps: 10.0,
    }
}
#[test]
fn flat_strategy_has_no_risk_and_benchmark_pays_both_costs() {
    let result = simulate(&series((0..30).map(|_| 100.0)), params()).unwrap();
    assert_eq!(result.trades, 0);
    assert_eq!(result.total_return, 0.0);
    assert_eq!(result.max_drawdown, 0.0);
    assert_eq!(result.sharpe, None);
    assert_eq!(result.cagr, None);
    let expected = (1.0 - 0.001) / (1.0 + 0.001) - 1.0;
    assert!((result.benchmark_return - expected).abs() < 1e-12);
}
#[test]
fn strategy_rejects_arithmetic_overflow_and_flat_means_stay_equal() {
    let mut p = params();
    p.fast = 20;
    p.slow = 60;
    let flat = simulate(&series((0..100).map(|_| 101.3)), p).unwrap();
    assert_eq!(flat.trades, 0);
    let mut p = params();
    p.initial_cash = 1e308;
    assert!(simulate(&series((0..100).map(|_| 1e-300)), p).is_err());
}
#[test]
fn increasing_market_exact_return_and_entry_exit_costs() {
    let result = simulate(&series((0..30).map(|i| 100.0 + i as f64)), params()).unwrap();
    let shares = 1000.0 / (104.0 * 1.001);
    let expected = shares * 129.0 * 0.999;
    assert!((result.points.last().unwrap().equity - expected).abs() < 1e-9);
    assert_eq!(result.trades, 2);
    assert!((result.total_return - result.benchmark_return).abs() < 1e-12);
    assert!((result.fees - (shares * 104.0 * 0.001 + shares * 129.0 * 0.001)).abs() < 1e-9);
    assert_eq!(result.exposure, 1.0);
}
#[test]
fn future_prices_cannot_change_earlier_equity() {
    let original = series((0..100).map(|i| 100.0 + (i as f64 * 0.2).sin() * 10.0));
    let mut changed = original.clone();
    for bar in &mut changed.bars[80..] {
        bar.open *= 2.0;
        bar.close *= 2.0;
        bar.high *= 2.0;
        bar.low *= 2.0;
    }
    let a = simulate(&original, params()).unwrap();
    let b = simulate(&changed, params()).unwrap();
    for (i, p) in a.points.iter().enumerate().take(75) {
        assert_eq!(p.equity, b.points[i].equity);
    }
}
#[test]
fn invalid_strategy_parameters_and_unsorted_data_fail() {
    let s = series((0..20).map(|i| 100.0 + i as f64));
    for p in [
        StrategyParams {
            fast: 0,
            ..params()
        },
        StrategyParams {
            slow: 2,
            ..params()
        },
        StrategyParams {
            cost_bps: f64::NAN,
            ..params()
        },
        StrategyParams {
            initial_cash: -1.0,
            ..params()
        },
    ] {
        assert!(simulate(&s, p).is_err());
    }
    let mut s = s;
    s.bars.swap(4, 5);
    assert!(simulate(&s, params()).is_err());
}
#[test]
fn provider_selection_updates_defaults_and_masks_existing_key() {
    let mut secrets = Secrets::default();
    secrets.set("RESEN_MODEL_API_KEY", "fixture-key".into());
    let mut form = SettingsForm::new(&Config::default(), &secrets, false);
    form.cycle(3);
    assert_eq!(form.config.provider, ProviderKind::Compatible);
    assert_eq!(form.fields[3].input.text, "");
    assert!(form.fields[3].hint.contains("configured"));
    assert!(!form.fields[3].hint.contains("fixture-key"));
}

#[test]
fn stream_redaction_hides_secrets_across_every_chunk_boundary() {
    let secret = "long-secret-fixture-token";
    let mut secrets = Secrets::default();
    secrets.set("RESEN_MODEL_API_KEY", secret.into());
    let text = format!("before {secret} after");
    for split in 0..text.len() {
        let mut redactor = secrets.stream_redactor();
        let result = format!(
            "{}{}{}",
            redactor.push(&text[..split]),
            redactor.push(&text[split..]),
            redactor.finish()
        );
        assert_eq!(result, "before [REDACTED] after", "split {split}");
    }
}
#[test]
fn invalid_form_submission_preserves_input() {
    let (_tmp, mut app) = app();
    app.compose(ResearchKind::Company, false);
    if let Some(Modal::Compose { symbols, focus, .. }) = &mut app.modal {
        *symbols = Input::new("bad/symbol");
        *focus = 1;
    }
    assert!(
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL))
            .is_err()
    );
    assert!(
        matches!(app.modal, Some(Modal::Compose { .. })),
        "Validation must leave the dialog and entered data open"
    );
}
#[test]
fn navigation_and_search_do_not_trigger_requests() {
    let (_tmp, mut app) = app();
    for i in 0..6 {
        app.handle_key(key(KeyCode::Char((b'1' + i) as char)))
            .unwrap();
        assert_eq!(app.page, Page::ALL[i as usize]);
        assert!(!app.busy());
    }
    app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL))
        .unwrap();
    assert!(matches!(app.modal, Some(Modal::Palette { .. })));
    assert_eq!(App::palette_matches("C#"), [10]);
}
