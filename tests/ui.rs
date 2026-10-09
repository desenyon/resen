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
    app.store.save(&run).unwrap();
    app.refresh_archive().unwrap();
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
            Modal::Help { scroll: 0 },
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

#[test]
fn pasted_palette_query_resets_selection_and_opens_match() {
    let (_dir, mut app) = app();
    app.modal = Some(Modal::Palette {
        input: Input::default(),
        selected: 10,
    });
    app.handle_event(crossterm::event::Event::Paste("connections".into()))
        .unwrap();
    app.handle_key(key(KeyCode::Enter)).unwrap();
    assert!(matches!(app.modal, Some(Modal::Settings(_))));
}

#[test]
fn palette_with_no_matches_stays_open_and_explains_recovery() {
    let (_dir, mut app) = app();
    app.modal = Some(Modal::Palette {
        input: Input::new("zz-no-command"),
        selected: 0,
    });
    assert!(draw(&mut app, 80, 24).contains("No matching commands"));
    app.handle_key(key(KeyCode::Enter)).unwrap();
    assert!(matches!(app.modal, Some(Modal::Palette { .. })));
}

#[test]
fn pasted_archive_query_resets_selection_and_normalizes_lines() {
    let (_dir, mut app) = app();
    seed(&mut app);
    app.page = Page::Archive;
    app.archive_search = true;
    app.archive_selected = 12;
    app.handle_event(crossterm::event::Event::Paste("NVDA\n".into()))
        .unwrap();
    assert_eq!(app.archive_selected, 0);
    assert_eq!(app.archive_filter.text, "NVDA ");
    app.handle_key(key(KeyCode::Enter)).unwrap();
    app.handle_key(key(KeyCode::Enter)).unwrap();
    assert_eq!(app.page, Page::Research);
}

#[test]
fn long_input_cursor_remains_visible_at_start_middle_and_end() {
    let (_dir, mut app) = app();
    let value = format!(
        "START {} MIDDLE {} END",
        "研究 words ".repeat(50),
        "more text ".repeat(50)
    );
    for cursor in [0, value.find("MIDDLE").unwrap(), value.len()] {
        app.modal = Some(Modal::Import {
            symbol: Input::new("AAPL"),
            path: Input {
                text: value.clone(),
                cursor,
            },
            focus: 1,
        });
        let output = draw(&mut app, 80, 24);
        assert!(output.contains('▏'), "cursor {cursor} hidden");
        if cursor == 0 {
            assert!(output.contains("START"));
        }
        if cursor == value.len() {
            assert!(output.contains("END"));
        }
    }
}

#[test]
fn multiline_question_start_cursor_is_visible() {
    let (_dir, mut app) = app();
    app.modal = Some(Modal::Compose {
        kind: ResearchKind::Company,
        symbols: Input::new("AAPL"),
        question: Input {
            text: "first line\n".to_string() + &"another line\n".repeat(30),
            cursor: 0,
        },
        focus: 2,
        prior: None,
    });
    let output = draw(&mut app, 80, 24);
    assert!(output.contains("▏first line"));
}

#[test]
fn compact_welcome_keeps_setup_and_demo_actions_visible() {
    let (_dir, mut app) = app();
    app.modal = Some(Modal::Welcome);
    for (w, h) in [(60, 18), (80, 24), (144, 46)] {
        let output = draw(&mut app, w, h);
        assert!(output.contains("enter  set up"), "setup hidden at {w}x{h}");
        assert!(output.contains("d  explore"), "demo hidden at {w}x{h}");
    }
}

#[test]
fn compact_forms_keep_focused_values_and_submission_visible() {
    let (_dir, mut app) = app();
    for (w, h) in [(60, 18), (80, 24), (144, 46)] {
        app.modal = Some(Modal::Compose {
            kind: ResearchKind::Company,
            symbols: Input::new("AAPL"),
            question: Input::new("unique-question"),
            focus: 2,
            prior: None,
        });
        let output = draw(&mut app, w, h);
        assert!(
            output.contains("unique-question"),
            "question hidden at {w}x{h}"
        );
        assert!(output.contains("ctrl+r"));
        for focus in 0..4 {
            app.modal = Some(Modal::Strategy {
                fields: [
                    Input::new("11"),
                    Input::new("66"),
                    Input::new("123456"),
                    Input::new("22"),
                ],
                focus,
            });
            let output = draw(&mut app, w, h);
            assert!(
                output.contains(["11▏", "66▏", "123456▏", "22▏"][focus]),
                "strategy focus {focus} hidden at {w}x{h}"
            );
            assert!(
                output.contains("enter run"),
                "strategy submit hidden at {w}x{h}"
            );
        }
        app.modal = Some(Modal::Lean {
            project: Input::new("reviewed-project"),
        });
        let output = draw(&mut app, w, h);
        assert!(output.contains("reviewed-project"));
        assert!(output.contains("enter execute"));
        assert!(output.contains("Docker"));
    }
}

