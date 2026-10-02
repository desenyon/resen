use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};
use resen::{
    app::JobEvent,
    app::{App, Input, Modal, Page},
    config::Paths,
    domain::{ResearchKind, ResearchRequest, ResearchRun},
    research::{ResearchEvent, demo_report},
};
fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}
fn app() -> (tempfile::TempDir, App) {
    let dir = tempfile::tempdir().unwrap();
    let app = App::new(Paths::new(Some(dir.path().to_path_buf())).unwrap(), true).unwrap();
    (dir, app)
}
fn seed(app: &mut App) {
    let mut run = ResearchRun::new(
        ResearchRequest {
            kind: ResearchKind::Company,
            symbols: vec!["NVDA".into()],
            question: "What makes this business durable?".into(),
            prior: None,
        },
        "Fixture".into(),
        "model".into(),
        true,
    );
    run.report = demo_report(&run.request.symbols);
    run.status = "complete".into();
    let mut source = resen::data::market_source(&resen::data::demo_market("NVDA"));
    source.id = 1;
    run.sources.push(source);
    app.runs.push(run.clone());
    app.current = Some(run);
}
fn draw(app: &mut App, width: u16, height: u16) -> String {
    let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
    t.draw(|f| resen::ui::render(f, app)).unwrap();
    t.backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect()
}
#[test]
fn all_pages_render_at_all_responsive_sizes() {
    let (_dir, mut app) = app();
    seed(&mut app);
    for size in [
        (20, 8),
        (59, 17),
        (60, 18),
        (80, 24),
        (100, 30),
        (110, 32),
        (144, 46),
        (180, 60),
    ] {
        for page in Page::ALL {
            app.page = page;
            let output = draw(&mut app, size.0, size.1);
            assert!(
                output.contains(if size.0 < 60 || size.1 < 18 {
                    "RESEN"
                } else {
                    "r e s e n"
                }),
                "{page:?} {size:?}"
            );
        }
    }
}
#[test]
fn compact_source_ledger_shows_selected_source_origin() {
    let (_dir, mut app) = app();
    seed(&mut app);
    app.page = Page::Sources;
    let output = draw(&mut app, 80, 24);
    assert!(output.contains("Retrieved"));
    assert!(output.contains("demo://"));
}
#[tokio::test]
async fn late_cancelled_job_cannot_replace_a_new_run() {
    let (_dir, mut app) = app();
    let request = ResearchRequest {
        kind: ResearchKind::Company,
        symbols: vec!["NVDA".into()],
        question: "Fixture research".into(),
        prior: None,
    };
    app.start_research(request.clone()).unwrap();
    let mut stale = app.current.as_ref().unwrap().clone();
    stale.status = "complete".into();
    stale.report = "Stale response".into();
    app.cancel().unwrap();
    app.start_research(request).unwrap();
    let current_id = app.current.as_ref().unwrap().id.clone();
    app.apply_job(JobEvent::Tagged {
        generation: 1,
        event: Box::new(JobEvent::Research(Box::new(ResearchEvent::Finished(stale)))),
    })
    .unwrap();
    assert_eq!(app.current.as_ref().unwrap().id, current_id);
    assert!(app.current.as_ref().unwrap().report.is_empty());
    assert!(app.busy());
    app.cancel().unwrap();
}
#[tokio::test]
async fn known_keys_and_control_sequences_are_removed_before_research_is_saved() {
    let (_dir, mut app) = app();
    app.secrets
        .set("RESEN_MODEL_API_KEY", "fixture-private-token".into());
    app.start_research(ResearchRequest {
        kind: ResearchKind::Custom,
        symbols: vec!["AAPL".into()],
        question: "Investigate fixture-private-token\u{1b}[31m".into(),
        prior: Some("Prior fixture-private-token".into()),
    })
    .unwrap();
    let run = app.current.as_ref().unwrap();
    assert!(!run.request.question.contains("fixture-private-token"));
    assert!(!run.request.question.contains('\u{1b}'));
    assert!(
        !run.request
            .prior
            .as_ref()
            .unwrap()
            .contains("fixture-private-token")
    );
    let saved = app.store.get(&run.id).unwrap();
    assert_eq!(saved.request.question, run.request.question);
    app.cancel().unwrap();
}
#[test]
fn all_dialogs_render_without_overflow_or_panics() {
    let (_dir, mut app) = app();
    for size in [(60, 18), (80, 24), (144, 46)] {
        for modal in [
            Modal::Welcome,
            Modal::Help,
            Modal::Palette {
                input: Input::new("research"),
                selected: 0,
            },
            Modal::Compose {
                kind: ResearchKind::Company,
                symbols: Input::new("AAPL"),
                question: Input::new("研究 a question\nsecond line"),
                focus: 2,
                prior: None,
            },
            Modal::Lean {
                project: Input::new("a project"),
            },
            Modal::Import {
                symbol: Input::new("AAPL"),
                path: Input::new("/path/with spaces/history.csv"),
                focus: 1,
            },
        ] {
            app.modal = Some(modal);
            draw(&mut app, size.0, size.1);
        }
        app.settings(true);
        for step in 0..3 {
            if let Some(Modal::Settings(form)) = &mut app.modal {
                form.step = step;
                form.focus = form.range().start;
            }
            draw(&mut app, size.0, size.1);
        }
        app.settings(false);
        if let Some(Modal::Settings(form)) = &mut app.modal {
            form.focus = 15;
        }
        draw(&mut app, size.0, size.1);
        app.strategy_form();
        draw(&mut app, size.0, size.1);
    }
}
#[test]
fn credentials_are_never_rendered_in_forms_or_connection_directory() {
    let (_dir, mut app) = app();
    app.settings(false);
    if let Some(Modal::Settings(form)) = &mut app.modal {
        form.focus = 3;
        form.fields[3].input = Input::new("sensitive-fixture-token");
    }
    let output = draw(&mut app, 144, 46);
    assert!(!output.contains("sensitive-fixture-token"));
    assert!(output.contains("••••"));
    app.modal = None;
    app.page = Page::Connections;
    draw(&mut app, 144, 46);
}
#[test]
fn demo_is_visible_on_dashboard_and_research() {
    let (_dir, mut app) = app();
    seed(&mut app);
    let output = draw(&mut app, 144, 46);
    assert!(output.contains("DEMO"));
    assert!(output.contains("SAMPLE"));
    assert!(output.contains("synthetic"));
    app.page = Page::Research;
    let output = draw(&mut app, 144, 46);
    assert!(output.contains("ANALYSIS / DEMO"));
}
#[test]
fn reports_scroll_by_rendered_lines_and_clamp() {
    let (_dir, mut app) = app();
    seed(&mut app);
    app.page = Page::Research;
    draw(&mut app, 80, 24);
    let first = draw(&mut app, 80, 24);
    app.handle_key(key(KeyCode::PageDown)).unwrap();
    let next = draw(&mut app, 80, 24);
    assert_ne!(first, next);
    app.scroll = u16::MAX;
    draw(&mut app, 80, 24);
    assert!(app.scroll < u16::MAX);
    app.handle_key(key(KeyCode::Home)).unwrap();
    assert_eq!(app.scroll, 0);
}
#[test]
fn archive_filter_matches_symbols_and_questions() {
    let (_dir, mut app) = app();
    seed(&mut app);
    app.page = Page::Archive;
    app.handle_key(key(KeyCode::Char('/'))).unwrap();
    for c in "nvda".chars() {
        app.handle_key(key(KeyCode::Char(c))).unwrap();
    }
    assert_eq!(app.filtered_runs(), [0]);
    app.handle_key(key(KeyCode::Enter)).unwrap();
    app.handle_key(key(KeyCode::Enter)).unwrap();
    assert_eq!(app.page, Page::Research);
}
#[test]
fn csv_setup_has_no_market_key_and_deleting_it_is_safe() {
    let (_dir, mut app) = app();
    app.config.data_provider = "csv".into();
    app.settings(false);
    if let Some(Modal::Settings(form)) = &mut app.modal {
        form.focus = 5;
        assert!(form.fields[5].hint.contains("No key needed"));
    }
    app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL))
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL))
        .unwrap();
    assert!(app.modal.is_none());
    assert_eq!(app.config.data_provider, "csv");
}
#[tokio::test]
async fn complete_demo_workflow_persists_exports_and_followup() {
    let (_dir, mut app) = app();
    app.compose(ResearchKind::Company, false);
    app.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL))
        .unwrap();
    assert!(app.busy());
    let timeout = tokio::time::sleep(std::time::Duration::from_secs(10));
    tokio::pin!(timeout);
    loop {
        tokio::select! {Some(event)=app.rx.recv()=>{let done=matches!(event.kind(),JobEvent::Research(e)if matches!(**e,ResearchEvent::Finished(_)));app.apply_job(event).unwrap();if done{break;}},_=&mut timeout=>panic!("Demo workflow did not finish")}
    }
    let run = app.current.as_ref().unwrap().clone();
    assert_eq!(run.status, "complete");
    assert!(run.demo);
    assert!(!run.sources.is_empty());
    assert!(run.report.contains("Working thesis"));
    assert_eq!(app.store.get(&run.id).unwrap().report, run.report);
    app.export().unwrap();
    assert!(app.paths.exports().join(format!("{}.md", run.id)).exists());
    app.compose(ResearchKind::Custom, true);
    assert!(
        matches!(&app.modal,Some(Modal::Compose{prior:Some(report),..})if report.contains("Working thesis"))
    );
}
#[tokio::test]
async fn cancelling_preserves_partial_work_and_clears_job() {
    let (_dir, mut app) = app();
    app.start_research(ResearchRequest {
        kind: ResearchKind::Company,
        symbols: vec!["AAPL".into()],
        question: "Research".into(),
        prior: None,
    })
    .unwrap();
    for _ in 0..8 {
        if let Some(event) = app.rx.recv().await {
            app.apply_job(event).unwrap();
        }
    }
    app.cancel().unwrap();
    let run = app.current.as_ref().unwrap();
    assert_eq!(run.status, "cancelled");
    assert!(!run.report.is_empty());
    assert!(!app.busy());
    assert_eq!(app.store.get(&run.id).unwrap().status, "cancelled");
}
#[tokio::test]
async fn archive_cannot_replace_inflight_research() {
    let (_dir, mut app) = app();
    seed(&mut app);
    app.start_research(ResearchRequest {
        kind: ResearchKind::Company,
        symbols: vec!["AAPL".into()],
        question: "Research".into(),
        prior: None,
    })
    .unwrap();
    let id = app.current.as_ref().unwrap().id.clone();
    app.page = Page::Archive;
    assert!(app.handle_key(key(KeyCode::Enter)).is_err());
    assert_eq!(app.current.as_ref().unwrap().id, id);
    app.cancel().unwrap();
}
#[test]
fn active_directory_is_locked_until_owner_closes() {
    let (dir, app) = app();
    let path = Paths::new(Some(dir.path().to_path_buf())).unwrap();
    assert!(App::new(path.clone(), true).is_err());
    drop(app);
    assert!(App::new(path, true).is_ok());
}
#[test]
fn palette_can_open_a_dialog_without_losing_it() {
    let (_dir, mut app) = app();
    app.modal = Some(Modal::Palette {
        input: Input::new("connections"),
        selected: 0,
    });
    app.handle_key(key(KeyCode::Enter)).unwrap();
    assert!(matches!(app.modal, Some(Modal::Settings(_))));
}
