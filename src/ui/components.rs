use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    prelude::Alignment,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, HighlightSpacing, List, ListItem, ListState, Paragraph, Row, Table, Wrap,
        block::{Position, Title},
    },
};
use std::{fs, time::Instant};

use crate::slurm::SlurmParser;
use crate::ui::App;
use crate::{
    AppState, FeedbackMessage,
    models::{Job, JobState},
};

fn render_text_popup(popup_text: String, app: &App, frame: &mut Frame) {
    let popup_area = centered_rect(30, 9, frame.area());
    frame.render_widget(Clear, popup_area);

    let popup = Paragraph::new(app.input.as_str())
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(popup_text)
                .style(Style::default().fg(Color::Yellow)),
        )
        .wrap(Wrap { trim: true })
        .alignment(Alignment::Center);

    frame.render_widget(popup, popup_area);
}

pub fn render_app(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(frame.area());

    render_status_bar(frame, app, chunks[0]);

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Percentage(60),
        ])
        .split(chunks[1]);

    render_jobs_list(frame, app, main_chunks[0]);

    let details_height = app
        .selected_job
        .as_ref()
        .map(|j| count_job_detail_lines(j) as u16 + 2)
        .unwrap_or(3);

    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(details_height),
            Constraint::Min(0),
        ])
        .split(main_chunks[1]);

    render_job_details(frame, app, right_chunks[0]);
    render_job_logs(frame, app, right_chunks[1]);

    render_help_bar(&app.state, frame, chunks[2]);

    match &app.state {
        AppState::UserSearchPopup => render_text_popup("Search User:".to_string(), app, frame),
        AppState::PartitionSearchPopup => {
            render_text_popup("Search Partition:".to_string(), app, frame)
        }
        AppState::NodelistSelectPopup {
            visible_indices,
            search,
            ..
        } => {
            let popup_area = centered_rect(70, 80, frame.area());

            frame.render_widget(Clear, popup_area);

            let block = Block::bordered()
                .title("Select nodes".to_string())
                .title(
                    Title::from(format!(" Fuzzy search: [{}] ", search)).position(Position::Bottom),
                )
                .border_style(Style::new().fg(Color::Cyan));

            let inner_area = block.inner(popup_area);

            let show_none = search.is_empty();
            let total_items = if show_none { 1 } else { 0 } + visible_indices.len();

            let nodes: Vec<ListItem> = (0..total_items)
                .map(|display_idx| {
                    let (display_text, is_none, checked, node_idx) = if show_none && display_idx == 0 {
                        ("None (clear all)".to_string(), true, app.current_nodelist.is_empty(), None)
                    } else {
                        let item_idx = if show_none { display_idx - 1 } else { display_idx };
                        let idx = visible_indices[item_idx];
                        let node = &app.nodelist[idx];
                        let checked = app.current_nodelist.contains(node);
                        (node.clone(), false, checked, Some(idx))
                    };

                    let prefix = if checked { " [✓] " } else { " [ ] " };

                    let style = if checked {
                        Style::new().fg(Color::Green).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };

                    let mut item = ListItem::new(format!("{prefix}{display_text}")).style(style);

                    if let Some(selected_idx) = app.node_list_state.selected()
                        && selected_idx < total_items
                    {
                        let selected_is_none = show_none && selected_idx == 0;
                        let selected_node_idx = if show_none {
                            if selected_idx == 0 {
                                None
                            } else {
                                Some(visible_indices[selected_idx - 1])
                            }
                        } else {
                            Some(visible_indices[selected_idx])
                        };

                        if (is_none && selected_is_none) || node_idx == selected_node_idx {
                            item = item.style(Style::new().bg(Color::DarkGray));
                        }
                    }

                    item
                })
                .collect();

            let list = List::new(nodes)
                .block(block)
                .highlight_style(Style::new().bg(Color::Blue).add_modifier(Modifier::BOLD))
                .highlight_symbol("➤ ")
                .highlight_spacing(HighlightSpacing::Always);

            frame.render_stateful_widget(list, inner_area, &mut app.node_list_state);
        }
        AppState::PartitionSelectPopup {
            visible_indices,
            search,
            ..
        } => {
            render_select_popup(
                frame,
                "Select Partition",
                search,
                visible_indices,
                &app.partition_list,
                &mut app.partition_list_state,
            );
        }
        AppState::UserSelectPopup {
            visible_indices,
            search,
            ..
        } => {
            render_select_popup(
                frame,
                "Select User",
                search,
                visible_indices,
                &app.user_list,
                &mut app.user_list_state,
            );
        }
        AppState::CancelJobPopup => {
            let popup_area = centered_rect_min(40, 15, 30, 5, frame.area());

            frame.render_widget(Clear, popup_area);

            if let Some(job) = &app.selected_job {
                let lines = vec![
                    Line::from(vec![
                        Span::raw("Cancel "),
                        Span::styled(&job.name, Style::default().fg(Color::Cyan)),
                        Span::raw(" ("),
                        Span::raw(&job.job_id),
                        Span::raw(")?"),
                    ]),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("y", Style::default().fg(Color::Green)),
                        Span::raw(" / "),
                        Span::styled("n", Style::default().fg(Color::Red)),
                        Span::raw(" / "),
                        Span::raw("esc"),
                    ]),
                ];

                let popup = Paragraph::new(lines)
                    .style(Style::default().fg(Color::White).bg(Color::Black))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_style(Style::default().fg(Color::Yellow)),
                    )
                    .alignment(Alignment::Center);

                frame.render_widget(popup, popup_area);
            } else {
                app.feedback_message = Some(FeedbackMessage {
                    message: "Job already finished - Reverting back to main view!".to_string(),
                    title: "Job Finished".to_string(),
                });
                app.state = AppState::Feedback;
                app.feedback_shown_at = Some(Instant::now());
            };
        }
        AppState::Feedback => {
            let popup_area = centered_rect_min(50, 7, 40, 5, frame.area());
            if let Some(feedback) = &app.feedback_message {
                let (msg_text, title_text) = feedback.as_parts();

                let style = Style::default().fg(Color::White);

                let popup = Paragraph::new(msg_text.to_string())
                    .style(style)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(title_text.to_string())
                            .style(Style::default().fg(Color::Yellow)),
                    )
                    .alignment(Alignment::Center)
                    .wrap(Wrap { trim: true });

                frame.render_widget(popup, popup_area);
            } else {
                app.state = AppState::Normal;
            }
        }
        AppState::JobLogPopup(log_state) => {
            render_job_log(frame, log_state, frame.area());
        }
        AppState::SortSelectPopup { .. } => {
            render_sort_popup(frame, app, frame.area());
        }

        _ => {}
    }
}

