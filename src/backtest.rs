use crate::{
    config::{Config, Secrets, atomic_write},
    data::{http_client, response_json},
    domain::MarketSeries,
};
use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{io::BufReader, process::Command, sync::mpsc};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StrategyParams {
    pub fast: usize,
    pub slow: usize,
    pub initial_cash: f64,
    pub cost_bps: f64,
}
impl Default for StrategyParams {
    fn default() -> Self {
        Self {
            fast: 20,
            slow: 60,
            initial_cash: 100_000.0,
            cost_bps: 10.0,
        }
    }
}
impl StrategyParams {
    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.fast > 0 && self.fast < self.slow && self.slow <= 252,
            "Windows must satisfy 0 < fast < slow <= 252."
        );
        anyhow::ensure!(
            self.initial_cash.is_finite() && self.initial_cash > 0.0,
            "Starting capital must be positive."
        );
        anyhow::ensure!(
            self.cost_bps.is_finite() && (0.0..=500.0).contains(&self.cost_bps),
            "Trading cost must be 0-500 basis points."
        );
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EquityPoint {
    pub date: String,
    pub equity: f64,
    pub benchmark: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BacktestResult {
    pub symbol: String,
    pub params: StrategyParams,
    pub points: Vec<EquityPoint>,
    pub total_return: f64,
    pub benchmark_return: f64,
    pub cagr: Option<f64>,
    pub sharpe: Option<f64>,
    pub max_drawdown: f64,
    pub trades: usize,
    pub fees: f64,
    pub exposure: f64,
    pub source: String,
    pub demo: bool,
}

/// Signals use only prior closes and execute at the next open. Long/cash, no leverage.
pub fn simulate(series: &MarketSeries, params: StrategyParams) -> Result<BacktestResult> {
    params.validate()?;
    anyhow::ensure!(
        series.bars.len() > params.slow + 2,
        "Need more than {} daily bars; available {}.",
        params.slow + 2,
        series.bars.len()
    );
    for bar in &series.bars {
        crate::data::validate_bar(bar)?;
    }
    anyhow::ensure!(
        series.bars.windows(2).all(|w| w[0].date < w[1].date),
        "History must be strictly chronological."
    );
    let cost = params.cost_bps / 10_000.0;
    let mut cash = params.initial_cash;
    let mut units = 0.0;
    let benchmark_units = params.initial_cash / (series.bars[params.slow].open * (1.0 + cost));
    anyhow::ensure!(
        benchmark_units.is_finite() && benchmark_units > 0.0,
        "Price/capital scale exceeds the simulator's numeric range."
    );
    let mut peak = cash;
    let mut drawdown: f64 = 0.0;
    let mut fees = 0.0;
    let mut trades = 0;
    let mut exposed = 0;
    let mut points = Vec::new();
    let mut returns = Vec::new();
    let mut previous = cash;
    for i in params.slow..series.bars.len() {
        let bar = &series.bars[i];
        let mean = |window: usize| {
            let mut average = 0.0;
            for (count, bar) in series.bars[i - window..i].iter().enumerate() {
                average += (bar.close - average) / (count + 1) as f64;
            }
            average
        };
        let long = mean(params.fast) > mean(params.slow);
        if long && units == 0.0 {
            units = cash / (bar.open * (1.0 + cost));
            let fee = units * bar.open * cost;
            fees += fee;
            cash = 0.0;
            trades += 1;
        } else if !long && units > 0.0 {
            let proceeds = units * bar.open;
            let fee = proceeds * cost;
            fees += fee;
            cash = proceeds - fee;
            units = 0.0;
            trades += 1;
        }
        if units > 0.0 {
            exposed += 1;
        }
        // Both portfolios liquidate at the final close; exits pay the same costs.
        if i == series.bars.len() - 1 && units > 0.0 {
            let proceeds = units * bar.close;
            let fee = proceeds * cost;
            fees += fee;
            cash += proceeds - fee;
            units = 0.0;
            trades += 1;
        }
        let equity = cash + units * bar.close;
        let benchmark = benchmark_units
            * bar.close
            * if i == series.bars.len() - 1 {
                1.0 - cost
            } else {
                1.0
            };
        anyhow::ensure!(
            equity.is_finite()
                && equity > 0.0
                && benchmark.is_finite()
                && benchmark > 0.0
                && fees.is_finite(),
            "Price/capital scale exceeds the simulator's numeric range."
        );
        peak = peak.max(equity);
        drawdown = drawdown.max(1.0 - equity / peak);
        returns.push(equity / previous - 1.0);
        previous = equity;
        points.push(EquityPoint {
            date: bar.date.clone(),
            equity,
            benchmark,
        });
    }
    let end = points.last().context("Empty backtest")?;
    let total_return = end.equity / params.initial_cash - 1.0;
    let benchmark_return = end.benchmark / params.initial_cash - 1.0;
    let mean = returns.iter().sum::<f64>() / returns.len() as f64;
    let variance =
        returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (returns.len() - 1) as f64;
    anyhow::ensure!(
        mean.is_finite()
            && variance.is_finite()
            && total_return.is_finite()
            && benchmark_return.is_finite(),
        "Returns exceed the simulator's numeric range."
    );
    let sharpe = if variance > 1e-16 {
        Some(mean / variance.sqrt() * 252f64.sqrt())
    } else {
        None
    };
    let first = chrono::NaiveDate::parse_from_str(&points[0].date, "%Y-%m-%d")?;
    let last = chrono::NaiveDate::parse_from_str(&end.date, "%Y-%m-%d")?;
    let years = (last - first).num_days() as f64 / 365.25;
    let cagr = (years >= 0.25).then(|| (1.0 + total_return).powf(1.0 / years) - 1.0);
    anyhow::ensure!(
        sharpe.is_none_or(f64::is_finite) && cagr.is_none_or(f64::is_finite),
        "Annualized metrics exceed the simulator's numeric range."
    );
    let exposure = exposed as f64 / points.len() as f64;
    Ok(BacktestResult {
        symbol: series.symbol.clone(),
        params,
        points,
        total_return,
        benchmark_return,
        cagr,
        sharpe,
        max_drawdown: drawdown,
        trades,
        fees,
        exposure,
        source: series.source.clone(),
        demo: series.demo,
    })
}

pub fn scaffold(workspace: &Path, language: &str) -> Result<PathBuf> {
    anyhow::ensure!(
        workspace.is_dir(),
        "Choose an existing LEAN workspace in Connections."
    );
    let project = workspace.join(if language == "csharp" {
        "ResenTrendCSharp"
    } else {
        "ResenTrendPython"
    });
    anyhow::ensure!(
        !project.exists(),
        "{} already exists; your files were preserved.",
        project.display()
    );
    std::fs::create_dir(&project)?;
    let code = if language == "csharp" {
        CSHARP_TEMPLATE
    } else {
        PYTHON_TEMPLATE
    };
    let name = if language == "csharp" {
        "Main.cs"
    } else {
        "main.py"
    };
    atomic_write(&project.join(name), code.as_bytes())?;
    atomic_write(&project.join("config.json"), serde_json::to_string_pretty(&json!({"algorithm-language": if language == "csharp" {"CSharp"} else {"Python"}, "parameters": {}, "description": "Resen long/cash trend research template. Review before execution."}))?.as_bytes())?;
    Ok(project)
}

pub async fn lean_run(
    workspace: &Path,
    project: &Path,
    output: &Path,
    tx: &mpsc::UnboundedSender<String>,
) -> Result<BTreeMap<String, String>> {
    anyhow::ensure!(
        workspace.join("lean.json").is_file(),
        "Workspace needs lean.json. Run `lean init` there first."
    );
    let workspace = workspace.canonicalize()?;
    let project = project
        .canonicalize()
        .context("LEAN project does not exist")?;
    anyhow::ensure!(
        project.starts_with(&workspace) && project != workspace,
        "Choose a project inside the configured LEAN workspace."
    );
    let docker = tokio::time::timeout(
        Duration::from_secs(15),
        Command::new("docker")
            .args(["info", "--format", "{{.ServerVersion}}"])
            .kill_on_drop(true)
            .output(),
    )
    .await
    .context("Docker check timed out")?
    .context("Install and start Docker to run LEAN locally")?;
    anyhow::ensure!(
        docker.status.success(),
        "Docker is unavailable. Start Docker Desktop first."
    );
    std::fs::create_dir_all(output)?;
    let mut child = Command::new("lean").arg("backtest").arg(&project).arg("--output").arg(output).arg("--no-update")
        .current_dir(&workspace).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true).spawn().context("Install LEAN CLI and run `lean login`, `lean init`, and a first backtest to cache its Docker image")?;
    let stdout = child.stdout.take().context("LEAN stdout unavailable")?;
    let stderr = child.stderr.take().context("LEAN stderr unavailable")?;
    let tx_err = tx.clone();
    let _stderr_task = crate::process::TaskGuard::new(tokio::spawn(async move {
        let mut lines = BufReader::new(stderr);
        let mut logged = 0;
        loop {
            match crate::process::bounded_line(&mut lines, 64_000).await {
                Ok(Some(line)) => emit_lean_log(&tx_err, &mut logged, &line),
                Ok(None) => break,
                Err(error) => {
                    let _ = tx_err.send(format!("Runner diagnostics could not be read: {error}"));
                    break;
                }
            }
        }
    }));
    tokio::time::timeout(Duration::from_secs(1200), async {
        let mut lines = BufReader::new(stdout);
        let mut logged = 0;
        while let Some(line) = crate::process::bounded_line(&mut lines, 64_000).await? {
            emit_lean_log(tx, &mut logged, &line);
        }
        let status = child.wait().await?;
        anyhow::ensure!(
            status.success(),
            "LEAN exited unsuccessfully; inspect the execution log."
        );
        read_lean_statistics(output)
    })
    .await
    .context("LEAN exceeded its twenty-minute deadline")?
}

