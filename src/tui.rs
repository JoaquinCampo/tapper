use crate::app::{App, Mode, OutputView};
use crate::capture::PipelineResult;
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap};
use std::io::stdout;

pub fn run(result: PipelineResult) -> Result<()> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;

    // Install Ctrl+C handler that restores the terminal before exiting
    ctrlc::set_handler(move || {
        // Best-effort terminal restore — ignore errors since we're in a signal handler
        let _ = disable_raw_mode();
        let _ = std::io::stdout().execute(LeaveAlternateScreen);
        std::process::exit(0);
    })?;

    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;
    let mut app = App::new(result);

    let res = run_loop(&mut terminal, &mut app);

    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;

    res
}

fn run_loop(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|frame| draw(frame, app))?;

        if let Event::Key(key) = event::read()? {
            match app.mode {
                Mode::Normal => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        app.should_quit = true;
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        app.should_quit = true;
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if key.modifiers.contains(KeyModifiers::SHIFT) {
                            app.scroll_down(1);
                        } else {
                            app.select_next_stage();
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        if key.modifiers.contains(KeyModifiers::SHIFT) {
                            app.scroll_up(1);
                        } else {
                            app.select_prev_stage();
                        }
                    }
                    KeyCode::Char('J') => app.scroll_down(1),
                    KeyCode::Char('K') => app.scroll_up(1),
                    KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        app.scroll_down(20);
                    }
                    KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        app.scroll_up(20);
                    }
                    KeyCode::PageDown => app.scroll_down(20),
                    KeyCode::PageUp => app.scroll_up(20),
                    KeyCode::Home | KeyCode::Char('g') => app.scroll_offset = 0,
                    KeyCode::End | KeyCode::Char('G') => {
                        let lines = app.current_output_text().lines().count();
                        app.scroll_offset = lines.saturating_sub(1);
                    }
                    KeyCode::Tab => app.toggle_output_view(),
                    KeyCode::Char('/') => app.start_search(),
                    KeyCode::Char('n') => app.next_match(),
                    KeyCode::Char('N') => app.prev_match(),
                    _ => {}
                },
                Mode::Search => match key.code {
                    KeyCode::Enter => app.finish_search(),
                    KeyCode::Esc => app.cancel_search(),
                    KeyCode::Char(c) => {
                        app.search_query.push(c);
                    }
                    KeyCode::Backspace => {
                        app.search_query.pop();
                    }
                    _ => {}
                },
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Main layout: header, body, footer
    let main_layout = Layout::vertical([
        Constraint::Length(3),  // header
        Constraint::Min(5),    // body
        Constraint::Length(1), // footer
    ])
    .split(area);

    draw_header(frame, app, main_layout[0]);

    // Body: stages list (left) + output preview (right)
    let body_layout = Layout::horizontal([
        Constraint::Percentage(35),
        Constraint::Percentage(65),
    ])
    .split(main_layout[1]);

    draw_stages(frame, app, body_layout[0]);
    draw_output(frame, app, body_layout[1]);
    draw_footer(frame, app, main_layout[2]);

    // Search overlay
    if app.mode == Mode::Search {
        draw_search(frame, app, area);
    }
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let pipeline_str = app
        .result
        .stages
        .iter()
        .map(|s| s.stage.command.as_str())
        .collect::<Vec<_>>()
        .join(" │ ");

    let total = format!(" [{:.2?}]", app.result.total_duration);

    let header = Paragraph::new(Line::from(vec![
        Span::styled(" tapper ", Style::default().fg(Color::Black).bg(Color::Cyan).bold()),
        Span::raw(" "),
        Span::styled(pipeline_str, Style::default().fg(Color::White)),
        Span::styled(total, Style::default().fg(Color::DarkGray)),
    ]))
    .block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(Color::DarkGray)),
    );

    frame.render_widget(header, area);
}

