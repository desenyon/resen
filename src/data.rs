use crate::{
    config::{Config, Secrets},
    domain::{Bar, MarketSeries, Source},
};
use anyhow::{Context, Result};
use chrono::{Duration as ChronoDuration, Utc};
use reqwest::{Client, Response};
use serde_json::{Value, json};
use std::time::Duration;

pub fn http_client(timeout: u64) -> Result<Client> {
    Ok(Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(timeout))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("Resen/0.1 financial-research-terminal")
        .build()?)
}

pub async fn response_bytes(response: Response, limit: usize) -> Result<Vec<u8>> {
    let status = response.status();
    anyhow::ensure!(
        status.is_success(),
        "Provider returned HTTP {}. {}",
        status.as_u16(),
        match status.as_u16() {
            401 | 403 => "Check credentials, permissions and subscription.",
            429 => "Rate limit reached; wait before retrying.",
            500..=599 => "The service is unavailable; try again later.",
            _ => "Check the endpoint and request settings.",
        }
    );
    if let Some(size) = response.content_length() {
        anyhow::ensure!(
            size <= limit as u64,
            "Provider response exceeded the size limit."
        );
    }
    let mut stream = response.bytes_stream();
    let mut result = Vec::new();
    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.without_url())?;
        anyhow::ensure!(
            result.len() + chunk.len() <= limit,
            "Provider response exceeded the size limit."
        );
        result.extend_from_slice(&chunk);
    }
    Ok(result)
}
pub async fn response_json(response: Response) -> Result<Value> {
    serde_json::from_slice(&response_bytes(response, 8_000_000).await?)
        .context("Provider returned invalid JSON")
}

#[derive(Clone)]
pub struct DataClient {
    client: Client,
    config: Config,
    secrets: Secrets,
    imports: Option<std::path::PathBuf>,
    sec_next_request: std::sync::Arc<tokio::sync::Mutex<tokio::time::Instant>>,
}
impl DataClient {
    pub fn new(config: Config, secrets: Secrets) -> Result<Self> {
        Ok(Self {
            client: http_client(30)?,
            config,
            secrets,
            imports: None,
            sec_next_request: std::sync::Arc::new(tokio::sync::Mutex::new(
                tokio::time::Instant::now(),
            )),
        })
    }
    pub fn with_imports(mut self, directory: std::path::PathBuf) -> Self {
        self.imports = Some(directory);
        self
    }
    async fn sec_get(&self, url: &str) -> Result<Response> {
        anyhow::ensure!(
            self.config.sec_contact.contains('@'),
            "Add your SEC contact email in Connections."
        );
        let slot = {
            let mut next = self.sec_next_request.lock().await;
            let slot = (*next).max(tokio::time::Instant::now());
            *next = slot + std::time::Duration::from_millis(125);
            slot
        };
        // Keep one desk below the SEC's ten-request-per-second access ceiling.
        tokio::time::sleep_until(slot).await;
        self.client
            .get(url)
            .header(
                "User-Agent",
                format!("Resen financial research {}", self.config.sec_contact),
            )
            .send()
            .await
            .map_err(|e| e.without_url().into())
    }

