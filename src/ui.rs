use crate::{
    app::{App, COMMANDS, Input, Modal, Page, SettingsForm},
    domain::MarketSeries,
};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span, Text},
    widgets::{
        Axis, Block, BorderType, Borders, Chart, Clear, Dataset, GraphType, List, ListItem,
        ListState, Padding, Paragraph, Row, Scrollbar, ScrollbarOrientation, ScrollbarState,
        Sparkline, Table, TableState, Wrap,
    },
};

pub const BG: Color = Color::Rgb(12, 16, 21);
pub const PANEL: Color = Color::Rgb(17, 23, 30);
pub const RAISED: Color = Color::Rgb(23, 31, 40);
pub const LINE: Color = Color::Rgb(38, 49, 61);
pub const TEXT: Color = Color::Rgb(221, 230, 236);
pub const MUTED: Color = Color::Rgb(132, 151, 167);
pub const DIM: Color = Color::Rgb(84, 104, 123);
pub const MINT: Color = Color::Rgb(133, 224, 190);
pub const BLUE: Color = Color::Rgb(137, 176, 234);
pub const GOLD: Color = Color::Rgb(229, 191, 123);
pub const RED: Color = Color::Rgb(238, 143, 145);
pub const SELECT: Color = Color::Rgb(27, 49, 48);

