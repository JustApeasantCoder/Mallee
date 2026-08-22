use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{ProjectManifest, paths::canonical_project_root};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectEntry {
    pub root: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSummary {
    pub root: PathBuf,
    pub manifest: ProjectManifest,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RegistryFile {
    #[serde(default)]
    projects: Vec<ProjectEntry>,
}

#[derive(Debug, Clone)]
pub struct RegistryStore {
    path: PathBuf,
}

impl RegistryStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn add(&self, root: impl AsRef<Path>) -> Result<ProjectSummary> {
        let root = canonical_project_root(root)?;
        let manifest = ProjectManifest::load(&root)?;
        let mut registry = self.read()?;
        if let Some(existing) = registry.projects.iter().find_map(|entry| {
            let existing_root = canonical_project_root(&entry.root).ok()?;
            if existing_root == root {
                return None;
            }
            let existing_manifest = ProjectManifest::load(&existing_root).ok()?;
            (existing_manifest.id == manifest.id).then_some(existing_root)
        }) {
            anyhow::bail!(
                "project id '{}' is already registered for {}; project ids must be unique",
                manifest.id,
                existing.display()
            );
        }
        if !registry
            .projects
            .iter()
            .any(|entry| canonical_project_root(&entry.root).is_ok_and(|existing| existing == root))
        {
            registry.projects.push(ProjectEntry { root: root.clone() });
            self.write(&registry)?;
        }
        Ok(ProjectSummary { root, manifest })
    }

    pub fn list(&self) -> Result<Vec<ProjectSummary>> {
        let mut summaries = Vec::new();
        for entry in self.read()?.projects {
            let root = canonical_project_root(&entry.root).unwrap_or(entry.root);
            match ProjectManifest::load(&root) {
                Ok(manifest) => summaries.push(ProjectSummary { root, manifest }),
                Err(error) => tracing::warn!(
                    subsystem = "registry",
                    event = "project.load.failed",
                    error_kind = "invalid_manifest",
                    project_root = %root.display(),
                    error = %error,
                    "[Projects] Registered project could not be loaded"
                ),
            }
        }
        Ok(summaries)
    }

    pub fn reorder(&self, project_ids: &[String]) -> Result<()> {
        let mut registry = self.read()?;
        let projects = self.list()?;
        if project_ids.len() != projects.len()
            || project_ids
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                != project_ids.len()
            || projects
                .iter()
                .any(|project| !project_ids.contains(&project.manifest.id))
        {
            anyhow::bail!("project order must include every registered project exactly once");
        }

        let mut entries = std::collections::HashMap::new();
        let mut unavailable = Vec::new();
        for entry in registry.projects.drain(..) {
            let root = canonical_project_root(&entry.root).unwrap_or(entry.root.clone());
            match ProjectManifest::load(&root) {
                Ok(manifest) => {
                    entries.insert(manifest.id, entry);
                }
                Err(_) => unavailable.push(entry),
            }
        }
        registry.projects = project_ids
            .iter()
            .map(|id| entries.remove(id).expect("validated project id"))
            .chain(unavailable)
            .collect();
        self.write(&registry)
    }

    fn read(&self) -> Result<RegistryFile> {
        if !self.path.exists() {
            return Ok(RegistryFile::default());
        }
        let text = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read {}", self.path.display()))?;
        toml::from_str(&text).with_context(|| format!("failed to parse {}", self.path.display()))
    }

    fn write(&self, registry: &RegistryFile) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(registry)?;
        fs::write(&self.path, text)
            .with_context(|| format!("failed to write {}", self.path.display()))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::RegistryStore;

    fn write_project(root: &std::path::Path, id: &str) {
        let config = root.join(".mallee");
        fs::create_dir_all(&config).unwrap();
        fs::write(
            config.join("project.toml"),
            format!("schema_version = 1\nid = \"{id}\"\nname = \"{id}\"\n"),
        )
        .unwrap();
    }

    #[test]
    fn rejects_a_duplicate_project_id_from_a_different_root() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("first");
        let second = directory.path().join("second");
        write_project(&first, "sample");
        write_project(&second, "sample");
        let store = RegistryStore::new(directory.path().join("registry.toml"));

        store.add(&first).unwrap();
        let error = store.add(&second).unwrap_err();
        assert!(error.to_string().contains("project ids must be unique"));
    }
}
