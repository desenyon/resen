# Resen

Build a terminal-native financial research desk. Keep demo data visibly labeled;
never invent live sources, quotes, credentials, or backtest results. Do not execute
model-generated code automatically. Never log secrets or place them in process
arguments. Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and
`cargo test` for functional changes. Use graph tools first for code discovery.

Keep the UI responsive while jobs run; every external operation needs a timeout
and cancellation. Preserve the terminal on normal exit, errors, and panics.
