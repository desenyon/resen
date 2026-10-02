use crate::domain::ProviderKind;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub provider: ProviderKind,
    pub endpoint: String,
    pub model: String,
    pub max_output_tokens: u32,
    pub watchlist: Vec<String>,
    pub setup_complete: bool,
    pub sec_contact: String,
    pub lean_workspace: String,
    pub qc_user_id: String,
    pub qc_project_id: String,
    pub data_provider: String,
    pub search_provider: String,
}
impl Default for Config {
    fn default() -> Self {
        let (endpoint, model) = ProviderKind::OpenAi.defaults();
        Self {
            provider: ProviderKind::OpenAi,
            endpoint: endpoint.into(),
            model: model.into(),
            max_output_tokens: 4096,
            watchlist: vec!["NVDA".into(), "AAPL".into(), "MSFT".into(), "SPY".into()],
            setup_complete: false,
            sec_contact: String::new(),
            lean_workspace: String::new(),
            qc_user_id: String::new(),
            qc_project_id: String::new(),
            data_provider: "stooq".into(),
            search_provider: "brave".into(),
        }
    }
}
impl Config {
    pub fn validate(&self) -> Result<()> {
        if !self.provider.is_cli() {
            validate_endpoint(&self.endpoint)?;
            anyhow::ensure!(!self.model.trim().is_empty(), "Enter a model name.");
        }
        anyhow::ensure!(
            (256..=16384).contains(&self.max_output_tokens),
            "Output tokens must be between 256 and 16384."
        );
        anyhow::ensure!(
            matches!(
                self.data_provider.as_str(),
                "stooq" | "alpha_vantage" | "csv"
            ),
            "Unknown market data provider."
        );
        anyhow::ensure!(
            matches!(self.search_provider.as_str(), "brave" | "tavily"),
            "Unknown search provider."
        );
        let symbols = crate::domain::parse_symbols(&self.watchlist.join(","))?;
        anyhow::ensure!(
            !symbols.is_empty() && symbols == self.watchlist,
            "Watchlist needs 1-6 unique uppercase symbols."
        );
        Ok(())
    }
    pub fn select_provider(&mut self, kind: ProviderKind) {
        self.provider = kind;
        let (endpoint, model) = kind.defaults();
        self.endpoint = endpoint.into();
        self.model = model.into();
    }
}

pub fn validate_endpoint(endpoint: &str) -> Result<()> {
    let url = reqwest::Url::parse(endpoint.trim()).context("Endpoint must be an absolute URL.")?;
    anyhow::ensure!(
        url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "Endpoint must not contain credentials, query parameters, or fragments."
    );
    let local = matches!(
        url.host_str(),
        Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
    );
    anyhow::ensure!(
        url.scheme() == "https" || (url.scheme() == "http" && local),
        "Use HTTPS for remote endpoints; HTTP is allowed on loopback for local models."
    );
    anyhow::ensure!(url.host_str().is_some(), "Endpoint needs a host.");
    Ok(())
}

/// Intentionally has no Debug implementation: never format credentials into logs.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Secrets {
    values: BTreeMap<String, String>,
}
impl Secrets {
    pub fn get(&self, name: &str) -> Option<String> {
        std::env::var(name)
            .ok()
            .filter(|v| !v.trim().is_empty())
            .or_else(|| self.values.get(name).filter(|s| !s.is_empty()).cloned())
    }
    pub fn set(&mut self, name: &str, value: String) {
        if value.trim().is_empty() {
            self.values.remove(name);
        } else {
            self.values.insert(name.into(), value.trim().into());
        }
    }
    pub fn redact(&self, text: &str) -> String {
        redact_original(&crate::clean_text(text), &self.redaction_values(), false).0
    }
    fn redaction_values(&self) -> Vec<String> {
        let mut values: Vec<_> = [
            "OPENAI_API_KEY",
            "ANTHROPIC_API_KEY",
            "OLLAMA_API_KEY",
            "RESEN_MODEL_API_KEY",
            "ALPHAVANTAGE_API_KEY",
            "STOOQ_API_KEY",
            "BRAVE_API_KEY",
            "TAVILY_API_KEY",
            "FRED_API_KEY",
            "QUANTCONNECT_API_TOKEN",
        ]
        .into_iter()
        .filter_map(|name| self.get(name))
        .map(|value| crate::clean_text(&value))
        .filter(|s| !s.is_empty())
        .collect();
        values.sort_by_key(|s| std::cmp::Reverse(s.len()));
        values.dedup();
        values
    }
    pub fn stream_redactor(&self) -> StreamRedactor {
        StreamRedactor {
            secrets: self.redaction_values(),
            pending: String::new(),
        }
    }
}

