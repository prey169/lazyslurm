use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    prelude::Alignment,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, HighlightSpacing, List, ListItem, ListState, Paragraph, Wrap,
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
            Constraint::Length(3),
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

    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Percentage(40),
            Constraint::Percentage(20),
        ])
        .split(main_chunks[1]);

    render_job_details(frame, app, right_chunks[0]);
    render_job_logs(frame, app, right_chunks[1]);
    render_quick_info(frame, app, right_chunks[2]);

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

            let nodes: Vec<ListItem> = visible_indices
                .iter()
                .copied()
                .map(|idx| {
                    let node = &app.nodelist[idx];
                    let checked = app.current_nodelist.contains(node);

                    let prefix = if checked { " [✓] " } else { " [ ] " };

                    let style = if checked {
                        Style::new().fg(Color::Green).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };

                    let mut item = ListItem::new(format!("{prefix}{node}")).style(style);

                    if let Some(selected_idx) = app.node_list_state.selected()
                        && selected_idx < visible_indices.len()
                        && visible_indices[selected_idx] == idx
                    {
                        item = item.style(Style::new().bg(Color::DarkGray));
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
            let popup_area = centered_rect(30, 7, frame.area());

            frame.render_widget(Clear, popup_area);
            let selected_job_clone = app.selected_job.clone();
            let popup: Paragraph;

            if let Some(job_clone) = selected_job_clone {
                let selected_job_id = job_clone.job_id;
                popup = Paragraph::new(format!("Cancel job id: {selected_job_id}? (y/n)",))
                    .style(Style::default().fg(Color::White))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title("Confirm")
                            .style(Style::default().fg(Color::Yellow)),
                    )
                    .wrap(Wrap { trim: true })
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
            let popup_area = centered_rect(50, 7, frame.area());
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
    let mut status_text = "LazySlurm".to_string();

    if let Some(user) = &app.current_user {
        status_text.push_str(&format!(" - User: {}", user));
    }

    if let Some(part) = &app.current_partition {
        status_text.push_str(&format!(" - Part: {}", part));
    }

    status_text.push_str(&format!(" - Jobs: {}", app.job_list.jobs.len()));

    if app.is_loading {
        status_text.push_str(" - Loading...");
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
    let jobs: Vec<ListItem> = app
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
            let job_name = truncate(&job.name, 15);
            let time_used = job.time_used.as_deref().unwrap_or("--");

            ListItem::new(Line::from(vec![
                Span::styled(format!("{:<12} ", job_id), Style::default()),
                Span::styled(format!("{:<15} ", job_name), Style::default()),
                Span::styled(format!("{} ", job.state), Style::default().fg(state_color)),
                Span::styled(time_used.to_string(), Style::default()),
            ]))
        })
        .collect();

    let title = format!("Jobs ({} total)", app.job_list.jobs.len());

    let jobs_list = List::new(jobs)
        .block(Block::default().title(title).borders(Borders::ALL))
        .highlight_style(Style::default().bg(Color::Blue).fg(Color::White))
        .highlight_symbol("➤ ")
        .scroll_padding(2)
        .repeat_highlight_symbol(false);

    frame.render_stateful_widget(jobs_list, area, &mut app.job_list_state);
}

fn render_job_details(frame: &mut Frame, app: &App, area: Rect) {
    let details = if let Some(job) = app.get_selected_job() {
        Paragraph::new(format_job_details(job))
            .block(Block::default().title("Job Details").borders(Borders::ALL))
            .wrap(Wrap { trim: true })
    } else if app.job_list.jobs.is_empty() {
        let lines = vec![
            Line::from(""),
            Line::from("        L A Z Y S L U R M       "),
            Line::from("    Tom Hill 2025 - tom@hill.xyz"),
            Line::from(""),
            Line::from(""),
            Line::from("No jobs found!"),
            Line::from(""),
            Line::from("Try running: lazyslurm --user <username>"),
            Line::from("or check if SLURM is available."),
            Line::from(""),
            Line::from(Span::styled(
                "\"We do not remember days; we remember moments.\" - Cesare Pavese",
                Style::default().add_modifier(Modifier::ITALIC),
            )),
        ];
        Paragraph::new(lines)
            .block(Block::default().title("Job Details").borders(Borders::ALL))
            .wrap(Wrap { trim: false })
    } else {
        Paragraph::new("Select a job to view details")
            .block(Block::default().title("Job Details").borders(Borders::ALL))
            .wrap(Wrap { trim: true })
    };

    frame.render_widget(details, area);
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

fn render_quick_info(frame: &mut Frame, app: &App, area: Rect) {
    let running_count = app.running_jobs().len();
    let pending_count = app.pending_jobs().len();
    let completed_count = app.completed_jobs().len();

    let content = format!(
        "Running: {} | Pending: {} | Completed: {}",
        running_count, pending_count, completed_count
    );

    let quick_info =
        Paragraph::new(content).block(Block::default().title("Summary").borders(Borders::ALL));

    frame.render_widget(quick_info, area);
}

fn render_help_bar(app_state: &AppState, frame: &mut Frame, area: Rect) {
    let help_text = match app_state {
        AppState::Normal => {
            "q: quit | ↑↓: navigate | r: refresh | c: cancel | p: partitions | u: users | n: nodes | o: output | e: error | s: sort"
        }
        AppState::CancelJobPopup => "y: confirm | n: reject | esc: reject",
        AppState::PartitionSearchPopup => "esc: close | Enter: submit",
        AppState::UserSearchPopup => "esc: close | Enter: submit",
        AppState::NodelistSelectPopup { .. } => {
            "esc: close | Enter: submit | Space: toggle | Search: <chars> | Ctrl-a/d/i: select/deselect/invert"
        }
        AppState::PartitionSelectPopup { .. } => {
            "esc: close | ↑↓: navigate | Enter: select | Search: <chars>"
        }
        AppState::UserSelectPopup { .. } => {
            "esc: close | ↑↓: navigate | Enter: select | Search: <chars>"
        }
        AppState::JobLogPopup(log_state) => {
            if log_state.is_searching {
                "Enter: search | Backspace: delete | Esc: cancel search"
            } else {
                "↑↓ scroll | PgUp/PgDn/Space page | g/G top/bottom | / search | n/N next/prev | Esc close"
            }
        }
        AppState::SortSelectPopup { .. } => {
            "esc: close | ↑↓: navigate | Enter: select"
        }
        AppState::Feedback => "",
    };
    let help = Paragraph::new(help_text)
        .block(Block::default().borders(Borders::ALL))
        .style(Style::default().fg(Color::Gray));

    frame.render_widget(help, area);
}

fn format_job_details(job: &Job) -> String {
    let mut details = Vec::new();

    let state_description = match job.state {
        JobState::Running => "Running",
        JobState::Pending => "Pending",
        JobState::Completed => "Completed",
        JobState::Cancelled => "Cancelled",
        JobState::Failed => "Failed",
        JobState::Timeout => "Timeout",
        JobState::NodeFail => "Node Fail",
        JobState::Preempted => "Preempted",
        JobState::Unknown(_) => "Unknown",
    };

    details.push(format!("Job ID: {}", job.display_id()));
    details.push(format!("Name: {}", job.name));
    details.push(format!("User: {}", job.user));
    details.push(format!("State: {} ({})", job.state, state_description));
    details.push(format!("Partition: {}", job.partition));

    if let Some(nodes) = job.nodes {
        details.push(format!("Nodes: {}", nodes));
    }

    if let Some(node_list) = &job.node_list {
        details.push(format!("Node List: {}", node_list));
    }

    if let Some(submit_time) = &job.submit_time {
        details.push(format!(
            "Submitted: {}",
            submit_time.format("%Y-%m-%d %H:%M:%S")
        ));
    }

    if let Some(start_time) = &job.start_time {
        details.push(format!(
            "Started: {}",
            start_time.format("%Y-%m-%d %H:%M:%S")
        ));
    }

    if let Some(duration) = job.duration() {
        let total_seconds = duration.num_seconds();
        let hours = total_seconds / 3600;
        let minutes = (total_seconds % 3600) / 60;
        let seconds = total_seconds % 60;
        details.push(format!("Duration: {}h {}m {}s", hours, minutes, seconds));
    }

    if let Some(working_dir) = &job.working_dir {
        details.push(format!("Work Dir: {}", working_dir));
    }

    if let Some(std_out) = &job.std_out {
        details.push(format!("Log File: {}", std_out));
    }

    if let Some(reason) = &job.reason {
        details.push(format!("Reason: {}", reason));
    }

    details.join("\n")
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
        .enumerate()
        .map(|(_, sort_field)| {
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