    pub async fn market(&self, symbol: &str) -> Result<MarketSeries> {
        let symbols = crate::domain::parse_symbols(symbol)?;
        anyhow::ensure!(symbols.len() == 1, "Retrieve one market symbol at a time.");
        let symbol = symbols.into_iter().next().context("Enter a symbol")?;
        if self.config.data_provider == "csv" {
            let directory = self
                .imports
                .as_ref()
                .context("Import a CSV into the Resen state directory first.")?;
            let path = directory.join(format!("{symbol}.json"));
            let series: MarketSeries = serde_json::from_slice(&std::fs::read(&path).with_context(|| format!("No imported prices for {symbol}. Use Import price CSV in the command palette."))?)?;
            anyhow::ensure!(
                series.symbol == symbol && !series.demo,
                "Imported price metadata is invalid."
            );
            for bar in &series.bars {
                validate_bar(bar)?;
            }
            return Ok(series);
        }
        let (bars, source) = if self.config.data_provider == "alpha_vantage" {
            let key = self
                .secrets
                .get("ALPHAVANTAGE_API_KEY")
                .context("Connect Alpha Vantage in Connections or set ALPHAVANTAGE_API_KEY.")?;
            let response = self
                .client
                .get("https://www.alphavantage.co/query")
                .query(&[
                    ("function", "TIME_SERIES_DAILY"),
                    ("symbol", symbol.as_str()),
                    ("apikey", key.as_str()),
                ])
                .send()
                .await
                .map_err(|e| e.without_url())?;
            (
                parse_alpha_bars(&response_json(response).await?)?,
                "Alpha Vantage • daily OHLCV (unadjusted)".into(),
            )
        } else {
            let key = self.secrets.get("STOOQ_API_KEY").context("Stooq requires a download API key. Add STOOQ_API_KEY in Connections, choose Alpha Vantage, or import a price CSV.")?;
            let ticker = stooq_symbol(&symbol);
            let now = Utc::now();
            let response = self
                .client
                .get("https://stooq.com/q/d/l/")
                .query(&[
                    ("s", ticker),
                    ("apikey", key),
                    ("i", "d".into()),
                    (
                        "d1",
                        (now - ChronoDuration::days(1096))
                            .format("%Y%m%d")
                            .to_string(),
                    ),
                    ("d2", now.format("%Y%m%d").to_string()),
                ])
                .send()
                .await
                .map_err(|e| e.without_url())?;
            let bytes = response_bytes(response, 2_000_000).await?;
            (
                parse_stooq(&bytes)?,
                "Stooq • daily OHLCV (adjustment policy unverified)".into(),
            )
        };
        anyhow::ensure!(
            bars.len() >= 2,
            "No usable daily price history for {symbol}."
        );
        Ok(MarketSeries {
            symbol,
            bars,
            source,
            retrieved_at: Utc::now(),
            demo: false,
        })
    }

    pub async fn search(&self, query: &str) -> Result<Vec<Source>> {
        let (data, publisher) = if self.config.search_provider == "tavily" {
            let key = self
                .secrets
                .get("TAVILY_API_KEY")
                .context("Connect Tavily in Connections or set TAVILY_API_KEY.")?;
            let response = self.client.post("https://api.tavily.com/search").json(&json!({"api_key": key, "query": query, "max_results": 5, "search_depth": "basic", "include_answer": false})).send().await.map_err(|e| e.without_url())?;
            (response_json(response).await?, "Tavily")
        } else {
            let key = self
                .secrets
                .get("BRAVE_API_KEY")
                .context("Connect Brave Search in Connections or set BRAVE_API_KEY.")?;
            let response = self
                .client
                .get("https://api.search.brave.com/res/v1/web/search")
                .header("X-Subscription-Token", key)
                .query(&[("q", query), ("count", "5")])
                .send()
                .await
                .map_err(|e| e.without_url())?;
            (response_json(response).await?, "Brave Search")
        };
        let entries = if publisher == "Tavily" {
            &data["results"]
        } else {
            &data["web"]["results"]
        };
        let entries = entries
            .as_array()
            .context("Search provider returned no results list")?;
        Ok(entries
            .iter()
            .take(5)
            .filter_map(|item| {
                let url = item["url"].as_str()?;
                let parsed = reqwest::Url::parse(url).ok()?;
                if !matches!(parsed.scheme(), "http" | "https") {
                    return None;
                }
                Some(Source {
                    id: 0,
                    title: crate::clean_text(item["title"].as_str().unwrap_or("Search result")),
                    url: url.into(),
                    publisher: publisher.into(),
                    retrieved_at: Utc::now(),
                    as_of: item["page_age"].as_str().map(str::to_string),
                    content: crate::clean_text(
                        item["content"]
                            .as_str()
                            .or(item["description"].as_str())
                            .unwrap_or("No excerpt supplied"),
                    )
                    .chars()
                    .take(3000)
                    .collect(),
                })
            })
            .collect())
    }