#[test]
fn help_scrolls_to_form_controls_and_back_at_small_sizes() {
    let (_dir, mut app) = app();
    for (w, h) in [(60, 18), (80, 24)] {
        app.modal = Some(Modal::Help { scroll: 0 });
        app.handle_key(key(KeyCode::End)).unwrap();
        let output = draw(&mut app, w, h);
        assert!(output.contains("Remove local credential"));
        assert!(output.contains("PgUp/PgDn"));
        app.handle_key(key(KeyCode::Up)).unwrap();
        draw(&mut app, w, h);
        app.handle_key(key(KeyCode::Home)).unwrap();
        assert!(draw(&mut app, w, h).contains("Make the desk your own."));
    }
}

#[test]
fn compact_navigation_names_current_page_and_palette() {
    let (_dir, mut app) = app();
    for page in Page::ALL {
        app.page = page;
        let output = draw(&mut app, 60, 18);
        assert!(
            output.contains(page.label()),
            "{page:?} missing from compact header"
        );
        assert!(output.contains("ctrl+k"));
    }
}

#[test]
fn compact_archive_shows_question_and_explicit_demo_origin() {
    let (_dir, mut app) = app();
    seed(&mut app);
    app.page = Page::Archive;
    for (w, h) in [(60, 18), (80, 24)] {
        let output = draw(&mut app, w, h);
        assert!(output.contains("What makes this business durable?"));
        assert!(output.contains("DEMO"));
        assert!(output.contains("complete"));
    }
    app.archive_filter = Input::new("unmatched-query");
    let output = draw(&mut app, 80, 24);
    assert!(output.contains("No matching research"));
    assert!(output.contains("/ to clear"));
}

#[test]
fn minimum_source_view_shows_selection_and_origin() {
    let (_dir, mut app) = app();
    seed(&mut app);
    app.page = Page::Sources;
    let output = draw(&mut app, 60, 18);
    assert!(output.contains("demo://"));
    assert!(output.contains("Retrieved"));
    assert!(output.contains("[1]"));
}

#[test]
fn minimum_desk_chart_discloses_series_and_date() {
    let (_dir, mut app) = app();
    let output = draw(&mut app, 60, 18);
    assert!(output.contains("SYNTHETIC SERIES"));
    let date = app.markets[app.selected_symbol()]
        .last()
        .unwrap()
        .date
        .clone();
    assert!(output.contains(&date));
}

#[test]
fn compact_lab_keeps_metrics_and_assumptions_readable() {
    let (_dir, mut app) = app();
    app.backtest = Some(
        resen::backtest::simulate(
            &resen::data::demo_market("NVDA"),
            resen::backtest::StrategyParams::default(),
        )
        .unwrap(),
    );
    app.page = Page::Lab;
    for (w, h) in [(60, 18), (80, 24)] {
        let output = draw(&mut app, w, h);
        assert!(output.contains("Return"));
        assert!(output.contains("Drawdown"));
        assert!(output.contains("next-open"));
        assert!(output.contains("not LEAN"));
    }
}

#[tokio::test]
async fn compact_busy_footer_keeps_cancellation_and_phase_visible() {
    let (_dir, mut app) = app();
    app.start_research(ResearchRequest {
        kind: ResearchKind::Company,
        symbols: vec!["AAPL".into()],
        question: "Research".into(),
        prior: None,
    })
    .unwrap();
    app.phase = "Collecting evidence".into();
    for page in Page::ALL {
        app.page = page;
        let output = draw(&mut app, 60, 18);
        assert!(
            output.contains("ctrl+c cancel"),
            "cancel hidden on {page:?}"
        );
        assert!(
            output.contains("Collecting evidence"),
            "phase hidden on {page:?}"
        );
    }
    app.cancel().unwrap();
}

