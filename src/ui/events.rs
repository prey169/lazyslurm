use crate::app::{App, AppState, LogType};
use crate::render_app;
use crate::utils::SortField;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{
    error::Error,
    io,
    time::{Duration, Instant},
};

pub async fn handle_key_event(app: &mut App, key: KeyEvent) -> Result<Option<()>, Box<dyn Error>> {
    if let KeyCode::Char('c') = key.code {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return Ok(Some(()));
        }
    }

    match app.state {
        AppState::Normal => event_normal_state(app, key).await,
        AppState::Feedback => event_feedback_state(app, key).await,
        AppState::UserSearchPopup => event_user_search_popup(app, key).await,
        AppState::CancelJobPopup => event_cancel_popup(app, key).await,
        AppState::PartitionSearchPopup => event_partition_search_popup(app, key).await,
        AppState::NodelistSelectPopup { .. } => event_nodelist_select_popup(app, key).await,
        AppState::PartitionSelectPopup { .. } => event_partition_select_popup(app, key).await,
        AppState::UserSelectPopup { .. } => event_user_select_popup(app, key).await,
        AppState::JobLogPopup(_) => event_job_log_popup(app, key).await,
        AppState::SortSelectPopup { .. } => event_sort_select_popup(app, key).await,
    }
}

pub async fn handle_text_event(app: &mut App, key: KeyEvent) -> Option<Option<String>> {
    match key.code {
        KeyCode::Enter => {
            if app.input.is_empty() {
                return Some(None);
            } else {
                return Some(Some(app.input.clone()));
            }
        }
        KeyCode::Esc => {
            app.input.clear();
            app.state = AppState::Normal;
        }
        KeyCode::Char(c) => {
            app.input.push(c);
        }
        KeyCode::Backspace => {
            app.input.pop();
        }
        _ => {}
    }
    None
}

pub async fn reset_popup_state_to_normal(app: &mut App) -> Result<(), Box<dyn Error>> {
    app.input.clear();
    app.state = AppState::Normal;
    app.refresh_jobs().await?;
    Ok(())
}

async fn event_normal_state(app: &mut App, key: KeyEvent) -> Result<Option<()>, Box<dyn Error>> {
    match (key.code, key.modifiers) {
        (KeyCode::Char('q'), _) | (KeyCode::Char('Q'), _) => {
            return Ok(Some(()));
        }
        (KeyCode::Char('r'), _) => {
            app.refresh_jobs().await?;
        }
        (KeyCode::Up, _) | (KeyCode::Char('k'), _) => {
            app.select_previous_job();
        }
        (KeyCode::Down, _) | (KeyCode::Char('j'), _) => {
            app.select_next_job();
        }
        (KeyCode::PageUp, _) => {
            app.select_page_up();
        }
        (KeyCode::PageDown, _) => {
            app.select_page_down();
        }
        (KeyCode::Home, _) => {
            app.select_first();
        }
        (KeyCode::End, _) => {
            app.select_last();
        }
        (KeyCode::Char('u'), _) => {
            app.user_list_state.select_first();
            let search = String::new();
            let visible_indices = fuzzy_filter(&app.user_list, &search);
            app.state = AppState::UserSelectPopup {
                search,
                visible_indices,
            };
        }
        (KeyCode::Char('p'), _) => {
            app.partition_list_state.select_first();
            let search = String::new();
            let visible_indices = fuzzy_filter(&app.partition_list, &search);
            app.state = AppState::PartitionSelectPopup {
                search,
                visible_indices,
            };
        }
        (KeyCode::Char('n'), _) => {
            app.current_nodelist
                .retain(|node| app.nodelist.contains(node));
            app.refresh_nodelist().await?;

            let mut visible_indices: Vec<usize> = Vec::new();
            let search = String::new();
            update_visible_nodes(&app.nodelist, &search, &mut visible_indices);

            app.state = AppState::NodelistSelectPopup {
                original_nodelist: app.current_nodelist.clone(),
                search,
                visible_indices,
            };
        }
        (KeyCode::Char('c'), _) if app.selected_job.is_some() => {
            app.confirm_action = false;
            app.state = AppState::CancelJobPopup;
        }
        (KeyCode::Char('o'), _) if app.selected_job.is_some() => {
            let log_state = app.load_job_log(app.selected_job.as_ref().unwrap(), LogType::Output);
            app.state = AppState::JobLogPopup(log_state);
        }
        (KeyCode::Char('e'), _) if app.selected_job.is_some() => {
            let log_state = app.load_job_log(app.selected_job.as_ref().unwrap(), LogType::Error);
            app.state = AppState::JobLogPopup(log_state);
        }
        (KeyCode::Char('s'), _) => {
            app.sort_list_state.select_first();
            app.state = AppState::SortSelectPopup {
                visible_indices: (0..SortField::all().len()).collect(),
            };
        }
        _ => {}
    }
    Ok(None)
}