    pub async fn fundamentals(&self, symbol: &str) -> Result<Source> {
        let key = self
            .secrets
            .get("ALPHAVANTAGE_API_KEY")
            .context("Alpha Vantage is not connected; company fundamentals unavailable.")?;
        let response = self
            .client
            .get("https://www.alphavantage.co/query")
            .query(&[
                ("function", "OVERVIEW"),
                ("symbol", symbol),
                ("apikey", key.as_str()),
            ])
            .send()
            .await
            .map_err(|e| e.without_url())?;
        let data = response_json(response).await?;
        alpha_error(&data)?;
        anyhow::ensure!(
            data["Symbol"]
                .as_str()
                .is_some_and(|returned| returned.eq_ignore_ascii_case(symbol)),
            "No matching company overview returned for {symbol}."
        );
        let fields = [
            "Symbol",
            "Name",
            "Description",
            "Sector",
            "Industry",
            "Currency",
            "LatestQuarter",
            "MarketCapitalization",
            "PERatio",
            "ForwardPE",
            "PEGRatio",
            "PriceToBookRatio",
            "RevenueTTM",
            "EPS",
            "ProfitMargin",
            "OperatingMarginTTM",
            "ReturnOnEquityTTM",
            "DividendYield",
            "AnalystTargetPrice",
            "QuarterlyRevenueGrowthYOY",
            "QuarterlyEarningsGrowthYOY",
        ];
        let selected: serde_json::Map<String, Value> = fields
            .into_iter()
            .filter_map(|k| data.get(k).cloned().map(|v| (k.into(), v)))
            .collect();
        Ok(Source {
            id: 0,
            title: format!("{symbol} company overview"),
            url: format!("https://www.alphavantage.co/query?function=OVERVIEW&symbol={symbol}"),
            publisher: "Alpha Vantage".into(),
            retrieved_at: Utc::now(),
            as_of: None,
            content: format!(
                "Vendor overview with mixed time bases. LatestQuarter identifies a financial reporting period, not the observation date of price-dependent ratios. ForwardPE is an estimate-based metric.\n{}",
                serde_json::to_string_pretty(&selected)?
            ),
        })
    }

    pub async fn macro_series(&self, series: &str) -> Result<Source> {
        let key = self
            .secrets
            .get("FRED_API_KEY")
            .context("Connect FRED to retrieve macro observations.")?;
        anyhow::ensure!(
            matches!(series, "FEDFUNDS" | "CPIAUCSL" | "UNRATE" | "DGS10" | "GDP"),
            "Unsupported FRED series."
        );
        let observations_request = self
            .client
            .get("https://api.stlouisfed.org/fred/series/observations")
            .query(&[
                ("series_id", series),
                ("api_key", key.as_str()),
                ("file_type", "json"),
                ("sort_order", "desc"),
                ("limit", "24"),
            ])
            .send();
        let metadata_request = self
            .client
            .get("https://api.stlouisfed.org/fred/series")
            .query(&[
                ("series_id", series),
                ("api_key", key.as_str()),
                ("file_type", "json"),
            ])
            .send();
        let (observations_response, metadata_response) =
            tokio::try_join!(observations_request, metadata_request)
                .map_err(|e| e.without_url())?;
        let (observations, metadata) = tokio::try_join!(
            response_json(observations_response),
            response_json(metadata_response)
        )?;
        fred_source(series, &metadata, &observations)
    }