fn draw_stages(frame: &mut Frame, app: &App, area: Rect) {
    let max_lines = app
        .result
        .stages
        .iter()
        .map(|s| s.line_count)
        .max()
        .unwrap_or(1)
        .max(1);

    // Bar width budget: leave room for "     " prefix and a space after bar
    let bar_max_width: usize = 12;

    let items: Vec<ListItem> = app
        .result
        .stages
        .iter()
        .enumerate()
        .map(|(i, stage)| {
            let is_selected = i == app.selected_stage;
            let exit_ok = stage.exit_code == Some(0) || stage.exit_code.is_none();

            let status_icon = if exit_ok { "●" } else { "✗" };
            let status_color = if exit_ok { Color::Green } else { Color::Red };

            let bytes = format_bytes_short(stage.byte_count);
            let duration = format!("{:.0?}", stage.duration);

            let line = Line::from(vec![
                Span::styled(
                    format!(" {} ", status_icon),
                    Style::default().fg(status_color),
                ),
                Span::styled(
                    format!("{}. ", i + 1),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    stage.stage.command.clone(),
                    Style::default()
                        .fg(if is_selected { Color::White } else { Color::Gray })
                        .bold(),
                ),
            ]);

            // Mini bar chart showing relative output size
            let bar_len = ((stage.line_count as f64 / max_lines as f64) * bar_max_width as f64)
                .ceil() as usize;
            let bar_len = bar_len.max(if stage.line_count > 0 { 1 } else { 0 });
            let bar_filled: String = "█".repeat(bar_len);
            let bar_empty: String = "░".repeat(bar_max_width.saturating_sub(bar_len));
            let bar_color = if bar_len > bar_max_width * 3 / 4 {
                Color::Cyan
            } else if bar_len > bar_max_width / 3 {
                Color::Blue
            } else {
                Color::DarkGray
            };

            let stats = Line::from(vec![
                Span::raw("     "),
                Span::styled(bar_filled, Style::default().fg(bar_color)),
                Span::styled(bar_empty, Style::default().fg(Color::Rgb(40, 40, 40))),
                Span::raw(" "),
                Span::styled(
                    format!("{} lines", stage.line_count),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(" · ", Style::default().fg(Color::DarkGray)),
                Span::styled(bytes, Style::default().fg(Color::DarkGray)),
                Span::styled(" · ", Style::default().fg(Color::DarkGray)),
                Span::styled(duration, Style::default().fg(Color::DarkGray)),
            ]);

            let style = if is_selected {
                Style::default().bg(Color::Rgb(30, 30, 50))
            } else {
                Style::default()
            };

            ListItem::new(vec![line, stats]).style(style)
        })
        .collect();

    let stages_block = Block::default()
        .title(Span::styled(
            " Stages ",
            Style::default().fg(Color::Cyan).bold(),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let list = List::new(items).block(stages_block);
    frame.render_widget(list, area);
}

fn draw_output(frame: &mut Frame, app: &App, area: Rect) {
    let stage = app.current_stage();
    let text = app.current_output_text();
    let lines: Vec<&str> = text.lines().collect();
    let total_lines = lines.len();

    let view_label = match app.output_view {
        OutputView::Stdout => "stdout",
        OutputView::Stderr => "stderr",
    };

    let title = format!(
        " {} · Stage {} · {} ",
        view_label,
        stage.stage.index + 1,
        stage.stage.command
    );

    let search_info = if !app.search_matches.is_empty() {
        format!(
            " [{}/{}] ",
            app.current_match + 1,
            app.search_matches.len()
        )
    } else if !app.search_query.is_empty() {
        " [no matches] ".to_string()
    } else {
        String::new()
    };

    // Build styled lines with line numbers
    let visible_lines: Vec<Line> = lines
        .iter()
        .enumerate()
        .skip(app.scroll_offset)
        .map(|(i, line)| {
            let is_match = app.search_matches.contains(&i);
            let line_num = Span::styled(
                format!("{:>4} ", i + 1),
                Style::default().fg(Color::DarkGray),
            );
            let content = if is_match {
                Span::styled(
                    line.to_string(),
                    Style::default().bg(Color::Rgb(60, 60, 0)).fg(Color::Yellow),
                )
            } else {
                Span::raw(line.to_string())
            };
            Line::from(vec![line_num, content])
        })
        .collect();

    let output_block = Block::default()
        .title(Span::styled(title, Style::default().fg(Color::Cyan).bold()))
        .title_bottom(Line::from(vec![
            Span::styled(search_info, Style::default().fg(Color::Yellow)),
            Span::styled(
                format!(" {}/{} ", app.scroll_offset + 1, total_lines.max(1)),
                Style::default().fg(Color::DarkGray),
            ),
        ]))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let paragraph = Paragraph::new(visible_lines)
        .block(output_block)
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);

    // Scrollbar
    if total_lines > 0 {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None);
        let mut scrollbar_state =
            ScrollbarState::new(total_lines).position(app.scroll_offset);
        frame.render_stateful_widget(
            scrollbar,
            area.inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut scrollbar_state,
        );
    }
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let keys = match app.mode {
        Mode::Normal => vec![
            ("↑↓/jk", "stage"),
            ("J/K", "scroll"),
            ("Tab", "stderr"),
            ("/", "search"),
            ("n/N", "next/prev"),
            ("q", "quit"),
        ],
        Mode::Search => vec![
            ("Enter", "confirm"),
            ("Esc", "cancel"),
        ],
    };

    let spans: Vec<Span> = keys
        .iter()
        .flat_map(|(key, desc)| {
            vec![
                Span::styled(
                    format!(" {} ", key),
                    Style::default().fg(Color::Black).bg(Color::DarkGray),
                ),
                Span::styled(format!(" {} ", desc), Style::default().fg(Color::DarkGray)),
                Span::raw(" "),
            ]
        })
        .collect();

    let footer = Paragraph::new(Line::from(spans));
    frame.render_widget(footer, area);
}

fn draw_search(frame: &mut Frame, app: &App, area: Rect) {
    let search_area = Rect {
        x: area.x,
        y: area.height.saturating_sub(3),
        width: area.width.min(50),
        height: 3,
    };

    let search = Paragraph::new(Line::from(vec![
        Span::styled(" / ", Style::default().fg(Color::Yellow).bold()),
        Span::raw(&app.search_query),
        Span::styled("█", Style::default().fg(Color::Yellow)),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow))
            .title(Span::styled(" Search ", Style::default().fg(Color::Yellow))),
    );

    frame.render_widget(Clear, search_area);
    frame.render_widget(search, search_area);
}

fn format_bytes_short(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{}B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.0}K", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1}M", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1}G", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