fn emit_lean_log(tx: &mpsc::UnboundedSender<String>, logged: &mut usize, line: &str) {
    const LIMIT: usize = 8_000_000;
    if *logged > LIMIT {
        return;
    }
    *logged += line.len();
    let text = if *logged > LIMIT {
        "Runner log exceeded 8 MB; further lines from this stream are suppressed.".into()
    } else {
        crate::clean_text(line.trim_end())
    };
    let _ = tx.send(text);
}

pub fn read_lean_statistics(directory: &Path) -> Result<BTreeMap<String, String>> {
    let mut files: Vec<_> = std::fs::read_dir(directory)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "json") && p.is_file())
        .collect();
    files.sort();
    for file in files {
        if std::fs::metadata(&file)?.len() > 20_000_000 {
            continue;
        }
        if let Ok(data) = serde_json::from_slice::<Value>(&std::fs::read(&file)?)
            && let Some(stats) = data["statistics"].as_object()
            && !stats.is_empty()
        {
            return Ok(stats
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        v.as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| v.to_string()),
                    )
                })
                .collect());
        }
    }
    anyhow::bail!(
        "LEAN exited but no statistics JSON was found in {}.",
        directory.display()
    )
}

pub fn qc_auth(user: &str, token: &str, timestamp: i64) -> String {
    let hash = Sha256::digest(format!("{token}:{timestamp}").as_bytes());
    let hash = format!("{hash:x}");
    format!("Basic {}", STANDARD.encode(format!("{user}:{hash}")))
}
pub async fn qc_request(
    config: &Config,
    secrets: &Secrets,
    endpoint: &str,
    body: Value,
) -> Result<Value> {
    anyhow::ensure!(
        matches!(
            endpoint,
            "authenticate" | "projects/read" | "backtests/list" | "backtests/read"
        ),
        "Unsupported QuantConnect request"
    );
    let token = secrets
        .get("QUANTCONNECT_API_TOKEN")
        .context("Connect a QuantConnect API token in Connections")?;
    anyhow::ensure!(
        !config.qc_user_id.is_empty() && config.qc_user_id.chars().all(|c| c.is_ascii_digit()),
        "Enter your numeric QuantConnect user ID."
    );
    let timestamp = chrono::Utc::now().timestamp();
    let response = http_client(30)?
        .post(format!("https://www.quantconnect.com/api/v2/{endpoint}"))
        .header(
            "Authorization",
            qc_auth(&config.qc_user_id, &token, timestamp),
        )
        .header("Timestamp", timestamp.to_string())
        .json(&body)
        .send()
        .await
        .map_err(|e| e.without_url())?;
    let data = response_json(response).await?;
    anyhow::ensure!(
        data["success"] == true,
        "QuantConnect rejected the request: {}",
        secrets.redact(&data["errors"].to_string())
    );
    Ok(data)
}