fn render_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let running = app.job_list.running_jobs().len();
    let pending = app.job_list.pending_jobs().len();
    let completed = app.job_list.completed_jobs().len();

    let job_counts = format!(
        "Jobs: {} (R:{running} PD:{pending} CD:{completed})",
        app.job_list.jobs.len()
    );

    let mut status_text = format!("LazySlurm | {}", job_counts);

    if let Some(user) = &app.current_user {
        status_text.push_str(&format!(" | User: {}", user));
    }

    if let Some(part) = &app.current_partition {
        status_text.push_str(&format!(" | Part: {}", part));
    }

    if app.is_loading {
        status_text.push_str(" | Loading...");
    }

    if let Some(error) = &app.error_message {
        status_text = format!("ERROR: {}", error);
    }

    let status = Paragraph::new(status_text).style(if app.error_message.is_some() {
        Style::default().fg(Color::Red)
    } else {
        Style::default()
    });

    frame.render_widget(status, area);
}

fn render_jobs_list(frame: &mut Frame, app: &mut App, area: Rect) {
    let header_style = Style::default()
        .fg(Color::White)
        .add_modifier(Modifier::BOLD);

    let header = Row::new(vec![
        Span::styled("ID", header_style),
        Span::styled("Name", header_style),
        Span::styled("State", header_style),
        Span::styled("Time", header_style),
        Span::styled("Part", header_style),
    ])
    .height(1);

    let rows: Vec<Row> = app
        .job_list
        .jobs
        .iter()
        .map(|job| {
            let state_color = match job.state {
                JobState::Running => Color::Green,
                JobState::Pending => Color::Yellow,
                JobState::Completed => Color::Cyan,
                JobState::Failed => Color::Red,
                JobState::Cancelled => Color::Magenta,
                _ => Color::Gray,
            };

            let job_id = job.display_id();
            let job_name = truncate(&job.name, 20);
            let time_used = job.time_used.as_deref().unwrap_or("--");

            Row::new(vec![
                Span::raw(job_id),
                Span::raw(job_name),
                Span::styled(format!("{}", job.state), Style::default().fg(state_color)),
                Span::raw(time_used),
                Span::raw(&job.partition),
            ])
        })
        .collect();

    let title = format!("Jobs ({} total)", app.job_list.jobs.len());

    let widths = calculate_table_widths(area.width);

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().title(title).borders(Borders::ALL))
        .highlight_style(Style::default().bg(Color::Blue).fg(Color::White))
        .highlight_symbol("")
        .highlight_spacing(HighlightSpacing::Always);

    frame.render_stateful_widget(table, area, &mut app.job_table_state);
}

