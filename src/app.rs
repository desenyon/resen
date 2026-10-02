use crate::{
    backtest::{BacktestResult, StrategyParams},
    config::{Config, Paths, Secrets, atomic_write},
    data::{DataClient, demo_market},
    domain::{
        MarketSeries, ProviderKind, ResearchKind, ResearchRequest, ResearchRun, parse_symbols,
    },
    research::{self, ResearchEvent},
    store::Store,
};
use anyhow::{Context, Result};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEventKind};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
use tokio::{sync::mpsc, task::JoinHandle};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    Desk,
    Research,
    Sources,
    Lab,
    Archive,
    Connections,
}
impl Page {
    pub const ALL: [Self; 6] = [
        Self::Desk,
        Self::Research,
        Self::Sources,
        Self::Lab,
        Self::Archive,
        Self::Connections,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Desk => "Research desk",
            Self::Research => "Research",
            Self::Sources => "Source ledger",
            Self::Lab => "Strategy lab",
            Self::Archive => "Archive",
            Self::Connections => "Connections",
        }
    }
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|p| *p == self).unwrap_or(0)
    }
}
#[derive(Clone, Default, Debug)]
pub struct Input {
    pub text: String,
    pub cursor: usize,
}
impl Input {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let cursor = text.len();
        Self { text, cursor }
    }
    pub fn insert(&mut self, text: &str) {
        let text = crate::clean_text(text);
        let remaining = 16_000usize.saturating_sub(self.text.len());
        let mut end = remaining.min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        self.text.insert_str(self.cursor, &text[..end]);
        self.cursor += end;
    }
    pub fn key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('a') if ctrl => self.cursor = 0,
            KeyCode::Char('e') if ctrl => self.cursor = self.text.len(),
            KeyCode::Char('u') if ctrl => {
                self.text.drain(..self.cursor);
                self.cursor = 0;
            }
            KeyCode::Char('w') if ctrl => {
                let prefix = &self.text[..self.cursor];
                let trimmed = prefix.trim_end();
                let end = trimmed
                    .rfind(char::is_whitespace)
                    .map(|i| i + 1)
                    .unwrap_or(0);
                self.text.drain(end..self.cursor);
                self.cursor = end;
            }
            KeyCode::Left => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    while !self.text.is_char_boundary(self.cursor) {
                        self.cursor -= 1;
                    }
                }
            }
            KeyCode::Right => {
                if self.cursor < self.text.len() {
                    self.cursor += 1;
                    while !self.text.is_char_boundary(self.cursor) {
                        self.cursor += 1;
                    }
                }
            }
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.text.len(),
            KeyCode::Backspace => {
                if self.cursor > 0 {
                    let end = self.cursor;
                    self.key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
                    self.text.drain(self.cursor..end);
                }
            }
            KeyCode::Delete => {
                if self.cursor < self.text.len() {
                    let start = self.cursor;
                    self.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
                    self.text.drain(start..self.cursor);
                    self.cursor = start;
                }
            }
            KeyCode::Char(c) if !ctrl && !key.modifiers.contains(KeyModifiers::ALT) => {
                self.insert(&c.to_string())
            }
            _ => {}
        }
    }
}