async fn event_user_search_popup(
    app: &mut App,
    key: KeyEvent,
) -> Result<Option<()>, Box<dyn Error>> {
    let user_search = handle_text_event(app, key).await;
    if let Some(user) = user_search {
        app.current_user = user;
        reset_popup_state_to_normal(app).await?;
    }
    Ok(None)
}

fn fuzzy_filter(items: &[String], search: &str) -> Vec<usize> {
    let query = search.to_lowercase();

    if search.is_empty() {
        return (0..items.len()).collect();
    }

    items
        .iter()
        .enumerate()
        .filter_map(|(idx, item)| {
            let lower_item = item.to_lowercase();
            if sublime_fuzzy::best_match(&query, &lower_item).is_some() {
                Some(idx)
            } else {
                None
            }
        })
        .collect()
}

fn update_visible_nodes(nodelist: &[String], search: &str, visible_indices: &mut Vec<usize>) {
    *visible_indices = fuzzy_filter(nodelist, search);
}

async fn event_partition_search_popup(
    app: &mut App,
    key: KeyEvent,
) -> Result<Option<()>, Box<dyn Error>> {
    let partition_search = handle_text_event(app, key).await;
    if let Some(partition) = partition_search {
        app.current_partition = partition;
        reset_popup_state_to_normal(app).await?;
    }
    Ok(None)
}
async fn event_nodelist_select_popup(
    app: &mut App,
    key: KeyEvent,
) -> Result<Option<()>, Box<dyn Error>> {
    if let AppState::NodelistSelectPopup {
        original_nodelist,
        search,
        visible_indices,
        ..
    } = &mut app.state
    {
        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Ok(Some(()));
            }

            KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if search.is_empty() {
                    app.current_nodelist.clear();
                } else {
                    let visible_nodes: Vec<String> = visible_indices
                        .iter()
                        .map(|&idx| app.nodelist[idx].clone())
                        .collect();
                    app.current_nodelist = visible_nodes;
                }
            }

            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let visible_set: Vec<String> = visible_indices
                    .iter()
                    .map(|&idx| app.nodelist[idx].clone())
                    .collect();

                app.current_nodelist
                    .retain(|node| !visible_set.contains(node));
            }

            KeyCode::Char('i') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let visible_set: Vec<String> = visible_indices
                    .iter()
                    .map(|&idx| app.nodelist[idx].clone())
                    .collect();

                for node in visible_set {
                    if app.current_nodelist.contains(&node) {
                        app.current_nodelist.retain(|n| n != &node);
                    } else {
                        app.current_nodelist.push(node);
                    }
                }
            }

            KeyCode::Down => {
                app.node_list_state.select_next();
            }

            KeyCode::Up => {
                app.node_list_state.select_previous();
            }

            KeyCode::Home => {
                app.node_list_state.select_first();
            }

            KeyCode::End => {
                if search.is_empty() {
                    app.node_list_state.select(Some(visible_indices.len()));
                } else {
                    app.node_list_state.select_last();
                }
            }

            KeyCode::Char(c) if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' => {
                search.push(c);
                update_visible_nodes(&app.nodelist, search, visible_indices);
                app.node_list_state.select_first();
            }

            KeyCode::Backspace => {
                search.pop();
                update_visible_nodes(&app.nodelist, search, visible_indices);
                app.node_list_state.select_first();
            }

            KeyCode::Char(' ') => {
                if let Some(selected_idx) = app.node_list_state.selected() {
                    let show_none = search.is_empty();
                    if show_none && selected_idx == 0 {
                        app.current_nodelist.clear();
                    } else {
                        let item_idx = if show_none { selected_idx - 1 } else { selected_idx };
                        if let Some(&actual_idx) = visible_indices.get(item_idx)
                            && let Some(node) = app.nodelist.get(actual_idx)
                        {
                            let node = node.clone();
                            if app.current_nodelist.contains(&node) {
                                app.current_nodelist.retain(|n| n != &node);
                            } else {
                                app.current_nodelist.push(node);
                            }
                        }
                    }
                }
            }
            KeyCode::Enter => {
                app.state = AppState::Normal;
            }

            KeyCode::Esc => {
                app.current_nodelist = original_nodelist.clone();
                app.state = AppState::Normal;
            }

            _ => {}
        }
    }
    Ok(None)
}

