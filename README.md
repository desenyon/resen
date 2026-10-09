<div align="center">

# resen

### A clearer view of the market.

Your model. Your evidence. Your research desk.<br>
A native financial workspace, built for the terminal.

[![Rust 1.89+](https://img.shields.io/badge/Rust-1.89%2B-d9ae73?style=flat-square&labelColor=171d28)](Cargo.toml)
[![Terminal native](https://img.shields.io/badge/interface-terminal_native-91d6bd?style=flat-square&labelColor=171d28)](docs/UI_RESEARCH.md)
[![MIT license](https://img.shields.io/badge/license-MIT-aab8d5?style=flat-square&labelColor=171d28)](LICENSE)
[![Download](https://img.shields.io/github/v/release/desenyon/resen?style=flat-square&labelColor=171d28&color=91d6bd)](https://github.com/desenyon/resen/releases/latest)

[Get started](#get-started) · [Models](#setup-and-models) · [Data sources](#evidence-connections) · [Strategy lab](#lean-and-quantconnect) · [CLI](#headless-use)

</div>

![Resen research desk: slate panels, mint price charts, a source ledger and explicitly labeled synthetic demo data](docs/assets/desk.png)

<p align="center"><sub>The real terminal interface, rendered from the app. All screenshots show labeled, synthetic demo data.</sub></p>

Resen brings financial research into one quiet, keyboard-driven workspace.
Connect a model, gather evidence, ask a question, and keep a source-linked memo
beside your charts. Research, connections, historical studies and the archive
share one native Rust executable. No application server is required.

| | In your workspace |
| :--- | :--- |
| **Research desk** | Daily price charts, a watchlist and a timestamped source ledger. |
| **Your intelligence** | Codex, Claude Code, Ollama, OpenAI, Anthropic or a compatible endpoint. |
| **Evidence-led memos** | Five research workflows, fresh-evidence follow-ups, Markdown and JSON exports. |
| **Strategy lab** | A transparent SMA study, Python/C# LEAN scaffolds and QuantConnect result access. |
| **Local archive** | SQLite persistence, interrupted-run recovery and searchable history. |
| **Considered controls** | Guided setup, masked key fields, a command palette and cancellable jobs. |

## Get started

Install the latest release on **macOS or Linux**, including Apple Silicon and
ARM64 Linux. No compiler or sudo required; SQLite is bundled.

```sh
curl -fsSL https://github.com/desenyon/resen/releases/latest/download/install.sh | sh

# Explore the full interface without keys or network access.
~/.local/bin/resen --demo

# Connect your model and evidence sources.
~/.local/bin/resen setup
```

The installer verifies SHA-256 checksums and tests the executable before replacing
an existing installation. Run the command again to upgrade; your research,
settings and credentials stay in their existing state directory. Add
`$HOME/.local/bin` to your PATH to use `resen` directly.

| Platform | Download |
| --- | --- |
| macOS 13+, Apple Silicon or Intel | Shell installer or native `.tar.gz` |
| Linux, ARM64 or x86_64 | Shell installer or static musl `.tar.gz` |
| Windows 10+, x86_64 | Extract `resen.exe` from the [release ZIP](https://github.com/desenyon/resen/releases/latest) and run `.\resen.exe --demo`; no separate Visual C++ runtime needed |

Archives and `SHA256SUMS` are available on [GitHub Releases](https://github.com/desenyon/resen/releases/latest).
macOS binaries are unsigned and not notarized. Choose a directory or pin a version:

```sh
curl -fsSL https://github.com/desenyon/resen/releases/download/v0.2.2/install.sh \
  | sh -s -- --version v0.2.2 --dir "$HOME/.local/bin"
```

<details>
<summary><strong>Build from source</strong></summary>

Rust 1.89+ and a C compiler are required.

```sh
git clone https://github.com/desenyon/resen.git
cd resen
cargo install --path . --locked
resen --demo

# Or run directly from the build directory.
cargo build --release --locked
./target/release/resen --demo
```

</details>

Use a truecolor terminal and a readable monospace font. No Nerd Font is required.
144×46 or larger gives the full research desk; 80×24 works in compact mode. The
minimum supported size is 60×18. The terminal controls font size and rendering.

`--demo` is completely offline: synthetic prices, a sample memo, no provider
requests, no model use. Demo badges remain attached to archived runs and exports.
Save Connections to leave demo mode.

## Inside the desk

<table>
<tr>
<td width="50%"><strong>Research</strong><br><sub>A memo, its evidence and the next question.</sub></td>
<td width="50%"><strong>Connections</strong><br><sub>Your model, sources and workspace in one form.</sub></td>
</tr>
<tr>
<td><img src="docs/assets/research.png" alt="Resen research page with a synthetic demo memo and source ledger"></td>
<td><img src="docs/assets/setup.png" alt="Resen connections form with masked credentials and model settings"></td>
</tr>
</table>

<details>
<summary><strong>Bring your own market history</strong></summary>

Import daily OHLCV from a local CSV for charts, research and historical studies.
The file stays attached to its evidence record.

![Resen CSV import dialog with a local file, symbol and provenance fields](docs/assets/import.png)

</details>

## Setup and models

The first-run wizard covers intelligence, sources, and the workspace. Existing
Codex or Claude executables are suggested automatically when found on PATH.
Optional connections can be skipped and added later. Keys are masked in forms.
Use **Ctrl+T** to test a model connection and **Ctrl+S** to save connections.

| Engine | Setup | Execution |
| --- | --- | --- |
| Codex CLI | Install and sign in to `codex`; select Codex CLI | Noninteractive JSONL, read-only sandbox, execution/search/browser/apps disabled, isolated user config; existing auth retained |
| Claude Code | Install and sign in to `claude`; select Claude Code | Print mode, safe mode, tools and MCP disabled; existing auth retained |
| Ollama | Start Ollama, pull a model, enter its exact name | OpenAI-compatible `/v1/chat/completions` |
| OpenAI | `OPENAI_API_KEY`, model and `https://api.openai.com/v1` | Streaming Chat Completions |
| Anthropic | `ANTHROPIC_API_KEY`, model and `https://api.anthropic.com/v1` | Streaming Messages |
| Compatible endpoint | Base URL including `/v1`, model, optional `RESEN_MODEL_API_KEY` | Streaming Chat Completions, with a nonstreaming JSON fallback |

The model field is editable. CLI providers may leave it blank to use the CLI's
default. Remote endpoints require HTTPS; loopback HTTP is allowed for local
models. Endpoints cannot embed credentials. The compatible adapter supports the
Chat Completions protocol, not every vendor's custom API. Reasoning-only models
that reject `max_tokens` require a compatible gateway or another supported model.

API-provider output caps range from 256 to 16,384 tokens. Research makes one model
invocation after gathering evidence. CLI providers keep their own model/output
limits; their invocation has a ten-minute deadline. API generation has a
five-minute request deadline. Configure local models with enough context for the
selected source ledger; Resen does not override the server's context window.
Follow-ups include up to 12,000 characters of the prior memo as context.
No generation occurs on startup or connection check.
CLI connection checks inspect the version and existing authentication without
generating text.

## Evidence connections

| Service | Credential | Use |
| --- | --- | --- |
| Stooq | `STOOQ_API_KEY` | Daily OHLCV; download access requires a vendor key |
| Imported CSV | Local file | Daily history with file provenance; no vendor or network required |
| Alpha Vantage | `ALPHAVANTAGE_API_KEY` | Daily unadjusted history and company overview/ratios |
| Brave Search | `BRAVE_API_KEY` | Search result excerpts with origin URLs |
| Tavily | `TAVILY_API_KEY` | Alternative search excerpts |
| FRED | `FRED_API_KEY` | Latest rates, CPI, unemployment and Treasury observations |
| SEC EDGAR | Contact email in setup | Filing index and selected US-GAAP/IFRS financial facts with units, periods and filing dates |
| QuantConnect | User ID + `QUANTCONNECT_API_TOKEN` + project ID | Authenticated cloud backtest list/full-result access |

Credentials are read from the environment first, then `credentials.json` in the
state directory. On Unix, the directory is mode 0700 and saved credential/config
files are mode 0600. The credential file is **not encrypted**; use environment variables if you
prefer not to persist keys. Ctrl+D in a secret field removes the locally stored
key; it cannot remove a credential inherited from the environment. There is no
telemetry or application server. Selected research evidence is sent to your model
provider, and your research query is sent to the selected search provider.

Quotes are end-of-day observations, never real-time quotes. The price endpoints
do not supply currency, so the UI displays no assumed currency. Search snippets
are not full articles; SEC indexes and selected XBRL facts are not full filings.
SEC observations preserve overlapping reporting periods and restatement dates;
the agent is instructed not to sum annual, quarterly and year-to-date records.
FRED observations use the current vintage. Every memo retains source timestamps,
limitations and its provider/model. Numbered citations are checked against the ledger; existence of
a citation does not prove that a model's interpretation is correct.

Up to six assets can be researched together. Data/search requests are bounded but
consume the connected services' quotas. Failures remain visible; live research
never silently substitutes demo data. If no evidence succeeds, no model call is
made. Missing hosted-model credentials are checked before evidence collection.

## Research workflows

Company deep dives, relative value comparisons, macro outlooks, strategy research,
and custom questions share a source-led workflow: collect evidence, search context,
then synthesize a memo. Follow-ups inherit the prior memo as untrusted context
and retrieve fresh evidence. Reports and partial work are stored in SQLite.
Interrupted runs are recovered on the next launch. One process owns each state
directory; use `--data-dir` for independent desks.

The app does not place orders or execute generated strategy code automatically.
Native provider adapters synthesize from the supplied source ledger. CLI adapters
disable their general execution/customization paths; they are model harnesses,
not autonomous access to your development workspace.

## Keyboard

Help scrolls with the keyboard or mouse wheel. Press **Esc** to dismiss an error
notice and keep the current form ready for retry. Compact layouts keep the
focused field, submission controls, job phase and cancellation visible.

| Key | Action |
| --- | --- |
| 1–6 / Tab | Navigate pages |
| Ctrl+K / `:` | Search command palette |
| `n` | New research |
| Tab / Shift+Tab | Navigate form fields |
| Ctrl+R | Submit research; Ctrl+Enter is also supported where the terminal reports it |
| Enter in question | Add a line |
| `r` on desk | Refresh daily history |
| ↑↓ / `j` / `k` | Select assets, sources or runs; scroll reports |
| PgUp / PgDn / Home / End | Navigate reports/help; previous/next/first/last archive page |
| `f` on research | Follow up |
| `e` on research/sources | Export Markdown and JSON |
| `/` in archive | Filter runs; press again to clear the filter |
| Enter / `s` in connections | Edit connections |
| `t` in connections | Test model |
| `b` in lab | Set parameters and run a historical study |
| ←→ in lab | Switch historical / LEAN / cloud tabs |
| `p` / `l` / `c` in lab | Create Python / run reviewed LEAN / read cloud results |
| Ctrl+C | Cancel the active job; otherwise quit |
| `?` / Esc / `q` | Help / close dialog / quit and save partial work |

## LEAN and QuantConnect

Install the [LEAN CLI](https://www.quantconnect.com/docs/v2/lean-cli/getting-started/installation),
start Docker, authenticate, initialize a workspace with `lean init`, and run a
backtest there once to obtain the required engine image and data. Your account,
organization permissions, and data entitlements still apply. Resen does not
install Docker or download paid data automatically.

Set the workspace in Connections. Create a reviewed starting point:

```sh
resen lean-create --language python
resen lean-create --language csharp
resen lean-run ResenTrendPython
```

The runner invokes `lean backtest <project> --output <run-folder> --no-update`,
streams diagnostics, and reads statistics from the resulting JSON files. Projects
must live inside the configured workspace. Execution is explicit and has a
twenty-minute deadline. Cancelling stops the runner; inspect Docker for any
remaining container. Python/C# templates are starting points, not validated
profitable strategies. Default template dates are 2022-01-01 through 2025-01-01.

Cloud access uses QuantConnect's timestamped SHA-256 authentication. It reads
results and does not start paid cloud jobs:

```sh
resen cloud
resen cloud --backtest-id YOUR_BACKTEST_ID
```

The built-in historical study is separate from LEAN. It models an SMA long/cash
signal using prior closes, fills at next opens, fixed costs on each side, and
terminal liquidation. Buy-and-hold uses the same period and cost assumptions.
It permits fractional shares, uses 252-period annualized zero risk-free Sharpe, and excludes dividends,
tax, financing and execution effects beyond the fixed cost. Vendor adjustment
policy can bias OHLC analysis. CAGR is omitted for periods shorter than a quarter.
Use LEAN and point-in-time data for richer validation.

## Headless use

```sh
resen doctor
resen doctor --online
resen research "Assess business durability and risks" --symbols AAPL
resen research "Compare competitive positions" --symbols AAPL,MSFT --kind comparison
resen research "Assess inflation and rates" --symbols "" --kind macro
resen history
resen export RUN_ID --format markdown --output ./memo.md
resen backtest --symbol SPY --fast 20 --slow 60 --cost-bps 10
resen backtest --csv ./prices.csv --symbol SPY
resen import-csv ./prices.csv --symbol SPY
resen snapshot --page desk --width 144 --height 46 --output ./desk.svg
```

CSV input uses `Date,Open,High,Low,Close,Volume` and ISO dates. Data must be positive,
finite and consistent; duplicate dates are rejected. The simulation requires
strict chronological order and sufficient warm-up history.

Import CSV from the command palette or CLI to use your own history in charts,
research evidence and strategy studies. Importing selects the CSV market source;
each watched asset needs its own imported file. Dates must be zero-padded and
cannot be in the future. Stooq's access requirements changed in 2026; obtain a
download key from the vendor or choose Alpha Vantage or CSV in Connections.

## Archive, recovery and migration

The archive searches **all saved runs**, including records older than the first
200. In page 5, press `/` and type or paste a query. Search matches a
case-insensitive literal substring of the run ID, question, symbols, workflow
label, provider, model or status. Report bodies and source excerpts are not
searched. `%` and `_` are ordinary characters, not wildcard operators.

The terminal loads 100 matching records per page. Use **PgUp/PgDn** to change
pages and **Home/End** to reach the first/last page. Search starts again at the
first page. Enter finishes editing the query; Enter again opens the selected
run. The archive header shows page position and the number of unreadable records.
Runs are ordered by creation time, newest first, then descending run ID for
stable ordering when timestamps match.

The existing `history` command still prints its familiar rows and defaults to
200 results. Query or page through the entire archive:

```sh
resen history --search AAPL --status complete --limit 50 --offset 0
resen history --search AAPL --status complete --limit 50 --offset 50
resen history --status interrupted --json
resen history --issues
resen history --issues --json
```

`--limit` accepts 1–1000; `--offset` counts matching readable records, starting at
zero. Status choices are `running`, `complete`, `failed`, `cancelled`, and
`interrupted`. Startup recovers abandoned running records before listing them,
so a normal new CLI process will not list them as `running`. JSON history has
`runs` (the full existing run objects), `total` (matching readable records),
`offset`, and `unreadable` (the archive-wide count). `--issues --json` instead
returns an array of `{ "id": ..., "reason": ... }`. Issue mode cannot be combined
with search, status or pagination options. Plain history prints its counts to
stderr, leaving the row format on stdout unchanged.

### What gets saved

A running record is committed before research starts. New sources and warnings
are saved immediately. Streamed report text is checkpointed after one second or
64 KiB of new text, whichever comes first. Both frontends run maintenance while
a provider is silent: every 125 ms in the terminal, every 250 ms in headless
research. These are scheduling targets, not hard real-time guarantees; a blocked
disk or process can delay a write. Completion and cancellation flush all accepted
text. Cancellation drains already queued events before invalidating the job, so
late messages cannot replace the next run.

Normal provider errors and unexpectedly stopped workers retain partial work in
a failed run. A failed write retains the in-memory result for retry; the terminal
retries during maintenance and shows the error. New work is blocked while an
unsaved result remains. A headless persistence error exits nonzero after attempting
cleanup. If the storage device remains unavailable, unsaved data cannot be
promised durable.

On the next launch, one transaction checks the **entire archive**, marks abandoned
`running` runs `interrupted`, and preserves their saved memo, evidence and elapsed
time. Recovery is idempotent and does not restart a model call. Open the partial
run and use a follow-up to begin a new investigation. SIGKILL or power loss can
lose text since the last successful checkpoint; they cannot execute child-process
cleanup. Inspect any external CLI process or Docker container after a hard kill.

### Existing databases and unreadable records

Schema 0/1 archives migrate automatically to schema 2 in a transaction. Migration
adds status/search metadata, unreadable-record diagnostics and ordering indexes.
The JSON run format and run IDs remain unchanged. Completed JSON bodies are not
rewritten; indexed timestamps are normalized to UTC with fixed precision. A failed
migration rolls back its schema and version changes. A database with a newer
schema is rejected instead of having its version overwritten.

Malformed JSON or a JSON ID that differs from its row ID does not block healthy
history. The original `runs.body` remains in `research.db`; a diagnostic marks
that row unreadable and excludes it from normal results. `history --issues`
identifies these rows without dumping their contents. Exporting an unreadable ID
fails explicitly. A corrected record is rechecked and restored to the searchable
archive on the next launch. This isolates malformed records; it cannot repair a
physically damaged SQLite file or reconstruct missing JSON.

Before upgrading or manually repairing data, close every Resen process using the
state directory and copy the **whole directory**, including any SQLite WAL/SHM
files. For example, with an explicitly selected directory:

```sh
# Run only after closing the desk using this directory.
cp -R "$HOME/resen-state" "$HOME/resen-state-backup"
resen --data-dir "$HOME/resen-state" history --json
```

There is no automatic backup or downgrade migration. Keep the backup if you need
to return to an older binary; do not point an old release at a migrated database.
Startup validation reads one record at a time but scans the full archive, so its
cost grows with total saved content. Search scans normalized metadata rather than
report bodies. Offset pagination is not a frozen snapshot across separate
commands: adding a new run can shift later pages. There is no retention/deletion
policy, cross-process concurrent desk, or full-text report search.

## Configuration and local files

Run `resen doctor` to print the selected state directory and readiness summary.
`--data-dir PATH` is global and works with every command; otherwise the OS-specific
local application-data directory is selected. One process holds `session.lock`
for that directory. A second process must wait or select a different directory;
removing the lock file while a desk is open is not a supported recovery procedure.

| File or directory | Purpose |
| --- | --- |
| `config.toml` | Model, source, watchlist and workspace preferences |
| `credentials.json` | Optional private plaintext credentials; environment values take precedence |
| `research.db`, `research.db-wal`, `research.db-shm` | SQLite archive and write-ahead-log state |
| `session.lock` | OS-managed exclusive desk lock; the file can remain after exit |
| `imports/` | Imported daily history and provenance |
| `exports/` | Markdown/JSON memos and the latest historical study |
| `agent-workspace/` | Working directory for isolated model CLI synthesis |
| `backtests/` | Explicit LEAN execution output |

Connections is the recommended settings editor. A minimal manual local-model
configuration, after installing and starting your compatible server, is:

```toml
provider = "compatible"
endpoint = "http://localhost:1234/v1"
model = "your-loaded-model"
max_output_tokens = 4096
watchlist = ["AAPL", "MSFT", "SPY"]
data_provider = "csv"
search_provider = "brave"
setup_complete = true
```

Select the exact model name supplied by your server and import a CSV for each
watched asset. A model connection alone supplies no market evidence. Other
provider values are `open_ai`, `anthropic`, `ollama`, `codex`, and `claude`.
Watchlists require 1–6 unique uppercase symbols. Research accepts up to six assets;
`--symbols ""` remains valid for asset-free macro questions. `$AAPL` normalizes to
`AAPL`, while a bare `$` is rejected. Questions must be nonblank and no more than
16,000 UTF-8 bytes.

Optional configuration keys are `sec_contact`, `lean_workspace`, `qc_user_id`,
and `qc_project_id`; their uses and required credentials are listed above.
`data_provider` is `stooq`, `alpha_vantage`, or `csv`, and `search_provider` is
`brave` or `tavily`. `max_output_tokens` accepts 256–16384. Do not put credentials
in endpoint URLs or command arguments. The archive and exports contain research
and source content and should be included in your own privacy and backup policy.

## How the application fits together

Resen is a single Rust process. CLI commands and terminal input share `App`, which
owns the archive, current memo and one active background job. Workers receive
cloned settings and return generation-tagged events. A shared supervisor detects
workers that panic or return without a result; completion and cancellation retire
the same job handles. The checkpoint policy is independent of redraw ticks.

`research` gathers evidence and makes one model call; `data` and `provider` own
the external protocols; `backtest` owns the historical simulator and explicit
LEAN/cloud operations. `store` owns migrations, recovery and archive queries.
`jobs` owns checkpoint policy and worker supervision, while `ui` only renders
application state. See [architecture](docs/ARCHITECTURE.md) for the ownership and
failure contracts.

## Development and validation

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo build --release --locked
python3 scripts/install_qa.py --binary target/release/resen
python3 scripts/pty_qa.py --binary target/release/resen
python3 scripts/process_qa.py --binary target/release/resen
```

The HTTP contract tests bind loopback ports and need a sandbox that permits that.
They use mock providers, not paid accounts. The PTY test is Unix-only and uses
Python's standard library. CI is configured for macOS, Linux and Windows; release
packaging is defined for tagged versions, and CI includes a RustSec dependency
audit. Archive regressions cover schema rollback and future-version rejection,
full recovery beyond 200 records, malformed/mismatched JSON, stable pagination,
CLI JSON queries and terminal search. Lifecycle tests cover stalled-stream
checkpoints, queued cancellation, stale events, failed-save retries and unexpected
worker exits. Process QA runs a controlled provider executable, verifies partial
text is checkpointed while it stalls, and checks SIGINT/SIGTERM cancellation plus
SIGKILL recovery on the next launch. No paid model, market service or Docker
engine is required by these tests. See [UI research](docs/UI_RESEARCH.md),
[architecture](docs/ARCHITECTURE.md), [QA evidence](docs/QA.md), and the
[UI review, completed fixes and next improvements](docs/UI_UPGRADE_PLAN.md).

[MIT](LICENSE). Built with [Ratatui](https://ratatui.rs/) and Rust.
