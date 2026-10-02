use crate::{
    config::{Config, Secrets},
    data::{http_client, response_json},
    domain::ProviderKind,
};
use anyhow::{Context, Result};
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::{path::Path, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncWriteExt, BufReader},
    process::Command,
    sync::mpsc,
};

#[derive(Clone, Debug)]
pub enum ModelEvent {
    Delta(String),
    Warning(String),
}

pub fn validate_ready(config: &Config, secrets: &Secrets) -> Result<()> {
    config.validate()?;
    if matches!(
        config.provider,
        ProviderKind::OpenAi | ProviderKind::Anthropic
    ) {
        anyhow::ensure!(
            secrets.get(config.provider.key_name()).is_some(),
            "Connect {} in Connections or set {} before running research.",
            config.provider.label(),
            config.provider.key_name()
        );
    }
    Ok(())
}

pub async fn check(config: &Config, secrets: &Secrets) -> Result<String> {
    config.validate()?;
    if config.provider.is_cli() {
        let name = if config.provider == ProviderKind::Codex {
            "codex"
        } else {
            "claude"
        };
        let output = tokio::time::timeout(
            Duration::from_secs(15),
            Command::new(name)
                .arg("--version")
                .kill_on_drop(true)
                .output(),
        )
        .await
        .context("CLI version check timed out")?
        .with_context(|| format!("{name} is not installed or not on PATH"))?;
        anyhow::ensure!(
            output.status.success(),
            "{name} did not pass its version check."
        );
        let arguments = if name == "codex" {
            ["login", "status"]
        } else {
            ["auth", "status"]
        };
        let auth = tokio::time::timeout(
            Duration::from_secs(15),
            Command::new(name)
                .args(arguments)
                .kill_on_drop(true)
                .output(),
        )
        .await
        .context("CLI authentication check timed out")?
        .context("Could not check CLI authentication")?;
        anyhow::ensure!(
            auth.status.success(),
            "{name} is installed but authentication is unavailable. Sign in through the CLI, then test again."
        );
        if name == "claude" {
            let status: Value = serde_json::from_slice(&auth.stdout)
                .context("Claude returned an unsupported authentication status; update the CLI.")?;
            anyhow::ensure!(
                status["loggedIn"] == true,
                "Claude is not signed in. Run `claude auth login`, then test again."
            );
        }
        return Ok(format!(
            "{} available and signed in. Model access and quota are checked on research.",
            crate::clean_text(&String::from_utf8_lossy(&output.stdout)).trim()
        ));
    }
    let client = http_client(20)?;
    let url = format!("{}/models", config.endpoint.trim_end_matches('/'));
    let request = client.get(url);
    let request = authenticate(request, config, secrets)?;
    let data = response_json(request.send().await.map_err(|e| e.without_url())?).await?;
    let models = data["data"]
        .as_array()
        .context("Endpoint did not return an OpenAI/Anthropic models list")?;
    let found = models
        .iter()
        .any(|m| m["id"].as_str() == Some(config.model.as_str()));
    Ok(if found {
        format!("Connected. {} is available.", config.model)
    } else {
        format!(
            "Connected ({} models). Model '{}' was not listed; check the name before running.",
            models.len(),
            config.model
        )
    })
}

