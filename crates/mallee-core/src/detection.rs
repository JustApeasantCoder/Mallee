use std::{collections::HashSet, fs, path::Path};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::{
    Action, ActionKind, ArtifactConfig, ConcurrencyPolicy, LogConfig, ProjectConfig,
    ProjectManifest, TerminalMode,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedAction {
    pub action: Action,
    pub source: String,
    pub reason: String,
}

pub fn detect_actions(root: &Path) -> Result<Vec<DetectedAction>> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut detected = Vec::new();
    let mut ids = HashSet::new();

    let search_directories = [
        root.join(".mallee").join("scripts"),
        root.clone(),
        root.join("scripts"),
        root.join("tools"),
        root.join("build"),
    ];
    for directory in search_directories {
        detect_powershell_scripts(&root, &directory, &mut ids, &mut detected)?;
    }

    let package_path = root.join("package.json");
    if package_path.is_file() {
        let text = fs::read_to_string(&package_path)?;
        let package: serde_json::Value = serde_json::from_str(&text)?;
        if let Some(scripts) = package.get("scripts").and_then(|value| value.as_object()) {
            for name in scripts.keys() {
                let id = sanitize_id(name);
                if id.is_empty() || !ids.insert(id.clone()) {
                    continue;
                }
                detected.push(DetectedAction {
                    action: command_action(
                        &id,
                        &title_from_id(&id),
                        "pnpm",
                        vec!["run".into(), name.clone()],
                    ),
                    source: "package.json".into(),
                    reason: format!("package script '{name}'"),
                });
            }
        }
    }

    if root.join("Cargo.toml").is_file() {
        for (id, label, argument) in [
            ("cargo-check", "Cargo Check", "check"),
            ("cargo-test", "Cargo Test", "test"),
            ("cargo-build", "Cargo Build", "build"),
        ] {
            if ids.insert(id.into()) {
                detected.push(DetectedAction {
                    action: command_action(
                        id,
                        label,
                        "cargo",
                        vec![argument.into(), "--workspace".into()],
                    ),
                    source: "Cargo.toml".into(),
                    reason: "Cargo workspace detected".into(),
                });
            }
        }
    }

    Ok(detected)
}

pub fn initialize_project(root: &Path, name: Option<&str>) -> Result<ProjectManifest> {
    let root = root
        .canonicalize()
        .with_context(|| format!("failed to resolve {}", root.display()))?;
    let config_directory = root.join(".mallee");
    let manifest_path = config_directory.join("project.toml");
    if manifest_path.exists() {
        bail!("{} already exists", manifest_path.display());
    }
    fs::create_dir_all(&config_directory)?;
    let fallback_name = root
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let project_name = name.unwrap_or(&fallback_name).trim().to_string();
    let id = sanitize_id(&project_name);
    if id.is_empty() {
        bail!("could not derive a valid project id from '{project_name}'");
    }
    let actions = detect_actions(&root)?
        .into_iter()
        .map(|candidate| candidate.action)
        .collect();
    let manifest = ProjectManifest {
        schema_version: 1,
        id,
        name: project_name,
        description: String::new(),
        project: ProjectConfig::default(),
        logs: LogConfig::default(),
        artifacts: ArtifactConfig::default(),
        actions,
    };
    manifest.validate()?;
    fs::write(&manifest_path, toml::to_string_pretty(&manifest)?)?;
    Ok(manifest)
}

fn detect_powershell_scripts(
    root: &Path,
    directory: &Path,
    ids: &mut HashSet<String>,
    detected: &mut Vec<DetectedAction>,
) -> Result<()> {
    if !directory.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file()
            || !path
                .extension()
                .is_some_and(|value| value.eq_ignore_ascii_case("ps1"))
        {
            continue;
        }
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        let id = sanitize_id(&stem);
        if id.is_empty() || !ids.insert(id.clone()) {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let mut action = command_action(
            &id,
            &title_from_id(&id),
            "pwsh",
            vec![
                "-NoLogo".into(),
                "-NoProfile".into(),
                "-File".into(),
                relative.clone(),
            ],
        );
        action.confirm = requires_confirmation(&id);
        detected.push(DetectedAction {
            action,
            source: relative,
            reason: "PowerShell script in a conventional project location".into(),
        });
    }
    Ok(())
}

fn command_action(id: &str, label: &str, program: &str, args: Vec<String>) -> Action {
    Action {
        id: id.into(),
        label: label.into(),
        icon: None,
        program: Some(program.into()),
        args,
        operation: None,
        working_directory: None,
        kind: ActionKind::Task,
        terminal: TerminalMode::Captured,
        concurrency: ConcurrencyPolicy::Allow,
        timeout_seconds: None,
        confirm: false,
    }
}

fn sanitize_id(value: &str) -> String {
    let mut result = String::new();
    let mut previous_hyphen = false;
    for character in value.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            result.push(character);
            previous_hyphen = false;
        } else if !previous_hyphen && !result.is_empty() {
            result.push('-');
            previous_hyphen = true;
        }
    }
    result.trim_matches('-').to_string()
}

fn title_from_id(id: &str) -> String {
    id.split('-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            match characters.next() {
                Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn requires_confirmation(id: &str) -> bool {
    ["release", "publish", "deploy", "sign", "delete", "remove"]
        .iter()
        .any(|term| id.contains(term))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_powershell_and_package_scripts_without_recursing() {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir_all(directory.path().join("scripts")).unwrap();
        fs::write(
            directory.path().join("scripts").join("Build-Installer.ps1"),
            "exit 0",
        )
        .unwrap();
        fs::write(
            directory.path().join("package.json"),
            r#"{"scripts":{"dev":"vite","build":"vite build"}}"#,
        )
        .unwrap();
        let actions = detect_actions(directory.path()).unwrap();
        assert!(
            actions
                .iter()
                .any(|entry| entry.action.id == "build-installer")
        );
        assert!(actions.iter().any(|entry| entry.action.id == "dev"));
        assert!(actions.iter().any(|entry| entry.action.id == "build"));
    }

    #[test]
    fn release_scripts_require_confirmation() {
        assert!(requires_confirmation("release"));
        assert!(!requires_confirmation("build-installer"));
    }

    #[test]
    fn initializes_a_loadable_manifest() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("Cargo.toml"), "[workspace]\n").unwrap();
        let created = initialize_project(directory.path(), Some("Sample Tool")).unwrap();
        assert_eq!(created.id, "sample-tool");
        let loaded = ProjectManifest::load(directory.path()).unwrap();
        assert_eq!(loaded.actions.len(), 3);
    }
}