    pub async fn filings(&self, symbol: &str) -> Result<(u64, Source)> {
        anyhow::ensure!(
            self.config.sec_contact.contains('@'),
            "Add a contact email in Connections to access SEC EDGAR."
        );
        let response = self
            .sec_get("https://www.sec.gov/files/company_tickers.json")
            .await?;
        let tickers = response_json(response).await?;
        let company = tickers
            .as_object()
            .context("Invalid SEC ticker index")?
            .values()
            .find(|c| {
                c["ticker"]
                    .as_str()
                    .is_some_and(|t| t.eq_ignore_ascii_case(symbol))
            })
            .context("Ticker not found in the SEC index")?;
        let cik = company["cik_str"]
            .as_u64()
            .context("SEC company has no CIK")?;
        let response = self
            .sec_get(&format!(
                "https://data.sec.gov/submissions/CIK{cik:010}.json"
            ))
            .await?;
        let data = response_json(response).await?;
        let recent = &data["filings"]["recent"];
        let forms = recent["form"].as_array().context("No recent SEC filings")?;
        let mut filings = Vec::new();
        for (i, form) in forms
            .iter()
            .enumerate()
            .filter(|(_, f)| matches!(f.as_str(), Some("10-K" | "10-Q" | "8-K" | "20-F" | "6-K")))
            .take(8)
        {
            let acc = recent["accessionNumber"][i]
                .as_str()
                .unwrap_or("")
                .replace('-', "");
            let doc = recent["primaryDocument"][i].as_str().unwrap_or("");
            filings.push(json!({"form": form, "filed": recent["filingDate"][i], "report_date": recent["reportDate"][i], "url": format!("https://www.sec.gov/Archives/edgar/data/{cik}/{acc}/{doc}")}));
        }
        Ok((
            cik,
            Source {
                id: 0,
                title: format!("{symbol} SEC filing index"),
                url: format!("https://www.sec.gov/edgar/browse/?CIK={cik}"),
                publisher: "US Securities and Exchange Commission".into(),
                retrieved_at: Utc::now(),
                as_of: recent["filingDate"][0].as_str().map(str::to_string),
                content: format!(
                    "Company: {}. This is a filing index, not the full text of these filings. Do not claim to have read their contents.\n{}",
                    data["name"],
                    serde_json::to_string_pretty(&filings)?
                ),
            },
        ))
    }

    pub async fn company_facts(&self, symbol: &str, cik: u64) -> Result<Source> {
        anyhow::ensure!(
            self.config.sec_contact.contains('@'),
            "Add your SEC contact email in Connections."
        );
        let response = self
            .sec_get(&format!(
                "https://data.sec.gov/api/xbrl/companyfacts/CIK{cik:010}.json"
            ))
            .await?;
        let data: Value = serde_json::from_slice(&response_bytes(response, 20_000_000).await?)
            .context("SEC returned invalid company facts JSON")?;
        sec_facts_source(symbol, cik, &data)
    }
}