fn authenticate(
    request: reqwest::RequestBuilder,
    config: &Config,
    secrets: &Secrets,
) -> Result<reqwest::RequestBuilder> {
    let key = secrets.get(config.provider.key_name());
    if config.provider == ProviderKind::Anthropic {
        let key = key.context("Set ANTHROPIC_API_KEY or add it in Connections.")?;
        Ok(request
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01"))
    } else {
        if config.provider == ProviderKind::OpenAi {
            anyhow::ensure!(
                key.is_some(),
                "Set OPENAI_API_KEY or add it in Connections."
            );
        }
        Ok(if let Some(key) = key {
            request.bearer_auth(key)
        } else {
            request
        })
    }
}

pub async fn generate(
    config: &Config,
    secrets: &Secrets,
    system: &str,
    prompt: &str,
    cwd: &Path,
    tx: &mpsc::UnboundedSender<ModelEvent>,
) -> Result<()> {
    validate_ready(config, secrets)?;
    if config.provider.is_cli() {
        return tokio::time::timeout(
            Duration::from_secs(600),
            generate_cli(config, secrets, system, prompt, cwd, tx),
        )
        .await
        .context("CLI research exceeded the ten-minute deadline")?;
    }
    let anthropic = config.provider == ProviderKind::Anthropic;
    let endpoint = format!(
        "{}/{}",
        config.endpoint.trim_end_matches('/'),
        if anthropic {
            "messages"
        } else {
            "chat/completions"
        }
    );
    let body = if anthropic {
        json!({"model": config.model, "max_tokens": config.max_output_tokens, "stream": true, "system": system, "messages": [{"role": "user", "content": prompt}]})
    } else {
        json!({"model": config.model, "max_tokens": config.max_output_tokens, "stream": true, "messages": [{"role": "system", "content": system}, {"role": "user", "content": prompt}]})
    };
    let client = http_client(300)?;
    let response = authenticate(client.post(endpoint).json(&body), config, secrets)?
        .send()
        .await
        .map_err(|e| e.without_url())?;
    let status = response.status();
    anyhow::ensure!(
        status.is_success(),
        "Model returned HTTP {}. Check the connection, model, credentials, and quota.",
        status.as_u16()
    );
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if content_type.contains("application/json") {
        let data = response_json(response).await?;
        let text = if anthropic {
            data["content"].as_array().map(|items| {
                items
                    .iter()
                    .filter_map(|i| i["text"].as_str())
                    .collect::<String>()
            })
        } else {
            data["choices"][0]["message"]["content"]
                .as_str()
                .map(str::to_string)
        }
        .context("Model returned no text")?;
        anyhow::ensure!(!text.trim().is_empty(), "Model returned an empty response");
        let _ = tx.send(ModelEvent::Delta(secrets.redact(&text)));
        if data["choices"][0]["finish_reason"] == "length" || data["stop_reason"] == "max_tokens" {
            let _ = tx.send(ModelEvent::Warning(
                "Model reached the output token cap; this memo may be incomplete.".into(),
            ));
        }
        return Ok(());
    }
    let mut decoder = SseDecoder::default();
    let mut redactor = secrets.stream_redactor();
    let mut stream = response.bytes_stream();
    let mut bytes = 0;
    let mut got_text = false;
    let mut finished = false;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.without_url())?;
        bytes += chunk.len();
        anyhow::ensure!(
            bytes <= 4_000_000,
            "Model stream exceeded the response limit"
        );
        for frame in decoder.push(&chunk)? {
            let (delta, done, truncated) = parse_sse(&frame, anthropic)?;
            if let Some(text) = delta {
                got_text |= !text.is_empty();
                let text = redactor.push(&text);
                if !text.is_empty() {
                    let _ = tx.send(ModelEvent::Delta(text));
                }
            }
            if truncated {
                let _ = tx.send(ModelEvent::Warning("The model reached the output token limit. Run a focused follow-up to complete the analysis.".into()));
            }
            finished |= done;
        }
        if finished {
            break;
        }
    }
    let tail = redactor.finish();
    if !tail.is_empty() {
        let _ = tx.send(ModelEvent::Delta(tail));
    }
    anyhow::ensure!(
        got_text,
        "Model returned no analysis. Check the model name and endpoint compatibility."
    );
    anyhow::ensure!(
        finished,
        "Model stream ended before its completion event. Partial text was preserved."
    );
    Ok(())
}

#[derive(Default)]
pub struct SseDecoder {
    pending: Vec<u8>,
}
impl SseDecoder {
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<String>> {
        self.pending.extend_from_slice(chunk);
        anyhow::ensure!(
            self.pending.len() <= 1_000_000,
            "Model sent an oversized SSE event"
        );
        let mut frames = Vec::new();
        while let Some((position, width)) = self
            .pending
            .windows(2)
            .position(|w| w == b"\n\n")
            .map(|p| (p, 2))
            .into_iter()
            .chain(
                self.pending
                    .windows(4)
                    .position(|w| w == b"\r\n\r\n")
                    .map(|p| (p, 4)),
            )
            .min_by_key(|(position, _)| *position)
        {
            let raw = String::from_utf8(self.pending[..position].to_vec())
                .context("Model sent invalid UTF-8")?;
            self.pending.drain(..position + width);
            let data = raw
                .lines()
                .filter_map(|line| {
                    line.strip_prefix("data:")
                        .map(|s| s.strip_prefix(' ').unwrap_or(s))
                })
                .collect::<Vec<_>>()
                .join("\n");
            if !data.is_empty() {
                frames.push(data);
            }
        }
        Ok(frames)
    }
}
pub fn parse_sse(frame: &str, anthropic: bool) -> Result<(Option<String>, bool, bool)> {
    if frame.trim() == "[DONE]" {
        return Ok((None, true, false));
    }
    let value: Value = serde_json::from_str(frame).context("Model sent malformed stream JSON")?;
    anyhow::ensure!(
        value.get("error").is_none() && value["type"] != "error",
        "Model reported a stream error. Check quota and retry."
    );
    if anthropic {
        Ok((
            value["delta"]["text"].as_str().map(str::to_string),
            value["type"] == "message_stop",
            value["delta"]["stop_reason"] == "max_tokens",
        ))
    } else {
        Ok((
            value["choices"][0]["delta"]["content"]
                .as_str()
                .map(str::to_string),
            false,
            value["choices"][0]["finish_reason"] == "length",
        ))
    }
}