fn calculate_table_widths(available_width: u16) -> [Constraint; 5] {
    let id_width = 8;
    let state_width = 6;
    let time_width = 7;
    let fixed_width = id_width + state_width + time_width + 3;
    let min_name = 10;
    let max_name = 25;
    let min_part = 8;

    let flex_space = available_width.saturating_sub(fixed_width);
    let min_needed = min_name + min_part;

    let (name_width, part_width) = if flex_space < min_needed {
        (min_name, min_part)
    } else {
        let extra = flex_space - min_needed;
        let name_extra = (extra as f32 * 0.6) as u16;
        let part_extra = extra - name_extra;
        (
            (min_name + name_extra).min(max_name),
            (min_part + part_extra).max(min_part),
        )
    };

    [
        Constraint::Length(id_width),
        Constraint::Length(name_width),
        Constraint::Length(state_width),
        Constraint::Length(time_width),
        Constraint::Length(part_width),
    ]
}

fn render_job_details(frame: &mut Frame, app: &App, area: Rect) {
    if let Some(job) = app.get_selected_job() {
        let lines = build_job_details_lines(job);
        let height = lines.len() as u16 + 2;

        let details = Paragraph::new(lines)
            .block(Block::default().title("Job Details").borders(Borders::ALL))
            .wrap(Wrap { trim: true });

        let inner_area = Rect::new(area.x, area.y, area.width, height);
        frame.render_widget(details, inner_area);
    } else if app.job_list.jobs.is_empty() {
        let text = Paragraph::new("No jobs found.\nTry: lazyslurm --user <username>")
            .block(Block::default().title("Job Details").borders(Borders::ALL))
            .wrap(Wrap { trim: true })
            .alignment(Alignment::Center);
        frame.render_widget(text, area);
    } else {
        let text = Paragraph::new("Select a job to view details")
            .block(Block::default().title("Job Details").borders(Borders::ALL))
            .wrap(Wrap { trim: true });
        frame.render_widget(text, area);
    }
}

fn count_job_detail_lines(job: &Job) -> usize {
    let mut count = 2;
    if job.nodes.is_some() {
        count += 1;
    }
    if job.node_list.is_some() {
        count += 1;
    }
    if job.submit_time.is_some() || job.start_time.is_some() {
        count += 1;
    }
    if job.reason.is_some() {
        count += 1;
    }
    count
}

