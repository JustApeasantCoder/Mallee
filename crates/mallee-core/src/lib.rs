mod artifacts;
mod detection;
mod history;
mod icon;
mod logging;
mod manifest;
mod paths;
mod registry;
mod runner;

pub use artifacts::{Artifact, discover_artifacts};
pub use detection::{DetectedAction, detect_actions, initialize_project};
pub use history::{HistoryStore, RunRecord};
pub use icon::find_project_icon;
pub use logging::{LoggingGuard, init_logging};
pub use manifest::{
    Action, ActionKind, ArtifactConfig, ConcurrencyPolicy, LogConfig, ManifestError, ProjectConfig,
    ProjectManifest, TerminalMode,
};
pub use paths::{app_data_dir, expand_environment_path};
pub use registry::{ProjectEntry, ProjectSummary, RegistryStore};
pub use runner::{OutputStream, RunOutput, RunResult, execute_action};

use std::path::{Path, PathBuf};

use anyhow::Result;

#[derive(Clone)]
pub struct CoreService {
    pub data_dir: PathBuf,
    pub registry: RegistryStore,
    pub history: HistoryStore,
}

impl CoreService {
    pub fn new() -> Result<Self> {
        Self::with_data_dir(app_data_dir()?)
    }

    pub fn with_data_dir(data_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&data_dir)?;
        let registry = RegistryStore::new(data_dir.join("registry.toml"));
        let history = HistoryStore::new(data_dir.join("mallee.db"));
        history.initialize()?;
        Ok(Self {
            data_dir,
            registry,
            history,
        })
    }

    pub fn add_project(&self, root: impl AsRef<Path>) -> Result<ProjectSummary> {
        self.registry.add(root)
    }

    pub fn projects(&self) -> Result<Vec<ProjectSummary>> {
        self.registry.list()
    }

    pub fn reorder_projects(&self, project_ids: &[String]) -> Result<()> {
        self.registry.reorder(project_ids)
    }
}
