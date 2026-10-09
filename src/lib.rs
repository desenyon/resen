pub mod app;
pub mod backtest;
pub mod config;
pub mod data;
pub mod domain;
pub mod jobs;
pub mod process;
pub mod provider;
pub mod research;
pub mod store;
pub mod ui;

/// External content must not be able to issue terminal escape sequences.
pub fn clean_text(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .collect()
}