#[derive(Clone)]
pub struct Field {
    pub label: &'static str,
    pub input: Input,
    pub secret: bool,
    pub hint: String,
}
#[derive(Clone)]
pub struct SettingsForm {
    pub config: Config,
    pub secrets: Secrets,
    pub fields: Vec<Field>,
    pub focus: usize,
    pub wizard: bool,
    pub step: usize,
}
impl SettingsForm {
    pub fn new(config: &Config, secrets: &Secrets, wizard: bool) -> Self {
        let mut form = Self {
            config: config.clone(),
            secrets: secrets.clone(),
            fields: vec![],
            focus: 0,
            wizard,
            step: 0,
        };
        let values = [
            config.provider.label().to_string(),
            config.model.clone(),
            config.endpoint.clone(),
            String::new(),
            config.data_provider.clone(),
            String::new(),
            config.search_provider.clone(),
            String::new(),
            String::new(),
            config.sec_contact.clone(),
            config.lean_workspace.clone(),
            config.qc_user_id.clone(),
            String::new(),
            config.qc_project_id.clone(),
            config.max_output_tokens.to_string(),
            config.watchlist.join(", "),
            String::new(),
        ];
        let labels = [
            "Model provider",
            "Model name",
            "Base endpoint",
            "Model API key",
            "Market data",
            "Market API key",
            "Web search",
            "Search API key",
            "FRED API key",
            "SEC contact email",
            "LEAN workspace",
            "QuantConnect user ID",
            "QuantConnect token",
            "QuantConnect project ID",
            "Output token cap",
            "Watchlist",
            "Fundamentals API key",
        ];
        let hints = [
            "Left/right to choose",
            "Editable; CLI blank uses its default",
            "Include /v1; local HTTP is supported",
            "Environment variable or protected local file",
            "Left/right: stooq / alpha_vantage / imported CSV",
            "Key for the selected market data service",
            "Left/right: brave / tavily",
            "Optional: cited web context",
            "Optional: macro observations",
            "Required by SEC fair-access rules",
            "Existing folder with lean.json",
            "Numeric ID from QuantConnect settings",
            "Optional: cloud project/results access",
            "Numeric cloud project ID",
            "256-16384 for API providers; CLI keeps its own limits",
            "Up to six symbols, comma separated",
            "Optional Alpha Vantage company overview; shares the market key if selected",
        ];
        for i in 0..labels.len() {
            form.fields.push(Field {
                label: labels[i],
                input: Input::new(values[i].clone()),
                secret: matches!(i, 3 | 5 | 7 | 8 | 12 | 16),
                hint: hints[i].into(),
            });
        }
        form.refresh_secret_hints();
        form
    }
    fn key_for(&self, index: usize) -> Option<&'static str> {
        match index {
            3 if !self.config.provider.is_cli() => Some(self.config.provider.key_name()),
            5 => match self.fields[4].input.text.as_str() {
                "alpha_vantage" => Some("ALPHAVANTAGE_API_KEY"),
                "stooq" => Some("STOOQ_API_KEY"),
                _ => None,
            },
            16 => Some("ALPHAVANTAGE_API_KEY"),
            7 => Some(if self.fields[6].input.text == "tavily" {
                "TAVILY_API_KEY"
            } else {
                "BRAVE_API_KEY"
            }),
            8 => Some("FRED_API_KEY"),
            12 => Some("QUANTCONNECT_API_TOKEN"),
            _ => None,
        }
    }
    fn refresh_secret_hints(&mut self) {
        for index in [3, 5, 7, 8, 12, 16] {
            if let Some(key) = self.key_for(index) {
                self.fields[index].hint = format!(
                    "{key} • {}",
                    if self.secrets.get(key).is_some() {
                        "configured; blank keeps it"
                    } else {
                        "not connected; optional except hosted model"
                    }
                );
            }
        }
        if self.fields[4].input.text == "csv" {
            self.fields[5].hint =
                "No key needed; import each asset from the command palette.".into();
        }
        if self.config.provider.is_cli() {
            self.fields[2].hint = "The CLI manages its connection; no endpoint is needed.".into();
            self.fields[3].label = "CLI authentication";
            self.fields[3].hint =
                "Sign in through the CLI; Resen uses its existing authentication.".into();
        } else {
            self.fields[2].hint = "Include /v1; local HTTP is supported".into();
            self.fields[3].label = "Model API key";
            self.fields[3].secret = true;
        }
    }
    pub fn range(&self) -> std::ops::Range<usize> {
        if !self.wizard {
            0..17
        } else {
            match self.step {
                0 => 0..4,
                1 => 4..10,
                _ => 10..17,
            }
        }
    }
    pub fn cycle(&mut self, direction: i32) {
        if self.focus == 0 {
            let current = ProviderKind::ALL
                .iter()
                .position(|p| *p == self.config.provider)
                .unwrap_or(0);
            let index = (current as i32 + direction).rem_euclid(6) as usize;
            self.config.select_provider(ProviderKind::ALL[index]);
            self.fields[0].input = Input::new(self.config.provider.label());
            self.fields[1].input = Input::new(self.config.model.clone());
            self.fields[2].input = Input::new(self.config.endpoint.clone());
            self.fields[3].input = Input::default();
        } else if self.focus == 4 {
            let providers = ["stooq", "alpha_vantage", "csv"];
            let current = providers
                .iter()
                .position(|p| *p == self.fields[4].input.text)
                .unwrap_or(0);
            let index = (current as i32 + direction).rem_euclid(3) as usize;
            self.fields[4].input = Input::new(providers[index]);
            self.fields[5].input = Input::default();
        } else if self.focus == 6 {
            self.fields[6].input = Input::new(if self.fields[6].input.text == "brave" {
                "tavily"
            } else {
                "brave"
            });
            self.fields[7].input = Input::default();
        }
        self.refresh_secret_hints();
    }
    pub fn materialize(&self) -> Result<(Config, Secrets)> {
        let mut config = self.config.clone();
        let mut secrets = self.secrets.clone();
        config.model = self.fields[1].input.text.trim().into();
        config.endpoint = self.fields[2]
            .input
            .text
            .trim()
            .trim_end_matches('/')
            .into();
        config.data_provider = self.fields[4].input.text.clone();
        config.search_provider = self.fields[6].input.text.clone();
        config.sec_contact = self.fields[9].input.text.trim().into();
        config.lean_workspace = self.fields[10].input.text.trim().into();
        config.qc_user_id = self.fields[11].input.text.trim().into();
        config.qc_project_id = self.fields[13].input.text.trim().into();
        config.max_output_tokens = self.fields[14]
            .input
            .text
            .parse()
            .context("Output token cap must be a number")?;
        config.watchlist = parse_symbols(&self.fields[15].input.text)?;
        anyhow::ensure!(
            !config.watchlist.is_empty(),
            "Watchlist needs at least one symbol."
        );
        config.setup_complete = true;
        for index in [3, 5, 7, 8, 12, 16] {
            if !self.fields[index].input.text.is_empty()
                && let Some(key) = self.key_for(index)
            {
                secrets.set(key, self.fields[index].input.text.clone());
            }
        }
        config.validate()?;
        Ok((config, secrets))
    }
}

