use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use crossterm::{
    event::{
        DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        EventStream,
    },
    execute,
};
use futures_util::StreamExt;
use ratatui::{Terminal, backend::TestBackend};
use resen::{
    app::{App, JobEvent, Page},
    backtest::{StrategyParams, simulate},
    config::{Paths, atomic_write},
    domain::{MarketSeries, ResearchKind, ResearchRequest},
    research::ResearchEvent,
};
use std::{io::IsTerminal, path::PathBuf};

#[derive(Parser)]
#[command(
    name = "resen",
    version,
    about = "A terminal research desk. Your models, your evidence, your edge.",
    long_about = "Resen is a terminal-native financial research desk with model connections, source-led memos, daily price charts, local history and LEAN backtesting. Run without a command for the full interface."
)]
struct Cli {
    #[arg(
        long,
        global = true,
        help = "Explore synthetic fixtures without contacting providers"
    )]
    demo: bool,
    #[arg(long, global = true, help = "Use an explicit local state directory")]
    data_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Commands>,
}
#[derive(Clone, Copy, ValueEnum)]
enum Kind {
    Company,
    Comparison,
    Macro,
    Strategy,
    Custom,
}
impl From<Kind> for ResearchKind {
    fn from(kind: Kind) -> Self {
        match kind {
            Kind::Company => Self::Company,
            Kind::Comparison => Self::Comparison,
            Kind::Macro => Self::Macro,
            Kind::Strategy => Self::Strategy,
            Kind::Custom => Self::Custom,
        }
    }
}
#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Markdown,
    Json,
}
#[derive(Clone, Copy, ValueEnum)]
enum View {
    Desk,
    Research,
    Sources,
    Lab,
    Archive,
    Connections,
    Welcome,
    Setup,
    Palette,
    Import,
}
#[derive(Subcommand)]
enum Commands {
    /// Import a daily OHLCV CSV for charts, research and historical studies.
    ImportCsv {
        path: PathBuf,
        #[arg(long)]
        symbol: String,
    },
    /// Open the guided connection setup.
    Setup,
    /// Inspect local readiness; --online checks the model endpoint without generating text.
    Doctor {
        #[arg(long)]
        online: bool,
    },
    /// Run one source-led question and print its memo.
    Research {
        question: String,
        #[arg(long, default_value = "SPY")]
        symbols: String,
        #[arg(long, value_enum, default_value = "company")]
        kind: Kind,
    },
    /// List saved research runs.
    History,
    /// Export a saved run with its provenance.
    Export {
        id: String,
        #[arg(long, value_enum, default_value = "markdown")]
        format: Format,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Run a transparent long/cash SMA study on daily OHLCV.
    Backtest {
        #[arg(long, default_value = "SPY")]
        symbol: String,
        #[arg(long)]
        csv: Option<PathBuf>,
        #[arg(long, default_value_t = 20)]
        fast: usize,
        #[arg(long, default_value_t = 60)]
        slow: usize,
        #[arg(long, default_value_t = 10.0)]
        cost_bps: f64,
        #[arg(long, default_value_t = 100000.0)]
        capital: f64,
    },
    /// Render the real terminal buffer to an SVG (offline sample data).
    Snapshot {
        #[arg(long, value_enum, default_value = "desk")]
        page: View,
        #[arg(long, default_value_t = 144)]
        width: u16,
        #[arg(long, default_value_t = 46)]
        height: u16,
        #[arg(long)]
        output: PathBuf,
    },
    /// Create a LEAN Python or C# project for manual review.
    LeanCreate {
        #[arg(long,value_parser=["python","csharp"],default_value="python")]
        language: String,
    },
    /// Execute a reviewed project in the configured LEAN workspace.
    LeanRun { project: PathBuf },
    /// Read cloud backtests; optionally retrieve one full result.
    Cloud {
        #[arg(long)]
        backtest_id: Option<String>,
    },
}

#[tokio::main]
async fn main() {
    if let Err(error) = execute_cli(Cli::parse()).await {
        eprintln!("resen: {error:#}");
        std::process::exit(1);
    }
}
async fn execute_cli(cli: Cli) -> Result<()> {
    let paths = Paths::new(cli.data_dir)?;
    let mut app = App::new(paths, cli.demo)?;
    let interactive = matches!(cli.command, None | Some(Commands::Setup));
    let operation = async {
        match cli.command {
            Some(Commands::ImportCsv { path, symbol }) => {
                app.import_csv(&symbol, &path)?;
                println!("Imported {symbol}. Market source is now local CSV.");
                Ok(())
            }
            None => run_tui(&mut app).await,
            Some(Commands::Setup) => {
                app.settings(true);
                run_tui(&mut app).await
            }
            Some(Commands::Doctor { online }) => {
                println!(
                    "Resen {}\nState: {}\nProvider: {}\nModel: {}\nDaily data: {}\nWeb search: {}\nSetup: {}\nResearch history: {} runs",
                    env!("CARGO_PKG_VERSION"),
                    app.paths.root.display(),
                    app.config.provider.label(),
                    app.config.model,
                    app.config.data_provider,
                    app.config.search_provider,
                    if app.config.setup_complete {
                        "complete"
                    } else {
                        "run resen setup"
                    },
                    app.runs.len()
                );
                app.config.validate()?;
                println!("Configuration: valid");
                for name in ["codex", "claude", "lean", "docker"] {
                    let result = tokio::time::timeout(
                        std::time::Duration::from_secs(5),
                        tokio::process::Command::new(name)
                            .arg("--version")
                            .kill_on_drop(true)
                            .output(),
                    )
                    .await;
                    println!(
                        "{name}: {}",
                        if matches!(result,Ok(Ok(ref o))if o.status.success()) {
                            "available"
                        } else {
                            "not available"
                        }
                    );
                }
                println!(
                    "Credentials: environment first, then local credentials.json (not encrypted).\nNo model generation or trading performed."
                );
                if online {
                    anyhow::ensure!(!app.demo, "Online checks are disabled in demo mode");
                    println!(
                        "{}",
                        resen::provider::check(&app.config, &app.secrets)
                            .await
                            .map_err(|e| anyhow::anyhow!(app.secrets.redact(&format!("{e:#}"))))?
                    );
                }
                Ok(())
            }
            Some(Commands::Research {
                question,
                symbols,
                kind,
            }) => {
                let request = ResearchRequest {
                    kind: kind.into(),
                    symbols: resen::domain::parse_symbols(&symbols)?,
                    question,
                    prior: None,
                };
                anyhow::ensure!(
                    !request.question.trim().is_empty(),
                    "Research question cannot be blank"
                );
                app.start_research(request)?;
                let interrupt = tokio::signal::ctrl_c();
                tokio::pin!(interrupt);
                loop {
                    tokio::select! {
                        Some(event) = app.rx.recv() => {
                            let done = matches!(event.kind(), JobEvent::Research(e) if matches!(**e, ResearchEvent::Finished(_)));
                            if let JobEvent::Research(e) = event.kind()
                                && let ResearchEvent::Phase(phase) = &**e {
                                eprintln!("resen: {phase}");
                            }
                            app.apply_job(event)?;
                            if done { break; }
                        }
                        _ = &mut interrupt => {
                            let result = app.cancel();
                            app.finish_cancelled().await;
                            result?;
                            anyhow::bail!("Research cancelled; partial work saved");
                        }
                    }
                }
                let run = app
                    .current
                    .as_ref()
                    .context("Research did not return a result")?;
                print!("{}", run.markdown());
                anyhow::ensure!(
                    run.status == "complete",
                    "Research failed; the run and limitations were saved in your archive"
                );
                Ok(())
            }
            Some(Commands::History) => {
                for run in &app.runs {
                    println!(
                        "{}  {}  {:11}  {}  {}{}",
                        run.id,
                        run.created_at.format("%Y-%m-%d %H:%M"),
                        run.status,
                        run.request.kind.label(),
                        run.request.symbols.join(","),
                        if run.demo { "  DEMO" } else { "" }
                    );
                }
                Ok(())
            }
            Some(Commands::Export { id, format, output }) => {
                let run = app.store.get(&id)?;
                let content = match format {
                    Format::Markdown => run.markdown(),
                    Format::Json => serde_json::to_string_pretty(&run)?,
                };
                if let Some(path) = output {
                    atomic_write(&path, &content.into_bytes())?;
                    println!("Exported {}", path.display());
                } else {
                    println!("{content}");
                }
                Ok(())
            }
            Some(Commands::Backtest {
                symbol,
                csv,
                fast,
                slow,
                cost_bps,
                capital,
            }) => {
                let series = if let Some(path) = csv {
                    MarketSeries {
                        symbol,
                        bars: resen::data::parse_stooq(&std::fs::read(path)?)?,
                        source: "User-provided OHLCV CSV; adjustment/currency unknown".into(),
                        retrieved_at: chrono::Utc::now(),
                        demo: false,
                    }
                } else if app.demo {
                    resen::data::demo_market(&symbol)
                } else {
                    resen::data::DataClient::new(app.config.clone(), app.secrets.clone())?
                        .with_imports(app.paths.root.join("imports"))
                        .market(&symbol)
                        .await
                        .map_err(|e| anyhow::anyhow!(app.secrets.redact(&format!("{e:#}"))))?
                };
                println!(
                    "{}",
                    serde_json::to_string_pretty(&simulate(
                        &series,
                        StrategyParams {
                            fast,
                            slow,
                            cost_bps,
                            initial_cash: capital
                        }
                    )?)?
                );
                Ok(())
            }
            Some(Commands::Snapshot {
                page,
                width,
                height,
                output,
            }) => {
                anyhow::ensure!(
                    (20..=300).contains(&width) && (8..=120).contains(&height),
                    "Snapshot size must be 20-300 columns and 8-120 rows"
                );
                app.demo = true;
                app.modal = None;
                app.markets = app
                    .config
                    .watchlist
                    .iter()
                    .map(|s| (s.clone(), resen::data::demo_market(s)))
                    .collect();
                let mut run = resen::domain::ResearchRun::new(
                    ResearchRequest {
                        kind: ResearchKind::Company,
                        symbols: vec!["NVDA".into()],
                        question: "What determines the durability of this business?".into(),
                        prior: None,
                    },
                    "Sample engine".into(),
                    "offline fixture".into(),
                    true,
                );
                run.report = resen::research::demo_report(&run.request.symbols);
                run.status = "complete".into();
                let mut source = resen::data::market_source(&resen::data::demo_market("NVDA"));
                source.id = 1;
                source.url = "demo://synthetic-market".into();
                run.sources.push(source);
                run.warnings
                    .push("Sample research; no live sources retrieved.".into());
                app.runs.insert(0, run.clone());
                app.current = Some(run);
                app.phase = "Research complete".into();
                match page {
                    View::Desk => app.page = Page::Desk,
                    View::Research => app.page = Page::Research,
                    View::Sources => app.page = Page::Sources,
                    View::Lab => {
                        app.page = Page::Lab;
                        app.backtest = Some(simulate(
                            &resen::data::demo_market("NVDA"),
                            StrategyParams::default(),
                        )?);
                    }
                    View::Archive => app.page = Page::Archive,
                    View::Connections => app.page = Page::Connections,
                    View::Welcome => app.modal = Some(resen::app::Modal::Welcome),
                    View::Setup => app.settings(true),
                    View::Palette => {
                        app.modal = Some(resen::app::Modal::Palette {
                            input: resen::app::Input::default(),
                            selected: 0,
                        })
                    }
                    View::Import => {
                        app.modal = Some(resen::app::Modal::Import {
                            symbol: resen::app::Input::new("NVDA"),
                            path: resen::app::Input::new("/path/to/daily-prices.csv"),
                            focus: 1,
                        });
                    }
                }
                let mut terminal = Terminal::new(TestBackend::new(width, height))?;
                terminal.draw(|f| resen::ui::render(f, &mut app))?;
                atomic_write(
                    &output,
                    resen::ui::snapshot_svg(terminal.backend().buffer()).as_bytes(),
                )?;
                println!(
                    "Rendered {} x {} terminal buffer to {}",
                    width,
                    height,
                    output.display()
                );
                Ok(())
            }
            Some(Commands::LeanCreate { language }) => {
                anyhow::ensure!(!app.demo, "Project creation is disabled in demo mode");
                let project = resen::backtest::scaffold(
                    std::path::Path::new(&app.config.lean_workspace),
                    &language,
                )?;
                println!(
                    "Created {}. Review it before running resen lean-run.",
                    project.display()
                );
                Ok(())
            }
            Some(Commands::LeanRun { project }) => {
                app.start_lean(&project.to_string_lossy())?;
                let interrupt = tokio::signal::ctrl_c();
                tokio::pin!(interrupt);
                loop {
                    tokio::select! {
                        Some(event) = app.rx.recv() => {
                            let done = matches!(event.kind(), JobEvent::LeanDone(_));
                            if let JobEvent::LeanLog(line) = event.kind() { eprintln!("{}", app.secrets.redact(line)); }
                            if let JobEvent::LeanDone(Err(error)) = event.kind() { anyhow::bail!("{error}"); }
                            app.apply_job(event)?;
                            if done { break; }
                        }
                        _ = &mut interrupt => {
                            let result = app.cancel();
                            app.finish_cancelled().await;
                            result?;
                            anyhow::bail!("LEAN runner cancelled. Check Docker for a remaining container.");
                        }
                    }
                }
                println!("{}", serde_json::to_string_pretty(&app.lean_stats)?);
                Ok(())
            }
            Some(Commands::Cloud { backtest_id }) => {
                anyhow::ensure!(!app.demo, "Cloud calls are disabled in demo mode");
                let project: u64 = app
                    .config
                    .qc_project_id
                    .parse()
                    .context("Set a QuantConnect project ID in Connections")?;
                let (endpoint, body) = if let Some(id) = backtest_id {
                    (
                        "backtests/read",
                        serde_json::json!({"projectId":project,"backtestId":id}),
                    )
                } else {
                    ("backtests/list", serde_json::json!({"projectId":project}))
                };
                let result = resen::backtest::qc_request(&app.config, &app.secrets, endpoint, body)
                    .await
                    .map_err(|e| anyhow::anyhow!(app.secrets.redact(&format!("{e:#}"))))?;
                println!("{}", serde_json::to_string_pretty(&result)?);
                Ok(())
            }
        }
    };
    let result = tokio::select! {
        result = operation => result,
        signal = async {
            if interactive {
                return std::future::pending::<Result<()>>().await;
            }
            #[cfg(unix)]
            {
                let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
                tokio::select! {
                    result = tokio::signal::ctrl_c() => result?,
                    _ = terminate.recv() => {},
                }
            }
            #[cfg(not(unix))]
            tokio::signal::ctrl_c().await?;
            Ok(())
        } => signal.and(Err(anyhow::anyhow!("Operation interrupted; partial research preserved"))),
    };
    let cancelled = app.cancel();
    app.finish_cancelled().await;
    result.and(cancelled)
}

struct TerminalGuard;
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(
            std::io::stdout(),
            DisableMouseCapture,
            DisableBracketedPaste
        );
        ratatui::restore();
    }
}
async fn run_tui(app: &mut App) -> Result<()> {
    anyhow::ensure!(
        std::io::stdin().is_terminal() && std::io::stdout().is_terminal(),
        "Interactive mode needs a terminal. Use --help for headless commands."
    );
    let _guard = TerminalGuard;
    let mut terminal = ratatui::try_init()?;
    execute!(std::io::stdout(), EnableMouseCapture, EnableBracketedPaste)?;
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(
            std::io::stdout(),
            DisableMouseCapture,
            DisableBracketedPaste
        );
        ratatui::restore();
        previous(info);
    }));
    let mut events = EventStream::new();
    let mut tick = tokio::time::interval(std::time::Duration::from_millis(125));
    let interrupt = tokio::signal::ctrl_c();
    tokio::pin!(interrupt);
    #[cfg(unix)]
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let result = async {
      loop {
        terminal.draw(|f| resen::ui::render(f, app))?;
        if app.quit {
            break;
        }
        tokio::select! {
            Some(event)=events.next()=>match event{Ok(event)=>if let Err(error)=app.handle_event(event){app.notify(format!("{error:#}"),true);},Err(error)=>return Err(error.into())},
            Some(event)=app.rx.recv()=>if let Err(error)=app.apply_job(event){app.notify(format!("{error:#}"),true);},
            _=tick.tick()=>app.on_tick(),
            _=&mut interrupt=>{app.cancel()?;app.quit=true;},
            _=async{#[cfg(unix)]{terminate.recv().await;}#[cfg(not(unix))]{std::future::pending::<()>().await;}}=>{app.cancel()?;app.quit=true;},
        }
      }
      Ok(())
    }.await;
    let cancelled = app.cancel();
    app.finish_cancelled().await;
    result.and(cancelled)
}
