use anyhow::Result;
use ratatui::widgets::ListState;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use crate::models::{Job, JobList};
use crate::slurm::{SlurmCommands, SlurmParser};
use crate::utils::SortField;

#[derive(Debug, Clone)]
pub enum AppEvent {
    Refresh,
    JobSelected(String),
    Quit,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LogType {
    Output,
    Error,
}

#[derive(Clone, Debug, PartialEq)]
pub struct JobLogState {
    pub log_type: LogType,
    pub lines: Vec<String>,
    pub scroll_offset: usize,
    pub search_query: String,
    pub search_matches: Vec<usize>,
    pub current_match_index: usize,
    pub is_searching: bool,
    pub file_path: String,
    pub previous_query: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AppState {
    Normal,
    Feedback,
    PartitionSearchPopup,
    UserSearchPopup,
    CancelJobPopup,
    NodelistSelectPopup {
        original_nodelist: Vec<String>,
        search: String,
        visible_indices: Vec<usize>,
    },
    PartitionSelectPopup {
        search: String,
        visible_indices: Vec<usize>,
    },
    UserSelectPopup {
        search: String,
        visible_indices: Vec<usize>,
    },
    SortSelectPopup {
        visible_indices: Vec<usize>,
    },
    JobLogPopup(JobLogState),
}

#[derive(Debug)]
pub struct FeedbackMessage {
    pub message: String,
    pub title: String,
}

impl FeedbackMessage {
    pub fn as_parts(&self) -> (&str, &str) {
        (&self.message, &self.title)
    }

    pub fn message(&self) -> &str {
        &self.message
    }
    pub fn title(&self) -> &str {
        &self.title
    }
}

#[derive(Debug)]
pub struct App {
    pub job_list: JobList,
    pub state: AppState,
    pub job_list_state: ListState,
    pub selected_job: Option<Job>,
    pub current_user: Option<String>,
    pub current_partition: Option<String>,
    pub nodelist: Vec<String>,
    pub current_nodelist: Vec<String>,
    pub node_list_state: ListState,
    pub partition_list: Vec<String>,
    pub user_list: Vec<String>,
    pub partition_list_state: ListState,
    pub user_list_state: ListState,
    pub sort_field: SortField,
    pub sort_list_state: ListState,
    pub config: Option<crate::utils::Config>,
    pub last_refresh: Instant,
    pub refresh_interval: Duration,
    pub is_loading: bool,
    pub error_message: Option<String>,
    pub event_sender: mpsc::UnboundedSender<AppEvent>,
    pub event_receiver: mpsc::UnboundedReceiver<AppEvent>,
    pub confirm_action: bool,
    pub input: String,
    pub feedback_message: Option<FeedbackMessage>,
    pub feedback_duration: Duration,
    pub feedback_shown_at: Option<Instant>,
}

impl App {
    pub fn new() -> Self {
        let (event_sender, event_receiver) = mpsc::unbounded_channel();
        let mut node_list_state = ListState::default();
        node_list_state.select_first();
        let mut partition_list_state = ListState::default();
        partition_list_state.select_first();
        let mut user_list_state = ListState::default();
        user_list_state.select_first();
        let mut sort_list_state = ListState::default();
        sort_list_state.select_first();

        Self {
            job_list: JobList::new(),
            state: AppState::Normal,
            job_list_state: ListState::default(),
            selected_job: None,
            current_user: std::env::var("USER").ok(),
            current_partition: None,
            nodelist: Vec::new(),
            current_nodelist: Vec::new(),
            node_list_state,
            partition_list: Vec::new(),
            user_list: Vec::new(),
            partition_list_state,
            user_list_state,
            sort_field: SortField::default(),
            sort_list_state,
            config: None,
            last_refresh: Instant::now(),
            refresh_interval: Duration::from_secs(2),
            is_loading: false,
            error_message: None,
            event_sender,
            event_receiver,
            confirm_action: false,
            input: "".to_string(),
            feedback_message: None,
            feedback_duration: Duration::from_secs(2),
            feedback_shown_at: None,
        }
    }

    pub fn with_cli(
        user: Option<String>,
        partition: Option<String>,
        nodelist: Vec<String>,
        all_users: bool,
        config: crate::utils::Config,
    ) -> Self {
        let mut app = Self::new();
        if user.is_some() {
            app.current_user = user;
        } else if user.is_none() && all_users {
            app.current_user = None;
        } else {
            app.current_user = std::env::var("USER").ok();
        }
        app.current_partition = partition;
        app.current_nodelist = nodelist;
        app.sort_field = config.sort_field;
        app.config = Some(config);
        app
    }

    pub async fn refresh_nodelist(&mut self) -> Result<()> {
        match SlurmCommands::sinfo_show_nodelist().await {
            Ok(nodes) => self.nodelist = nodes,
            Err(e) => self.error_message = Some(format!("Failed to fetch nodes: {}", e)),
        }
        Ok(())
    }