fn build_job_details_lines(job: &Job) -> Vec<Line<'_>> {
    let state_color = match job.state {
        JobState::Running => Color::Green,
        JobState::Pending => Color::Yellow,
        JobState::Completed => Color::Cyan,
        JobState::Failed => Color::Red,
        JobState::Cancelled => Color::Magenta,
        _ => Color::Gray,
    };

    let label_style = Style::default().fg(Color::DarkGray);
    let state_style = Style::default().fg(state_color);
    let sep = |c: &'static str| Span::styled(c, Style::default().fg(Color::DarkGray));

    let mut lines = Vec::new();

    lines.push(Line::from(vec![
        Span::styled("ID: ", label_style),
        Span::raw(job.display_id()),
        sep(" | "),
        Span::styled("User: ", label_style),
        Span::raw(&job.user),
        sep(" | "),
        Span::styled("Part: ", label_style),
        Span::raw(&job.partition),
    ]));

    lines.push(Line::from(vec![
        Span::styled("State: ", label_style),
        Span::styled(format!("{}", job.state), state_style),
        sep(" | "),
        Span::styled("Time: ", label_style),
        Span::raw(job.time_used.as_deref().unwrap_or("--")),
    ]));

    if let Some(nodes) = job.nodes {
        lines.push(Line::from(vec![
            Span::styled("Nodes: ", label_style),
            Span::raw(nodes.to_string()),
        ]));
    }

    if let Some(node_list) = &job.node_list {
        lines.push(Line::from(vec![
            Span::styled("Node List: ", label_style),
            Span::raw(node_list),
        ]));
    }

    if job.submit_time.is_some() || job.start_time.is_some() {
        let mut spans = Vec::new();
        if let Some(submit_time) = &job.submit_time {
            spans.push(Span::raw(format!("Submitted: {}", submit_time.format("%Y-%m-%d %H:%M"))));
        }
        if job.submit_time.is_some() && job.start_time.is_some() {
            spans.push(sep(" | "));
        }
        if let Some(start_time) = &job.start_time {
            spans.push(Span::raw(format!("Started: {}", start_time.format("%Y-%m-%d %H:%M"))));
        }
        lines.push(Line::from(spans));
    }

    if let Some(reason) = &job.reason {
        lines.push(Line::from(vec![
            Span::styled("Reason: ", label_style),
            Span::raw(reason),
        ]));
    }

    lines
}

fn render_job_logs(frame: &mut Frame, app: &App, area: Rect) {
    let content = if let Some(job) = app.get_selected_job() {
        let available_lines = area.height.saturating_sub(4) as usize;
        read_job_logs(job, available_lines)
    } else {
        "Select a job to view logs".to_string()
    };

    let logs = Paragraph::new(content)
        .block(Block::default().title("Job Logs").borders(Borders::ALL))
        .wrap(Wrap { trim: true });

    frame.render_widget(logs, area);
}

fn render_help_bar(app_state: &AppState, frame: &mut Frame, area: Rect) {
    let help_text = match app_state {
        AppState::Normal => {
            "q:quit | r:refresh | c:cancel | p:part | u:user | n:node | s:sort | o:out | e:err | ↑↓:nav"
        }
        AppState::CancelJobPopup => "y:confirm | n:reject | esc:cancel",
        AppState::PartitionSearchPopup => "esc:close | Enter:select",
        AppState::UserSearchPopup => "esc:close | Enter:select",
        AppState::NodelistSelectPopup { .. } => {
            "esc:cancel | Enter:close | Space:toggle | Ctrl-a/d/i:select/deselect/invert"
        }
        AppState::PartitionSelectPopup { .. } => {
            "esc:close | ↑↓:nav | Enter:select"
        }
        AppState::UserSelectPopup { .. } => {
            "esc:close | ↑↓:nav | Enter:select"
        }
        AppState::JobLogPopup(log_state) => {
            if log_state.is_searching {
                "Enter:search | Backspace:delete | Esc:cancel"
            } else {
                "↑↓/PgUp/PgDn:scroll | g/G:top/bottom | /:search | n/N:next/prev | Esc:close"
            }
        }
        AppState::SortSelectPopup { .. } => {
            "esc:close | ↑↓:nav | Enter:select"
        }
        AppState::Feedback => "",
    };

    let help_bar = Paragraph::new(help_text)
        .style(Style::new().bg(Color::DarkGray).fg(Color::White))
        .alignment(Alignment::Center);

    frame.render_widget(help_bar, area);
}

