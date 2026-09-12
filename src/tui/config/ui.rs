use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, Paragraph, Wrap};

use super::app::{App, CaptureItem, Category, Modal, RuleTarget};

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
    if let Some(modal) = &app.modal {
        draw_modal(frame, modal);
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

fn draw_settings(frame: &mut Frame, app: &mut App, area: Rect) {
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
    app.settings_state.select(Some(app.selected));
    frame.render_stateful_widget(
        List::new(items)
            .block(Block::bordered().title(format!(" {} ", app.current_category().label()))),
        area,
        &mut app.settings_state,
    );
}

fn setting_rows(app: &App) -> Vec<(String, String)> {
    match app.current_category() {
        Category::Keybinding => keybinding_rows(app),
        Category::Capture => capture_rows(app),
        Category::Appearance => vec![
            (
                "Preview lines".to_string(),
                app.config.ui.preview_lines.to_string(),
            ),
            (
                "List pane width".to_string(),
                format!("{}%", app.config.ui.list_width_pct),
            ),
            (
                "Timestamp format".to_string(),
                format!(
                    "{} · {}",
                    app.config.ui.date_format,
                    super::app::date_preview(&app.config.ui.date_format)
                ),
            ),
        ],
        Category::Shell => vec![
            ("Detected shell".to_string(), app.shell_name.clone()),
            (
                "Startup file".to_string(),
                app.shell_profile
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "Unavailable".to_string()),
            ),
            ("Current setup".to_string(), shell_setup_label(app)),
            (
                "Runtime".to_string(),
                format!(
                    "hooks {} · proxy {}",
                    on_off(std::env::var_os("RECALL_SESSION").is_some()),
                    on_off(std::env::var_os("RECALL_PROXY_ACTIVE").is_some())
                ),
            ),
            ("Use automatic capture".to_string(), "Apply".to_string()),
            ("Use hooks only".to_string(), "Apply".to_string()),
            ("Remove integration".to_string(), "Confirm…".to_string()),
        ],
    }
}

fn shell_setup_label(app: &App) -> String {
    if let Some(error) = &app.shell_error {
        return format!("Error: {error}");
    }
    match app.shell_setup {
        Some(crate::commands::setup::ManagedSetup::Auto) => "Automatic capture".to_string(),
        Some(crate::commands::setup::ManagedSetup::Hooks) => "Hooks only".to_string(),
        Some(crate::commands::setup::ManagedSetup::LegacyAuto) => {
            "Legacy automatic setup".to_string()
        }
        Some(crate::commands::setup::ManagedSetup::LegacyHooks) => "Legacy hooks setup".to_string(),
        Some(crate::commands::setup::ManagedSetup::UnmanagedHooks) => "Unmanaged hooks".to_string(),
        Some(crate::commands::setup::ManagedSetup::None) => "Not configured".to_string(),
        Some(crate::commands::setup::ManagedSetup::Invalid) => {
            "Malformed recall markers — repair manually".to_string()
        }
        None => "Unavailable".to_string(),
    }
}

fn keybinding_rows(app: &App) -> Vec<(String, String)> {
    let mut rows = vec![(
        "Search shortcut".to_string(),
        app.config.ui.search_key.clone(),
    )];
    for (name, label) in [
        ("zsh", "zsh"),
        ("bash", "bash"),
        ("fish", "fish"),
        ("pwsh", "PowerShell"),
    ] {
        let value = crate::shell::Shell::from_name(name)
            .and_then(|shell| shell.semantic_search_key(&app.config.ui.search_key))
            .map(|encoded| format!("Compatible · {encoded}"))
            .unwrap_or_else(|| "Custom / not validated".to_string());
        rows.push((format!("  {label}"), value));
    }
    rows
}