/// Retains any suffix that could become a secret in the next network chunk.
pub struct StreamRedactor {
    secrets: Vec<String>,
    pending: String,
}
impl StreamRedactor {
    pub fn push(&mut self, text: &str) -> String {
        self.pending.push_str(&crate::clean_text(text));
        let (output, consumed) = redact_original(&self.pending, &self.secrets, true);
        self.pending.drain(..consumed);
        output
    }
    pub fn finish(&mut self) -> String {
        if self.pending.is_empty() {
            String::new()
        } else {
            self.pending.clear();
            "[REDACTED]".into()
        }
    }
}

// Scan only original input: short keys must never match the replacement marker.
fn redact_original(text: &str, secrets: &[String], streaming: bool) -> (String, usize) {
    let mut output = String::with_capacity(text.len());
    let mut offset = 0;
    while offset < text.len() {
        let remaining = &text[offset..];
        if remaining.starts_with("[REDACTED]") {
            output.push_str("[REDACTED]");
            offset += "[REDACTED]".len();
            continue;
        }
        if streaming
            && secrets
                .iter()
                .any(|secret| secret.len() > remaining.len() && secret.starts_with(remaining))
        {
            break;
        }
        if let Some(secret) = secrets
            .iter()
            .find(|secret| remaining.starts_with(secret.as_str()))
        {
            output.push_str("[REDACTED]");
            offset += secret.len();
        } else {
            let character = remaining.chars().next().unwrap();
            output.push(character);
            offset += character.len_utf8();
        }
    }
    (output, offset)
}

#[derive(Clone, Debug)]
pub struct Paths {
    pub root: PathBuf,
}
impl Paths {
    pub fn new(root: Option<PathBuf>) -> Result<Self> {
        let root = root
            .or_else(|| {
                directories::ProjectDirs::from("dev", "resen", "resen")
                    .map(|d| d.data_local_dir().to_path_buf())
            })
            .context("Cannot locate your data directory; supply --data-dir.")?;
        fs::create_dir_all(&root).context("Cannot create data directory")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
        }
        Ok(Self { root })
    }
    pub fn load(&self) -> Result<(Config, Secrets)> {
        let config_path = self.root.join("config.toml");
        let config: Config = if config_path.exists() {
            toml::from_str(&fs::read_to_string(config_path)?)
                .context("Invalid config.toml; fix the file or use a different --data-dir")?
        } else {
            Config::default()
        };
        config
            .validate()
            .context("Invalid configuration; fix config.toml or use a different --data-dir")?;
        let secrets_path = self.root.join("credentials.json");
        let secrets = if secrets_path.exists() {
            serde_json::from_str(&fs::read_to_string(secrets_path)?).map_err(|_| {
                anyhow::anyhow!("Invalid credentials.json; expected a string key map.")
            })?
        } else {
            Secrets::default()
        };
        Ok((config, secrets))
    }
    pub fn save(&self, config: &Config, secrets: &Secrets) -> Result<()> {
        config.validate()?;
        atomic_write(
            &self.root.join("credentials.json"),
            &serde_json::to_vec(secrets)?,
        )?;
        atomic_write(
            &self.root.join("config.toml"),
            toml::to_string_pretty(config)?.as_bytes(),
        )
    }
    pub fn exports(&self) -> PathBuf {
        self.root.join("exports")
    }
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .context("Destination needs a parent directory")?;
    fs::create_dir_all(parent)?;
    let temp = parent.join(format!(".resen-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}