async fn generate_cli(
    config: &Config,
    secrets: &Secrets,
    system: &str,
    prompt: &str,
    cwd: &Path,
    tx: &mpsc::UnboundedSender<ModelEvent>,
) -> Result<()> {
    let codex = config.provider == ProviderKind::Codex;
    let name = if codex { "codex" } else { "claude" };
    std::fs::create_dir_all(cwd)?;
    let mut command = Command::new(name);
    if codex {
        command.args([
            "exec",
            "--ignore-user-config",
            "--ignore-rules",
            "-c",
            "features.shell_tool=false",
            "-c",
            "features.unified_exec=false",
            "-c",
            "mcp_servers={}",
            "-c",
            "web_search=\"disabled\"",
            "--sandbox",
            "read-only",
            "--skip-git-repo-check",
            "--ephemeral",
            "--json",
            "--color",
            "never",
            "-",
        ]);
        for feature in [
            "multi_agent",
            "multi_agent_v2",
            "apps",
            "plugins",
            "hooks",
            "browser_use",
            "computer_use",
            "in_app_browser",
            "code_mode_host",
            "skill_search",
            "view_image",
        ] {
            command.args(["-c", &format!("features.{feature}=false")]);
        }
    } else {
        command.args([
            "-p",
            "--safe-mode",
            "--output-format",
            "stream-json",
            "--verbose",
            "--tools",
            "",
            "--strict-mcp-config",
            "--setting-sources",
            "",
            "--no-session-persistence",
            "--disable-slash-commands",
        ]);
    }
    if !config.model.trim().is_empty() {
        command.args(["--model", &config.model]);
    }
    command
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .with_context(|| format!("Cannot start {name}. Install it and sign in first."))?;
    let mut stdin = child.stdin.take().context("CLI stdin unavailable")?;
    let payload = format!("{system}\n\n{prompt}");
    let input_task = crate::process::TaskGuard::new(tokio::spawn(async move {
        stdin.write_all(payload.as_bytes()).await
    }));
    let stdout = child.stdout.take().context("CLI stdout unavailable")?;
    let stderr = child.stderr.take().context("CLI stderr unavailable")?;
    let mut lines = BufReader::new(stdout);
    // Drain stderr concurrently so diagnostics cannot fill the pipe and deadlock.
    let drain = crate::process::TaskGuard::new(tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut bytes = Vec::new();
        let mut stderr = BufReader::new(stderr);
        let mut chunk = [0u8; 4096];
        while let Ok(count) = stderr.read(&mut chunk).await {
            if count == 0 {
                break;
            }
            let keep = count.min(64_000usize.saturating_sub(bytes.len()));
            bytes.extend_from_slice(&chunk[..keep]);
        }
        bytes
    }));
    let mut got_text = false;
    let mut total = 0;
    while let Some(line) = crate::process::bounded_line(&mut lines, 4_000_000).await? {
        total += line.len();
        anyhow::ensure!(total <= 4_000_000, "CLI output exceeded the response limit");
        if let Ok(value) = serde_json::from_str::<Value>(&line) {
            let text = cli_text(&value, codex);
            if let Some(text) = text {
                got_text |= !text.is_empty();
                let _ = tx.send(ModelEvent::Delta(secrets.redact(&text)));
            }
            anyhow::ensure!(
                value["type"] != "turn.failed" && value["is_error"] != true,
                "{name} could not complete research. Check its authentication and usage limits."
            );
        }
    }
    let status = child.wait().await?;
    input_task.join().await.context("CLI input task failed")??;
    let diagnostics = drain.join().await.unwrap_or_default();
    anyhow::ensure!(
        status.success(),
        "{name} exited unsuccessfully: {}",
        secrets
            .redact(&String::from_utf8_lossy(&diagnostics))
            .chars()
            .take(500)
            .collect::<String>()
    );
    anyhow::ensure!(
        got_text,
        "{name} returned no analysis. Check authentication and update the CLI."
    );
    Ok(())
}
pub fn cli_text(value: &Value, codex: bool) -> Option<String> {
    if codex {
        if value["type"] == "item.completed" && value["item"]["type"] == "agent_message" {
            return value["item"]["text"].as_str().map(|s| format!("{s}\n"));
        }
    } else if value["type"] == "assistant" {
        return value["message"]["content"].as_array().map(|items| {
            items
                .iter()
                .filter(|i| i["type"] == "text")
                .filter_map(|i| i["text"].as_str())
                .collect::<String>()
        });
    }
    None
}
