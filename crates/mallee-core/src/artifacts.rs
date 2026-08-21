use std::{
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use anyhow::Result;
use glob::glob;
use serde::{Deserialize, Serialize};

use crate::{ProjectManifest, expand_environment_path};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    pub modified_ms: u64,
}

pub fn discover_artifacts(root: &Path, manifest: &ProjectManifest) -> Result<Vec<Artifact>> {
    let mut artifacts = Vec::new();
    for configured in &manifest.artifacts.paths {
        let pattern = expand_environment_path(configured, root)
            .to_string_lossy()
            .to_string();
        for entry in glob(&pattern)? {
            let Ok(path) = entry else { continue };
            if !path.is_file() {
                continue;
            }
            let metadata = fs::metadata(&path)?;
            let modified_ms = metadata
                .modified()
                .ok()
                .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                .map(|value| value.as_millis() as u64)
                .unwrap_or_default();
            artifacts.push(Artifact {
                name: path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
                path,
                size: metadata.len(),
                modified_ms,
            });
        }
    }
    artifacts.sort_by(|left, right| right.modified_ms.cmp(&left.modified_ms));
    Ok(artifacts)
}
