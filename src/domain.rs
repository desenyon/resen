use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    #[default]
    OpenAi,
    Anthropic,
    Ollama,
    Compatible,
    Codex,
    Claude,
}

impl ProviderKind {
    pub const ALL: [Self; 6] = [
        Self::OpenAi,
        Self::Anthropic,
        Self::Ollama,
        Self::Compatible,
        Self::Codex,
        Self::Claude,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::OpenAi => "OpenAI",
            Self::Anthropic => "Anthropic",
            Self::Ollama => "Ollama",
            Self::Compatible => "Compatible endpoint",
            Self::Codex => "Codex CLI",
            Self::Claude => "Claude Code",
        }
    }
    pub fn defaults(self) -> (&'static str, &'static str) {
        match self {
            Self::OpenAi => ("https://api.openai.com/v1", "gpt-4.1"),
            Self::Anthropic => ("https://api.anthropic.com/v1", "claude-sonnet-5-5"),
            Self::Ollama => ("http://localhost:11434/v1", "qwen3:8b"),
            Self::Compatible => ("http://localhost:1234/v1", "local-model"),
            Self::Codex | Self::Claude => ("", ""),
        }
    }
    pub fn key_name(self) -> &'static str {
        match self {
            Self::Anthropic => "ANTHROPIC_API_KEY",
            Self::Compatible => "RESEN_MODEL_API_KEY",
            Self::Ollama => "OLLAMA_API_KEY",
            _ => "OPENAI_API_KEY",
        }
    }
    pub fn is_cli(self) -> bool {
        matches!(self, Self::Codex | Self::Claude)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResearchKind {
    #[default]
    Company,
    Comparison,
    Macro,
    Strategy,
    Custom,
}
impl ResearchKind {
    pub const ALL: [Self; 5] = [
        Self::Company,
        Self::Comparison,
        Self::Macro,
        Self::Strategy,
        Self::Custom,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Company => "Company deep dive",
            Self::Comparison => "Relative value",
            Self::Macro => "Macro outlook",
            Self::Strategy => "Strategy research",
            Self::Custom => "Open research",
        }
    }
    pub fn prompt(self) -> &'static str {
        match self {
            Self::Company => {
                "Analyze the business, competitive position, valuation considerations, catalysts and downside risks. Separate evidence from assumptions and identify what would invalidate the thesis."
            }
            Self::Comparison => {
                "Compare these assets: business quality, valuation considerations, catalysts, exposures and risks. Provide a decision matrix and explain missing evidence."
            }
            Self::Macro => {
                "Assess inflation, interest rates, economic growth and the market regime. Discuss scenarios, transmission mechanisms and evidence that would change the outlook."
            }
            Self::Strategy => {
                "Develop a testable strategy hypothesis with explicit signals, universe, rebalance rules, costs and failure conditions. Discuss look-ahead bias, overfitting, survivorship and a LEAN validation plan."
            }
            Self::Custom => {
                "Answer the research question using the supplied evidence. Explain uncertainty and what further investigation is needed."
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResearchRequest {
    pub kind: ResearchKind,
    pub symbols: Vec<String>,
    pub question: String,
    pub prior: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Source {
    pub id: usize,
    pub title: String,
    pub url: String,
    pub publisher: String,
    pub retrieved_at: DateTime<Utc>,
    pub as_of: Option<String>,
    pub content: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResearchRun {
    pub id: String,
    pub request: ResearchRequest,
    pub created_at: DateTime<Utc>,
    pub provider: String,
    pub model: String,
    pub status: String,
    pub report: String,
    pub sources: Vec<Source>,
    pub warnings: Vec<String>,
    pub elapsed_ms: u64,
    pub demo: bool,
}
impl ResearchRun {
    pub fn new(request: ResearchRequest, provider: String, model: String, demo: bool) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            request,
            created_at: Utc::now(),
            provider,
            model,
            status: "running".into(),
            report: String::new(),
            sources: vec![],
            warnings: vec![],
            elapsed_ms: 0,
            demo,
        }
    }
    pub fn markdown(&self) -> String {
        let mut out = format!(
            "# {}\n\n{}\n\n- Run: {}\n- Created: {}\n- Provider: {} / {}\n- Status: {}\n- Mode: {}\n\n{}\n\n## Source ledger\n\n",
            self.request.kind.label(),
            self.request.question,
            self.id,
            self.created_at.to_rfc3339(),
            self.provider,
            if self.model.is_empty() {
                "CLI default (not reported)"
            } else {
                &self.model
            },
            self.status,
            if self.demo {
                "DEMO — fictional fixture, not market evidence"
            } else {
                "Connected research — see source dates and provenance"
            },
            self.report
        );
        for source in &self.sources {
            out.push_str(&format!(
                "[{}] {} — {}\n{}\nRetrieved: {} | As of: {}\n\n",
                source.id,
                source.title,
                source.publisher,
                source.url,
                source.retrieved_at.to_rfc3339(),
                source.as_of.as_deref().unwrap_or("not supplied")
            ));
        }
        if !self.warnings.is_empty() {
            out.push_str("## Limitations\n\n");
            for w in &self.warnings {
                out.push_str(&format!("- {w}\n"));
            }
        }
        out
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Bar {
    pub date: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarketSeries {
    pub symbol: String,
    pub bars: Vec<Bar>,
    pub source: String,
    pub retrieved_at: DateTime<Utc>,
    pub demo: bool,
}
impl MarketSeries {
    pub fn last(&self) -> Option<&Bar> {
        self.bars.last()
    }
    pub fn change_pct(&self) -> Option<f64> {
        let n = self.bars.len();
        if n < 2 {
            return None;
        }
        Some((self.bars[n - 1].close / self.bars[n - 2].close - 1.0) * 100.0)
    }
}

pub fn parse_symbols(input: &str) -> anyhow::Result<Vec<String>> {
    let mut result = Vec::new();
    for raw in input
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|s| !s.is_empty())
    {
        let symbol = raw.trim_start_matches('$').to_ascii_uppercase();
        anyhow::ensure!(
            symbol.len() <= 15
                && symbol
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '^')),
            "Invalid symbol: {raw}. Use letters, digits, dots or dashes."
        );
        if !result.contains(&symbol) {
            result.push(symbol);
        }
    }
    anyhow::ensure!(result.len() <= 6, "Research up to six symbols at a time.");
    Ok(result)
}