pub fn sec_facts_source(symbol: &str, cik: u64, data: &Value) -> Result<Source> {
    anyhow::ensure!(
        data["cik"].as_u64() == Some(cik),
        "SEC facts returned the wrong company."
    );
    let concepts = [
        (
            "us-gaap",
            &[
                "RevenueFromContractWithCustomerExcludingAssessedTax",
                "Revenues",
                "SalesRevenueNet",
                "NetIncomeLoss",
                "EarningsPerShareDiluted",
                "Assets",
                "Liabilities",
                "StockholdersEquity",
                "CashAndCashEquivalentsAtCarryingValue",
                "NetCashProvidedByUsedInOperatingActivities",
                "PaymentsToAcquirePropertyPlantAndEquipment",
                "LongTermDebtCurrent",
                "LongTermDebtNoncurrent",
            ][..],
        ),
        (
            "ifrs-full",
            &[
                "Revenue",
                "ProfitLoss",
                "Assets",
                "Liabilities",
                "Equity",
                "CashAndCashEquivalents",
                "CashFlowsFromUsedInOperatingActivities",
                "PurchaseOfPropertyPlantAndEquipment",
                "DilutedEarningsLossPerShare",
            ][..],
        ),
    ];
    let mut selected = Vec::new();
    let mut latest_period: Option<String> = None;
    for (taxonomy, tags) in concepts {
        for tag in tags {
            let concept = &data["facts"][taxonomy][tag];
            let Some(units) = concept["units"].as_object() else {
                continue;
            };
            for (unit, rows) in units.iter().take(4) {
                let Some(rows) = rows.as_array() else {
                    continue;
                };
                let mut rows: Vec<_> = rows
                    .iter()
                    .filter(|row| {
                        let valid_date = |field: &str| {
                            row[field].as_str().is_some_and(|s| {
                                chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
                                    .is_ok_and(|d| d <= Utc::now().date_naive())
                            })
                        };
                        row["val"].as_f64().is_some_and(f64::is_finite)
                            && valid_date("filed")
                            && valid_date("end")
                            && matches!(
                                row["form"].as_str(),
                                Some(
                                    "10-K"
                                        | "10-Q"
                                        | "20-F"
                                        | "40-F"
                                        | "10-K/A"
                                        | "10-Q/A"
                                        | "20-F/A"
                                        | "40-F/A"
                                )
                            )
                    })
                    .collect();
                rows.sort_by(|a, b| {
                    b["filed"]
                        .as_str()
                        .cmp(&a["filed"].as_str())
                        .then_with(|| b["end"].as_str().cmp(&a["end"].as_str()))
                });
                let mut observations = Vec::new();
                for row in rows {
                    let record = json!({"start":row["start"], "end":row["end"], "val":row["val"], "accn":row["accn"], "fy":row["fy"], "fp":row["fp"], "form":row["form"], "filed":row["filed"], "frame":row["frame"]});
                    if observations.contains(&record) {
                        continue;
                    }
                    let end = row["end"].as_str().unwrap();
                    if latest_period.as_deref().is_none_or(|latest| end > latest) {
                        latest_period = Some(end.into());
                    }
                    observations.push(record);
                    if observations.len() == 8 {
                        break;
                    }
                }
                if !observations.is_empty() {
                    selected.push(json!({"taxonomy":taxonomy,"concept":tag,"label":concept["label"],"unit":unit,"observations":observations}));
                }
            }
        }
    }
    anyhow::ensure!(
        !selected.is_empty(),
        "No supported financial concepts in SEC company facts."
    );
    Ok(Source {
        id: 0,
        title: format!("{symbol} SEC reported financial facts"),
        url: format!("https://data.sec.gov/api/xbrl/companyfacts/CIK{cik:010}.json"),
        publisher: "US Securities and Exchange Commission".into(),
        retrieved_at: Utc::now(),
        as_of: latest_period,
        content: format!(
            "Company: {}. Selected entity-wide US-GAAP/IFRS XBRL observations, not full filings. Each unit and reporting period is separate; records may overlap annual, year-to-date and quarterly durations. Do not sum them or infer quarters from fiscal-period labels. Current retrieval may include restatements; use filing dates for point-in-time analysis. Coverage is limited to the displayed concepts.\n{}",
            data["entityName"],
            serde_json::to_string(&selected)?
        ),
    })
}

pub fn stooq_symbol(symbol: &str) -> String {
    let lower = symbol.to_ascii_lowercase();
    if lower.starts_with('^') || lower.contains('.') {
        lower
    } else {
        format!("{lower}.us")
    }
}