async fn event_feedback_state(
    _app: &mut App,
    _key: KeyEvent,
) -> Result<Option<()>, Box<dyn Error>> {
    Ok(None)
}

async fn event_cancel_popup(app: &mut App, key: KeyEvent) -> Result<Option<()>, Box<dyn Error>> {
    match key.code {
        KeyCode::Char('y') => {
            app.confirm_action = true;
            app.state = AppState::Normal;
            app.refresh_jobs().await?;
        }
        KeyCode::Char('n') | KeyCode::Esc => {
            app.confirm_action = false;
            app.state = AppState::Normal;
            app.refresh_jobs().await?;
        }
        _ => {}
    }
    app.handle_cancel_popup().await?;
    Ok(None)
}

async fn event_partition_select_popup(
    app: &mut App,
    key: KeyEvent,
) -> Result<Option<()>, Box<dyn Error>> {
    if let AppState::PartitionSelectPopup {
        search,
        visible_indices,
        ..
    } = &mut app.state
    {
        match key.code {
            KeyCode::Down => {
                app.partition_list_state.select_next();
            }
            KeyCode::Up => {
                app.partition_list_state.select_previous();
            }
            KeyCode::Home => {
                app.partition_list_state.select_first();
            }
            KeyCode::End => {
                app.partition_list_state.select_last();
            }
            KeyCode::Char(c) if c.is_alphanumeric() || c == '_' || c == '-' => {
                search.push(c);
                *visible_indices = fuzzy_filter(&app.partition_list, search);
                app.partition_list_state.select_first();
            }
            KeyCode::Backspace => {
                search.pop();
                *visible_indices = fuzzy_filter(&app.partition_list, search);
                app.partition_list_state.select_first();
            }
            KeyCode::Enter => {
                if let Some(selected_idx) = app.partition_list_state.selected() {
                    if search.is_empty() && selected_idx == 0 {
                        app.current_partition = None;
                    } else {
                        let item_idx = if search.is_empty() {
                            selected_idx - 1
                        } else {
                            selected_idx
                        };
                        if let Some(&actual_idx) = visible_indices.get(item_idx)
                            && let Some(partition) = app.partition_list.get(actual_idx)
                        {
                            app.current_partition = Some(partition.clone());
                        }
                    }
                }
                app.state = AppState::Normal;
                app.refresh_jobs().await?;
            }
            KeyCode::Esc => {
                app.state = AppState::Normal;
            }
            _ => {}
        }
    }
    Ok(None)
}