fn style(color: Color) -> Style {
    Style::new().fg(color)
}
fn bold(color: Color) -> Style {
    style(color).add_modifier(Modifier::BOLD)
}
fn panel(title: impl Into<Line<'static>>) -> Block<'static> {
    Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(style(LINE))
        .title(title)
        .title_style(style(MUTED))
        .style(Style::new().bg(PANEL))
        .padding(Padding::new(1, 1, 0, 0))
}
fn text(f: &mut Frame, area: Rect, value: impl Into<Text<'static>>, color: Color) {
    f.render_widget(
        Paragraph::new(value)
            .style(style(color))
            .wrap(Wrap { trim: false }),
        area,
    );
}
fn inset(area: Rect, h: u16, v: u16) -> Rect {
    area.inner(Margin {
        horizontal: h,
        vertical: v,
    })
}
fn split(area: Rect, constraints: impl IntoIterator<Item = Constraint>) -> std::rc::Rc<[Rect]> {
    Layout::vertical(constraints).split(area)
}

pub fn render(f: &mut Frame, app: &mut App) {
    let area = f.area();
    f.render_widget(Block::new().style(Style::new().bg(BG).fg(TEXT)), area);
    if area.width < 60 || area.height < 18 {
        text(
            f,
            inset(area, 2, 1),
            "RESEN\n\nGive your research a little more room.\nMinimum: 60 columns x 18 rows.\n\nResize the terminal or press q to exit.",
            MINT,
        );
        return;
    }
    let root = split(
        area,
        [
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(2),
        ],
    );
    header(f, root[0], app);
    let show_sidebar = area.width >= 110;
    let body = Layout::horizontal([
        Constraint::Length(if show_sidebar { 23 } else { 0 }),
        Constraint::Min(0),
    ])
    .split(root[1]);
    if show_sidebar {
        sidebar(f, body[0], app);
    }
    let content = inset(body[1], 2, 1);
    match app.page {
        Page::Desk => desk(f, content, app),
        Page::Research => research(f, content, app),
        Page::Sources => sources(f, content, app),
        Page::Lab => lab(f, content, app),
        Page::Archive => archive(f, content, app),
        Page::Connections => connections(f, content, app),
    }
    footer(f, root[2], app);
    if let Some(modal) = &app.modal {
        render_modal(f, area, modal, app);
    }
    if let Some((message, _, error)) = &app.toast {
        let width = area.width.saturating_sub(6).min(104);
        let height = if message.chars().count() > width as usize {
            5
        } else {
            3
        };
        let toast = Rect::new(
            area.x + area.width.saturating_sub(width) / 2,
            area.y + area.height.saturating_sub(height + 2),
            width,
            height,
        );
        f.render_widget(Clear, toast);
        f.render_widget(
            Paragraph::new(message.clone())
                .style(style(if *error { RED } else { MINT }))
                .block(panel(if *error { " Attention " } else { " Saved " }))
                .wrap(Wrap { trim: false }),
            toast,
        );
    }
}
fn header(f: &mut Frame, area: Rect, app: &App) {
    let row = Layout::horizontal([
        Constraint::Length(24),
        Constraint::Min(0),
        Constraint::Length(32),
    ])
    .split(inset(area, 2, 0));
    text(
        f,
        row[0],
        Line::from(vec![
            Span::styled("◈  ", bold(MINT)),
            Span::styled("r e s e n", bold(TEXT)),
            Span::styled("  /  01", style(DIM)),
        ]),
        TEXT,
    );
    text(
        f,
        row[1],
        Line::from(vec![
            Span::styled("FINANCIAL RESEARCH", style(MUTED)),
            Span::styled("   /   ", style(DIM)),
            Span::styled(app.page.label(), style(TEXT)),
        ]),
        TEXT,
    );
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                if app.demo { "● DEMO" } else { "● RESEARCH" },
                style(if app.demo { GOLD } else { MINT }),
            ),
            Span::styled(
                format!("    {} UTC", chrono::Utc::now().format("%H:%M")),
                style(MUTED),
            ),
        ]))
        .alignment(Alignment::Right),
        row[2],
    );
    let line = Rect::new(
        area.x,
        area.y + area.height.saturating_sub(1),
        area.width,
        1,
    );
    f.render_widget(
        Block::new()
            .borders(Borders::BOTTOM)
            .border_style(style(LINE)),
        line,
    );
}
fn sidebar(f: &mut Frame, area: Rect, app: &App) {
    f.render_widget(
        Block::new()
            .style(Style::new().bg(PANEL))
            .borders(Borders::RIGHT)
            .border_style(style(LINE)),
        area,
    );
    let a = inset(area, 2, 1);
    text(f, Rect::new(a.x, a.y, a.width, 1), "WORKSPACE", DIM);
    for (i, page) in Page::ALL.iter().enumerate() {
        let row = Rect::new(area.x + 1, a.y + 2 + i as u16 * 2, area.width - 2, 1);
        let selected = *page == app.page;
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(if selected { "▎ " } else { "  " }, style(MINT)),
                Span::styled(
                    format!("{}  {}", i + 1, page.label()),
                    if selected { bold(MINT) } else { style(MUTED) },
                ),
            ]))
            .style(Style::new().bg(if selected { SELECT } else { PANEL })),
            row,
        );
    }
    if a.height >= 24 {
        let bottom = Rect::new(a.x, area.y + area.height.saturating_sub(11), a.width, 9);
        let model = if app.config.model.is_empty() {
            "CLI default model".to_string()
        } else {
            app.config.model.clone()
        };
        text(
            f,
            bottom,
            Text::from(vec![
                Line::styled("ENGINE", style(DIM)),
                Line::raw(""),
                Line::styled(app.config.provider.label(), style(TEXT)),
                Line::styled(model, style(MUTED)),
                Line::raw(""),
                Line::styled(
                    if app.busy() {
                        "◌ working"
                    } else {
                        "○ ready"
                    },
                    style(if app.busy() { GOLD } else { MINT }),
                ),
                Line::styled(
                    if app.demo {
                        "Synthetic data only"
                    } else {
                        "Local research history"
                    },
                    style(DIM),
                ),
                Line::raw(""),
                Line::styled("ctrl+k  command palette", style(MUTED)),
            ]),
            TEXT,
        );
    }
}
fn footer(f: &mut Frame, area: Rect, app: &App) {
    let hint = match app.page {
        Page::Desk => "n new research   r refresh   ↑↓ asset",
        Page::Research => "f follow-up   e export   ↑↓ scroll",
        Page::Sources => "↑↓ source   PgUp/PgDn excerpt   e export",
        Page::Lab => "b backtest   ←→ tabs   p Python   l run LEAN   c cloud",
        Page::Archive => "/ search   ↑↓ select   enter open",
        Page::Connections => "enter edit   t test model   PgUp/PgDn scroll",
    };
    let parts =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(29)]).split(inset(area, 2, 0));
    text(
        f,
        parts[0],
        Line::from(vec![
            Span::styled(hint, style(MUTED)),
            Span::styled(
                if app.busy() { "   ctrl+c cancel" } else { "" },
                style(GOLD),
            ),
        ]),
        MUTED,
    );
    f.render_widget(
        Paragraph::new("1–6 pages  ? help  q quit")
            .style(style(DIM))
            .alignment(Alignment::Right),
        parts[1],
    );
}
fn title(f: &mut Frame, area: Rect, eyebrow: &str, heading: &str, caption: &str) {
    let rows = split(
        area,
        [
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Min(0),
        ],
    );
    text(f, rows[0], eyebrow.to_string(), MINT);
    text(
        f,
        rows[1],
        Line::styled(heading.to_string(), bold(TEXT)),
        TEXT,
    );
    text(f, rows[2], caption.to_string(), MUTED);
}
fn desk(f: &mut Frame, area: Rect, app: &mut App) {
    let compact = area.height < 29;
    let rows = split(
        area,
        [
            Constraint::Length(if compact { 4 } else { 5 }),
            Constraint::Length(if compact { 5 } else { 6 }),
            Constraint::Min(5),
            Constraint::Length(if compact { 0 } else { 6 }),
        ],
    );
    let intro = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(if area.width >= 100 { 31 } else { 0 }),
    ])
    .split(rows[0]);
    title(
        f,
        intro[0],
        "THE RESEARCH DESK",
        "A clearer view of the market.",
        if app.demo {
            "A working sample of your desk. Every price below is synthetic."
        } else {
            "Evidence first. Thoughtful analysis. Your models, your sources."
        },
    );
    if intro[1].width > 0 {
        text(
            f,
            intro[1],
            Text::from(vec![
                Line::styled("RESEARCH ON YOUR TERMS", style(DIM)),
                Line::raw(""),
                Line::styled("Bring your own intelligence.", style(MUTED)),
                Line::styled("Keep your work local.", style(MUTED)),
            ]),
            MUTED,
        );
    }
    let count = app
        .config
        .watchlist
        .len()
        .min(if area.width < 80 { 3 } else { 4 });
    let cards = Layout::horizontal(vec![Constraint::Ratio(1, count.max(1) as u32); count])
        .spacing(1)
        .split(rows[1]);
    let first = app.selected.saturating_sub(count.saturating_sub(1));
    for (i, card) in cards.iter().enumerate() {
        let index = first + i;
        let symbol = &app.config.watchlist[index];
        quote_card(
            f,
            *card,
            symbol,
            app.markets.get(symbol),
            index == app.selected,
            app.demo,
        );
    }
    let lower = Layout::horizontal([
        Constraint::Percentage(if area.width < 88 { 100 } else { 64 }),
        Constraint::Min(0),
    ])
    .spacing(2)
    .split(rows[2]);
    if let Some(series) = app.markets.get(app.selected_symbol()) {
        price_chart(f, lower[0], series);
    } else {
        let symbol = app.selected_symbol();
        let message = if let Some(error) = app.market_errors.get(symbol) {
            format!(
                "{symbol}\n\nSource unavailable\n{error}\n\nEdit Connections or press r to retry."
            )
        } else {
            format!(
                "{symbol} / daily history\n\nYour market canvas is ready.\n\nPress r to load end-of-day prices.\nPress n to start a research question."
            )
        };
        f.render_widget(
            Paragraph::new(message)
                .style(style(MUTED))
                .block(panel(" MARKET CONTEXT "))
                .wrap(Wrap { trim: false }),
            lower[0],
        );
    }
    if lower[1].width > 0 {
        workflow_panel(f, lower[1]);
    }
    if !compact {
        let block = panel(" RECENT RESEARCH ");
        let inner = block.inner(rows[3]);
        f.render_widget(block, rows[3]);
        if app.runs.is_empty() {
            text(
                f,
                inset(inner, 0, 1),
                "Your next insight starts here. Press n to ask a question.\nCompleted and interrupted research stays in your local archive.",
                MUTED,
            );
        } else {
            let items: Vec<_> = app
                .runs
                .iter()
                .take(3)
                .map(|r| {
                    Row::new([
                        r.request.kind.label().to_string(),
                        r.request.symbols.join(" · "),
                        r.status.clone(),
                        r.created_at.format("%b %d · %H:%M").to_string(),
                    ])
                    .style(style(MUTED))
                })
                .collect();
            f.render_widget(
                Table::new(
                    items,
                    [
                        Constraint::Percentage(35),
                        Constraint::Percentage(25),
                        Constraint::Percentage(20),
                        Constraint::Percentage(20),
                    ],
                ),
                inner,
            );
        }
    }
}
fn quote_card(
    f: &mut Frame,
    area: Rect,
    symbol: &str,
    series: Option<&MarketSeries>,
    selected: bool,
    demo: bool,
) {
    let demo = series.map_or(demo, |series| series.demo);
    let block = panel(Line::from(vec![
        Span::styled(
            format!(" {symbol} "),
            if selected { bold(MINT) } else { bold(TEXT) },
        ),
        Span::styled(if demo { " SAMPLE " } else { " EOD " }, style(DIM)),
    ]))
    .border_style(style(if selected { MINT } else { LINE }));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let rows = split(
        inner,
        [
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ],
    );
    if let Some(series) = series {
        let price = series
            .last()
            .map(|b| format!("{:.2}", b.close))
            .unwrap_or_else(|| "—".into());
        let change = series.change_pct().unwrap_or(0.0);
        text(
            f,
            rows[0],
            Line::from(vec![
                Span::styled(price, bold(TEXT)),
                Span::styled(
                    format!("  {change:+.2}%"),
                    style(if change >= 0.0 { MINT } else { RED }),
                ),
            ]),
            TEXT,
        );
        let data: Vec<_> = series
            .bars
            .iter()
            .rev()
            .take(35)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(|b| (b.close * 100.0) as u64)
            .collect();
        let min = data.iter().min().copied().unwrap_or(0);
        let values: Vec<_> = data.iter().map(|v| v - min + 1).collect();
        f.render_widget(
            Sparkline::default()
                .data(&values)
                .style(style(if selected { MINT } else { BLUE })),
            rows[1],
        );
        if rows[2].height > 0 {
            text(
                f,
                rows[2],
                series.last().map(|b| b.date.clone()).unwrap_or_default(),
                DIM,
            );
        }
    } else {
        text(f, inner, "—\nawaiting data", DIM);
    }
}
fn price_chart(f: &mut Frame, area: Rect, series: &MarketSeries) {
    let block = panel(format!(" {} / PRICE CONTEXT ", series.symbol));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height < 4 {
        return;
    }
    let rows = split(
        inner,
        [
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ],
    );
    text(
        f,
        rows[0],
        Line::from(vec![
            Span::styled(
                if series.demo {
                    "SYNTHETIC SERIES"
                } else {
                    "DAILY CLOSE · NOT REAL TIME"
                },
                style(DIM),
            ),
            Span::styled(
                format!("    {} observations", series.bars.len()),
                style(MUTED),
            ),
        ]),
        MUTED,
    );
    let points: Vec<_> = series
        .bars
        .iter()
        .enumerate()
        .map(|(i, b)| (i as f64, b.close))
        .collect();
    let min = points.iter().map(|(_, v)| *v).fold(f64::INFINITY, f64::min);
    let max = points
        .iter()
        .map(|(_, v)| *v)
        .fold(f64::NEG_INFINITY, f64::max);
    let pad = ((max - min) * 0.12).max(max * 0.002);
    let data = Dataset::default()
        .data(&points)
        .marker(symbols::Marker::Braille)
        .graph_type(GraphType::Line)
        .style(style(MINT));
    f.render_widget(
        Chart::new(vec![data])
            .style(Style::new().bg(PANEL))
            .x_axis(
                Axis::default()
                    .bounds([0.0, (points.len().saturating_sub(1)).max(1) as f64])
                    .style(style(LINE))
                    .labels([
                        Span::styled(
                            series
                                .bars
                                .first()
                                .map(|b| b.date.clone())
                                .unwrap_or_default(),
                            style(DIM),
                        ),
                        Span::styled(
                            series.last().map(|b| b.date.clone()).unwrap_or_default(),
                            style(DIM),
                        ),
                    ]),
            )
            .y_axis(
                Axis::default()
                    .bounds([min - pad, max + pad])
                    .style(style(LINE))
                    .labels([
                        Span::styled(format!("{min:.0}"), style(DIM)),
                        Span::styled(format!("{max:.0}"), style(DIM)),
                    ]),
            ),
        rows[1],
    );
    text(f, rows[2], series.source.clone(), DIM);
}
fn workflow_panel(f: &mut Frame, area: Rect) {
    let block = panel(" FIND YOUR NEXT QUESTION ");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let content = Text::from(vec![
        Line::raw(""),
        Line::styled("01  Company deep dive", bold(TEXT)),
        Line::styled("    Business, value, catalysts, risk", style(DIM)),
        Line::raw(""),
        Line::styled("02  Relative value", bold(TEXT)),
        Line::styled("    Compare the opportunity set", style(DIM)),
        Line::raw(""),
        Line::styled("03  Macro outlook", bold(TEXT)),
        Line::styled("    Rates, inflation, market regime", style(DIM)),
        Line::raw(""),
        Line::styled("04  Strategy research", bold(TEXT)),
        Line::styled("    Hypothesis → evidence → test", style(DIM)),
        Line::raw(""),
        Line::styled("ctrl+k  explore all workflows", style(MINT)),
    ]);
    text(f, inner, content, TEXT);
}
fn markdown(value: &str) -> Text<'static> {
    Text::from(
        crate::clean_text(value)
            .lines()
            .map(|line| {
                if let Some(s) = line.strip_prefix("# ") {
                    Line::styled(s.to_string(), bold(MINT))
                } else if let Some(s) = line.strip_prefix("## ") {
                    Line::styled(s.to_string(), bold(BLUE))
                } else if let Some(s) = line.strip_prefix("### ") {
                    Line::styled(s.to_string(), bold(TEXT))
                } else if let Some(item) = line.strip_prefix("- ") {
                    Line::from(vec![
                        Span::styled("• ", style(MINT)),
                        Span::styled(item.replace("**", ""), style(TEXT)),
                    ])
                } else if line.starts_with("```") {
                    Line::styled(line.to_string(), style(DIM))
                } else {
                    Line::styled(line.replace("**", ""), style(TEXT))
                }
            })
            .collect::<Vec<_>>(),
    )
}
fn scroll_text(f: &mut Frame, area: Rect, content: Text<'static>, app: &mut App) {
    let text_area = Rect {
        width: area.width.saturating_sub(2),
        ..area
    };
    let paragraph = Paragraph::new(content).wrap(Wrap { trim: false });
    let lines = paragraph.line_count(text_area.width);
    app.report_lines = lines;
    app.scroll = app.scroll.min(
        lines
            .saturating_sub(area.height as usize)
            .min(u16::MAX as usize) as u16,
    );
    f.render_widget(paragraph.scroll((app.scroll, 0)), text_area);
    if lines > area.height as usize {
        let mut state = ScrollbarState::new(lines)
            .position(app.scroll as usize)
            .viewport_content_length(area.height as usize);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .thumb_symbol("┃")
                .track_symbol(Some("│"))
                .thumb_style(style(DIM))
                .track_style(style(LINE)),
            Rect::new(
                area.x + area.width.saturating_sub(1),
                area.y,
                1,
                area.height,
            ),
            &mut state,
        );
    }
}
fn research(f: &mut Frame, area: Rect, app: &mut App) {
    let rows = split(area, [Constraint::Length(4), Constraint::Min(0)]);
    let caption = app
        .current
        .as_ref()
        .map(|r| {
            format!(
                "{}  /  {}  /  {} sources  /  {}",
                r.request.symbols.join(" · "),
                r.provider,
                r.sources.len(),
                r.status
            )
        })
        .unwrap_or_else(|| "Your evidence, analysis, and next questions in one place.".into());
    title(
        f,
        rows[0],
        "RESEARCH WORKSPACE",
        app.current
            .as_ref()
            .map(|r| r.request.kind.label())
            .unwrap_or("Start with a question."),
        &caption,
    );
    let body = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(if area.width >= 100 { 29 } else { 0 }),
    ])
    .spacing(2)
    .split(rows[1]);
    if let Some(run) = &app.current {
        let block = panel(if run.demo {
            " ANALYSIS / DEMO "
        } else {
            " ANALYSIS "
        });
        let inner = block.inner(body[0]);
        f.render_widget(block, body[0]);
        let report = if run.report.is_empty() {
            format!(
                "# {}\n\n{}\n\n{}\n\nEvidence is collected before model synthesis.\nYou can navigate while this research runs.",
                run.request.symbols.join(" / "),
                run.request.question,
                app.phase
            )
        } else {
            run.report.clone()
        };
        scroll_text(f, inner, markdown(&report), app);
        if body[1].width > 0 {
            research_rail(f, body[1], app);
        }
    } else {
        empty(
            f,
            body[0],
            "A useful answer starts with\na thoughtful question.",
            "Press n to begin a company deep dive.\nUse ctrl+k for comparison, macro, or strategy research.",
        );
    }
}
fn research_rail(f: &mut Frame, area: Rect, app: &App) {
    let run = app.current.as_ref().unwrap();
    let block = panel(" RESEARCH TRACE ");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let mut lines = vec![
        Line::raw(""),
        Line::styled(
            if app.busy() {
                format!(
                    "{}  working",
                    ["◐", "◓", "◑", "◒"][(app.tick / 2 % 4) as usize]
                )
            } else {
                format!("○  {}", run.status)
            },
            bold(if app.busy() { GOLD } else { MINT }),
        ),
        Line::raw(""),
        Line::styled(app.phase.clone(), style(TEXT)),
        Line::raw(""),
        Line::styled(
            format!("{:02} SOURCES RETRIEVED", run.sources.len()),
            style(DIM),
        ),
        Line::raw(""),
    ];
    for s in run.sources.iter().take(5) {
        lines.push(Line::styled(
            format!("[{}] {}", s.id, s.publisher),
            style(MUTED),
        ));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled("LIMITATIONS", style(DIM)));
    if run.warnings.is_empty() {
        lines.push(Line::styled(
            "Source checks will appear here.",
            style(MUTED),
        ));
    } else {
        for w in run.warnings.iter().take(5) {
            lines.push(Line::styled(format!("· {w}"), style(GOLD)));
            lines.push(Line::raw(""));
        }
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled("f  ask a follow-up", style(MINT)));
    lines.push(Line::styled("3  inspect sources", style(MUTED)));
    lines.push(Line::styled("e  export your memo", style(MUTED)));
    text(f, inner, Text::from(lines), TEXT);
}
fn sources(f: &mut Frame, area: Rect, app: &mut App) {
    let source_count = app.current.as_ref().map_or(0, |run| run.sources.len());
    let table_height = (source_count.saturating_add(4).min(9) as u16)
        .min(area.height.saturating_sub(10).clamp(4, 9));
    let rows = split(
        area,
        [
            Constraint::Length(4),
            Constraint::Length(table_height),
            Constraint::Min(0),
        ],
    );
    title(
        f,
        rows[0],
        "EVIDENCE, WITH ORIGINS",
        "The source ledger.",
        "Trace every observation. Search excerpts and filing indexes are labeled as such.",
    );
    let Some(run) = &app.current else {
        empty(
            f,
            rows[1],
            "No source ledger yet.",
            "Start research with n or open an archived run.",
        );
        return;
    };
    let entries = run.sources.iter().map(|s| {
        Row::new([
            format!("[{}]", s.id),
            s.title.clone(),
            s.publisher.clone(),
            s.as_of.clone().unwrap_or_else(|| "not supplied".into()),
        ])
        .style(style(MUTED))
    });
    let mut state = TableState::default().with_selected(Some(app.source_selected));
    let table = Table::new(
        entries,
        [
            Constraint::Length(5),
            Constraint::Percentage(43),
            Constraint::Percentage(28),
            Constraint::Min(10),
        ],
    )
    .header(
        Row::new(["ID", "SOURCE", "ORIGIN", "AS OF"])
            .style(style(DIM))
            .bottom_margin(1),
    )
    .block(panel(" COLLECTED EVIDENCE "))
    .row_highlight_style(Style::new().fg(MINT).bg(SELECT))
    .highlight_symbol("▎");
    f.render_stateful_widget(table, rows[1], &mut state);
    if let Some(s) = run.sources.get(app.source_selected) {
        let content = format!(
            "# {}\n\n{}\nRetrieved {}\n\n{}",
            s.title,
            s.url,
            s.retrieved_at.to_rfc3339(),
            s.content
        );
        let block = panel(" SOURCE EXCERPT ");
        let inner = block.inner(rows[2]);
        f.render_widget(block, rows[2]);
        scroll_text(f, inner, markdown(&content), app);
    }
}
fn archive(f: &mut Frame, area: Rect, app: &mut App) {
    let rows = split(
        area,
        [
            Constraint::Length(4),
            Constraint::Length(3),
            Constraint::Min(0),
        ],
    );
    title(
        f,
        rows[0],
        "YOUR RESEARCH, KEPT",
        "The archive.",
        "Local history. Completed memos and partial work. Open a run to continue the investigation.",
    );
    f.render_widget(
        Paragraph::new(format!(
            "{} {}",
            if app.archive_search { "/" } else { "⌕" },
            if app.archive_filter.text.is_empty() {
                "Search questions, symbols, workflows…"
            } else {
                &app.archive_filter.text
            }
        ))
        .style(style(if app.archive_search { MINT } else { DIM }))
        .block(panel(" FILTER ")),
        rows[1],
    );
    let indices = app.filtered_runs();
    if indices.is_empty() {
        empty(
            f,
            rows[2],
            "Room for your next insight.",
            "Press n to start research. Your results will appear here.",
        );
        return;
    }
    let entries: Vec<_> = indices
        .iter()
        .map(|i| {
            let r = &app.runs[*i];
            Row::new([
                r.created_at.format("%b %d %H:%M").to_string(),
                r.request.symbols.join(", "),
                r.request.kind.label().into(),
                r.status.clone(),
                r.request.question.replace('\n', " "),
            ])
            .style(style(if r.demo { GOLD } else { MUTED }))
            .height(2)
        })
        .collect();
    let mut state = TableState::default().with_selected(Some(app.archive_selected));
    f.render_stateful_widget(
        Table::new(
            entries,
            [
                Constraint::Length(13),
                Constraint::Length(13),
                Constraint::Length(19),
                Constraint::Length(12),
                Constraint::Min(10),
            ],
        )
        .header(
            Row::new(["CREATED", "ASSETS", "WORKFLOW", "STATUS", "QUESTION"])
                .style(style(DIM))
                .bottom_margin(1),
        )
        .row_highlight_style(Style::new().fg(MINT).bg(SELECT))
        .highlight_symbol("▎ ")
        .block(panel(" RESEARCH HISTORY ")),
        rows[2],
        &mut state,
    );
}
fn connections(f: &mut Frame, area: Rect, app: &mut App) {
    let rows = split(area, [Constraint::Length(4), Constraint::Min(0)]);
    title(
        f,
        rows[0],
        "BRING YOUR OWN INTELLIGENCE",
        "Everything, connected.",
        "One desk. Your preferred models, market evidence, search and backtesting engines.",
    );
    let selected = app.config.provider;
    let model_key = app.secrets.get(selected.key_name()).is_some();
    let services = vec![
        (
            "INTELLIGENCE",
            selected.label().to_string(),
            if selected.is_cli() {
                "CLI authentication"
            } else if model_key {
                "credential configured"
            } else if matches!(
                selected,
                crate::domain::ProviderKind::Ollama | crate::domain::ProviderKind::Compatible
            ) {
                "local / optional key"
            } else {
                "needs credential"
            },
            app.config.model.clone(),
        ),
        (
            "MARKET DATA",
            app.config.data_provider.clone(),
            if app.config.data_provider == "csv" {
                "imported local history"
            } else if app.config.data_provider == "stooq" {
                if app.secrets.get("STOOQ_API_KEY").is_some() {
                    "credential configured"
                } else {
                    "needs download key"
                }
            } else if app.secrets.get("ALPHAVANTAGE_API_KEY").is_some() {
                "credential configured"
            } else {
                "needs credential"
            },
            "Daily price history and chart context".into(),
        ),
        (
            "WEB SEARCH",
            app.config.search_provider.clone(),
            if app
                .secrets
                .get(if app.config.search_provider == "tavily" {
                    "TAVILY_API_KEY"
                } else {
                    "BRAVE_API_KEY"
                })
                .is_some()
            {
                "credential configured"
            } else {
                "optional / not connected"
            },
            "Cited search excerpts and catalysts".into(),
        ),
        (
            "MACRO",
            "FRED".into(),
            if app.secrets.get("FRED_API_KEY").is_some() {
                "credential configured"
            } else {
                "optional / not connected"
            },
            "Rates, inflation and labor observations".into(),
        ),
        (
            "FILINGS",
            "SEC EDGAR".into(),
            if app.config.sec_contact.contains('@') {
                "contact configured"
            } else {
                "optional / add email"
            },
            "Filing index and reported financial facts".into(),
        ),
        (
            "BACKTESTING",
            "LEAN + QuantConnect".into(),
            if !app.config.lean_workspace.is_empty() {
                "workspace configured"
            } else {
                "optional / not connected"
            },
            "Python / C# projects and cloud result access".into(),
        ),
    ];
    let mut lines = vec![Line::raw("")];
    for (category, name, status, desc) in services {
        lines.push(Line::styled(category, style(DIM)));
        lines.push(Line::from(vec![
            Span::styled(format!("{name}  "), bold(TEXT)),
            Span::styled(
                status,
                style(if status.contains("needs") { GOLD } else { MINT }),
            ),
        ]));
        lines.push(Line::styled(desc, style(MUTED)));
        lines.push(Line::raw(""));
    }
    lines.extend([
        Line::styled("enter  edit connections    t  test model", style(MINT)),
        Line::raw(""),
        Line::styled(
            "Keys can come from environment variables or your protected local credential file.",
            style(DIM),
        ),
        Line::styled(
            "Connection status means configured; use a test or research run to verify access.",
            style(DIM),
        ),
    ]);
    let block = panel(" CONNECTION DIRECTORY ");
    let inner = block.inner(rows[1]);
    f.render_widget(block, rows[1]);
    scroll_text(f, inner, Text::from(lines), app);
}
fn lab(f: &mut Frame, area: Rect, app: &mut App) {
    let rows = split(
        area,
        [
            Constraint::Length(4),
            Constraint::Length(2),
            Constraint::Min(0),
        ],
    );
    title(
        f,
        rows[0],
        "TURN A THESIS INTO A TEST",
        "The strategy lab.",
        "Transparent signals. Real execution boundaries. Results with their assumptions attached.",
    );
    text(
        f,
        rows[1],
        Line::from(
            (0..3)
                .flat_map(|i| {
                    [
                        Span::styled(
                            format!(
                                " {} ",
                                ["01 Historical", "02 LEAN local", "03 QuantConnect"][i]
                            ),
                            if i == app.lab_tab {
                                bold(MINT).bg(SELECT)
                            } else {
                                style(DIM)
                            },
                        ),
                        Span::raw("   "),
                    ]
                })
                .collect::<Vec<_>>(),
        ),
        TEXT,
    );
    if app.lab_tab == 0 {
        if let Some(result) = &app.backtest {
            let layout = split(
                rows[2],
                [
                    Constraint::Length(5),
                    Constraint::Min(4),
                    Constraint::Length(5),
                ],
            );
            let metrics = Layout::horizontal([Constraint::Percentage(25); 4])
                .spacing(1)
                .split(layout[0]);
            for (i, (label, value, color)) in [
                (
                    "NET RETURN",
                    format!("{:+.2}%", result.total_return * 100.0),
                    MINT,
                ),
                (
                    "MAX DRAWDOWN",
                    format!("{:.2}%", result.max_drawdown * 100.0),
                    RED,
                ),
                (
                    if metrics[2].width < 24 {
                        "SHARPE / 252"
                    } else {
                        "SHARPE / 252 / RF 0"
                    },
                    result
                        .sharpe
                        .map(|v| format!("{v:.2}"))
                        .unwrap_or_else(|| "undefined".into()),
                    BLUE,
                ),
                (
                    "TRADES / FEES",
                    format!("{} / {:.0}", result.trades, result.fees),
                    GOLD,
                ),
            ]
            .iter()
            .enumerate()
            {
                f.render_widget(
                    Paragraph::new(value.clone())
                        .style(bold(*color))
                        .block(panel(format!(" {label} ")).padding(Padding::new(1, 1, 1, 0))),
                    metrics[i],
                );
            }
            let data: Vec<_> = result
                .points
                .iter()
                .enumerate()
                .map(|(i, p)| (i as f64, p.equity / result.params.initial_cash * 100.0))
                .collect();
            let benchmark: Vec<_> = result
                .points
                .iter()
                .enumerate()
                .map(|(i, p)| (i as f64, p.benchmark / result.params.initial_cash * 100.0))
                .collect();
            let min = data
                .iter()
                .chain(&benchmark)
                .map(|(_, v)| *v)
                .fold(100.0, f64::min);
            let max = data
                .iter()
                .chain(&benchmark)
                .map(|(_, v)| *v)
                .fold(100.0, f64::max);
            f.render_widget(
                Chart::new(vec![
                    Dataset::default()
                        .name("trend")
                        .data(&data)
                        .marker(symbols::Marker::Braille)
                        .graph_type(GraphType::Line)
                        .style(style(MINT)),
                    Dataset::default()
                        .name("buy & hold")
                        .data(&benchmark)
                        .marker(symbols::Marker::Braille)
                        .graph_type(GraphType::Line)
                        .style(style(BLUE)),
                ])
                .block(panel(format!(
                    " {} / EQUITY INDEX {} ",
                    result.symbol,
                    if result.demo { "/ DEMO" } else { "" }
                )))
                .x_axis(
                    Axis::default()
                        .bounds([0.0, (data.len() - 1) as f64])
                        .style(style(LINE)),
                )
                .y_axis(
                    Axis::default()
                        .bounds([min - 2.0, max + 2.0])
                        .labels([format!("{min:.0}"), format!("{max:.0}")])
                        .style(style(DIM)),
                ),
                layout[1],
            );
            text(
                f,
                layout[2],
                format!(
                    "SMA {} / {} · next-open execution · {:.1} bps each way · {:.0}% exposure\nBuy & hold {:+.2}% on the same dates. Long/cash, fractional shares, no leverage.\n{} · no dividends/tax/slippage beyond fixed costs. Daily OHLC adjustments may bias results.\nResearch simulation, not LEAN. b change parameters; JSON saved in exports.",
                    result.params.fast,
                    result.params.slow,
                    result.params.cost_bps,
                    result.exposure * 100.0,
                    result.benchmark_return * 100.0,
                    result.source
                ),
                MUTED,
            );
        } else {
            empty(
                f,
                rows[2],
                "Give your hypothesis\na measurable outcome.",
                "b  run a moving-average trend study\nLoad daily history on the desk first. Signals use prior closes,\nexecute at the next open, and include explicit trading costs.\n\nUse LEAN for event-driven validation and richer strategies.",
            );
        }
    } else if app.lab_tab == 1 {
        let mut lines = vec![
            Line::styled("LEAN / LOCAL EXECUTION", bold(MINT)),
            Line::raw(""),
            Line::styled("Python and C# strategy templates", bold(TEXT)),
            Line::styled(
                "p creates a Python project; ctrl+k offers C#.",
                style(MUTED),
            ),
            Line::styled(
                "Review your code, then press l to run a project.",
                style(MUTED),
            ),
            Line::raw(""),
            Line::styled(
                "Prerequisites: LEAN CLI, Docker, login, initialized workspace, data and cached image.",
                style(DIM),
            ),
            Line::styled(
                "Run lean init and lean backtest in that workspace once before using this runner.",
                style(DIM),
            ),
            Line::raw(""),
        ];
        for (k, v) in &app.lean_stats {
            lines.push(Line::from(vec![
                Span::styled(format!("{k:30}"), style(MUTED)),
                Span::styled(v.clone(), style(MINT)),
            ]));
        }
        lines.push(Line::raw(""));
        lines.push(Line::styled("EXECUTION LOG", style(DIM)));
        for e in &app.events {
            lines.push(Line::styled(e.clone(), style(MUTED)));
        }
        let block = panel(" LEAN ENGINE ");
        let inner = block.inner(rows[2]);
        f.render_widget(block, rows[2]);
        scroll_text(f, inner, Text::from(lines), app);
    } else {
        let value = if app.events.is_empty() {
            "# QuantConnect / cloud research\n\nConnect your user ID, API token and project ID in Connections.\n\nPress c to read your cloud backtest list.\nCloud calls use timestamped SHA-256 authentication.\n\nThis desk does not start cloud jobs or place orders.".into()
        } else {
            app.events.join("\n")
        };
        let block = panel(" QUANTCONNECT CLOUD ");
        let inner = block.inner(rows[2]);
        f.render_widget(block, rows[2]);
        scroll_text(f, inner, markdown(&value), app);
    }
}
fn empty(f: &mut Frame, area: Rect, heading: &str, caption: &str) {
    let block = panel(" RESEN ");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let a = inset(inner, 3, if inner.height > 12 { 3 } else { 1 });
    let lines = split(
        a,
        [
            Constraint::Length(1),
            Constraint::Length(4),
            Constraint::Min(0),
        ],
    );
    text(f, lines[0], "◈", MINT);
    text(
        f,
        lines[1],
        Line::styled(heading.to_string(), bold(TEXT)),
        TEXT,
    );
    text(f, lines[2], caption.to_string(), MUTED);
}
fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(4));
    let height = height.min(area.height.saturating_sub(2));
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}
fn render_modal(f: &mut Frame, area: Rect, modal: &Modal, app: &App) {
    // Dim the existing interface while preserving its spatial context.
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            let c = &mut f.buffer_mut()[(x, y)];
            c.set_fg(DIM);
        }
    }
    let (width, height, heading) = match modal {
        Modal::Import { .. } => (82, 18, " IMPORT PRICE HISTORY "),
        Modal::Welcome => (78, 25, " WELCOME TO RESEN "),
        Modal::Help => (82, 30, " THE KEYBOARD IS YOUR COMMAND CENTER "),
        Modal::Settings(form) => (
            90,
            if form.wizard {
                30
            } else {
                area.height.saturating_sub(4)
            },
            " CONNECTIONS ",
        ),
        Modal::Compose { .. } => (88, 25, " NEW RESEARCH "),
        Modal::Palette { .. } => (78, 22, " COMMAND PALETTE "),
        Modal::Strategy { .. } => (72, 24, " HISTORICAL STUDY "),
        Modal::Lean { .. } => (84, 19, " RUN REVIEWED LEAN PROJECT "),
    };
    let rect = centered(area, width, height);
    f.render_widget(Clear, rect);
    let block = panel(heading)
        .border_style(style(MINT))
        .padding(Padding::new(2, 2, 1, 1));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    match modal {
        Modal::Import {
            symbol,
            path,
            focus,
        } => {
            let rows = split(
                inner,
                [
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Min(0),
                ],
            );
            text(
                f,
                rows[0],
                "Bring daily OHLCV from your own data source.\nThe imported history becomes evidence for charts, research and backtests.",
                MUTED,
            );
            input_widget(f, rows[1], symbol, *focus == 0, false, " ASSET ");
            input_widget(f, rows[2], path, *focus == 1, false, " CSV FILE PATH ");
            text(
                f,
                rows[3],
                "Date,Open,High,Low,Close,Volume · ISO dates · max 2 MB\nenter import   tab field   esc close",
                MINT,
            );
        }
        Modal::Welcome => {
            text(
                f,
                inner,
                Text::from(vec![
                    Line::styled("◈  r e s e n", bold(MINT)),
                    Line::raw(""),
                    Line::styled("Your edge starts with a better question.", bold(TEXT)),
                    Line::raw(""),
                    Line::styled(
                        "A financial research desk that lives in your terminal.",
                        style(MUTED),
                    ),
                    Line::styled(
                        "Bring the intelligence. Connect the evidence. Keep the work.",
                        style(MUTED),
                    ),
                    Line::raw(""),
                    Line::styled("01  Choose your engine", bold(TEXT)),
                    Line::styled(
                        "    Codex, Claude, Ollama, or any compatible endpoint",
                        style(DIM),
                    ),
                    Line::raw(""),
                    Line::styled("02  Connect your sources", bold(TEXT)),
                    Line::styled(
                        "    Market history, search, macro, filings and LEAN",
                        style(DIM),
                    ),
                    Line::raw(""),
                    Line::styled("03  Follow the evidence", bold(TEXT)),
                    Line::styled(
                        "    Source-led research with a persistent local archive",
                        style(DIM),
                    ),
                    Line::raw(""),
                    Line::styled(
                        "enter  set up your desk     d  explore the demo",
                        style(MINT),
                    ),
                    Line::styled("esc  explore without setup", style(DIM)),
                ]),
                TEXT,
            );
        }
        Modal::Help => text(
            f,
            inner,
            Text::from(vec![
                Line::styled("Make the desk your own.", bold(MINT)),
                Line::raw(""),
                Line::raw("1–6 / tab     Navigate workspace pages"),
                Line::raw("ctrl+k / :    Search the command palette"),
                Line::raw("n             Start a research question"),
                Line::raw("↑↓ / j k      Select assets, runs, sources; scroll reports"),
                Line::raw("PgUp/PgDn     Scroll longer reports and excerpts"),
                Line::raw("r             Refresh daily history on the desk"),
                Line::raw("f             Follow up from the research page"),
                Line::raw("e             Export the current memo as Markdown + JSON"),
                Line::raw("/             Search the archive"),
                Line::raw("b / ←→        Backtest parameters / lab tabs"),
                Line::raw("p / l / c     Python template / run LEAN / cloud results"),
                Line::raw("ctrl+c        Cancel a running job; otherwise quit"),
                Line::raw("q             Save partial research and quit"),
                Line::raw(""),
                Line::styled("IN FORMS", style(DIM)),
                Line::raw("tab / shift+tab   Move between fields"),
                Line::raw("ctrl+a / e / u / w   Start / end / clear prefix / delete word"),
                Line::raw("ctrl+r        Submit research (enter adds a question line)"),
                Line::raw("ctrl+s / t    Save connections / test model"),
                Line::raw("ctrl+d        Remove local credential in a secret field"),
                Line::raw("esc           Close a dialog without saving"),
                Line::raw(""),
                Line::styled(
                    "No Nerd Font required. Truecolor and a readable monospace font recommended.",
                    style(MUTED),
                ),
            ]),
            TEXT,
        ),
        Modal::Settings(form) => settings_modal(f, inner, form),
        Modal::Compose {
            kind,
            symbols,
            question,
            focus,
            prior,
        } => {
            let rows = split(
                inner,
                [
                    Constraint::Length(2),
                    Constraint::Length(3),
                    Constraint::Length(1),
                    Constraint::Length(3),
                    Constraint::Length(1),
                    Constraint::Min(3),
                    Constraint::Length(2),
                ],
            );
            text(
                f,
                rows[0],
                if prior.is_some() {
                    "Build on your previous research."
                } else {
                    "What would you like to understand?"
                },
                MUTED,
            );
            f.render_widget(
                Paragraph::new(format!("‹  {}  ›", kind.label()))
                    .style(style(if *focus == 0 { MINT } else { TEXT }))
                    .block(panel(" WORKFLOW ").border_style(style(if *focus == 0 {
                        MINT
                    } else {
                        LINE
                    }))),
                rows[1],
            );
            input_widget(
                f,
                rows[3],
                symbols,
                *focus == 1,
                false,
                " ASSETS / COMMA SEPARATED ",
            );
            input_widget(f, rows[5], question, *focus == 2, false, " YOUR QUESTION ");
            text(
                f,
                rows[6],
                "tab fields  ←→ workflow  ctrl+r research  esc close",
                MINT,
            );
        }
        Modal::Palette { input, selected } => {
            let rows = split(
                inner,
                [
                    Constraint::Length(3),
                    Constraint::Min(0),
                    Constraint::Length(1),
                ],
            );
            input_widget(f, rows[0], input, true, false, " SEARCH COMMANDS ");
            let matches = App::palette_matches(&input.text);
            let items: Vec<_> = matches
                .iter()
                .map(|i| {
                    ListItem::new(Line::from(vec![
                        Span::styled(format!("{:02}  ", i + 1), style(DIM)),
                        Span::styled(COMMANDS[*i].0, style(TEXT)),
                    ]))
                })
                .collect();
            let mut state = ListState::default().with_selected(Some(*selected));
            f.render_stateful_widget(
                List::new(items)
                    .highlight_style(Style::new().bg(SELECT).fg(MINT))
                    .highlight_symbol("▎ "),
                rows[1],
                &mut state,
            );
            text(f, rows[2], "↑↓ choose   enter run   esc close", MUTED);
        }
        Modal::Strategy { fields, focus } => {
            let rows = split(
                inner,
                [
                    Constraint::Length(2),
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Min(0),
                ],
            );
            text(
                f,
                rows[0],
                format!("{} / long-cash moving-average trend", app.selected_symbol()),
                MUTED,
            );
            for (i, label) in [
                " FAST WINDOW (DAYS) ",
                " SLOW WINDOW (DAYS) ",
                " INITIAL CAPITAL ",
                " TRADING COST (BPS / EACH SIDE) ",
            ]
            .iter()
            .enumerate()
            {
                input_widget(f, rows[i + 1], &fields[i], *focus == i, false, label);
            }
            text(
                f,
                rows[5],
                "Signals use previous closes; fills use next opens.\nenter run study   tab fields   esc close",
                MINT,
            );
        }
        Modal::Lean { project } => {
            let rows = split(
                inner,
                [
                    Constraint::Length(5),
                    Constraint::Length(3),
                    Constraint::Min(0),
                ],
            );
            text(
                f,
                rows[0],
                "This executes code in your reviewed project through LEAN + Docker.\nSet the workspace in Connections; run lean init and cache its image first.\n\nThe desk never executes model-generated code automatically.",
                MUTED,
            );
            input_widget(
                f,
                rows[1],
                project,
                true,
                false,
                " PROJECT PATH / RELATIVE TO WORKSPACE ",
            );
            text(
                f,
                rows[2],
                "lean backtest <project> --output <run-folder> --no-update\n\nenter execute reviewed project   esc close\nCtrl+C cancels the runner; inspect Docker for any remaining container.",
                GOLD,
            );
        }
    }
}
fn settings_modal(f: &mut Frame, area: Rect, form: &SettingsForm) {
    let rows = split(
        area,
        [
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(2),
        ],
    );
    text(
        f,
        rows[0],
        if form.wizard {
            format!(
                "0{} / 03   {}\nOptional evidence connections can be added later.",
                form.step + 1,
                [
                    "Choose your intelligence",
                    "Connect your evidence",
                    "Complete your workspace"
                ][form.step]
            )
        } else {
            "Your keys. Your tools. A single place to connect them.\nKeys are masked and stored locally with restricted file permissions.".into()
        },
        MUTED,
    );
    let range = form.range();
    let capacity = (rows[1].height / 4).max(1) as usize;
    let start = if form.focus >= range.start + capacity {
        form.focus + 1 - capacity
    } else {
        range.start
    };
    for (i, index) in (start..range.end).take(capacity).enumerate() {
        let field = &form.fields[index];
        let field_area = Rect::new(rows[1].x, rows[1].y + i as u16 * 4, rows[1].width, 3);
        input_widget(
            f,
            field_area,
            &field.input,
            form.focus == index,
            field.secret,
            &format!(" {} ", field.label.to_uppercase()),
        );
        text(
            f,
            Rect::new(
                field_area.x + 1,
                field_area.y + 3,
                field_area.width.saturating_sub(2),
                1,
            ),
            field.hint.clone(),
            DIM,
        );
    }
    text(
        f,
        rows[2],
        if form.wizard {
            "tab fields  ←→ choose  enter next/finish  ctrl+t test  esc close"
        } else {
            "tab fields  ←→ choose  ctrl+s save  ctrl+t test  ctrl+d clear key  esc close"
        },
        MINT,
    );
}
fn input_widget(
    f: &mut Frame,
    area: Rect,
    input: &Input,
    focused: bool,
    secret: bool,
    label: &str,
) {
    let block = panel(label.to_string()).border_style(style(if focused { MINT } else { LINE }));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let displayed = if secret {
        "•".repeat(input.text.chars().count())
    } else {
        input.text.clone()
    };
    let mut lines = displayed
        .split('\n')
        .map(str::to_string)
        .collect::<Vec<_>>();
    if focused {
        let prefix = &input.text[..input.cursor];
        let line_index = prefix.matches('\n').count();
        let col = prefix.rsplit('\n').next().unwrap_or("").chars().count();
        if let Some(line) = lines.get_mut(line_index) {
            let byte = line
                .char_indices()
                .nth(col)
                .map(|(i, _)| i)
                .unwrap_or(line.len());
            line.insert(byte, '▏');
        }
    }
    let value = lines.join("\n");
    let paragraph = Paragraph::new(value)
        .style(style(if focused { TEXT } else { MUTED }))
        .wrap(Wrap { trim: false });
    let total = paragraph.line_count(inner.width);
    let scroll = total
        .saturating_sub(inner.height as usize)
        .min(u16::MAX as usize) as u16;
    f.render_widget(paragraph.scroll((scroll, 0)), inner);
}