pub fn parse_stooq(bytes: &[u8]) -> Result<Vec<Bar>> {
    let mut reader = csv::Reader::from_reader(bytes);
    let header = reader.headers().context("Invalid price CSV")?;
    anyhow::ensure!(
        header
            .iter()
            .eq(["Date", "Open", "High", "Low", "Close", "Volume"]),
        "Market source returned no daily OHLCV data. Try Alpha Vantage or another symbol."
    );
    let mut bars = Vec::new();
    for row in reader.records() {
        let row = row?;
        let number = |i| -> Result<f64> {
            row.get(i)
                .context("Missing OHLCV field")?
                .parse::<f64>()
                .context("Invalid OHLCV number")
        };
        let bar = Bar {
            date: row[0].into(),
            open: number(1)?,
            high: number(2)?,
            low: number(3)?,
            close: number(4)?,
            volume: number(5)?,
        };
        validate_bar(&bar)?;
        bars.push(bar);
    }
    bars.sort_by(|a, b| a.date.cmp(&b.date));
    anyhow::ensure!(
        bars.windows(2).all(|w| w[0].date != w[1].date),
        "Duplicate dates in price history"
    );
    Ok(bars)
}
fn alpha_error(data: &Value) -> Result<()> {
    for key in ["Error Message", "Note", "Information"] {
        if let Some(message) = data[key].as_str() {
            anyhow::bail!(
                "Alpha Vantage: {}",
                crate::clean_text(message)
                    .chars()
                    .take(400)
                    .collect::<String>()
            );
        }
    }
    Ok(())
}
pub fn parse_alpha_bars(data: &Value) -> Result<Vec<Bar>> {
    alpha_error(data)?;
    let series = data["Time Series (Daily)"]
        .as_object()
        .context("Alpha Vantage did not return daily history")?;
    let mut bars = Vec::new();
    for (date, item) in series {
        let number = |key| -> Result<f64> {
            item[key]
                .as_str()
                .context("Missing Alpha Vantage field")?
                .parse()
                .context("Invalid Alpha Vantage number")
        };
        let bar = Bar {
            date: date.clone(),
            open: number("1. open")?,
            high: number("2. high")?,
            low: number("3. low")?,
            close: number("4. close")?,
            volume: number("5. volume")?,
        };
        validate_bar(&bar)?;
        bars.push(bar);
    }
    bars.sort_by(|a, b| a.date.cmp(&b.date));
    Ok(bars)
}
pub fn fred_source(series: &str, metadata: &Value, data: &Value) -> Result<Source> {
    let details = metadata["seriess"]
        .as_array()
        .and_then(|a| a.first())
        .context("FRED did not return series metadata")?;
    anyhow::ensure!(
        details["id"].as_str() == Some(series),
        "FRED metadata returned the wrong series."
    );
    for field in ["title", "units", "frequency", "seasonal_adjustment"] {
        anyhow::ensure!(
            details[field].as_str().is_some_and(|s| !s.is_empty()),
            "FRED metadata is missing {field}."
        );
    }
    let observations = data["observations"]
        .as_array()
        .filter(|a| !a.is_empty())
        .context("FRED did not return observations")?;
    let selected = serde_json::json!({"id":series, "title":details["title"], "units":details["units"], "frequency":details["frequency"], "seasonal_adjustment":details["seasonal_adjustment"], "last_updated":details["last_updated"], "observations":observations});
    Ok(Source {
        id: 0,
        title: format!("FRED {series}: {}", details["title"].as_str().unwrap()),
        url: format!("https://fred.stlouisfed.org/series/{series}"),
        publisher: "Federal Reserve Bank of St. Louis".into(),
        retrieved_at: Utc::now(),
        as_of: observations
            .first()
            .and_then(|o| o["date"].as_str())
            .map(str::to_string),
        content: format!(
            "Values are observations, not forecasts. Vintage: retrieved today, not point-in-time historical vintage. A value of '.' means missing, not zero.\n{}",
            serde_json::to_string_pretty(&selected)?
        ),
    })
}

pub fn validate_bar(bar: &Bar) -> Result<()> {
    let date =
        chrono::NaiveDate::parse_from_str(&bar.date, "%Y-%m-%d").context("Invalid price date")?;
    anyhow::ensure!(
        date.format("%Y-%m-%d").to_string() == bar.date,
        "Price dates must use YYYY-MM-DD."
    );
    anyhow::ensure!(
        date <= chrono::Utc::now().date_naive(),
        "Price history contains a future date."
    );
    anyhow::ensure!(
        [bar.open, bar.high, bar.low, bar.close]
            .iter()
            .all(|v| v.is_finite() && *v > 0.0)
            && bar.volume.is_finite()
            && bar.volume >= 0.0,
        "Price data contains nonpositive or nonfinite values."
    );
    anyhow::ensure!(
        bar.high >= bar.open.max(bar.close).max(bar.low) && bar.low <= bar.open.min(bar.close),
        "Inconsistent OHLC prices"
    );
    Ok(())
}