fn read_job_logs(job: &Job, max_lines: usize) -> String {
    let log_paths = SlurmParser::get_job_log_paths(job);

    for path in &log_paths {
        if let Ok(content) = fs::read_to_string(path) {
            if content.is_empty() {
                return format!("Log file exists but is empty: {}", path);
            }

            let lines: Vec<&str> = content.lines().collect();

            if lines.len() <= max_lines {
                return format!("Log file: {}\n{}\n{}", path, "-".repeat(50), content);
            }

            let start = lines.len().saturating_sub(max_lines);
            let tail_lines = &lines[start..];

            return format!(
                "Log file: {}  (showing last {} of {} lines)\n{}\n{}",
                path,
                max_lines,
                lines.len(),
                "-".repeat(50),
                tail_lines.join("\n")
            );
        }
    }

    if log_paths.is_empty() {
        "No log file paths available".to_string()
    } else {
        format!("No logs found. Checked paths:\n{}", log_paths.join("\n"))
    }
}

fn render_select_popup(
    frame: &mut Frame,
    title: &str,
    search: &str,
    visible_indices: &[usize],
    items: &[String],
    list_state: &mut ListState,
) {
    let popup_area = centered_rect(40, 60, frame.area());

    frame.render_widget(Clear, popup_area);

    let block = Block::bordered()
        .title(title.to_string())
        .title(
            Title::from(format!(" Search: [{}] ", search)).position(Position::Bottom),
        )
        .border_style(Style::new().fg(Color::Cyan));

    let inner_area = block.inner(popup_area);

    let total_items = if search.is_empty() { 1 } else { 0 } + visible_indices.len();

    let list_items: Vec<ListItem> = (0..total_items)
        .map(|display_idx| {
            let (display_text, actual_idx_for_style) = if search.is_empty() && display_idx == 0 {
                ("None (show all)".to_string(), None)
            } else {
                let item_idx = if search.is_empty() {
                    display_idx - 1
                } else {
                    display_idx
                };
                let actual_idx = visible_indices.get(item_idx).copied().unwrap_or(0);
                (
                    items.get(actual_idx).cloned().unwrap_or_default(),
                    Some(actual_idx),
                )
            };

            let mut item = ListItem::new(format!("  {}", display_text));

            if let Some(selected_idx) = list_state.selected()
                && selected_idx < total_items
            {
                let selected_actual = if search.is_empty() && selected_idx == 0 {
                    None
                } else {
                    let item_idx = if search.is_empty() {
                        selected_idx - 1
                    } else {
                        selected_idx
                    };
                    visible_indices.get(item_idx).copied()
                };
                if actual_idx_for_style == selected_actual {
                    item = item.style(Style::new().bg(Color::DarkGray));
                }
            }

            item
        })
        .collect();

    let list = List::new(list_items)
        .block(block)
        .highlight_style(Style::new().bg(Color::Blue).add_modifier(Modifier::BOLD))
        .highlight_symbol("➤ ")
        .highlight_spacing(HighlightSpacing::Always);

    frame.render_stateful_widget(list, inner_area, list_state);
}

