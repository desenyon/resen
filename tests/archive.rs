use resen::{
    config::Paths,
    domain::{ResearchKind, ResearchRequest, ResearchRun, parse_symbols},
    store::Store,
};
use rusqlite::{Connection, params};

fn run(id: usize) -> ResearchRun {
    let mut run = ResearchRun::new(
        ResearchRequest {
            kind: ResearchKind::Custom,
            symbols: vec!["TEST".into()],
            question: format!("Question {id}"),
            prior: None,
        },
        "fixture".into(),
        "offline".into(),
        true,
    );
    run.id = format!("run-{id:04}");
    run.created_at = "2025-01-01T00:00:00Z".parse().unwrap();
    run.report = "preserved partial memo".into();
    run
}
fn legacy(paths: &Paths, version: i64) -> Connection {
    let db = Connection::open(paths.root.join("research.db")).unwrap();
    db.execute_batch(
        "CREATE TABLE runs (id TEXT PRIMARY KEY, created_at TEXT NOT NULL, body TEXT NOT NULL);",
    )
    .unwrap();
    db.pragma_update(None, "user_version", version).unwrap();
    db
}
fn insert(db: &Connection, run: &ResearchRun) {
    db.execute(
        "INSERT INTO runs VALUES(?1,?2,?3)",
        params![
            run.id,
            run.created_at.to_rfc3339(),
            serde_json::to_string(run).unwrap()
        ],
    )
    .unwrap();
}
#[test]
fn recovery_reaches_running_records_older_than_two_hundred() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(Some(dir.path().to_path_buf())).unwrap();
    let db = legacy(&paths, 1);
    for id in 0..250 {
        insert(&db, &run(id));
    }
    drop(db);
    let store = Store::open(&paths).unwrap();
    assert_eq!(store.recover_interrupted().unwrap(), 250);
    assert_eq!(store.recover_interrupted().unwrap(), 0);
    assert_eq!(
        store.get("run-0000").unwrap().report,
        "preserved partial memo"
    );
}
#[test]
fn malformed_record_cannot_block_healthy_history_or_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(Some(dir.path().to_path_buf())).unwrap();
    let db = legacy(&paths, 1);
    insert(&db, &run(1));
    db.execute(
        "INSERT INTO runs VALUES('broken','2026-01-01','{broken')",
        [],
    )
    .unwrap();
    drop(db);
    let store = Store::open(&paths).unwrap();
    assert_eq!(store.recover_interrupted().unwrap(), 1);
    assert_eq!(store.list().unwrap().len(), 1);
    let error = store.get("broken").unwrap_err().to_string();
    assert!(error.contains("broken"), "{error}");
    let db = Connection::open(paths.root.join("research.db")).unwrap();
    let body: String = db
        .query_row("SELECT body FROM runs WHERE id='broken'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(body, "{broken");
}
#[test]
fn newer_schema_is_rejected_without_overwriting_version() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(Some(dir.path().to_path_buf())).unwrap();
    let db = legacy(&paths, 99);
    assert!(Store::open(&paths).is_err());
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        99
    );
}
#[test]
fn normalization_rejects_bare_dollar_symbols() {
    assert!(parse_symbols("$").is_err());
    assert!(parse_symbols("$$").is_err());
    assert_eq!(parse_symbols("$aapl,MSFT").unwrap(), ["AAPL", "MSFT"]);
    assert!(parse_symbols("").unwrap().is_empty());
}