pub fn snapshot_svg(buffer: &ratatui::buffer::Buffer) -> String {
    let cell_width = 9;
    let cell_height = 18;
    let width = buffer.area.width as usize * cell_width + 32;
    let height = buffer.area.height as usize * cell_height + 32;
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\"><rect width=\"100%\" height=\"100%\" fill=\"#0c1015\"/><g font-family=\"Menlo,DejaVu Sans Mono,monospace\" font-size=\"14\">"
    );
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            let c = &buffer[(x, y)];
            let px = x as usize * cell_width + 16;
            let py = y as usize * cell_height + 16;
            if c.bg != Color::Reset {
                svg.push_str(&format!("<rect x=\"{px}\" y=\"{py}\" width=\"{cell_width}\" height=\"{cell_height}\" fill=\"{}\"/>",css_color(c.bg)));
            }
            if c.symbol() != " " {
                let symbol = c
                    .symbol()
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;")
                    .replace('"', "&quot;");
                svg.push_str(&format!(
                    "<text x=\"{px}\" y=\"{}\" fill=\"{}\"{}>{symbol}</text>",
                    py + 14,
                    css_color(c.fg),
                    if c.modifier.contains(Modifier::BOLD) {
                        " font-weight=\"bold\""
                    } else {
                        ""
                    }
                ));
            }
        }
    }
    svg.push_str("</g></svg>");
    svg
}
fn css_color(color: Color) -> String {
    match color {
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        Color::Black => "#000000".into(),
        Color::White => "#ffffff".into(),
        Color::Green => "#85e0be".into(),
        Color::Red => "#ee8f91".into(),
        _ => "#8497a7".into(),
    }
}
