use std::{collections::HashSet, fs, path::Path};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use toml_edit::{Array, DocumentMut, Item, Table, value};

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("failed to read {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("failed to parse {path}: {source}")]
    Parse {
        path: String,
        source: toml::de::Error,
    },
    #[error("invalid manifest: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectManifest {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub project: ProjectConfig,
    #[serde(default)]
    pub logs: LogConfig,
    #[serde(default)]
    pub artifacts: ArtifactConfig,
    #[serde(default)]
    pub actions: Vec<Action>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    #[serde(default = "default_working_directory")]
    pub working_directory: String,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            working_directory: default_working_directory(),
        }
    }
}

fn default_working_directory() -> String {
    ".".to_string()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LogConfig {
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub open_with_deebugee: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ArtifactConfig {
    #[serde(default)]
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub id: String,
    pub label: String,
    /// Optional presentation glyph selected by a client. This does not affect execution.
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub program: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub operation: Option<String>,
    #[serde(default)]
    pub working_directory: Option<String>,
    #[serde(default)]
    pub kind: ActionKind,
    #[serde(default)]
    pub terminal: TerminalMode,
    #[serde(default)]
    pub concurrency: ConcurrencyPolicy,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
    #[serde(default)]
    pub confirm: bool,
    #[serde(default)]
    pub sound_notification: bool,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    #[default]
    Task,
    LongRunning,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TerminalMode {
    #[default]
    Captured,
    Interactive,
    Hidden,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConcurrencyPolicy {
    #[default]
    Allow,
    Reject,
    ReplaceSameAction,
}

impl ProjectManifest {
    pub fn load(root: &Path) -> Result<Self, ManifestError> {
        let path = root.join(".mallee").join("project.toml");
        let text = fs::read_to_string(&path).map_err(|source| ManifestError::Read {
            path: path.display().to_string(),
            source,
        })?;
        let manifest: Self = toml::from_str(&text).map_err(|source| ManifestError::Parse {
            path: path.display().to_string(),
            source,
        })?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.schema_version != 1 {
            return Err(ManifestError::Invalid(format!(
                "unsupported schema_version {}; expected 1",
                self.schema_version
            )));
        }
        if !valid_id(&self.id) {
            return Err(ManifestError::Invalid(format!(
                "project id '{}' must contain only lowercase letters, digits, and hyphens",
                self.id
            )));
        }
        if self.name.trim().is_empty() {
            return Err(ManifestError::Invalid(
                "project name cannot be empty".into(),
            ));
        }
        let mut ids = HashSet::new();
        for action in &self.actions {
            if !valid_id(&action.id) {
                return Err(ManifestError::Invalid(format!(
                    "action id '{}' must contain only lowercase letters, digits, and hyphens",
                    action.id
                )));
            }
            if !ids.insert(&action.id) {
                return Err(ManifestError::Invalid(format!(
                    "duplicate action id '{}'",
                    action.id
                )));
            }
            if action.label.trim().is_empty() {
                return Err(ManifestError::Invalid(format!(
                    "action '{}' has an empty label",
                    action.id
                )));
            }
            match (&action.program, &action.operation) {
                (Some(program), None) if !program.trim().is_empty() => {}
                (None, Some(operation)) if !operation.trim().is_empty() => {}
                _ => {
                    return Err(ManifestError::Invalid(format!(
                        "action '{}' must declare exactly one non-empty program or operation",
                        action.id
                    )));
                }
            }
        }
        Ok(())
    }

    pub fn action(&self, id: &str) -> Option<&Action> {
        self.actions.iter().find(|action| action.id == id)
    }

    pub fn append_action(root: &Path, action: Action) -> Result<Self, ManifestError> {
        let path = root.join(".mallee").join("project.toml");
        let text = fs::read_to_string(&path).map_err(|source| ManifestError::Read {
            path: path.display().to_string(),
            source,
        })?;
        let mut candidate = Self::load(root)?;
        candidate.actions.push(action.clone());
        candidate.validate()?;

        let mut document = text.parse::<DocumentMut>().map_err(|source| {
            ManifestError::Invalid(format!("failed to edit {}: {source}", path.display()))
        })?;
        let mut table = Table::new();
        table["id"] = value(action.id);
        table["label"] = value(action.label);
        if let Some(icon) = action.icon {
            table["icon"] = value(icon);
        }
        if let Some(program) = action.program {
            table["program"] = value(program);
        }
        let mut args = Array::new();
        for argument in action.args {
            args.push(argument);
        }
        if !args.is_empty() {
            table["args"] = value(args);
        }
        if let Some(operation) = action.operation {
            table["operation"] = value(operation);
        }
        if let Some(directory) = action.working_directory {
            table["working_directory"] = value(directory);
        }
        table["kind"] = value(match action.kind {
            ActionKind::Task => "task",
            ActionKind::LongRunning => "long_running",
        });
        table["terminal"] = value(match action.terminal {
            TerminalMode::Captured => "captured",
            TerminalMode::Interactive => "interactive",
            TerminalMode::Hidden => "hidden",
        });
        table["concurrency"] = value(match action.concurrency {
            ConcurrencyPolicy::Allow => "allow",
            ConcurrencyPolicy::Reject => "reject",
            ConcurrencyPolicy::ReplaceSameAction => "replace_same_action",
        });
        if let Some(timeout) = action.timeout_seconds {
            table["timeout_seconds"] = value(timeout as i64);
        }
        if action.confirm {
            table["confirm"] = value(true);
        }
        if action.sound_notification {
            table["sound_notification"] = value(true);
        }

        document
            .entry("actions")
            .or_insert(Item::ArrayOfTables(Default::default()))
            .as_array_of_tables_mut()
            .ok_or_else(|| ManifestError::Invalid("actions must be an array of tables".into()))?
            .push(table);

        let rendered = document.to_string();
        let parsed: Self = toml::from_str(&rendered).map_err(|source| ManifestError::Parse {
            path: path.display().to_string(),
            source,
        })?;
        parsed.validate()?;
        let backup = path.with_extension("toml.bak");
        fs::copy(&path, &backup).map_err(|source| ManifestError::Read {
            path: path.display().to_string(),
            source,
        })?;
        fs::write(&path, rendered).map_err(|source| ManifestError::Read {
            path: path.display().to_string(),
            source,
        })?;
        Ok(parsed)
    }

    pub fn reorder_actions(root: &Path, action_ids: &[String]) -> Result<Self, ManifestError> {
        let mut manifest = Self::load(root)?;
        if action_ids.len() != manifest.actions.len()
            || action_ids.iter().collect::<HashSet<_>>().len() != action_ids.len()
            || manifest
                .actions
                .iter()
                .any(|action| !action_ids.contains(&action.id))
        {
            return Err(ManifestError::Invalid(
                "action order must include every action exactly once".into(),
            ));
        }
        let path = root.join(".mallee").join("project.toml");
        let text = fs::read_to_string(&path).map_err(|source| ManifestError::Read {
            path: path.display().to_string(),
            source,
        })?;
        let mut document = text.parse::<DocumentMut>().map_err(|source| {
            ManifestError::Invalid(format!("failed to edit {}: {source}", path.display()))
        })?;
        let tables = document["actions"]
            .as_array_of_tables_mut()
            .ok_or_else(|| ManifestError::Invalid("actions must be an array of tables".into()))?;
        let mut by_id = std::collections::HashMap::new();
        while !tables.is_empty() {
            let table = tables.remove(0);
            let id = table["id"]
                .as_str()
                .ok_or_else(|| ManifestError::Invalid("every action must have an id".into()))?
                .to_string();
            by_id.insert(id, table);
        }
        for id in action_ids {
            tables.push(
                by_id
                    .remove(id)
                    .ok_or_else(|| ManifestError::Invalid(format!("unknown action '{id}'")))?,
            );
        }
        manifest
            .actions
            .sort_by_key(|action| action_ids.iter().position(|id| id == &action.id).unwrap());
        Self::write_document(&path, document, manifest)
    }

    pub fn remove_action(root: &Path, action_id: &str) -> Result<Self, ManifestError> {
        let mut manifest = Self::load(root)?;
        let index = manifest
            .actions
            .iter()
            .position(|action| action.id == action_id)
            .ok_or_else(|| ManifestError::Invalid(format!("unknown action '{action_id}'")))?;
        let path = root.join(".mallee").join("project.toml");
        let text = fs::read_to_string(&path).map_err(|source| ManifestError::Read {
            path: path.display().to_string(),
            source,
        })?;
        let mut document = text.parse::<DocumentMut>().map_err(|source| {
            ManifestError::Invalid(format!("failed to edit {}: {source}", path.display()))
        })?;
        let tables = document["actions"]
            .as_array_of_tables_mut()
            .ok_or_else(|| ManifestError::Invalid("actions must be an array of tables".into()))?;
        if index >= tables.len() {
            return Err(ManifestError::Invalid("action table is missing".into()));
        }
        tables.remove(index);
        manifest.actions.remove(index);
        Self::write_document(&path, document, manifest)
    }

    pub fn update_action_presentation(
        root: &Path,
        action_id: &str,
        label: String,
        icon: Option<String>,
    ) -> Result<Self, ManifestError> {
        let mut manifest = Self::load(root)?;
        let index = manifest
            .actions
            .iter()
            .position(|action| action.id == action_id)
            .ok_or_else(|| ManifestError::Invalid(format!("unknown action '{action_id}'")))?;
        manifest.actions[index].label = label;
        manifest.actions[index].icon = icon;

        let path = root.join(".mallee").join("project.toml");
        let text = fs::read_to_string(&path).map_err(|source| ManifestError::Read {
            path: path.display().to_string(),
            source,
        })?;
        let mut document = text.parse::<DocumentMut>().map_err(|source| {
            ManifestError::Invalid(format!("failed to edit {}: {source}", path.display()))
        })?;
        let table = document["actions"]
            .as_array_of_tables_mut()
            .and_then(|actions| actions.get_mut(index))
            .ok_or_else(|| ManifestError::Invalid("action table is missing".into()))?;
        table["label"] = value(manifest.actions[index].label.clone());
        if let Some(icon) = &manifest.actions[index].icon {
            table["icon"] = value(icon.clone());
        } else {
            table.remove("icon");
        }
        Self::write_document(&path, document, manifest)
    }

    fn write_document(
        path: &Path,
        document: DocumentMut,
        manifest: Self,
    ) -> Result<Self, ManifestError> {
        let rendered = document.to_string();
        let parsed: Self = toml::from_str(&rendered).map_err(|source| ManifestError::Parse {
            path: path.display().to_string(),
            source,
        })?;
        parsed.validate()?;
        let backup = path.with_extension("toml.bak");
        fs::copy(path, &backup).map_err(|source| ManifestError::Read {
            path: path.display().to_string(),
            source,
        })?;
        fs::write(path, rendered).map_err(|source| ManifestError::Read {
            path: path.display().to_string(),
            source,
        })?;
        Ok(manifest)
    }
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_duplicate_action_ids() {
        let manifest = ProjectManifest {
            schema_version: 1,
            id: "sample".into(),
            name: "Sample".into(),
            description: String::new(),
            project: ProjectConfig::default(),
            logs: LogConfig::default(),
            artifacts: ArtifactConfig::default(),
            actions: vec![action("build"), action("build")],
        };
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn project_defaults_to_repository_root() {
        let manifest: ProjectManifest = toml::from_str(
            r#"schema_version = 1
id = "sample"
name = "Sample"
"#,
        )
        .unwrap();
        assert_eq!(manifest.project.working_directory, ".");
    }

    #[test]
    fn appends_an_action_without_discarding_comments() {
        let directory = tempfile::tempdir().unwrap();
        let config = directory.path().join(".mallee");
        fs::create_dir_all(&config).unwrap();
        let path = config.join("project.toml");
        fs::write(
            &path,
            "# keep this comment\nschema_version = 1\nid = \"sample\"\nname = \"Sample\"\n",
        )
        .unwrap();
        let mut build = action("build");
        build.sound_notification = true;
        let updated = ProjectManifest::append_action(directory.path(), build).unwrap();
        assert_eq!(updated.actions.len(), 1);
        assert!(updated.actions[0].sound_notification);
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains("# keep this comment")
        );
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains("sound_notification = true")
        );
        assert!(path.with_extension("toml.bak").is_file());
    }

    #[test]
    fn reorders_and_removes_actions_without_rewriting_the_manifest() {
        let directory = tempfile::tempdir().unwrap();
        let config = directory.path().join(".mallee");
        fs::create_dir_all(&config).unwrap();
        let path = config.join("project.toml");
        fs::write(
            &path,
            "# keep this comment\nschema_version = 1\nid = \"sample\"\nname = \"Sample\"\n\n[[actions]]\nid = \"build\"\nlabel = \"Build\"\nprogram = \"cargo\"\n\n[[actions]]\nid = \"test\"\nlabel = \"Test\"\nprogram = \"cargo\"\n",
        )
        .unwrap();

        let reordered =
            ProjectManifest::reorder_actions(directory.path(), &["test".into(), "build".into()])
                .unwrap();
        assert_eq!(
            reordered
                .actions
                .iter()
                .map(|action| action.id.as_str())
                .collect::<Vec<_>>(),
            ["test", "build"]
        );
        let removed = ProjectManifest::remove_action(directory.path(), "test").unwrap();
        assert_eq!(
            removed
                .actions
                .iter()
                .map(|action| action.id.as_str())
                .collect::<Vec<_>>(),
            ["build"]
        );
        let rendered = fs::read_to_string(path).unwrap();
        assert!(rendered.contains("# keep this comment"));
        assert!(!rendered.contains("id = \"test\""));
    }

    #[test]
    fn updates_action_presentation_without_changing_execution_fields() {
        let directory = tempfile::tempdir().unwrap();
        let config = directory.path().join(".mallee");
        fs::create_dir_all(&config).unwrap();
        fs::write(
            config.join("project.toml"),
            "schema_version = 1\nid = \"sample\"\nname = \"Sample\"\n\n[[actions]]\nid = \"build\"\nlabel = \"Build\"\nprogram = \"cargo\"\nargs = [\"build\"]\n",
        )
        .unwrap();

        let updated = ProjectManifest::update_action_presentation(
            directory.path(),
            "build",
            "Build installer".into(),
            Some("◇".into()),
        )
        .unwrap();
        assert_eq!(updated.actions[0].label, "Build installer");
        assert_eq!(updated.actions[0].icon.as_deref(), Some("◇"));
        assert_eq!(updated.actions[0].args, ["build"]);
        assert!(
            fs::read_to_string(config.join("project.toml"))
                .unwrap()
                .contains("icon = \"◇\"")
        );
    }

    fn action(id: &str) -> Action {
        Action {
            id: id.into(),
            label: "Build".into(),
            icon: None,
            program: Some("cargo".into()),
            args: vec![],
            operation: None,
            working_directory: None,
            kind: ActionKind::Task,
            terminal: TerminalMode::Captured,
            concurrency: ConcurrencyPolicy::Allow,
            timeout_seconds: None,
            confirm: false,
            sound_notification: false,
        }
    }

    #[test]
    fn sound_notifications_are_opt_in() {
        let disabled: ProjectManifest = toml::from_str(
            "schema_version = 1\nid = \"sample\"\nname = \"Sample\"\n\n[[actions]]\nid = \"build\"\nlabel = \"Build\"\nprogram = \"cargo\"\n",
        )
        .unwrap();
        assert!(!disabled.actions[0].sound_notification);

        let enabled: ProjectManifest = toml::from_str(
            "schema_version = 1\nid = \"sample\"\nname = \"Sample\"\n\n[[actions]]\nid = \"build\"\nlabel = \"Build\"\nprogram = \"cargo\"\nsound_notification = true\n",
        )
        .unwrap();
        assert!(enabled.actions[0].sound_notification);
    }
}