#[test]
fn archive_pages_search_all_records_with_stable_ties_and_literal_queries() {
    use resen::store::ArchiveQuery;
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(Some(dir.path().to_path_buf())).unwrap();
    let store = Store::open(&paths).unwrap();
    for id in 0..251 {
        store.save(&run(id)).unwrap();
    }
    let page = store.query(&ArchiveQuery::default()).unwrap();
    assert_eq!(page.total, 251);
    assert_eq!(page.runs.len(), 100);
    assert_eq!(page.runs[0].id, "run-0250");
    let page = store
        .query(&ArchiveQuery {
            offset: 200,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(page.runs.len(), 51);
    assert_eq!(page.runs.last().unwrap().id, "run-0000");
    let page = store
        .query(&ArchiveQuery {
            search: "QUESTION 0".into(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.runs[0].id, "run-0000");
    let page = store
        .query(&ArchiveQuery {
            search: "%_".into(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(page.total, 0);
    assert!(
        store
            .query(&ArchiveQuery {
                limit: 0,
                ..Default::default()
            })
            .is_err()
    );
    assert!(
        store
            .query(&ArchiveQuery {
                limit: 1001,
                ..Default::default()
            })
            .is_err()
    );
    assert_eq!(store.recover_interrupted().unwrap(), 251);
    assert_eq!(
        store
            .query(&ArchiveQuery {
                status: Some("running".into()),
                ..Default::default()
            })
            .unwrap()
            .total,
        0
    );
    assert_eq!(
        store
            .query(&ArchiveQuery {
                status: Some("interrupted".into()),
                ..Default::default()
            })
            .unwrap()
            .total,
        251
    );
}
#[test]
fn migration_is_atomic_and_repaired_records_rejoin_history() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(Some(dir.path().to_path_buf())).unwrap();
    let db = legacy(&paths, 0);
    insert(&db, &run(1));
    db.execute_batch("CREATE INDEX runs_archive_order ON runs(id);")
        .unwrap();
    assert!(Store::open(&paths).is_err());
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(db.prepare("SELECT status FROM runs").is_err());
    db.execute_batch("DROP INDEX runs_archive_order;").unwrap();
    let store = Store::open(&paths).unwrap();
    db.execute("UPDATE runs SET body='{}' WHERE id='run-0001'", [])
        .unwrap();
    assert!(store.list().unwrap().is_empty());
    assert_eq!(store.issues().unwrap()[0].id, "run-0001");
    db.execute(
        "UPDATE runs SET body=?1 WHERE id='run-0001'",
        [serde_json::to_string(&run(1)).unwrap()],
    )
    .unwrap();
    assert_eq!(store.recover_interrupted().unwrap(), 1);
    assert_eq!(store.list().unwrap().len(), 1);
    assert!(store.issues().unwrap().is_empty());
    drop(store);
    assert!(Store::open(&paths).unwrap().issues().unwrap().is_empty());
}
#[test]
fn recovery_rolls_back_all_changes_when_a_write_fails() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(Some(dir.path().to_path_buf())).unwrap();
    let store = Store::open(&paths).unwrap();
    store.save(&run(1)).unwrap();
    store.save(&run(2)).unwrap();
    let db = Connection::open(paths.root.join("research.db")).unwrap();
    db.execute_batch("CREATE TRIGGER fail_recovery BEFORE UPDATE ON runs WHEN OLD.id='run-0002' BEGIN SELECT RAISE(ABORT, 'fixture'); END;").unwrap();
    assert!(store.recover_interrupted().is_err());
    assert_eq!(store.get("run-0001").unwrap().status, "running");
    db.execute_batch("DROP TRIGGER fail_recovery;").unwrap();
    assert_eq!(store.recover_interrupted().unwrap(), 2);
}
#[test]
fn mismatched_json_identity_is_preserved_without_overwriting_other_runs() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(Some(dir.path().to_path_buf())).unwrap();
    let db = legacy(&paths, 1);
    insert(&db, &run(1));
    db.execute(
        "INSERT INTO runs VALUES('wrong','2025-01-01',?1)",
        [serde_json::to_string(&run(1)).unwrap()],
    )
    .unwrap();
    let store = Store::open(&paths).unwrap();
    assert_eq!(store.recover_interrupted().unwrap(), 1);
    assert_eq!(store.issues().unwrap()[0].id, "wrong");
    assert!(store.get("wrong").is_err());
}
#[test]
fn history_cli_can_find_old_records_and_report_unreadable_ids_as_json() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(Some(dir.path().to_path_buf())).unwrap();
    let db = legacy(&paths, 1);
    for id in 0..251 {
        insert(&db, &run(id));
    }
    db.execute(
        "INSERT INTO runs VALUES('broken','2026-01-01','{broken')",
        [],
    )
    .unwrap();
    drop(db);
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_resen"))
        .args([
            "--data-dir",
            paths.root.to_str().unwrap(),
            "history",
            "--search",
            "Question 0",
            "--status",
            "interrupted",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["runs"][0]["id"], "run-0000");
    assert_eq!(result["total"], 1);
    assert_eq!(result["unreadable"], 1);
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_resen"))
        .args([
            "--data-dir",
            paths.root.to_str().unwrap(),
            "history",
            "--issues",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result[0]["id"], "broken");
}

#[test]
fn tui_search_paste_and_paging_reach_the_entire_archive() {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    use resen::app::{App, Page};
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(Some(dir.path().to_path_buf())).unwrap();
    let store = Store::open(&paths).unwrap();
    for id in 0..251 {
        store.save(&run(id)).unwrap();
    }
    drop(store);
    let mut app = App::new(paths, true).unwrap();
    app.page = Page::Archive;
    let key = |code| KeyEvent::new(code, KeyModifiers::NONE);
    app.handle_key(key(KeyCode::End)).unwrap();
    assert_eq!(app.archive_offset, 200);
    assert_eq!(app.runs.len(), 51);
    app.handle_key(key(KeyCode::PageUp)).unwrap();
    assert_eq!(app.archive_offset, 100);
    app.handle_key(key(KeyCode::Home)).unwrap();
    assert_eq!(app.archive_offset, 0);
    app.handle_key(key(KeyCode::Char('/'))).unwrap();
    app.handle_event(Event::Paste("Question 0".into())).unwrap();
    assert_eq!(app.archive_total, 1);
    assert_eq!(app.runs[0].id, "run-0000");
    app.handle_key(key(KeyCode::Enter)).unwrap();
    app.handle_key(key(KeyCode::Enter)).unwrap();
    assert_eq!(app.current.as_ref().unwrap().id, "run-0000");
}
#[test]
fn migration_preserves_completed_json_and_orders_fractional_timestamps() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(Some(dir.path().to_path_buf())).unwrap();
    let db = legacy(&paths, 1);
    let mut early = run(1);
    early.status = "complete".into();
    let mut late = run(2);
    late.created_at = "2025-01-01T00:00:00.001Z".parse().unwrap();
    late.status = "complete".into();
    insert(&db, &early);
    insert(&db, &late);
    let original: String = db
        .query_row("SELECT body FROM runs WHERE id='run-0001'", [], |r| {
            r.get(0)
        })
        .unwrap();
    let store = Store::open(&paths).unwrap();
    assert_eq!(store.recover_interrupted().unwrap(), 0);
    assert_eq!(store.list().unwrap()[0].id, "run-0002");
    let saved: String = db
        .query_row("SELECT body FROM runs WHERE id='run-0001'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(original, saved);
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        resen::store::SCHEMA_VERSION
    );
}