pub const PYTHON_TEMPLATE: &str = r#"from AlgorithmImports import *


class ResenTrend(QCAlgorithm):
    # Research template. Review universe, periods, fees and fill assumptions.
    def initialize(self):
        self.set_start_date(2022, 1, 1)
        self.set_end_date(2025, 1, 1)
        self.set_cash(100000)
        self.symbol = self.add_equity("SPY", Resolution.DAILY).symbol
        self.fast = self.sma(self.symbol, 20, Resolution.DAILY)
        self.slow = self.sma(self.symbol, 60, Resolution.DAILY)
        self.set_warm_up(60, Resolution.DAILY)
        self.set_benchmark(self.symbol)

    def on_data(self, data: Slice):
        if self.is_warming_up or not self.slow.is_ready:
            return
        if not data.contains_key(self.symbol):
            return
        target = 0.95 if self.fast.current.value > self.slow.current.value else 0
        if target > 0 and not self.portfolio[self.symbol].invested:
            self.set_holdings(self.symbol, target)
        elif target == 0 and self.portfolio[self.symbol].invested:
            self.liquidate(self.symbol)
"#;
pub const CSHARP_TEMPLATE: &str = r#"using QuantConnect;
using QuantConnect.Algorithm;
using QuantConnect.Data;
using QuantConnect.Indicators;

public class ResenTrend : QCAlgorithm
{
    private Symbol _symbol;
    private SimpleMovingAverage _fast;
    private SimpleMovingAverage _slow;

    public override void Initialize()
    {
        SetStartDate(2022, 1, 1);
        SetEndDate(2025, 1, 1);
        SetCash(100000);
        _symbol = AddEquity("SPY", Resolution.Daily).Symbol;
        _fast = SMA(_symbol, 20, Resolution.Daily);
        _slow = SMA(_symbol, 60, Resolution.Daily);
        SetWarmUp(60, Resolution.Daily);
        SetBenchmark(_symbol);
    }

    public override void OnData(Slice data)
    {
        if (IsWarmingUp || !_slow.IsReady || !data.ContainsKey(_symbol)) return;
        if (_fast.Current.Value > _slow.Current.Value && !Portfolio[_symbol].Invested)
            SetHoldings(_symbol, 0.95m);
        else if (_fast.Current.Value <= _slow.Current.Value && Portfolio[_symbol].Invested)
            Liquidate(_symbol);
    }
}
"#;
