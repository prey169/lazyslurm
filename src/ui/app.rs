use anyhow::Result;
use ratatui::widgets::ListState;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use crate::models::{Job, JobList};
use crate::slurm::{SlurmCommands, SlurmParser};

#[derive(Debug, Clone)]
pub enum AppEvent {
    Refresh,
    JobSelected(String),
    Quit,
}

#[derive(Clone, Eq, Debug, PartialEq)]
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
            last_refresh: Instant::now(),
            refresh_interval: Duration::from_secs(2), // Refresh every 2 seconds
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
        app
    }

    pub async fn refresh_nodelist(&mut self) -> Result<()> {
        match SlurmCommands::sinfo_show_nodelist().await {
            Ok(nodes) => self.nodelist = nodes,
            Err(e) => self.error_message = Some(format!("Failed to fetch nodes: {}", e)),
        }
        Ok(())
    }

    pub async fn refresh_jobs(&mut self) -> Result<()> {
        self.is_loading = true;
        self.error_message = None;

        match self.fetch_jobs().await {
            Ok(jobs) => {
                self.job_list.update(jobs);
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
