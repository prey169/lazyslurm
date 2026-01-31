use crate::app::{App, AppState};
use crate::render_app;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{
    error::Error,
    io,
    time::{Duration, Instant},
};

pub async fn handle_key_event(app: &mut App, key: KeyEvent) -> Result<Option<()>, Box<dyn Error>> {
    match app.state {
        AppState::Normal => event_normal_state(app, key).await,
        AppState::Feedback => event_feedback_state(app, key).await,
        AppState::UserSearchPopup => event_user_search_popup(app, key).await,
        AppState::CancelJobPopup => event_cancel_popup(app, key).await,
        AppState::PartitionSearchPopup => event_partition_search_popup(app, key).await,
        AppState::NodelistSelectPopup { .. } => event_nodelist_select_popup(app, key).await,
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
        (KeyCode::Char('q'), _)
        | (KeyCode::Char('Q'), _)
        | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
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
        (KeyCode::Char('u'), _) => {
            app.state = AppState::UserSearchPopup;
        }
        (KeyCode::Char('p'), _) => {
            app.state = AppState::PartitionSearchPopup;
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

fn update_visible_nodes(nodelist: &[String], search: &str, visible_indices: &mut Vec<usize>) {
    let query = search.to_lowercase();

    let filtered: Vec<(String, usize)> = nodelist
        .iter()
        .enumerate()
        .filter_map(|(idx, node)| {
            if search.is_empty() {
                return Some((node.clone(), idx));
            }

            let lower_node = node.to_lowercase();
            if sublime_fuzzy::best_match(&query, &lower_node).is_some() {
                Some((node.clone(), idx))
            } else {
                None
            }
        })
        .collect();

    *visible_indices = filtered.into_iter().map(|(_, idx)| idx).collect();
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
                let visible_nodes: Vec<String> = visible_indices
                    .iter()
                    .map(|&idx| app.nodelist[idx].clone())
                    .collect();

                app.current_nodelist = visible_nodes;
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
                app.node_list_state.select_last();
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
                if let Some(index) = app.node_list_state.selected()
                    && let Some(node) = app.nodelist.get(index)
                {
                    let node = node.clone();

                    if app.current_nodelist.contains(&node) {
                        app.current_nodelist.retain(|n| n != &node);
                    } else {
                        app.current_nodelist.push(node);
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
