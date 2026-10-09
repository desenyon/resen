use resen::{
    app::{App, JobEvent},
    config::Paths,
    domain::{ResearchKind, ResearchRequest},
    research::ResearchEvent,
};
fn app() -> (tempfile::TempDir, App) {
    let dir = tempfile::tempdir().unwrap();
    let app = App::new(Paths::new(Some(dir.path().to_path_buf())).unwrap(), true).unwrap();
    (dir, app)
}
fn request() -> ResearchRequest {
    ResearchRequest {
        kind: ResearchKind::Custom,
        symbols: vec!["TEST".into()],
        question: "Lifecycle fixture".into(),
        prior: None,
    }
}
#[tokio::test]
async fn headless_stream_does_not_write_every_token_at_tick_zero() {
    let (_dir, mut app) = app();
    app.start_research(request()).unwrap();
    app.apply_job(JobEvent::Research(Box::new(ResearchEvent::Delta(
        "small chunk".into(),
    ))))
    .unwrap();
    let id = app.current.as_ref().unwrap().id.clone();
    assert_eq!(app.store.get(&id).unwrap().report, "");
    app.cancel().unwrap();
    assert_eq!(app.store.get(&id).unwrap().report, "small chunk");
}
#[tokio::test]
async fn large_stream_checkpoints_independently_of_ui_tick() {
    let (_dir, mut app) = app();
    app.start_research(request()).unwrap();
    app.tick = 1;
    let text = "x".repeat(70_000);
    app.apply_job(JobEvent::Research(Box::new(ResearchEvent::Delta(
        text.clone(),
    ))))
    .unwrap();
    assert_eq!(
        app.store
            .get(&app.current.as_ref().unwrap().id)
            .unwrap()
            .report,
        text
    );
    app.cancel().unwrap();
}
#[tokio::test]
async fn cancellation_preserves_already_queued_output() {
    let (_dir, mut app) = app();
    app.start_research(request()).unwrap();
    app.tx
        .send(JobEvent::Tagged {
            generation: 1,
            event: Box::new(JobEvent::Research(Box::new(ResearchEvent::Delta(
                "queued partial memo".into(),
            )))),
        })
        .unwrap();
    app.cancel().unwrap();
    let run = app.current.as_ref().unwrap();
    assert_eq!(run.status, "cancelled");
    assert_eq!(
        app.store.get(&run.id).unwrap().report,
        "queued partial memo"
    );
}
#[tokio::test]
async fn terminal_research_ignores_late_untagged_deltas() {
    let (_dir, mut app) = app();
    app.start_research(request()).unwrap();
    app.cancel().unwrap();
    app.apply_job(JobEvent::Research(Box::new(ResearchEvent::Delta(
        "late text".into(),
    ))))
    .unwrap();
    assert_eq!(app.current.as_ref().unwrap().report, "");
}

#[tokio::test]
async fn stalled_stream_is_saved_by_maintenance_without_another_delta() {
    let (_dir, mut app) = app();
    app.start_research(request()).unwrap();
    app.apply_job(JobEvent::Research(Box::new(ResearchEvent::Delta(
        "stalled output".into(),
    ))))
    .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    app.maintain_jobs().unwrap();
    let saved = app.store.get(&app.current.as_ref().unwrap().id).unwrap();
    assert_eq!(saved.report, "stalled output");
    assert!(saved.elapsed_ms >= 1000);
    app.cancel().unwrap();
}
#[tokio::test]
async fn failed_final_save_keeps_final_result_for_retry() {
    let (dir, mut app) = app();
    app.start_research(request()).unwrap();
    let db = rusqlite::Connection::open(dir.path().join("research.db")).unwrap();
    db.execute_batch("CREATE TRIGGER no_writes BEFORE UPDATE ON runs BEGIN SELECT RAISE(ABORT, 'fixture disk full'); END;").unwrap();
    let mut final_run = app.current.as_ref().unwrap().clone();
    final_run.status = "complete".into();
    final_run.report = "final memo".into();
    assert!(
        app.apply_job(JobEvent::Research(Box::new(ResearchEvent::Finished(
            final_run
        ))))
        .is_err()
    );
    assert_eq!(app.current.as_ref().unwrap().report, "final memo");
    assert!(app.busy());
    db.execute_batch("DROP TRIGGER no_writes;").unwrap();
    app.maintain_jobs().unwrap();
    assert!(!app.busy());
    let saved = app.store.get(&app.current.as_ref().unwrap().id).unwrap();
    assert_eq!(saved.status, "complete");
    assert_eq!(saved.report, "final memo");
}
#[tokio::test]
async fn failed_cancellation_save_cannot_be_overwritten_by_new_research() {
    let (dir, mut app) = app();
    app.start_research(request()).unwrap();
    let db = rusqlite::Connection::open(dir.path().join("research.db")).unwrap();
    db.execute_batch("CREATE TRIGGER no_writes BEFORE UPDATE ON runs BEGIN SELECT RAISE(ABORT, 'fixture disk full'); END;").unwrap();
    assert!(app.cancel().is_err());
    let id = app.current.as_ref().unwrap().id.clone();
    assert!(app.start_research(request()).is_err());
    assert_eq!(app.current.as_ref().unwrap().id, id);
    db.execute_batch("DROP TRIGGER no_writes;").unwrap();
    app.maintain_jobs().unwrap();
    assert_eq!(app.store.get(&id).unwrap().status, "cancelled");
}
#[tokio::test]
async fn direct_research_requests_reject_empty_or_unnormalized_symbols() {
    let (_dir, mut app) = app();
    for symbol in ["", "$", "aapl", "AAPL,MSFT"] {
        assert!(
            app.start_research(ResearchRequest {
                symbols: vec![symbol.into()],
                ..request()
            })
            .is_err()
        );
    }
    assert!(app.store.list().unwrap().is_empty());
}