async fn event_user_select_popup(
    app: &mut App,
    key: KeyEvent,
) -> Result<Option<()>, Box<dyn Error>> {
    if let AppState::UserSelectPopup {
        search,
        visible_indices,
        ..
    } = &mut app.state
    {
        match key.code {
            KeyCode::Down => {
                app.user_list_state.select_next();
            }
            KeyCode::Up => {
                app.user_list_state.select_previous();
            }
            KeyCode::Home => {
                app.user_list_state.select_first();
            }
            KeyCode::End => {
                app.user_list_state.select_last();
            }
            KeyCode::Char(c) if c.is_alphanumeric() || c == '_' || c == '-' => {
                search.push(c);
                *visible_indices = fuzzy_filter(&app.user_list, search);
                app.user_list_state.select_first();
            }
            KeyCode::Backspace => {
                search.pop();
                *visible_indices = fuzzy_filter(&app.user_list, search);
                app.user_list_state.select_first();
            }
            KeyCode::Enter => {
                if let Some(selected_idx) = app.user_list_state.selected() {
                    if search.is_empty() && selected_idx == 0 {
                        app.current_user = None;
                    } else {
                        let item_idx = if search.is_empty() {
                            selected_idx - 1
                        } else {
                            selected_idx
                        };
                        if let Some(&actual_idx) = visible_indices.get(item_idx)
                            && let Some(user) = app.user_list.get(actual_idx)
                        {
                            app.current_user = Some(user.clone());
                        }
                    }
                }
                app.state = AppState::Normal;
                app.refresh_jobs().await?;
            }
            KeyCode::Esc => {
                app.state = AppState::Normal;
            }
            _ => {}
        }
    }
    Ok(None)
}

async fn event_job_log_popup(
    app: &mut App,
    key: KeyEvent,
) -> Result<Option<()>, Box<dyn Error>> {
    if let AppState::JobLogPopup(ref mut log_state) = app.state {
        if log_state.is_searching {
            match key.code {
                KeyCode::Enter => {
                    let query = log_state.search_query.to_lowercase();
                    
                    if query.is_empty() {
                        log_state.is_searching = false;
                    } else {
                        let all_matches: Vec<usize> = log_state
                            .lines
                            .iter()
                            .enumerate()
                            .filter(|(_, line)| line.to_lowercase().contains(&query))
                            .map(|(idx, _)| idx)
                            .collect();
                        
                        let is_repeat_search = log_state.previous_query == log_state.search_query 
                            && !log_state.previous_query.is_empty();
                        
                        if is_repeat_search {
                            log_state.current_match_index = (log_state.current_match_index + 1)
                                % all_matches.len();
                        } else {
                            log_state.previous_query = log_state.search_query.clone();
                            log_state.current_match_index = 0;
                        }
                        
                        log_state.search_matches = all_matches;
                        log_state.is_searching = false;
                        
                        if let Some(&match_pos) = log_state.search_matches.get(log_state.current_match_index) {
                            log_state.scroll_offset = match_pos;
                        }
                    }
                }
                KeyCode::Char(c) => {
                    log_state.search_query.push(c);
                }
                KeyCode::Backspace => {
                    log_state.search_query.pop();
                }
                KeyCode::Esc => {
                    log_state.is_searching = false;
                    log_state.search_query.clear();
                    log_state.search_matches.clear();
                }
                _ => {}
            }
        } else {
            match key.code {
                KeyCode::Up => {
                    if log_state.scroll_offset > 0 {
                        log_state.scroll_offset -= 1;
                    }
                }
                KeyCode::Down => {
                    if log_state.scroll_offset < log_state.lines.len().saturating_sub(1) {
                        log_state.scroll_offset += 1;
                    }
                }
                KeyCode::PageUp => {
                    log_state.scroll_offset = log_state.scroll_offset.saturating_sub(20);
                }
                KeyCode::PageDown => {
                    log_state.scroll_offset = (log_state.scroll_offset + 20)
                        .min(log_state.lines.len().saturating_sub(1));
                }
                KeyCode::Home => {
                    log_state.scroll_offset = 0;
                }
                KeyCode::End => {
                    log_state.scroll_offset = log_state.lines.len().saturating_sub(1);
                }
                KeyCode::Char('g') => {
                    log_state.scroll_offset = 0;
                }
                KeyCode::Char('G') => {
                    log_state.scroll_offset = log_state.lines.len().saturating_sub(1);
                }
                KeyCode::Char(' ') => {
                    log_state.scroll_offset = (log_state.scroll_offset + 20)
                        .min(log_state.lines.len().saturating_sub(1));
                }
                KeyCode::Char('/') => {
                    log_state.is_searching = true;
                    log_state.search_query = log_state.previous_query.clone();
                    log_state.search_matches.clear();
                }
                KeyCode::Char('n') => {
                    if !log_state.search_matches.is_empty() {
                        log_state.current_match_index = (log_state.current_match_index + 1)
                            % log_state.search_matches.len();
                        log_state.scroll_offset = log_state.search_matches[log_state.current_match_index];
                    }
                }
                KeyCode::Char('N') => {
                    if !log_state.search_matches.is_empty() {
                        log_state.current_match_index = if log_state.current_match_index == 0 {
                            log_state.search_matches.len() - 1
                        } else {
                            log_state.current_match_index - 1
                        };
                        log_state.scroll_offset = log_state.search_matches[log_state.current_match_index];
                    }
                }
                KeyCode::Esc | KeyCode::Char('q') => {
                    app.state = AppState::Normal;
                }
                _ => {}
            }
        }
    }
    Ok(None)
}