#[derive(Clone)]
pub enum Modal {
    Import {
        symbol: Input,
        path: Input,
        focus: usize,
    },
    Welcome,
    Help,
    Settings(Box<SettingsForm>),
    Compose {
        kind: ResearchKind,
        symbols: Input,
        question: Input,
        focus: usize,
        prior: Option<String>,
    },
    Palette {
        input: Input,
        selected: usize,
    },
    Strategy {
        fields: [Input; 4],
        focus: usize,
    },
    Lean {
        project: Input,
    },
}
#[derive(Debug)]
pub enum JobEvent {
    Tagged {
        generation: u64,
        event: Box<JobEvent>,
    },
    Research(Box<ResearchEvent>),
    Market(Vec<(String, Result<MarketSeries, String>)>),
    Checked(Result<String, String>),
    LeanLog(String),
    LeanDone(Result<BTreeMap<String, String>, String>),
    Cloud(Result<String, String>),
}
impl JobEvent {
    pub fn kind(&self) -> &Self {
        match self {
            Self::Tagged { event, .. } => event.kind(),
            _ => self,
        }
    }
}
#[derive(Clone)]
pub struct JobSender {
    tx: mpsc::UnboundedSender<JobEvent>,
    generation: u64,
}
impl JobSender {
    pub fn send(&self, event: JobEvent) -> Result<(), mpsc::error::SendError<JobEvent>> {
        self.tx.send(JobEvent::Tagged {
            generation: self.generation,
            event: Box::new(event),
        })
    }
}
pub struct App {
    pub paths: Paths,
    pub config: Config,
    pub secrets: Secrets,
    pub store: Store,
    pub demo: bool,
    pub page: Page,
    pub modal: Option<Modal>,
    pub markets: BTreeMap<String, MarketSeries>,
    pub market_errors: BTreeMap<String, String>,
    pub selected: usize,
    pub scroll: u16,
    pub report_lines: usize,
    pub source_selected: usize,
    pub archive_selected: usize,
    pub archive_filter: Input,
    pub archive_search: bool,
    pub runs: Vec<ResearchRun>,
    pub current: Option<ResearchRun>,
    pub phase: String,
    pub events: Vec<String>,
    pub backtest: Option<BacktestResult>,
    pub strategy_params: StrategyParams,
    pub lean_stats: BTreeMap<String, String>,
    pub lab_tab: usize,
    pub toast: Option<(String, Instant, bool)>,
    pub tick: u64,
    pub quit: bool,
    pub job: Option<JoinHandle<()>>,
    retired_jobs: Vec<JoinHandle<()>>,
    pub job_started: Option<Instant>,
    job_generation: u64,
    pub tx: mpsc::UnboundedSender<JobEvent>,
    pub rx: mpsc::UnboundedReceiver<JobEvent>,
}
impl Drop for App {
    fn drop(&mut self) {
        if let Some(job) = &self.job {
            job.abort();
        }
        for job in &self.retired_jobs {
            job.abort();
        }
    }
}
pub const COMMANDS: [(&str, &str); 15] = [
    ("New company deep dive", "research"),
    ("Compare assets", "compare"),
    ("Macro outlook", "macro"),
    ("Strategy research", "strategy"),
    ("Open research", "custom"),
    ("Refresh market data", "refresh"),
    ("Edit connections", "settings"),
    ("Test model connection", "check"),
    ("Run historical strategy", "backtest"),
    ("Create LEAN Python project", "python"),
    ("Create LEAN C# project", "csharp"),
    ("Run reviewed LEAN project", "lean"),
    ("Read QuantConnect cloud backtests", "cloud"),
    ("Export current research", "export"),
    ("Import price CSV", "import"),
];