pub fn market_source(series: &MarketSeries) -> Source {
    let closes: Vec<_> = series
        .bars
        .iter()
        .rev()
        .take(60)
        .map(|b| json!({"date": b.date, "close": b.close, "volume": b.volume}))
        .collect();
    Source {
        id: 0,
        title: format!("{} daily price history", series.symbol),
        url: if series.demo {
            "demo://synthetic-market".into()
        } else if let Some(path) = series.source.strip_prefix("Imported CSV • ") {
            reqwest::Url::from_file_path(path)
                .map(|url| url.to_string())
                .unwrap_or_else(|_| format!("resen://imports/{}", series.symbol))
        } else if series.source.starts_with("Stooq") {
            format!("https://stooq.com/q/?s={}", stooq_symbol(&series.symbol))
        } else {
            format!(
                "https://www.alphavantage.co/query?function=TIME_SERIES_DAILY&symbol={}",
                series.symbol
            )
        },
        publisher: series.source.clone(),
        retrieved_at: series.retrieved_at,
        as_of: series.last().map(|b| b.date.clone()),
        content: format!(
            "{} Last {} closes in reverse chronological order:\n{}",
            if series.demo {
                "DEMO: synthetic daily observations. No live provider was contacted; no currency is supplied."
            } else {
                "End-of-day data, not a live quote. Currency is not supplied by this source; do not assume one."
            },
            closes.len(),
            serde_json::to_string(&closes).unwrap_or_default()
        ),
    }
}

pub fn import_prices(
    directory: &std::path::Path,
    symbol: &str,
    path: &std::path::Path,
) -> Result<MarketSeries> {
    let symbols = crate::domain::parse_symbols(symbol)?;
    anyhow::ensure!(symbols.len() == 1, "Import one asset at a time.");
    let symbol = symbols[0].clone();
    let path = path.canonicalize().context("Price CSV does not exist")?;
    anyhow::ensure!(
        std::fs::metadata(&path)?.len() <= 2_000_000,
        "Price CSV is limited to 2 MB."
    );
    let bars = parse_stooq(&std::fs::read(&path)?)?;
    anyhow::ensure!(bars.len() >= 2, "Import at least two daily bars.");
    let series = MarketSeries {
        symbol: symbol.clone(),
        bars,
        source: format!("Imported CSV • {}", path.display()),
        retrieved_at: Utc::now(),
        demo: false,
    };
    crate::config::atomic_write(
        &directory.join(format!("{symbol}.json")),
        &serde_json::to_vec(&series)?,
    )?;
    Ok(series)
}

pub fn demo_market(symbol: &str) -> MarketSeries {
    let base = match symbol {
        "NVDA" => 124.0,
        "AAPL" => 218.0,
        "MSFT" => 428.0,
        "SPY" => 565.0,
        _ => 100.0,
    };
    let start = chrono::NaiveDate::from_ymd_opt(2025, 1, 2).unwrap();
    let mut bars = Vec::new();
    let mut date = start;
    use chrono::Datelike;
    for i in 0..160 {
        while date.weekday().number_from_monday() > 5 {
            date += ChronoDuration::days(1);
        }
        let x = i as f64;
        let close =
            base * (0.89 + x * 0.0008 + (x * 0.13).sin() * 0.025 + (x * 0.37).cos() * 0.008);
        bars.push(Bar {
            date: date.to_string(),
            open: close * 0.997,
            high: close * 1.009,
            low: close * 0.99,
            close,
            volume: 30_000_000.0 + x * 1100.0,
        });
        date += ChronoDuration::days(1);
    }
    MarketSeries {
        symbol: symbol.into(),
        bars,
        source: "DEMO • synthetic fixture".into(),
        retrieved_at: Utc::now(),
        demo: true,
    }
}