    pub async fn refresh_partitions(&mut self) -> Result<()> {
        match SlurmCommands::sinfo_show_partitions().await {
            Ok(partitions) => self.partition_list = partitions,
            Err(e) => self.error_message = Some(format!("Failed to fetch partitions: {}", e)),
        }
        Ok(())
    }

    pub async fn refresh_users(&mut self) -> Result<()> {
        match SlurmCommands::squeue_show_users().await {
            Ok(users) => self.user_list = users,
            Err(e) => self.error_message = Some(format!("Failed to fetch users: {}", e)),
        }
        Ok(())
    }

    pub fn load_job_log(&self, job: &Job, log_type: LogType) -> JobLogState {
        let log_paths = SlurmParser::get_job_log_paths(job);

        let target_suffix = match log_type {
            LogType::Output => "out",
            LogType::Error => "err",
        };

        let mut found_path: Option<String> = None;
        let mut content = String::new();

        for path in &log_paths {
            if path.ends_with(&format!(".{target_suffix}"))
                || (target_suffix == "out" && !path.ends_with(".err"))
            {
                if let Ok(read_content) = std::fs::read_to_string(path) {
                    found_path = Some(path.clone());
                    content = read_content;
                    break;
                }
            }
        }

        if found_path.is_none() {
            for path in &log_paths {
                if std::path::Path::new(path).exists() {
                    if let Ok(read_content) = std::fs::read_to_string(path) {
                        found_path = Some(path.clone());
                        content = read_content;
                        break;
                    }
                }
            }
        }

        let lines: Vec<String> = if content.is_empty() {
            vec!["No log file found.".to_string()]
        } else {
            content.lines().map(|s| s.to_string()).collect()
        };

        let file_path = found_path.unwrap_or_else(|| {
            if log_type == LogType::Output {
                "output log".to_string()
            } else {
                "error log".to_string()
            }
        });

        JobLogState {
            log_type,
            lines,
            scroll_offset: 0,
            search_query: String::new(),
            search_matches: Vec::new(),
            current_match_index: 0,
            is_searching: false,
            file_path,
            previous_query: String::new(),
        }
    }

    pub fn sort_jobs(&mut self, field: SortField) {
        self.sort_field = field;
        
        match field {
            SortField::StateCompletingFirst => {
                self.job_list.jobs.sort_by(|a, b| {
                    let order = |job: &Job| match job.state {
                        crate::models::JobState::Completed => 0,
                        crate::models::JobState::Failed => 0,
                        crate::models::JobState::Cancelled => 0,
                        crate::models::JobState::Timeout => 0,
                        crate::models::JobState::NodeFail => 0,
                        crate::models::JobState::Preempted => 0,
                        crate::models::JobState::Running => 1,
                        _ => 2,
                    };
                    order(a).cmp(&order(b))
                });
            }
            SortField::StatePendingFirst => {
                self.job_list.jobs.sort_by(|a, b| {
                    let order = |job: &Job| match job.state {
                        crate::models::JobState::Pending => 0,
                        _ => 1,
                    };
                    order(a).cmp(&order(b))
                });
            }
            SortField::QueueTimeShortest => {
                self.job_list.jobs.sort_by(|a, b| {
                    let a_time = a.submit_time;
                    let b_time = b.submit_time;
                    match (a_time, b_time) {
                        (Some(a), Some(b)) => a.cmp(&b),
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        (None, None) => std::cmp::Ordering::Equal,
                    }
                });
            }
            SortField::QueueTimeLongest => {
                self.job_list.jobs.sort_by(|a, b| {
                    let a_time = a.submit_time;
                    let b_time = b.submit_time;
                    match (a_time, b_time) {
                        (Some(a), Some(b)) => b.cmp(&a),
                        (Some(_), None) => std::cmp::Ordering::Greater,
                        (None, Some(_)) => std::cmp::Ordering::Less,
                        (None, None) => std::cmp::Ordering::Equal,
                    }
                });
            }
            SortField::RuntimeShortest => {
                self.job_list.jobs.sort_by(|a, b| {
                    let a_dur = a.duration();
                    let b_dur = b.duration();
                    match (a_dur, b_dur) {
                        (Some(a), Some(b)) => a.cmp(&b),
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        (None, None) => std::cmp::Ordering::Equal,
                    }
                });
            }
            SortField::RuntimeLongest => {
                self.job_list.jobs.sort_by(|a, b| {
                    let a_dur = a.duration();
                    let b_dur = b.duration();
                    match (a_dur, b_dur) {
                        (Some(a), Some(b)) => b.cmp(&a),
                        (Some(_), None) => std::cmp::Ordering::Greater,
                        (None, Some(_)) => std::cmp::Ordering::Less,
                        (None, None) => std::cmp::Ordering::Equal,
                    }
                });
            }
            SortField::JobIdOldest => {
                self.job_list.jobs.sort_by(|a, b| a.job_id.cmp(&b.job_id));
            }
            SortField::JobIdNewest => {
                self.job_list.jobs.sort_by(|a, b| b.job_id.cmp(&a.job_id));
            }
            SortField::NameAZ => {
                self.job_list.jobs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
            }
            SortField::NameZA => {
                self.job_list.jobs.sort_by(|a, b| b.name.to_lowercase().cmp(&a.name.to_lowercase()));
            }
            SortField::SubmittedOldest => {
                self.job_list.jobs.sort_by(|a, b| {
                    let a_time = a.submit_time;
                    let b_time = b.submit_time;
                    match (a_time, b_time) {
                        (Some(a), Some(b)) => a.cmp(&b),
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        (None, None) => std::cmp::Ordering::Equal,
                    }
                });
            }
            SortField::SubmittedNewest => {
                self.job_list.jobs.sort_by(|a, b| {
                    let a_time = a.submit_time;
                    let b_time = b.submit_time;
                    match (a_time, b_time) {
                        (Some(a), Some(b)) => b.cmp(&a),
                        (Some(_), None) => std::cmp::Ordering::Greater,
                        (None, Some(_)) => std::cmp::Ordering::Less,
                        (None, None) => std::cmp::Ordering::Equal,
                    }
                });
            }
        }
    }

