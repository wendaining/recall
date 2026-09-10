use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Clear, List, ListItem, Paragraph, Wrap};
use unicode_width::UnicodeWidthStr;

use crate::model::{Block as RecallBlock, BlockKind};
use crate::tui::app::{App, Focus};
use crate::util;

pub fn draw(frame: &mut Frame, app: &mut App) {
    let areas = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(frame.area());

    draw_search(frame, app, areas[0]);

    let panes = Layout::horizontal([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(areas[1]);
    draw_list(frame, app, panes[0]);
    draw_detail(frame, app, panes[1]);

    draw_status(frame, app, areas[2]);

    if app.show_help {
        draw_help(frame);
    }
}

fn draw_search(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Search;
    let title = if focused {
        "recall — search"
    } else {
        "recall"
    };
    let block = Block::bordered()
        .title(title)
        .border_style(border_style(focused));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let line = Line::from(vec![
        Span::styled("> ", Style::default().fg(Color::Cyan)),
        Span::raw(app.query.clone()),
    ]);
    frame.render_widget(Paragraph::new(line), inner);

    if focused {
        let x = inner.x + 2 + UnicodeWidthStr::width(app.query.as_str()) as u16;
        let x = x.min(inner.x + inner.width.saturating_sub(1));
        frame.set_cursor_position((x, inner.y));
    }
}

fn draw_list(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Detail;
    let title = format!(" blocks ({}) ", app.results.len());
    let block = Block::bordered()
        .title(title)
        .border_style(border_style(!focused));

    let items: Vec<ListItem> = app
        .results
        .iter()
        .map(|entry| ListItem::new(block_lines(entry, &app.config)))
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(Color::Indexed(238))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("");

    app.list_state.select(Some(app.selected));
    frame.render_stateful_widget(list, area, &mut app.list_state);
}

fn block_lines(block: &RecallBlock, config: &crate::config::Config) -> Text<'static> {
    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        "─".repeat(200),
        Style::default().fg(Color::Indexed(240)),
    )));
    lines.push(header_line(block, config));
    lines.push(Line::from(vec![
        Span::styled("$ ", Style::default().fg(Color::Green)),
        Span::styled(
            first_line(&block.command),
            Style::default().add_modifier(Modifier::BOLD),
        ),
    ]));

    let preview = config.ui.preview_lines;
    if preview > 0 {
        if let Some(output) = &block.output {
            let text = String::from_utf8_lossy(output);
            for line in text.lines().take(preview) {
                lines.push(Line::from(Span::styled(
                    format!("  {line}"),
                    Style::default().fg(Color::Indexed(245)),
                )));
            }
        } else if block.kind != BlockKind::Normal {
            lines.push(Line::from(Span::styled(
                format!("  [{}]", kind_label(block.kind)),
                Style::default().fg(Color::Indexed(244)),
            )));
        }
    }

    Text::from(lines)
}

fn header_line(block: &RecallBlock, config: &crate::config::Config) -> Line<'static> {
    let time = util::format_time(block.started_at, &config.ui.date_format);
    let duration = block
        .duration_ns
        .map(util::format_duration)
        .unwrap_or_else(|| "-".to_string());
    let (exit_text, exit_style) = match block.exit_code {
        Some(0) => ("exit 0".to_string(), Style::default().fg(Color::Green)),
        Some(code) => (format!("exit {code}"), Style::default().fg(Color::Red)),
        None => ("exit ?".to_string(), Style::default().fg(Color::DarkGray)),
    };
    let cwd = block.cwd.as_deref().map(shorten_home).unwrap_or_default();

    Line::from(vec![
        Span::styled(time, Style::default().fg(Color::Blue)),
        Span::raw("  "),
        Span::styled(duration, Style::default().fg(Color::Magenta)),
        Span::raw("  "),
        Span::styled(exit_text, exit_style),
        Span::raw("  "),
        Span::styled(cwd, Style::default().fg(Color::DarkGray)),
    ])
}

