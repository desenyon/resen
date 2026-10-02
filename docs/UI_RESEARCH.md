# Terminal UI research and design decision

Research performed on 2 October 2026, using current official documentation and
Context7. The requirement is an executable that renders *inside* an existing
terminal. Visual quality also depends on font, cell size, and the terminal emulator.

| Option | Strength | Tradeoff for Resen |
| --- | --- | --- |
| [GPUI](https://github.com/zed-industries/zed/blob/main/crates/gpui/README.md) | GPU-accelerated native UI; rich graphical layout | Creates graphical windows. Does not satisfy the terminal-native requirement. |
| [Ratatui](https://docs.rs/ratatui/0.30.2/ratatui/) | Precise cell-level control, Braille charts, tables, buffer testing; native Rust executable | Application state, forms, async work, and styling need careful composition. |
| [Charm Bubble Tea](https://github.com/charmbracelet/bubbletea) with [Lip Gloss](https://github.com/charmbracelet/lipgloss) and [Bubbles](https://github.com/charmbracelet/bubbles) | Excellent terminal components and styling; Go binaries; model/update/view architecture | Strong alternative. Financial charts still need extra components or custom work. |
| [Textual](https://textual.textualize.io/) | Rich retained widgets, CSS-like layout, reactive app model and a strong test pilot | Python runtime/packaging tradeoff compared with a Rust executable. |

**Decision: Rust, Ratatui 0.30.2, Crossterm, and Tokio.** This is a fit decision,
not a claim that one library is objectively the most beautiful. Ratatui offers the
rendering precision and chart primitives this research desk needs. See the
[official widget gallery](https://ratatui.rs/showcase/widgets/) and
[testing recipes](https://ratatui.rs/recipes/testing/).

## Components selected

- Rounded blocks with restrained borders and a dark slate palette.
- Braille line charts for price history and comparative strategy equity.
- Compact sparklines for the selected watchlist window.
- Stateful tables for source provenance and research history.
- Scrollable Markdown paragraphs with rendered-line counts.
- Searchable command palette and custom Unicode-safe, masked form inputs.
- Source/phase trace, notices, explicit sample-data badges and empty states.
- Responsive layouts: sidebar at 110+ columns, compact view down to 60x18.

The rendered-line-count API is behind Ratatui's
`unstable-rendered-line-info` feature. The library version is pinned and wrapping
behavior is covered by tests. Other components use standard Ratatui widgets.
No Nerd Font, graphical window, browser, or terminal image protocol is required.

## Visual principles

Mint identifies focus and actions. Blue identifies secondary analysis; amber
identifies limitations; muted slate carries metadata. Tables use restrained
selection surfaces. Financial values never imply a currency absent from the
source. The UI distinguishes configuration from a verified connection, end-of-day
prices from real-time prices, and demo fixtures from research evidence.

SVG previews are generated from the application's actual Ratatui buffer, rather
than a separate web mockup. Real executable interaction is exercised by the PTY
QA script, including resize and terminal restoration.
