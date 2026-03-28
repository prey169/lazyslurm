use lazyslurm::models::{Job, JobList, JobState};
use lazyslurm::slurm::SlurmParser;
use lazyslurm::ui::events::fuzzy_filter;
use lazyslurm::utils::{Config, SortField};

#[test]
fn test_fuzzy_filter_empty_search() {
    let items = vec![
        "node01".to_string(),
        "node02".to_string(),
        "node03".to_string(),
    ];
    let result = fuzzy_filter(&items, "");
    assert_eq!(result, vec![0, 1, 2]);
}

#[test]
fn test_fuzzy_filter_exact_match() {
    let items = vec![
        "node01".to_string(),
        "node02".to_string(),
        "node03".to_string(),
    ];
    let result = fuzzy_filter(&items, "node02");
    assert_eq!(result, vec![1]);
}

#[test]
fn test_fuzzy_filter_partial_match() {
    let items = vec![
        "compute-node-01".to_string(),
        "compute-node-02".to_string(),
        "gpu-node-01".to_string(),
    ];
    let result = fuzzy_filter(&items, "compute");
    assert!(result.contains(&0));
    assert!(result.contains(&1));
    assert!(!result.contains(&2));
}

#[test]
fn test_fuzzy_filter_case_insensitive() {
    let items = vec!["NODE01".to_string(), "node02".to_string()];
    let result = fuzzy_filter(&items, "node");
    assert_eq!(result.len(), 2);
}

#[test]
fn test_fuzzy_filter_no_match() {
    let items = vec!["node01".to_string(), "node02".to_string()];
    let result = fuzzy_filter(&items, "xyz123");
    assert!(result.is_empty());
}

#[test]
fn test_sort_field_display_names() {
    let all_fields = SortField::all();
    for field in all_fields {
        let name = field.display_name();
        assert!(
            !name.is_empty(),
            "SortField {:?} has empty display_name",
            field
        );
    }
}

#[test]
fn test_sort_field_count() {
    let all_fields = SortField::all();
    assert_eq!(all_fields.len(), 12, "Should have 12 sort fields");
}

#[test]
fn test_job_display_id_simple() {
    let job = Job::new(
        "12345".to_string(),
        "test_job".to_string(),
        "user1".to_string(),
        JobState::Running,
    );
    assert_eq!(job.display_id(), "12345");
}

#[test]
fn test_job_display_id_array() {
    let mut job = Job::new(
        "12345_".to_string(),
        "test_job".to_string(),
        "user1".to_string(),
        JobState::Running,
    );
    job.array_job_id = Some("12345".to_string());
    job.array_task_id = Some(3);
    assert_eq!(job.display_id(), "12345_3");
}

#[test]
fn test_job_is_running() {
    let running_job = Job::new(
        "1".to_string(),
        "job".to_string(),
        "user".to_string(),
        JobState::Running,
    );
    let pending_job = Job::new(
        "2".to_string(),
        "job".to_string(),
        "user".to_string(),
        JobState::Pending,
    );
    assert!(running_job.is_running());
    assert!(!pending_job.is_running());
}

#[test]
fn test_job_is_completed() {
    let completed_job = Job::new(
        "1".to_string(),
        "job".to_string(),
        "user".to_string(),
        JobState::Completed,
    );
    let running_job = Job::new(
        "2".to_string(),
        "job".to_string(),
        "user".to_string(),
        JobState::Running,
    );
    assert!(completed_job.is_completed());
    assert!(!running_job.is_completed());
}

#[test]
fn test_job_list_running_jobs() {
    let mut job_list = JobList::new();
    job_list.jobs = vec![
        Job::new(
            "1".to_string(),
            "r1".to_string(),
            "u".to_string(),
            JobState::Running,
        ),
        Job::new(
            "2".to_string(),
            "r2".to_string(),
            "u".to_string(),
            JobState::Pending,
        ),
        Job::new(
            "3".to_string(),
            "r3".to_string(),
            "u".to_string(),
            JobState::Running,
        ),
        Job::new(
            "4".to_string(),
            "r4".to_string(),
            "u".to_string(),
            JobState::Completed,
        ),
    ];

    let running = job_list.running_jobs();
    assert_eq!(running.len(), 2);
    assert_eq!(running[0].name, "r1");
    assert_eq!(running[1].name, "r3");
}

#[test]
fn test_job_list_pending_jobs() {
    let mut job_list = JobList::new();
    job_list.jobs = vec![
        Job::new(
            "1".to_string(),
            "r1".to_string(),
            "u".to_string(),
            JobState::Running,
        ),
        Job::new(
            "2".to_string(),
            "r2".to_string(),
            "u".to_string(),
            JobState::Pending,
        ),
        Job::new(
            "3".to_string(),
            "r3".to_string(),
            "u".to_string(),
            JobState::Pending,
        ),
    ];

    let pending = job_list.pending_jobs();
    assert_eq!(pending.len(), 2);
}

#[test]
fn test_job_list_completed_jobs() {
    let mut job_list = JobList::new();
    job_list.jobs = vec![
        Job::new(
            "1".to_string(),
            "r1".to_string(),
            "u".to_string(),
            JobState::Completed,
        ),
        Job::new(
            "2".to_string(),
            "r2".to_string(),
            "u".to_string(),
            JobState::Running,
        ),
        Job::new(
            "3".to_string(),
            "r3".to_string(),
            "u".to_string(),
            JobState::Completed,
        ),
    ];

    let completed = job_list.completed_jobs();
    assert_eq!(completed.len(), 2);
}

#[test]
fn test_config_default() {
    let config = Config::default();
    assert_eq!(config.sort_field, SortField::StateCompletingFirst);
    assert_eq!(config.cache_duration_secs, 30);
}

#[test]
fn test_config_with_custom_values() {
    let config = Config {
        sort_field: SortField::NameAZ,
        cache_duration_secs: 60,
    };
    assert_eq!(config.sort_field, SortField::NameAZ);
    assert_eq!(config.cache_duration_secs, 60);
}

#[test]
fn test_parse_squeue_output_valid() {
    let output = r#"12345,test_job,user1,R,00:30:00,node01,gpu
67890,another_job,user2,PD,00:00:00,node02,cpu"#;

    let result = SlurmParser::parse_squeue_output(output);
    assert!(result.is_ok());

    let jobs = result.unwrap();
    assert_eq!(jobs.len(), 2);
    assert_eq!(jobs[0].job_id, "12345");
    assert_eq!(jobs[0].name, "test_job");
    assert_eq!(jobs[0].user, "user1");
}

#[test]
fn test_parse_squeue_output_header() {
    let output = r#"JOBID,NAME,USER,ST,TIME,NODES,PARTITION
12345,test_job,user1,R,00:30:00,node01,gpu"#;

    let result = SlurmParser::parse_squeue_output(output);
    assert!(result.is_ok());
}

#[test]
fn test_parse_squeue_output_empty() {
    let output = "";
    let result = SlurmParser::parse_squeue_output(output);
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}