fn draw_detail(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Detail;
    let block = Block::bordered()
        .title(" detail ")
        .border_style(border_style(focused));

    let text = match &app.detail {
        Some(entry) => detail_text(entry, &app.config),
        None => Text::from(Line::from(Span::styled(
            "no block selected",
            Style::default().fg(Color::DarkGray),
        ))),
    };

    let paragraph = Paragraph::new(text)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((app.detail_scroll, 0));
    frame.render_widget(paragraph, area);
}

fn detail_text(block: &RecallBlock, config: &crate::config::Config) -> Text<'static> {
    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(
            block.command.clone(),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        header_line(block, config),
        Line::from(vec![
            Span::styled("kind: ", Style::default().fg(Color::DarkGray)),
            Span::raw(kind_label(block.kind).to_string()),
            Span::styled("   bytes: ", Style::default().fg(Color::DarkGray)),
            Span::raw(block.output_bytes.to_string()),
            Span::styled("   lines: ", Style::default().fg(Color::DarkGray)),
            Span::raw(block.output_lines.to_string()),
        ]),
        Line::from(Span::styled(
            "─".repeat(200),
            Style::default().fg(Color::Indexed(240)),
        )),
    ];

    match &block.output {
        Some(output) => {
            let text = String::from_utf8_lossy(output);
            for line in text.lines() {
                lines.push(Line::raw(line.to_string()));
            }
        }
        None => lines.push(Line::from(Span::styled(
            format!("[{}]", kind_label(block.kind)),
            Style::default().fg(Color::DarkGray),
        ))),
    }

    Text::from(lines)
}

fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    let style = if app.status_is_error {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let message = app.status.clone().unwrap_or_else(|| default_hint(app));
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(message, style))),
        area,
    );
}

fn default_hint(app: &App) -> String {
    format!(
        "{} results · ↑/↓ move/scroll · Enter focus · Tab edit · Ctrl+Enter/Ctrl+E run · Ctrl+Y copy cmd · Ctrl+O copy output · q quit · F1 help",
        app.results.len()
    )
}

fn draw_help(frame: &mut Frame) {
    let area = centered_rect(60, 70, frame.area());
    frame.render_widget(Clear, area);
    let text = vec![
        Line::from(Span::styled(
            "recall — key bindings",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
        Line::raw("type            search command and output"),
        Line::raw("↑/↓             move selection (search) / scroll output (detail)"),
        Line::raw("PgUp/PgDn       scroll output"),
        Line::raw("Enter           switch focus (search/detail)"),
        Line::raw("Tab             edit selected command (insert into prompt)"),
        Line::raw("Ctrl+Enter      execute selected command"),
        Line::raw("Ctrl+E          execute (fallback for terminals without"),
        Line::raw("                the kitty keyboard protocol)"),
        Line::raw("Ctrl+Y          copy command"),
        Line::raw("Ctrl+O          copy output"),
        Line::raw("Esc             clear search / leave detail"),
        Line::raw("q / Ctrl+C      quit"),
        Line::raw("F1              toggle this help"),
    ];
    let paragraph = Paragraph::new(text)
        .block(Block::bordered().title(" help "))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn kind_label(kind: BlockKind) -> &'static str {
    match kind {
        BlockKind::Normal => "normal",
        BlockKind::Empty => "no output",
        BlockKind::Interactive => "interactive (output skipped)",
        BlockKind::Binary => "binary output (skipped)",
        BlockKind::Redirected => "output redirected",
        BlockKind::Unavailable => "output unavailable",
    }
}

fn first_line(command: &str) -> String {
    command.lines().next().unwrap_or("").to_string()
}

fn shorten_home(path: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        let home = home.to_string_lossy();
        if let Some(rest) = path.strip_prefix(home.as_ref()) {
            return format!("~{rest}");
        }
    }
    path.to_string()
}

fn border_style(focused: bool) -> Style {
    if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);
    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(vertical[1])[1]
}