#[test]
fn wizard_replacing_watchlist_resets_asset_selection_and_errors() {
    let (_dir, mut app) = app();
    app.config.watchlist = ["NVDA", "AAPL", "MSFT", "SPY", "QQQ", "TSLA"]
        .map(str::to_string)
        .to_vec();
    app.selected = 5;
    app.market_errors
        .insert("TSLA".into(), "Old source error".into());
    app.settings(true);
    if let Some(Modal::Settings(form)) = &mut app.modal {
        form.step = 2;
        form.fields[15].input = Input::new("NVDA");
    }
    app.handle_key(key(KeyCode::Enter)).unwrap();
    assert_eq!(app.selected, 0);
    assert!(app.market_errors.is_empty());
    draw(&mut app, 144, 46);
}

#[test]
fn undersized_terminal_quit_works_even_when_a_dialog_is_open() {
    let (_dir, mut app) = app();
    app.modal = Some(Modal::Welcome);
    draw(&mut app, 20, 8);
    app.handle_key(key(KeyCode::Char('q'))).unwrap();
    assert!(app.quit);
}

#[test]
fn literal_caret_in_long_input_cannot_hide_the_real_start_cursor() {
    let (_dir, mut app) = app();
    app.modal = Some(Modal::Import {
        symbol: Input::new("AAPL"),
        path: Input {
            text: "START\n".to_string() + &"literal ▏ text\n".repeat(30),
            cursor: 0,
        },
        focus: 1,
    });
    assert!(draw(&mut app, 80, 24).contains("▏START"));
}

#[test]
fn dismissing_error_keeps_form_values_for_retry() {
    let (_dir, mut app) = app();
    app.modal = Some(Modal::Import {
        symbol: Input::new("AAPL"),
        path: Input::new("/missing/history.csv"),
        focus: 1,
    });
    let error = app.handle_key(key(KeyCode::Enter)).unwrap_err();
    app.notify(format!("{error:#}"), true);
    assert!(draw(&mut app, 60, 18).contains("esc dismiss"));
    app.handle_key(key(KeyCode::Esc)).unwrap();
    assert!(app.toast.is_none());
    assert!(
        matches!(&app.modal,Some(Modal::Import{path,..}) if path.text == "/missing/history.csv")
    );
    assert!(draw(&mut app, 60, 18).contains("enter import"));
}

#[test]
fn mouse_wheel_scrolls_help_without_scrolling_background() {
    let (_dir, mut app) = app();
    app.modal = Some(Modal::Help { scroll: 0 });
    app.handle_event(crossterm::event::Event::Mouse(
        crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::ScrollDown,
            column: 10,
            row: 10,
            modifiers: KeyModifiers::NONE,
        },
    ))
    .unwrap();
    assert!(matches!(app.modal, Some(Modal::Help { scroll: 3 })));
    assert_eq!(app.scroll, 0);
}

#[test]
fn empty_research_pages_explain_the_next_action_at_minimum_size() {
    let (_dir, mut app) = app();
    for page in [Page::Research, Page::Sources, Page::Lab] {
        app.page = page;
        let output = draw(&mut app, 60, 18);
        assert!(
            output.contains(match page {
                Page::Lab => "b  run",
                _ => "n to",
            }),
            "next action hidden on {page:?}"
        );
        assert!(!output.contains("witha"));
        assert!(!output.contains("hypothesisa"));
    }
    seed(&mut app);
    app.current.as_mut().unwrap().sources.clear();
    app.page = Page::Sources;
    let output = draw(&mut app, 60, 18);
    assert!(output.contains("No evidence was retrieved"));
    assert!(output.contains("Press 2"));
}

#[test]
fn deleting_word_after_unicode_whitespace_preserves_valid_text() {
    for separator in [' ', '\u{2003}', '\u{3000}', '\n'] {
        let mut input = Input::new(format!("研究{separator}word"));
        input.key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL));
        assert_eq!(input.text, format!("研究{separator}"));
        assert_eq!(input.cursor, input.text.len());
        input.key(key(KeyCode::Backspace));
        assert_eq!(input.text, "研究");
    }
}