    pub async fn refresh_jobs(&mut self) -> Result<()> {
        self.is_loading = true;
        self.error_message = None;

        match self.fetch_jobs().await {
            Ok(jobs) => {
                self.job_list.update(jobs);
                self.sort_jobs(self.sort_field);
                self.update_selected_job_from_state();
                self.last_refresh = Instant::now();
            }
            Err(e) => {
                self.error_message = Some(format!("Failed to fetch jobs: {}", e));
            }
        }

        self.is_loading = false;
        Ok(())
    }

    async fn fetch_jobs(&self) -> Result<Vec<Job>> {
        // Get basic job list from squeue
        let squeue_output = SlurmCommands::squeue(
            self.current_user.as_deref(),
            self.current_partition.as_deref(),
            &self.current_nodelist,
        )
        .await?;
        let mut jobs = SlurmParser::parse_squeue_output(&squeue_output)?;

        // For each job, get detailed info from scontrol (but only for first few to avoid overwhelming)
        for job in jobs.iter_mut().take(10) {
            if let Ok(scontrol_output) = SlurmCommands::scontrol_show_job(&job.job_id).await
                && let Ok(fields) = SlurmParser::parse_scontrol_output(&scontrol_output)
            {
                SlurmParser::enhance_job_with_scontrol_data(job, fields);
            }
        }

        Ok(jobs)
    }

    pub fn should_refresh(&self) -> bool {
        self.last_refresh.elapsed() >= self.refresh_interval
    }

    pub fn select_next_job(&mut self) {
        self.job_list_state.select_next();
        self.update_selected_job_from_state();
    }

    pub fn select_previous_job(&mut self) {
        self.job_list_state.select_previous();
        self.update_selected_job_from_state();
    }

    pub fn select_first(&mut self) {
        self.job_list_state.select_first();
        self.update_selected_job_from_state();
    }

    pub fn select_last(&mut self) {
        self.job_list_state.select_last();
        self.update_selected_job_from_state();
    }

    pub fn get_selected_job(&self) -> Option<&Job> {
        self.selected_job.as_ref()
    }

    pub fn running_jobs(&self) -> Vec<&Job> {
        self.job_list.running_jobs()
    }

    pub fn pending_jobs(&self) -> Vec<&Job> {
        self.job_list.pending_jobs()
    }

    pub fn completed_jobs(&self) -> Vec<&Job> {
        self.job_list.completed_jobs()
    }

    fn update_selected_job_from_state(&mut self) {
        if self.job_list.jobs.is_empty() {
            self.selected_job = None;
            return;
        }

        match self.job_list_state.selected() {
            Some(idx) => {
                self.selected_job = self.job_list.jobs.get(idx).cloned();
            }
            None => {
                self.job_list_state.select_first();
            }
        }
    }

    pub async fn handle_cancel_popup(&mut self) -> Result<()> {
        if self.confirm_action && self.selected_job.is_some() {
            if let Err(e) = self.cancel_selected_job().await {
                self.error_message = Some(format!("Failed to cancel job: {}", e));
            }
            self.confirm_action = false;
        }
        Ok(())
    }

    pub async fn cancel_selected_job(&mut self) -> Result<()> {
        if let Some(job) = &self.selected_job {
            SlurmCommands::scancel(&job.job_id).await?;
            // Refresh immediately to show the change
            self.refresh_jobs().await?;
        }
        Ok(())
    }

    pub fn send_event(&self, event: AppEvent) -> Result<()> {
        self.event_sender.send(event)?;
        Ok(())
    }

    pub async fn receive_event(&mut self) -> Option<AppEvent> {
        self.event_receiver.recv().await
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
