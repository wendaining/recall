use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, Paragraph, Wrap};

use super::app::{App, Category};

pub(crate) fn draw(frame: &mut Frame, app: &mut App) {
    if frame.area().width < 72 || frame.area().height < 20 {
        frame.render_widget(
            Paragraph::new(
                "recall config needs a terminal of at least 72x20\n\nPress q or Esc to exit",
            )
            .block(Block::bordered().title(" recall config "))
            .wrap(Wrap { trim: false }),
            frame.area(),
        );
        return;
    }

    let areas = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(frame.area());
    frame.render_widget(
        Paragraph::new("Configure capture, search, and shell integration safely")
            .block(Block::bordered().title(" recall config ")),
        areas[0],
    );

    let panes = Layout::horizontal([Constraint::Length(24), Constraint::Min(0)]).split(areas[1]);
    draw_categories(frame, app, panes[0]);
    draw_settings(frame, app, panes[1]);

    let style = if app.status_is_error {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let status = app
        .status
        .as_deref()
        .unwrap_or("Tab category · ↑/↓ setting · Space toggle · Enter edit · q quit · F1 help");
    frame.render_widget(Paragraph::new(status).style(style), areas[2]);

    if app.show_help {
        draw_help(frame);
    }
}

fn draw_categories(frame: &mut Frame, app: &App, area: Rect) {
    let items = Category::ALL
        .iter()
        .enumerate()
        .map(|(index, category)| {
            let marker = if index == app.category { "› " } else { "  " };
            ListItem::new(format!("{marker}{}", category.label())).style(if index == app.category {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            })
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        List::new(items).block(Block::bordered().title(" Categories ")),
        area,
    );
}

fn draw_settings(frame: &mut Frame, app: &App, area: Rect) {
    let rows = setting_rows(app);
    let items = rows
        .into_iter()
        .enumerate()
        .map(|(index, (label, value))| {
            let marker = if index == app.selected { "›" } else { " " };
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{marker} {label:<22}"),
                    if index == app.selected {
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    },
                ),
                Span::raw(value),
            ]))
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        List::new(items)
            .block(Block::bordered().title(format!(" {} ", app.current_category().label()))),
        area,
    );
}

fn setting_rows(app: &App) -> Vec<(&'static str, String)> {
    match app.current_category() {
        Category::Keybinding => vec![("Search shortcut", app.config.ui.search_key.clone())],
        Category::Capture => vec![
            ("Secret filtering", on_off(app.config.proxy.secrets_filter)),
            (
                "Interactive commands",
                on_off(app.config.proxy.mark_interactive),
            ),
            (
                "Excluded commands",
                app.config.proxy.exclude.len().to_string(),
            ),
            (
                "Output exclusions",
                app.config.proxy.exclude_output.len().to_string(),
            ),
        ],
        Category::Appearance => vec![
            ("Preview lines", app.config.ui.preview_lines.to_string()),
            (
                "List pane width",
                format!("{}%", app.config.ui.list_width_pct),
            ),
            ("Timestamp format", app.config.ui.date_format.clone()),
        ],
        Category::Shell => vec![
            ("Detected shell", crate::util::login_shell()),
            (
                "Proxy active",
                on_off(std::env::var_os("RECALL_PROXY_ACTIVE").is_some()),
            ),
            ("Setup mode", "checking profile…".to_string()),
        ],
    }
}

fn on_off(value: bool) -> String {
    if value { "On" } else { "Off" }.to_string()
}

fn draw_help(frame: &mut Frame) {
    let area = centered_rect(64, 70, frame.area());
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(
            "Tab / Shift+Tab   Change category\n↑ / ↓             Select setting\nSpace             Toggle setting\nEnter             Edit or apply\nEsc / q           Back or quit\nF1                Close this help",
        )
        .block(Block::bordered().title(" Help ")),
        area,
    );
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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn renders_current_configuration() {
        let mut config = crate::config::Config::default();
        config.ui.search_key = "ctrl-t".to_string();
        let mut app = App::new(config);
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();

        let text = terminal.backend().to_string();
        assert!(text.contains("Keybinding"));
        assert!(text.contains("Search shortcut"));
        assert!(text.contains("ctrl-t"));
    }

    #[test]
    fn renders_small_terminal_message() {
        let mut app = App::new(crate::config::Config::default());
        let backend = TestBackend::new(60, 15);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();
        assert!(terminal.backend().to_string().contains("at least 72x20"));
    }
}