pub async fn run_event_loop(
    app: &mut App,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<(), Box<dyn Error>> {
    let tick_rate = Duration::from_millis(100);
    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|frame| render_app(frame, app))?;
        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or(Duration::from_secs(0));

        if event::poll(timeout)?
            && let Event::Key(key) = event::read()?
            && let Ok(Some(())) = handle_key_event(app, key).await
        {
            return Ok(());
        }

        if app.should_refresh() {
            app.refresh_jobs().await?;
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
        if app.state == AppState::Feedback
            && let Some(shown_at) = app.feedback_shown_at
            && shown_at.elapsed() >= app.feedback_duration
        {
            app.feedback_message = None;
            app.feedback_shown_at = None;
            app.state = AppState::Normal;
        }
    }
}

async fn event_sort_select_popup(
    app: &mut App,
    key: KeyEvent,
) -> Result<Option<()>, Box<dyn Error>> {
    if let AppState::SortSelectPopup { visible_indices } = &mut app.state {
        let sort_options = SortField::all();
        let _max_index = sort_options.len().saturating_sub(1);

        match key.code {
            KeyCode::Down => {
                app.sort_list_state.select_next();
            }
            KeyCode::Up => {
                app.sort_list_state.select_previous();
            }
            KeyCode::Home => {
                app.sort_list_state.select_first();
            }
            KeyCode::End => {
                app.sort_list_state.select_last();
            }
            KeyCode::Enter => {
                if let Some(selected_idx) = app.sort_list_state.selected() {
                    if let Some(&actual_idx) = visible_indices.get(selected_idx) {
                        if let Some(sort_field) = sort_options.get(actual_idx) {
                            app.sort_jobs(*sort_field);
                            if let Some(config) = app.config.as_mut() {
                                config.sort_field = *sort_field;
                                let _ = config.save();
                            }
                        }
                    }
                }
                app.state = AppState::Normal;
            }
            KeyCode::Esc => {
                app.state = AppState::Normal;
            }
            _ => {}
        }
    }
    Ok(None)
}
