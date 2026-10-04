# QA evidence

Validation date: 2 October 2026. Local host: macOS arm64, Rust/Cargo 1.99.0.
This is a release candidate; the evidence below distinguishes local application
verification from account-dependent service acceptance. Final checks completed
2 October 2026 on macOS arm64 and Linux arm64.

## Verified application behavior

- Optimized macOS and Linux arm64 executables build successfully; version and
  command help work.
- Formatting and Clippy across all targets pass with warnings denied on both
  toolchains: macOS Rust 1.99.0 and Linux Rust 1.89.0.
- All 60 Rust integration tests pass, including the loopback HTTP contract suite,
  financial fixtures, bounded-I/O failures, UI, persistence and complete CLI workflows,
  on both platforms.
- 24 actual executable checks pass through a Unix pseudo-terminal. They cover
  Unicode bracketed paste, research submission, source persistence, exports,
  archive navigation, strategy execution, resizing, help, cancellation, normal
  exit, SIGTERM and SIGINT. Screen, paste mode and terminal attributes are restored.
- The signal checks passed again after fixing a listener-lifetime race.
- 24 additional executable process checks verify a real test-provider child is
  stopped on headless SIGINT/SIGTERM, terminal quit and terminal SIGTERM. Partial
  source ledgers are saved, and interactive terminal state is restored. Both
  24-check suites pass on both platforms.
- RustSec audit of 338 locked dependencies reports zero known vulnerabilities
  and zero advisory warnings (database updated 2 October 2026).
- All six pages render at eight sizes, including the 60×18 minimum and an
  intentionally undersized terminal. Dialogs render at three supported sizes.
- Wide and 80×24 compact buffers were rendered to SVG/PNG and visually inspected.
  These are the actual Ratatui buffers, with visibly labeled synthetic data.

Previews: [desk](assets/desk.png), [research](assets/research.png),
[setup](assets/setup.png), [CSV import](assets/import.png). The corresponding SVG
files retain the styled terminal cell buffer.

## Real model connection

The release executable completed one authenticated Codex CLI research request
through the user's existing ChatGPT authentication. It took 7,198 ms, collected
one imported CSV source and saved a complete memo with a valid `[1]` citation.
The prompt identified the file as a synthetic QA fixture, and the model explicitly
preserved that distinction. A second source-only run using the restricted
tool configuration completed in 7,340 ms, again preserving fixture provenance
and unspecified currency/volume units. This proves the model/persistence/source workflow;
it is not a financial result or a live market-data acceptance test.

The CLI connection checker also verified Codex version 0.160.0 and authentication
without generation. Claude's installed CLI is currently signed out. Its isolated
generation protocol and authentication-status behavior pass controlled contract
tests. OpenAI-compatible streaming and Anthropic Messages use local HTTP mocks.

## Financial and source checks

- Exact entry/exit costs and benchmark results match independent mathematical
  fixtures. Signals use prior closes and fill at the next open.
- Changing future prices cannot change earlier portfolio equity. Unsorted,
  duplicate, inconsistent, nonfinite and future-dated prices are rejected.
- Stable moving averages retain equality on constant prices. Extreme numeric
  scales fail clearly rather than exporting nonfinite metrics.
- FRED evidence retains units, frequency, seasonal adjustment and missing-value
  semantics. Its current retrieval vintage is disclosed.
- SEC facts retain currency/unit distinctions, overlapping durations, original
  values, fiscal labels, filing dates, accession numbers and amendments. Facts
  are not automatically summed or transformed into valuation conclusions.
- A live Stooq download probe failed to return CSV. Research identified its 2026
  API-key access change; setup now requests the key and offers imported CSV.
  No sample prices are substituted for a failed vendor request.

## Weaknesses found and resolved