fn render_sort_popup(frame: &mut Frame, app: &mut App, area: Rect) {
    let popup_area = centered_rect(40, 70, area);

    frame.render_widget(Clear, popup_area);

    let block = Block::bordered()
        .title("Sort Jobs")
        .border_style(Style::new().fg(Color::Cyan));

    let inner_area = block.inner(popup_area);

    let sort_options = crate::utils::SortField::all();
    let current_sort = app.sort_field;

    let list_items: Vec<ListItem> = sort_options
        .iter()
        .map(|sort_field| {
            let display_name = sort_field.display_name();
            let is_selected = *sort_field == current_sort;
            
            let prefix = if is_selected { "➤ " } else { "   " };
            
            let style = if is_selected {
                Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            ListItem::new(format!("{}{}", prefix, display_name)).style(style)
        })
        .collect();

    let list = List::new(list_items)
        .block(block)
        .highlight_style(Style::new().bg(Color::Blue).add_modifier(Modifier::BOLD))
        .highlight_symbol("➤ ")
        .highlight_spacing(HighlightSpacing::Always);

    frame.render_stateful_widget(list, inner_area, &mut app.sort_list_state);
}

fn render_job_log(frame: &mut Frame, log_state: &crate::ui::JobLogState, area: Rect) {
    frame.render_widget(Clear, area);

    let log_type_name = match log_state.log_type {
        crate::ui::LogType::Output => "Output Log",
        crate::ui::LogType::Error => "Error Log",
    };

    let title = format!("{} - {}", log_type_name, log_state.file_path);

    let block = Block::bordered()
        .title(title)
        .border_style(Style::new().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black).fg(Color::White));

    let inner_area = block.inner(area);

    let visible_height = inner_area.height as usize;
    let total_lines = log_state.lines.len();

    let start_line = log_state.scroll_offset;
    let end_line = (start_line + visible_height).min(total_lines);

    let visible_lines = &log_state.lines[start_line..end_line];

    let search_query_lower = log_state.search_query.to_lowercase();
    let current_match = log_state.search_matches.get(log_state.current_match_index);

    let content: Vec<Line> = visible_lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let line_num = start_line + i + 1;
            let actual_line_idx = start_line + i;

            if !search_query_lower.is_empty()
                && line.to_lowercase().contains(&search_query_lower)
            {
                let lower_line = line.to_lowercase();
                let mut spans: Vec<Span> = vec![Span::styled(
                    format!("{:5}| ", line_num),
                    Style::new().fg(Color::DarkGray),
                )];
                let mut last_end = 0;

                for (start, _) in lower_line.match_indices(&search_query_lower) {
                    if start > last_end {
                        spans.push(Span::raw(&line[last_end..start]));
                    }
                    let is_current_match = current_match == Some(&actual_line_idx);
                    let highlight_style = if is_current_match {
                        Style::new().bg(Color::Cyan).fg(Color::Black).add_modifier(Modifier::BOLD)
                    } else {
                        Style::new().bg(Color::Yellow).fg(Color::Black)
                    };
                    spans.push(Span::styled(&line[start..start + search_query_lower.len()], highlight_style));
                    last_end = start + search_query_lower.len();
                }

                if last_end < line.len() {
                    spans.push(Span::raw(&line[last_end..]));
                }

                Line::from(spans)
            } else {
                Line::from(vec![
                    Span::styled(format!("{:5}| ", line_num), Style::new().fg(Color::DarkGray)),
                    Span::raw(line),
                ])
            }
        })
        .collect();

    let paragraph = Paragraph::new(content)
        .block(block)
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);

    if log_state.is_searching {
        let search_width = 30u16.min(area.width.saturating_sub(4));
        let search_x = area.x + (area.width - search_width) / 2;
        let search_area = Rect::new(search_x, area.y + area.height - 3, search_width, 3);

        frame.render_widget(Clear, search_area);

        let search_bar = Paragraph::new(format!("/{}", log_state.search_query))
            .style(Style::new().bg(Color::DarkGray).fg(Color::White))
            .block(
                Block::default()
                    .title("Search")
                    .borders(Borders::ALL)
                    .border_style(Style::new().fg(Color::Yellow)),
            );

        frame.render_widget(search_bar, search_area);
    } else {
        let help_area = Rect::new(area.x, area.y + area.height - 1, area.width, 1);

        let help_text = "↑↓ scroll | PgUp/PgDn/Space page | g/G top/bottom | / search | n/N next/prev | Esc close";
        
        let match_info = if log_state.search_matches.is_empty() {
            help_text.to_string()
        } else {
            format!(
                "{} | Match {}/{}",
                help_text,
                log_state.current_match_index + 1,
                log_state.search_matches.len()
            )
        };

        let help_bar = Paragraph::new(match_info)
            .style(Style::new().bg(Color::DarkGray).fg(Color::White))
            .alignment(Alignment::Center);

        frame.render_widget(help_bar, help_area);
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn centered_rect_min(percent_x: u16, percent_y: u16, min_w: u16, min_h: u16, r: Rect) -> Rect {
    let target_w = (r.width as f32 * percent_x as f32 / 100.0).ceil() as u16;
    let target_h = (r.height as f32 * percent_y as f32 / 100.0).ceil() as u16;
    let w = target_w.max(min_w).min(r.width.saturating_sub(2));
    let h = target_h.max(min_h).min(r.height.saturating_sub(2));
    let x = (r.width - w) / 2 + r.x;
    let y = (r.height - h) / 2 + r.y;
    Rect::new(x, y, w, h)
}