impl App {
    pub fn new(paths: Paths, demo: bool) -> Result<Self> {
        let (mut config, secrets) = paths.load()?;
        if !paths.root.join("config.toml").exists() {
            for (name, provider) in [
                ("codex", ProviderKind::Codex),
                ("claude", ProviderKind::Claude),
            ] {
                let available = std::env::var_os("PATH").is_some_and(|path| {
                    std::env::split_paths(&path).any(|directory| directory.join(name).is_file())
                });
                if available {
                    config.select_provider(provider);
                    break;
                }
            }
        }
        let store = Store::open(&paths)?;
        store.recover_interrupted()?;
        let runs = store.list()?;
        let (tx, rx) = mpsc::unbounded_channel();
        let modal = if !demo && !config.setup_complete {
            Some(Modal::Welcome)
        } else {
            None
        };
        let markets = if demo {
            config
                .watchlist
                .iter()
                .map(|s| (s.clone(), demo_market(s)))
                .collect()
        } else {
            BTreeMap::new()
        };
        Ok(Self {
            paths,
            config,
            secrets,
            store,
            demo,
            page: Page::Desk,
            modal,
            markets,
            market_errors: BTreeMap::new(),
            selected: 0,
            scroll: 0,
            report_lines: 0,
            source_selected: 0,
            archive_selected: 0,
            archive_filter: Input::default(),
            archive_search: false,
            runs,
            current: None,
            phase: "Ready for a question".into(),
            events: vec![],
            backtest: None,
            strategy_params: StrategyParams::default(),
            lean_stats: BTreeMap::new(),
            lab_tab: 0,
            toast: None,
            tick: 0,
            quit: false,
            job: None,
            retired_jobs: Vec::new(),
            job_started: None,
            job_generation: 0,
            tx,
            rx,
        })
    }
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }
    pub fn notify(&mut self, text: impl Into<String>, error: bool) {
        self.toast = Some((self.secrets.redact(&text.into()), Instant::now(), error));
    }
    pub fn selected_symbol(&self) -> &str {
        self.config
            .watchlist
            .get(self.selected)
            .map(String::as_str)
            .unwrap_or("SPY")
    }
    pub fn palette_matches(query: &str) -> Vec<usize> {
        COMMANDS
            .iter()
            .enumerate()
            .filter(|(_, (label, _))| label.to_lowercase().contains(&query.to_lowercase()))
            .map(|(i, _)| i)
            .collect()
    }
    pub fn filtered_runs(&self) -> Vec<usize> {
        let q = self.archive_filter.text.to_lowercase();
        self.runs
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                format!(
                    "{} {} {}",
                    r.request.question,
                    r.request.symbols.join(" "),
                    r.request.kind.label()
                )
                .to_lowercase()
                .contains(&q)
            })
            .map(|(i, _)| i)
            .collect()
    }
    pub fn compose(&mut self, kind: ResearchKind, followup: bool) {
        self.modal = Some(Modal::Compose {
            kind,
            symbols: Input::new(if kind == ResearchKind::Macro {
                String::new()
            } else if followup {
                self.current
                    .as_ref()
                    .map(|r| r.request.symbols.join(", "))
                    .unwrap_or_else(|| self.selected_symbol().into())
            } else {
                self.selected_symbol().into()
            }),
            question: Input::new(if followup {
                String::new()
            } else {
                kind.prompt().into()
            }),
            focus: 0,
            prior: if followup {
                self.current.as_ref().map(|r| r.report.clone())
            } else {
                None
            },
        });
    }
    pub fn handle_event(&mut self, event: Event) -> Result<()> {
        match event {
            Event::Key(key) if key.kind != crossterm::event::KeyEventKind::Release => {
                self.handle_key(key)?
            }
            Event::Paste(text) => self.paste(&text),
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::ScrollDown => self.scroll = self.scroll.saturating_add(3),
                MouseEventKind::ScrollUp => self.scroll = self.scroll.saturating_sub(3),
                _ => {}
            },
            _ => {}
        }
        Ok(())
    }
    fn paste(&mut self, text: &str) {
        if let Some(modal) = &mut self.modal {
            match modal {
                Modal::Import {
                    symbol,
                    path,
                    focus,
                } => {
                    if *focus == 0 {
                        symbol.insert(&text.replace(['\n', '\t'], " "));
                    } else {
                        path.insert(&text.replace(['\n', '\t'], " "));
                    }
                }
                Modal::Compose {
                    symbols,
                    question,
                    focus,
                    ..
                } => {
                    if *focus == 1 {
                        symbols.insert(&text.replace(['\n', '\t'], " "))
                    } else if *focus == 2 {
                        question.insert(text)
                    }
                }
                Modal::Settings(form) => {
                    if !(matches!(form.focus, 0 | 4 | 6)
                        || form.focus == 5 && form.fields[4].input.text == "csv"
                        || matches!(form.focus, 2 | 3) && form.config.provider.is_cli())
                    {
                        form.fields[form.focus]
                            .input
                            .insert(&text.replace(['\n', '\t'], " "))
                    }
                }
                Modal::Palette { input, .. } => input.insert(&text.replace(['\n', '\t'], " ")),
                Modal::Strategy { fields, focus } => {
                    fields[*focus].insert(&text.replace(['\n', '\t'], " "))
                }
                Modal::Lean { project } => project.insert(&text.replace(['\n', '\t'], " ")),
                _ => {}
            }
        } else if self.archive_search {
            self.archive_filter.insert(text);
        }
    }
    pub fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && key.code == KeyCode::Char('c') {
            if self.busy() {
                self.cancel()?;
            } else {
                self.quit = true;
            }
            return Ok(());
        }
        if self.modal.is_some() {
            return self.modal_key(key);
        }
        if self.archive_search {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => self.archive_search = false,
                _ => {
                    self.archive_filter.key(key);
                    self.archive_selected = 0;
                }
            }
            return Ok(());
        }
        match key.code {
            KeyCode::Char('q') => {
                self.cancel()?;
                self.quit = true;
            }
            KeyCode::Char('k') if ctrl => {
                self.modal = Some(Modal::Palette {
                    input: Input::default(),
                    selected: 0,
                })
            }
            KeyCode::Char(':') => {
                self.modal = Some(Modal::Palette {
                    input: Input::default(),
                    selected: 0,
                })
            }
            KeyCode::Char('?') => self.modal = Some(Modal::Help),
            KeyCode::Char(c @ '1'..='6') => {
                self.page = Page::ALL[c as usize - '1' as usize];
                self.scroll = 0;
            }
            KeyCode::Tab => {
                self.page = Page::ALL[(self.page.index() + 1) % 6];
                self.scroll = 0;
            }
            KeyCode::BackTab => {
                self.page = Page::ALL[(self.page.index() + 5) % 6];
                self.scroll = 0;
            }
            KeyCode::Char('n') => self.compose(ResearchKind::Company, false),
            KeyCode::Char('f') if self.page == Page::Research => {
                self.compose(ResearchKind::Custom, true)
            }
            KeyCode::Char('r') if self.page == Page::Desk => self.refresh()?,
            KeyCode::Char('e') if matches!(self.page, Page::Research | Page::Sources) => {
                self.export()?
            }
            KeyCode::Char('/') if self.page == Page::Archive => {
                self.archive_search = true;
                self.archive_filter = Input::default();
            }
            KeyCode::Char('s') if self.page == Page::Connections => self.settings(false),
            KeyCode::Char('t') if self.page == Page::Connections => {
                self.check_connection(self.config.clone(), self.secrets.clone())?
            }
            KeyCode::Char('b') if self.page == Page::Lab => self.strategy_form(),
            KeyCode::Char('l') if self.page == Page::Lab => {
                self.modal = Some(Modal::Lean {
                    project: Input::default(),
                })
            }
            KeyCode::Char('c') if self.page == Page::Lab => self.cloud()?,
            KeyCode::Left if self.page == Page::Lab => self.lab_tab = (self.lab_tab + 2) % 3,
            KeyCode::Right if self.page == Page::Lab => self.lab_tab = (self.lab_tab + 1) % 3,
            KeyCode::Char('p') if self.page == Page::Lab => self.create_lean("python")?,
            KeyCode::Char('j') | KeyCode::Down => self.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection(-1),
            KeyCode::PageDown => self.scroll = self.scroll.saturating_add(12),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(12),
            KeyCode::Home => self.scroll = 0,
            KeyCode::End => {
                self.scroll = self.report_lines.saturating_sub(10).min(u16::MAX as usize) as u16
            }
            KeyCode::Enter if self.page == Page::Desk => self.compose(ResearchKind::Company, false),
            KeyCode::Enter if self.page == Page::Archive => {
                self.ensure_idle()?;
                if let Some(index) = self.filtered_runs().get(self.archive_selected).copied() {
                    self.current = Some(self.runs[index].clone());
                    self.page = Page::Research;
                    self.scroll = 0;
                    self.source_selected = 0;
                }
            }
            KeyCode::Enter if self.page == Page::Connections => self.settings(false),
            _ => {}
        }
        Ok(())
    }
    fn move_selection(&mut self, direction: i32) {
        let next = |current: usize, len: usize| {
            if direction > 0 {
                (current + 1).min(len.saturating_sub(1))
            } else {
                current.saturating_sub(1)
            }
        };
        match self.page {
            Page::Desk => self.selected = next(self.selected, self.config.watchlist.len()),
            Page::Sources => {
                self.source_selected = next(
                    self.source_selected,
                    self.current.as_ref().map(|r| r.sources.len()).unwrap_or(0),
                );
                self.scroll = 0;
            }
            Page::Archive => {
                self.archive_selected = next(self.archive_selected, self.filtered_runs().len())
            }
            _ => {
                if direction > 0 {
                    self.scroll = self.scroll.saturating_add(1)
                } else {
                    self.scroll = self.scroll.saturating_sub(1)
                }
            }
        }
    }
    pub fn settings(&mut self, wizard: bool) {
        self.modal = Some(Modal::Settings(Box::new(SettingsForm::new(
            &self.config,
            &self.secrets,
            wizard,
        ))));
    }
    fn modal_key(&mut self, key: KeyEvent) -> Result<()> {
        let original = self.modal.clone();
        let result = self.modal_key_inner(key);
        if result.is_err() {
            self.modal = original;
        }
        result
    }
    fn modal_key_inner(&mut self, key: KeyEvent) -> Result<()> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if key.code == KeyCode::Esc {
            self.modal = None;
            return Ok(());
        }
        let modal = self.modal.take().unwrap();
        let mut retain = true;
        let mut modal = modal;
        match &mut modal {
            Modal::Import {
                symbol,
                path,
                focus,
            } => match key.code {
                KeyCode::Tab | KeyCode::BackTab => *focus = 1 - *focus,
                KeyCode::Enter => {
                    self.import_csv(&symbol.text, std::path::Path::new(&path.text))?;
                    retain = false;
                }
                _ => {
                    if *focus == 0 {
                        symbol.key(key)
                    } else {
                        path.key(key)
                    }
                }
            },
            Modal::Welcome => match key.code {
                KeyCode::Enter => {
                    self.settings(true);
                    retain = false;
                }
                KeyCode::Char('d') => {
                    self.demo = true;
                    self.markets = self
                        .config
                        .watchlist
                        .iter()
                        .map(|s| (s.clone(), demo_market(s)))
                        .collect();
                    retain = false;
                }
                _ => {}
            },
            Modal::Help => {
                if key.code == KeyCode::Enter {
                    retain = false;
                }
            }
            Modal::Compose {
                kind,
                symbols,
                question,
                focus,
                prior,
            } => match key.code {
                KeyCode::Tab => *focus = (*focus + 1) % 3,
                KeyCode::BackTab => *focus = (*focus + 2) % 3,
                KeyCode::Left | KeyCode::Right if *focus == 0 => {
                    let index = ResearchKind::ALL.iter().position(|k| k == kind).unwrap();
                    *kind = ResearchKind::ALL
                        [(index + if key.code == KeyCode::Right { 1 } else { 4 }) % 5];
                    *question = Input::new(kind.prompt());
                }
                KeyCode::Enter | KeyCode::Char('r') if ctrl => {
                    let request = ResearchRequest {
                        kind: *kind,
                        symbols: parse_symbols(&symbols.text)?,
                        question: question.text.trim().into(),
                        prior: prior.clone(),
                    };
                    anyhow::ensure!(!request.question.is_empty(), "Enter a research question.");
                    self.start_research(request)?;
                    retain = false;
                }
                KeyCode::Enter => {
                    if *focus == 2 {
                        question.insert("\n")
                    } else {
                        *focus = (*focus + 1) % 3
                    }
                }
                _ => {
                    if *focus == 1 {
                        symbols.key(key)
                    } else if *focus == 2 {
                        question.key(key)
                    }
                }
            },
            Modal::Settings(form) => match key.code {
                KeyCode::Tab | KeyCode::Down => {
                    let range = form.range();
                    form.focus = if form.focus + 1 >= range.end {
                        range.start
                    } else {
                        form.focus + 1
                    };
                }
                KeyCode::BackTab | KeyCode::Up => {
                    let range = form.range();
                    form.focus = if form.focus == range.start {
                        range.end - 1
                    } else {
                        form.focus - 1
                    };
                }
                KeyCode::Left | KeyCode::Right if matches!(form.focus, 0 | 4 | 6) => {
                    form.cycle(if key.code == KeyCode::Right { 1 } else { -1 })
                }
                KeyCode::Char('d') if ctrl && form.fields[form.focus].secret => {
                    if let Some(name) = form.key_for(form.focus) {
                        form.secrets.set(name, String::new());
                    }
                    form.fields[form.focus].input = Input::default();
                    form.refresh_secret_hints();
                }
                KeyCode::Char('t') if ctrl => {
                    let (config, secrets) = form.materialize()?;
                    self.check_connection(config, secrets)?;
                }
                KeyCode::Char('s') if ctrl => {
                    let (config, secrets) = form.materialize()?;
                    self.paths.save(&config, &secrets)?;
                    self.config = config;
                    self.secrets = secrets;
                    self.demo = false;
                    self.markets.clear();
                    self.market_errors.clear();
                    self.selected = 0;
                    self.notify(
                        "Connections saved. Press r on the desk to load data.",
                        false,
                    );
                    retain = false;
                }
                KeyCode::Enter if form.wizard => {
                    if form.step < 2 {
                        form.step += 1;
                        form.focus = form.range().start;
                    } else {
                        let (config, secrets) = form.materialize()?;
                        self.paths.save(&config, &secrets)?;
                        self.config = config;
                        self.secrets = secrets;
                        self.demo = false;
                        self.markets.clear();
                        self.notify(
                            "Your research desk is ready. Press r to load market data.",
                            false,
                        );
                        retain = false;
                    }
                }
                _ => {
                    if !matches!(form.focus, 0 | 4 | 6) {
                        form.fields[form.focus].input.key(key)
                    }
                }
            },
            Modal::Palette { input, selected } => match key.code {
                KeyCode::Down => {
                    *selected = (*selected + 1)
                        .min(Self::palette_matches(&input.text).len().saturating_sub(1))
                }
                KeyCode::Up => *selected = selected.saturating_sub(1),
                KeyCode::Enter => {
                    if let Some(i) = Self::palette_matches(&input.text).get(*selected) {
                        self.command(COMMANDS[*i].1)?;
                    }
                    retain = false;
                }
                _ => {
                    input.key(key);
                    *selected = 0;
                }
            },
            Modal::Strategy { fields, focus } => match key.code {
                KeyCode::Tab | KeyCode::Down => *focus = (*focus + 1) % 4,
                KeyCode::BackTab | KeyCode::Up => *focus = (*focus + 3) % 4,
                KeyCode::Enter => {
                    let params = StrategyParams {
                        fast: fields[0]
                            .text
                            .parse()
                            .context("Fast window must be an integer")?,
                        slow: fields[1]
                            .text
                            .parse()
                            .context("Slow window must be an integer")?,
                        initial_cash: fields[2].text.parse().context("Capital must be a number")?,
                        cost_bps: fields[3].text.parse().context("Cost must be a number")?,
                    };
                    let series = self
                        .markets
                        .get(self.selected_symbol())
                        .context("Load market history on the desk first (r)")?;
                    self.backtest = Some(crate::backtest::simulate(series, params.clone())?);
                    self.strategy_params = params;
                    self.page = Page::Lab;
                    self.lab_tab = 0;
                    self.scroll = 0;
                    self.save_backtest()?;
                    retain = false;
                }
                _ => fields[*focus].key(key),
            },
            Modal::Lean { project } => match key.code {
                KeyCode::Enter => {
                    self.start_lean(&project.text)?;
                    retain = false;
                }
                _ => project.key(key),
            },
        }
        if retain {
            self.modal = Some(modal);
        }
        Ok(())
    }
    pub fn command(&mut self, action: &str) -> Result<()> {
        match action {
            "research" => self.compose(ResearchKind::Company, false),
            "compare" => self.compose(ResearchKind::Comparison, false),
            "macro" => self.compose(ResearchKind::Macro, false),
            "strategy" => self.compose(ResearchKind::Strategy, false),
            "custom" => self.compose(ResearchKind::Custom, false),
            "refresh" => self.refresh()?,
            "settings" => self.settings(false),
            "check" => self.check_connection(self.config.clone(), self.secrets.clone())?,
            "backtest" => self.strategy_form(),
            "python" => self.create_lean("python")?,
            "csharp" => self.create_lean("csharp")?,
            "lean" => {
                self.modal = Some(Modal::Lean {
                    project: Input::default(),
                })
            }
            "cloud" => self.cloud()?,
            "export" => self.export()?,
            "import" => {
                self.modal = Some(Modal::Import {
                    symbol: Input::new(self.selected_symbol()),
                    path: Input::default(),
                    focus: 1,
                })
            }
            _ => {}
        }
        Ok(())
    }
    fn ensure_idle(&self) -> Result<()> {
        anyhow::ensure!(!self.busy(), "A job is already running. Ctrl+C cancels it.");
        Ok(())
    }
    fn job_sender(&mut self) -> JobSender {
        self.job_generation += 1;
        JobSender {
            tx: self.tx.clone(),
            generation: self.job_generation,
        }
    }
    pub fn start_research(&mut self, mut request: ResearchRequest) -> Result<()> {
        self.ensure_idle()?;
        request.question = self.secrets.redact(&request.question);
        request.prior = request.prior.map(|prior| self.secrets.redact(&prior));
        anyhow::ensure!(
            !request.question.trim().is_empty(),
            "Enter a research question."
        );
        anyhow::ensure!(
            request.question.len() <= 16_000,
            "Keep research questions under 16,000 bytes."
        );
        anyhow::ensure!(request.symbols.len() <= 6, "Research up to six symbols.");
        let run = ResearchRun::new(
            request,
            if self.demo {
                "Offline sample engine".into()
            } else {
                self.config.provider.label().into()
            },
            if self.demo {
                "offline fixture".into()
            } else {
                self.config.model.clone()
            },
            self.demo,
        );
        self.store.save(&run)?;
        self.current = Some(run.clone());
        self.phase = "Starting research".into();
        self.events.clear();
        self.page = Page::Research;
        self.scroll = 0;
        self.source_selected = 0;
        let config = self.config.clone();
        let secrets = self.secrets.clone();
        let paths = self.paths.clone();
        let tx = self.job_sender();
        self.job_started = Some(Instant::now());
        self.job = Some(tokio::spawn(async move {
            let (research_tx, mut rx) = mpsc::unbounded_channel();
            let task = research::run(run, config, secrets, &paths, research_tx);
            tokio::pin!(task);
            loop {
                tokio::select! {result=&mut task=>{while let Ok(event)=rx.try_recv() {let _=tx.send(JobEvent::Research(Box::new(event)));}let _=result;break;},Some(event)=rx.recv()=>{let _=tx.send(JobEvent::Research(Box::new(event)));}}
            }
        }));
        Ok(())
    }
    pub fn refresh(&mut self) -> Result<()> {
        self.ensure_idle()?;
        if self.demo {
            self.markets = self
                .config
                .watchlist
                .iter()
                .map(|s| (s.clone(), demo_market(s)))
                .collect();
            self.notify("Demo fixtures refreshed. No network requests made.", false);
            return Ok(());
        }
        let client = DataClient::new(self.config.clone(), self.secrets.clone())?
            .with_imports(self.paths.root.join("imports"));
        let symbols = self.config.watchlist.clone();
        let secrets = self.secrets.clone();
        let tx = self.job_sender();
        self.phase = "Loading daily market history".into();
        self.job_started = Some(Instant::now());
        self.job = Some(tokio::spawn(async move {
            let mut results = vec![];
            for symbol in symbols {
                let result = client
                    .market(&symbol)
                    .await
                    .map_err(|e| secrets.redact(&format!("{e:#}")));
                results.push((symbol, result));
            }
            let _ = tx.send(JobEvent::Market(results));
        }));
        Ok(())
    }
    pub fn check_connection(&mut self, config: Config, secrets: Secrets) -> Result<()> {
        self.ensure_idle()?;
        if self.demo {
            self.notify(
                "Leave demo mode by saving Connections before contacting a provider.",
                false,
            );
            return Ok(());
        }
        let tx = self.job_sender();
        self.phase = "Checking model connection".into();
        self.job_started = Some(Instant::now());
        self.job = Some(tokio::spawn(async move {
            let result = crate::provider::check(&config, &secrets)
                .await
                .map_err(|e| secrets.redact(&format!("{e:#}")));
            let _ = tx.send(JobEvent::Checked(result));
        }));
        Ok(())
    }
    pub fn cancel(&mut self) -> Result<()> {
        self.job_generation += 1;
        if let Some(job) = self.job.take() {
            job.abort();
            self.retired_jobs.push(job);
        }
        if let Some(run) = &mut self.current
            && run.status == "running"
        {
            run.status = "cancelled".into();
            run.warnings
                .push("Cancelled by user; partial work preserved.".into());
            run.elapsed_ms = self
                .job_started
                .map(|s| s.elapsed().as_millis() as u64)
                .unwrap_or(0);
            self.store.save(run)?;
            self.runs = self.store.list()?;
        }
        self.phase = "Cancelled".into();
        while self.rx.try_recv().is_ok() {}
        Ok(())
    }
    pub async fn finish_cancelled(&mut self) {
        let jobs = std::mem::take(&mut self.retired_jobs);
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            for job in jobs {
                let _ = job.await;
            }
        })
        .await;
    }
    pub fn apply_job(&mut self, event: JobEvent) -> Result<()> {
        match event {
            JobEvent::Tagged { generation, event } => {
                if generation == self.job_generation {
                    self.apply_job(*event)?;
                }
            }
            JobEvent::Research(event) => match *event {
                ResearchEvent::Phase(phase) => {
                    self.phase = phase.clone();
                    self.events.push(phase);
                }
                ResearchEvent::Source(source) => {
                    if let Some(run) = &mut self.current {
                        run.sources.push(source);
                        self.store.save(run)?;
                    }
                }
                ResearchEvent::Warning(w) => {
                    self.events.push(format!("! {w}"));
                    if let Some(run) = &mut self.current {
                        run.warnings.push(w);
                        self.store.save(run)?;
                    }
                }
                ResearchEvent::Delta(text) => {
                    if let Some(run) = &mut self.current {
                        run.report.push_str(&text);
                        if self.tick.is_multiple_of(10) {
                            self.store.save(run)?;
                        }
                    }
                }
                ResearchEvent::Finished(run) => {
                    self.phase = if run.status == "complete" {
                        "Research complete".into()
                    } else {
                        "Research needs attention".into()
                    };
                    self.store.save(&run)?;
                    self.notify(self.phase.clone(), run.status != "complete");
                    self.current = Some(run);
                    self.runs = self.store.list()?;
                    self.job = None;
                }
            },
            JobEvent::Market(results) => {
                for (symbol, result) in results {
                    match result {
                        Ok(series) => {
                            self.market_errors.remove(&symbol);
                            self.markets.insert(symbol, series);
                        }
                        Err(error) => {
                            self.market_errors.insert(symbol, error);
                        }
                    }
                }
                self.job = None;
                self.phase = "Daily history refreshed".into();
                self.notify(
                    format!(
                        "{} assets loaded; {} source errors. End-of-day data.",
                        self.markets.len(),
                        self.market_errors.len()
                    ),
                    !self.market_errors.is_empty(),
                );
            }
            JobEvent::Checked(result) => {
                self.job = None;
                match result {
                    Ok(text) => self.notify(text, false),
                    Err(text) => self.notify(text, true),
                }
            }
            JobEvent::LeanLog(line) => {
                self.events.push(self.secrets.redact(&line));
                if self.events.len() > 2000 {
                    self.events.remove(0);
                }
            }
            JobEvent::LeanDone(result) => {
                self.job = None;
                match result {
                    Ok(stats) => {
                        self.lean_stats = stats;
                        self.notify("LEAN backtest complete. Statistics loaded.", false)
                    }
                    Err(e) => self.notify(e, true),
                }
            }
            JobEvent::Cloud(result) => {
                self.job = None;
                match result {
                    Ok(text) => {
                        self.events = text.lines().map(str::to_string).collect();
                        self.notify("QuantConnect cloud results loaded.", false)
                    }
                    Err(e) => self.notify(e, true),
                }
            }
        }
        Ok(())
    }
    pub fn export(&mut self) -> Result<()> {
        let run = self.current.as_ref().context("Open a research run first")?;
        let folder = self.paths.exports();
        std::fs::create_dir_all(&folder)?;
        let path = folder.join(format!("{}.md", run.id));
        atomic_write(&path, run.markdown().as_bytes())?;
        atomic_write(
            &folder.join(format!("{}.json", run.id)),
            &serde_json::to_vec_pretty(run)?,
        )?;
        self.notify(
            format!("Exported Markdown + JSON: {}", path.display()),
            false,
        );
        Ok(())
    }
    pub fn strategy_form(&mut self) {
        let p = &self.strategy_params;
        self.modal = Some(Modal::Strategy {
            fields: [
                Input::new(p.fast.to_string()),
                Input::new(p.slow.to_string()),
                Input::new(p.initial_cash.to_string()),
                Input::new(p.cost_bps.to_string()),
            ],
            focus: 0,
        });
    }
    pub fn import_csv(&mut self, symbol: &str, path: &std::path::Path) -> Result<()> {
        self.ensure_idle()?;
        let symbols = parse_symbols(symbol)?;
        anyhow::ensure!(symbols.len() == 1, "Import one asset at a time.");
        if !self.config.watchlist.contains(&symbols[0]) {
            anyhow::ensure!(
                self.config.watchlist.len() < 6,
                "Your watchlist is full. Edit Connections before importing another symbol."
            );
        }
        let series = crate::data::import_prices(&self.paths.root.join("imports"), symbol, path)?;
        if !self.config.watchlist.contains(&series.symbol) {
            self.config.watchlist.push(series.symbol.clone());
        }
        self.config.data_provider = "csv".into();
        self.paths.save(&self.config, &self.secrets)?;
        self.selected = self
            .config
            .watchlist
            .iter()
            .position(|s| *s == series.symbol)
            .unwrap_or(0);
        self.market_errors.remove(&series.symbol);
        self.notify(format!("Imported {} daily bars for {}. Market source set to CSV; research can use this evidence.", series.bars.len(), series.symbol), false);
        self.markets.retain(|_, series| !series.demo);
        self.markets.insert(series.symbol.clone(), series);
        self.demo = false;
        self.page = Page::Desk;
        Ok(())
    }
    fn save_backtest(&self) -> Result<()> {
        if let Some(result) = &self.backtest {
            atomic_write(
                &self.paths.exports().join("latest-strategy.json"),
                &serde_json::to_vec_pretty(result)?,
            )?;
        }
        Ok(())
    }
    pub fn create_lean(&mut self, language: &str) -> Result<()> {
        self.ensure_idle()?;
        anyhow::ensure!(
            !self.demo,
            "Project creation is disabled in demo mode. Save Connections to leave demo."
        );
        let workspace = std::path::Path::new(&self.config.lean_workspace);
        let project = crate::backtest::scaffold(workspace, language)?;
        self.notify(
            format!(
                "Created {}. Review the code, then use Run LEAN.",
                project.display()
            ),
            false,
        );
        Ok(())
    }
    pub fn start_lean(&mut self, project: &str) -> Result<()> {
        self.ensure_idle()?;
        anyhow::ensure!(
            !self.demo,
            "LEAN execution is disabled in demo mode. Save Connections to leave demo."
        );
        anyhow::ensure!(!project.trim().is_empty(), "Enter a reviewed project path.");
        let workspace = std::path::PathBuf::from(&self.config.lean_workspace);
        let input = std::path::PathBuf::from(project);
        let project = if input.is_absolute() {
            input
        } else {
            workspace.join(input)
        };
        let output = self
            .paths
            .root
            .join("backtests")
            .join(uuid::Uuid::new_v4().to_string());
        let tx = self.job_sender();
        let secrets = self.secrets.clone();
        self.events.clear();
        self.lean_stats.clear();
        self.page = Page::Lab;
        self.lab_tab = 1;
        self.phase = "Running LEAN backtest".into();
        self.job_started = Some(Instant::now());
        self.job = Some(tokio::spawn(async move {
            let (log_tx, mut log_rx) = mpsc::unbounded_channel();
            let task = crate::backtest::lean_run(&workspace, &project, &output, &log_tx);
            tokio::pin!(task);
            loop {
                tokio::select! {result=&mut task=>{while let Ok(line)=log_rx.try_recv(){let _=tx.send(JobEvent::LeanLog(line));}let _=tx.send(JobEvent::LeanDone(result.map_err(|e|secrets.redact(&format!("{e:#}")))));break;},Some(line)=log_rx.recv()=>{let _=tx.send(JobEvent::LeanLog(line));}}
            }
        }));
        Ok(())
    }
    pub fn cloud(&mut self) -> Result<()> {
        self.ensure_idle()?;
        anyhow::ensure!(!self.demo, "Cloud calls are disabled in demo mode.");
        let project: u64 = self
            .config
            .qc_project_id
            .parse()
            .context("Set a numeric QuantConnect project ID in Connections")?;
        let config = self.config.clone();
        let secrets = self.secrets.clone();
        let tx = self.job_sender();
        self.page = Page::Lab;
        self.lab_tab = 2;
        self.events.clear();
        self.phase = "Reading cloud backtests".into();
        self.job_started = Some(Instant::now());
        self.job = Some(tokio::spawn(async move {
            let result = crate::backtest::qc_request(
                &config,
                &secrets,
                "backtests/list",
                serde_json::json!({"projectId":project}),
            )
            .await
            .and_then(|v| Ok(serde_json::to_string_pretty(&v)?))
            .map_err(|e| secrets.redact(&format!("{e:#}")));
            let _ = tx.send(JobEvent::Cloud(result));
        }));
        Ok(())
    }
    pub fn on_tick(&mut self) {
        self.tick += 1;
        if self
            .toast
            .as_ref()
            .is_some_and(|(_, start, _)| start.elapsed() > Duration::from_secs(10))
        {
            self.toast = None;
        }
    }
}