| Finding | Resolution and evidence |
| --- | --- |
| CLI diagnostics could fill stderr pipes | Concurrent draining; stress fixtures exceed 64 KB |
| Cancelled-job messages could affect a new run | Generation tags and a stale-completion regression |
| Secrets could cross stream chunk boundaries | Prefix-retaining redaction tested at every split |
| Mixed SSE newline formats could merge frames | Earliest-delimiter parsing and ordering regression |
| CSV import could retain demo prices | Clear synthetic series on mode transition |
| Invalid config could reach the renderer | Validate persisted configuration before startup |
| Credential parse errors could echo malformed values | Generic credential-file diagnostics |
| Headless LEAN logs bypassed redaction | Redact before printing; secret-bearing log fixture |
| Annual/quarterly SEC records could lose context | Retain period and filing metadata; no aggregation |
| OS SIGINT could be missed between redraws | Keep the signal future alive across event iterations |
| PTY shutdown could block on unread output | Drain the terminal while waiting for signal exit |
| Closing the app could detach a cancelled worker | Abort and await cancelled jobs before shutdown; physical child-process QA |
| Short credentials could repeatedly transform the mask | Scan original input and preserve existing masks; overlapping-key regression |
| Compact source view hid provenance | Size the table to its entries; 80x24 origin-visibility regression |
| Oversized child output could allocate before validation | Bounded UTF-8 line reads; LEAN forwards at most 8 MB per stream while draining excess output; large-output fixture |

## External acceptance gates

Live Alpha Vantage, authenticated Stooq, Brave/Tavily, FRED, SEC and QuantConnect
service acceptance needs the appropriate contact information, keys and account
entitlements. Their current API documentation was researched; no credentials,
quotes or live backtest statistics have been fabricated.

LEAN scaffolding, command arguments, diagnostic handling and result parsing pass
controlled executable tests. Real LEAN CLI 1.0.229 was installed in an isolated QA
environment; its backtest options match Resen's invocation. Docker Desktop's
Linux arm64 daemon is running (29.5.3). No QuantConnect credentials were found,
so account-authenticated engine execution and data entitlements remain unverified.
Cancellation kills the runner; a Docker container may remain and needs inspection,
as documented in the app guide.

The result reader's lowercase `statistics` key matches LEAN's
[camel-case serializer](https://github.com/QuantConnect/Lean/blob/master/Engine/Results/BaseResultsHandler.cs).
The engine writes a compact `-summary.json` with the same statistics in its
[backtest result handler](https://github.com/QuantConnect/Lean/blob/master/Engine/Results/BacktestingResultHandler.cs).
This verifies the format against source, without claiming an authenticated run.

Linux arm64 passed formatting, Clippy, all 60 tests, the optimized build and both
24-check executable suites using Rust 1.89.0 in an isolated official Debian
Bookworm container. Its image digest is
`sha256:948f9b08a66e7fe01b03a98ef1c7568292e07ec2e4fe90d88c07bb14563c84ff`.
The source, tests and scripts match the macOS candidate byte for byte. Windows
CI is configured but has not run here. Local archives include source and
checksums; public release artifacts are not published, Developer ID signed
or notarized.

Credentials saved by setup use a private plaintext file, not an OS secret vault.
Citation-ID validation does not prove that every model claim is supported.
The historical simulator uses fractional shares, fixed costs, 252-period Sharpe
annualization and a zero risk-free rate; dividends, taxes, realistic market
impact and point-in-time data reconstruction are outside its model.

## Reproduce

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo build --release --locked
python3 scripts/pty_qa.py --binary target/release/resen
python3 scripts/process_qa.py --binary target/release/resen
```

The HTTP contract suite requires loopback socket permission. Runtime evidence,
model QA records and transient tooling live under ignored `.qa/`; they are not
part of the application distribution.

## UI upgrade verification — 3 October 2026

The [UI review and upgrade plan](UI_UPGRADE_PLAN.md) records the observed issues,
implemented fixes, compact screenshots and remaining product improvements.
Fresh local formatting, Clippy, all 79 tests and the optimized macOS arm64 build
pass. This includes 19 new UI regressions, with actual content/interaction
assertions beyond the original crash-free rendering checks.

Both committed executable QA suites pass again (24 PTY and 24 process checks).
An additional isolated actual-executable walkthrough passes 30 checks, including
all setup stages, six pages, help scrolling, Unicode cursor visibility, pasted
searches, form error dismissal/retry, studies, resize, cancellation and minimum-size
exit. Terminal attributes, alternate screen and bracketed-paste mode restore.
54 styled buffers were rendered for responsive and dialog inspection. Updated
README screenshots and compact examples show labeled synthetic fixtures.

Native Terminal access was blocked by the computer-use tool; foreground terminal
emulator acceptance is still outstanding. No live API acceptance is claimed.
Runtime reports are retained locally under ignored `.qa/ui-audit/`.
