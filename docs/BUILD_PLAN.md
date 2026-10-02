# Resen acceptance plan

## Product

A beautiful terminal executable for financial research. Rust + Ratatui 0.30,
Crossterm for input, Tokio for background work, SQLite for local research history.
No server or browser required. The terminal font and renderer remain the user's.

## Acceptance

- Polished research desk, watchlist, chart, research workspace, sources, archive,
  strategy lab, connections, help, searchable command palette and first-run setup.
- Keyboard-first navigation, bracketed paste, Unicode input, mouse scrolling,
  resizing, narrow layouts, terminal restoration and graceful cancellation.
- OpenAI-compatible endpoints including Ollama, direct Anthropic, Codex CLI and
  Claude CLI. Connection checks. Existing CLI authentication stays with each CLI.
- Daily history, imported CSV, web search, macro metadata and SEC reported facts with explicit timestamps,
  provenance, source failures and no silent fallback to sample data.
- Bounded research workflow with source collection and model synthesis; stored
  results, citations, follow-up research and Markdown/JSON export.
- QuantConnect authenticated connection and cloud results; LEAN local Python/C#
  scaffolding, explicit execution of reviewed projects, logged progress/results.
- Built-in transparent historical strategy analysis as an independent workflow;
  no claim that this substitutes for LEAN or executes trades.
- Protected credential storage and redaction, no implicit paid model calls on
  startup, no automatic execution of generated code, budgeted/time-bounded jobs.
- Unit, mocked HTTP contract, integration, layout and interactive PTY tests;
  format/lint/release build and an evidence-based QA report.

## Waves

1. Foundation and UI -> tests of persistence, input, layouts and setup.
2. Research -> tests of provider/data contracts and end-to-end recorded research.
3. Strategy lab -> mathematical fixtures, LEAN commands and cloud auth/results.
4. QA -> real PTY exercise, visual inspection, failure review, fixes and release.

## Boundaries

Financial research, not brokerage/order execution. Live paid API acceptance needs
user credentials and provider entitlement; those gates are reported honestly.
End-of-day data is never presented as real-time market data. Sample mode never
contacts providers. Stop when verified acceptance is met or twelve hours/usage
limits are reached. The build began 2026-10-02 07:38:58 UTC.
