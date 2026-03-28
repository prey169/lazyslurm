use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SortField {
    #[default]
    StateCompletingFirst,
    StatePendingFirst,
    QueueTimeShortest,
    QueueTimeLongest,
    RuntimeShortest,
    RuntimeLongest,
    JobIdOldest,
    JobIdNewest,
    NameAZ,
    NameZA,
    SubmittedOldest,
    SubmittedNewest,
}

impl SortField {
    pub fn display_name(&self) -> &'static str {
        match self {
            SortField::StateCompletingFirst => "State (Completing first)",
            SortField::StatePendingFirst => "State (Pending first)",
            SortField::QueueTimeShortest => "Queue Time (Shortest first)",
            SortField::QueueTimeLongest => "Queue Time (Longest first)",
            SortField::RuntimeShortest => "Runtime (Shortest first)",
            SortField::RuntimeLongest => "Runtime (Longest first)",
            SortField::JobIdOldest => "Job ID (Oldest first)",
            SortField::JobIdNewest => "Job ID (Newest first)",
            SortField::NameAZ => "Name (A → Z)",
            SortField::NameZA => "Name (Z → A)",
            SortField::SubmittedOldest => "Submitted (Oldest first)",
            SortField::SubmittedNewest => "Submitted (Newest first)",
        }
    }

    pub fn all() -> Vec<SortField> {
        vec![
            SortField::StateCompletingFirst,
            SortField::StatePendingFirst,
            SortField::QueueTimeShortest,
            SortField::QueueTimeLongest,
            SortField::RuntimeShortest,
            SortField::RuntimeLongest,
            SortField::JobIdOldest,
            SortField::JobIdNewest,
            SortField::NameAZ,
            SortField::NameZA,
            SortField::SubmittedOldest,
            SortField::SubmittedNewest,
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub sort_field: SortField,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            sort_field: SortField::default(),
        }
    }
}

impl Config {
    pub fn load() -> Config {
        if let Some(path) = get_config_path() {
            if path.exists() {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(config) = toml::from_str(&content) {
                        return config;
                    }
                }
            }
        }
        Config::default()
    }

    pub fn save(&self) -> io::Result<()> {
        if let Some(path) = get_config_path() {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let content = toml::to_string_pretty(self)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            let mut file = fs::File::create(&path)?;
            file.write_all(content.as_bytes())?;
        }
        Ok(())
    }
}

fn get_config_path() -> Option<PathBuf> {
    let config_home = std::env::var("XDG_CONFIG_HOME")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let mut path = dirs::home_dir().unwrap_or_default();
            path.push(".config");
            path
        });

    Some(config_home.join("lazyslurm.toml"))
}