fn capture_rows(app: &App) -> Vec<(String, String)> {
    app.capture_items()
        .into_iter()
        .map(|item| match item {
            CaptureItem::Secrets => (
                "Secret filtering".to_string(),
                on_off(app.config.proxy.secrets_filter),
            ),
            CaptureItem::Interactive => (
                "Interactive commands".to_string(),
                on_off(app.config.proxy.mark_interactive),
            ),
            CaptureItem::Preset {
                label,
                target,
                pattern,
            } => {
                let value = if pattern == super::app::RECALL_COMMAND {
                    "[x] Required".to_string()
                } else {
                    checkbox(app.preset_enabled(target, pattern))
                };
                (label.to_string(), value)
            }
            CaptureItem::Add(RuleTarget::Command) => {
                ("+ Command rule".to_string(), "Enter to add".to_string())
            }
            CaptureItem::Add(RuleTarget::Output) => {
                ("+ Output rule".to_string(), "Enter to add".to_string())
            }
            CaptureItem::Rule {
                target: RuleTarget::Command,
                pattern,
                ..
            } => ("Command rule".to_string(), pattern),
            CaptureItem::Rule {
                target: RuleTarget::Output,
                pattern,
                ..
            } => ("Output rule".to_string(), pattern),
        })
        .collect()
}

fn on_off(value: bool) -> String {
    if value { "On" } else { "Off" }.to_string()
}

fn checkbox(value: bool) -> String {
    if value { "[x]" } else { "[ ]" }.to_string()
}

fn draw_modal(frame: &mut Frame, modal: &Modal) {
    let area = centered_rect(72, 40, frame.area());
    frame.render_widget(Clear, area);
    match modal {
        Modal::KeyRecorder { chords } => {
            let value = if chords.is_empty() {
                "Waiting for a shortcut…".to_string()
            } else {
                chords.join("  then  ")
            };
            frame.render_widget(
                Paragraph::new(format!(
                    "{value}\n\nPress one or two Ctrl/Alt chords.\nEnter save · Backspace remove · Esc cancel"
                ))
                .block(Block::bordered().title(" Record search shortcut "))
                .wrap(Wrap { trim: false }),
                area,
            );
        }
        Modal::DateEditor { input } => {
            frame.render_widget(
                Paragraph::new(format!(
                    "{input}\n\nPreview: {}\n\nEnter save · Esc cancel",
                    super::app::date_preview(input)
                ))
                .block(Block::bordered().title(" Custom timestamp format "))
                .wrap(Wrap { trim: false }),
                area,
            );
        }
        Modal::RemoveSetup { profile } => {
            frame.render_widget(
                Paragraph::new(format!(
                    "Remove recall-managed setup from this startup file?\n\n{}\n\nEnter/y confirm · Esc/n cancel",
                    profile.display()
                ))
                .block(Block::bordered().title(" Remove shell integration "))
                .wrap(Wrap { trim: false }),
                area,
            );
        }
        Modal::RuleEditor {
            target,
            index,
            input,
        } => {
            let action = if index.is_some() { "Edit" } else { "Add" };
            let kind = match target {
                RuleTarget::Command => "command exclusion",
                RuleTarget::Output => "output exclusion",
            };
            frame.render_widget(
                Paragraph::new(format!(
                    "{input}\n\nEnter save · Esc cancel\nThe expression must be a valid Rust regular expression."
                ))
                .block(Block::bordered().title(format!(" {action} {kind} ")))
                .wrap(Wrap { trim: false }),
                area,
            );
        }
        Modal::DeleteRule { pattern, .. } => {
            frame.render_widget(
                Paragraph::new(format!(
                    "Delete this custom rule?\n\n{pattern}\n\nEnter/y confirm · Esc/n cancel"
                ))
                .block(Block::bordered().title(" Delete capture rule "))
                .wrap(Wrap { trim: false }),
                area,
            );
        }
    }
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
        app.category = 0;
        app.selected = 0;
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

    #[test]
    fn keeps_a_long_capture_selection_visible() {
        let mut config = crate::config::Config::default();
        for index in 0..20 {
            config.proxy.exclude.push(format!("^custom-{index}"));
        }
        let mut app = App::new(config);
        app.category = 1;
        app.selected = app.row_count() - 1;
        let backend = TestBackend::new(100, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, &mut app)).unwrap();

        assert!(terminal.backend().to_string().contains("^custom-19"));
    }
}
