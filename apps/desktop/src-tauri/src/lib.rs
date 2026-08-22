use std::{
    collections::{HashMap, HashSet},
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex},
    time::Instant,
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use mallee_core::{
    Action, Artifact, ConcurrencyPolicy, CoreService, LoggingGuard, OutputStream, ProjectManifest,
    ProjectSummary, RunOutput, RunRecord, discover_artifacts, execute_action,
    expand_environment_path, find_project_icon, init_logging,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

const RUN_EVENT: &str = "mallee://run-event";

#[derive(Default)]
struct RuntimeSessions {
    processes: Mutex<HashMap<String, ActiveProcess>>,
    cancelled: Mutex<HashSet<String>>,
}

#[derive(Clone)]
struct ActiveProcess {
    process_id: Option<u32>,
    project_id: String,
    action_id: String,
}

struct AppState {
    core: CoreService,
    runtime: Arc<RuntimeSessions>,
    _logging: Arc<Mutex<Option<LoggingGuard>>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RunEventPayload {
    run_id: String,
    project_id: String,
    action_id: String,
    kind: String,
    stream: Option<OutputStream>,
    line: Option<String>,
    status: Option<String>,
    exit_code: Option<i32>,
    duration_ms: Option<i64>,
}

#[tauri::command]
fn list_projects(state: State<'_, AppState>) -> Result<Vec<ProjectSummary>, String> {
    state.core.projects().map_err(display_error)
}

#[tauri::command]
fn add_project(path: String, state: State<'_, AppState>) -> Result<ProjectSummary, String> {
    let project = state.core.add_project(path).map_err(display_error)?;
    tracing::info!(
        subsystem = "registry",
        event = "project.registered",
        status = "success",
        project_id = %project.manifest.id,
        project_root = %project.root.display(),
        "[Projects] Project registered"
    );
    Ok(project)
}

#[tauri::command]
fn reorder_projects(project_ids: Vec<String>, state: State<'_, AppState>) -> Result<(), String> {
    state
        .core
        .reorder_projects(&project_ids)
        .map_err(display_error)
}

#[tauri::command]
fn project_icon(project_id: String, state: State<'_, AppState>) -> Result<Option<String>, String> {
    const MAX_ICON_BYTES: u64 = 2 * 1024 * 1024;

    let project = find_project(&state.core, &project_id)?;
    let Some(path) = find_project_icon(&project.root) else {
        return Ok(None);
    };
    let metadata = std::fs::metadata(&path).map_err(display_error)?;
    if metadata.len() > MAX_ICON_BYTES {
        tracing::warn!(
            subsystem = "projects",
            event = "project.icon.skipped",
            project_id = %project_id,
            icon_path = %path.display(),
            icon_bytes = metadata.len(),
            "[Projects] Icon exceeds the sidebar size limit"
        );
        return Ok(None);
    }
    let mime = match path.extension().and_then(|extension| extension.to_str()) {
        Some(extension) if extension.eq_ignore_ascii_case("png") => "image/png",
        Some(extension) if extension.eq_ignore_ascii_case("svg") => "image/svg+xml",
        Some(extension) if extension.eq_ignore_ascii_case("ico") => "image/x-icon",
        _ => return Ok(None),
    };
    let bytes = std::fs::read(&path).map_err(display_error)?;
    Ok(Some(format!(
        "data:{mime};base64,{}",
        STANDARD.encode(bytes)
    )))
}

#[tauri::command]
fn add_action(
    project_id: String,
    action: Action,
    state: State<'_, AppState>,
) -> Result<ProjectSummary, String> {
    let project = find_project(&state.core, &project_id)?;
    let manifest = ProjectManifest::append_action(&project.root, action).map_err(display_error)?;
    tracing::info!(
        subsystem = "manifest",
        event = "manifest.action.added",
        status = "success",
        project_id = %manifest.id,
        action_count = manifest.actions.len(),
        "[Actions] Project action added"
    );
    Ok(ProjectSummary {
        root: project.root,
        manifest,
    })
}

#[tauri::command]
fn open_manifest(project_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let project = find_project(&state.core, &project_id)?;
    let manifest = project.root.join(".mallee").join("project.toml");
    if !manifest.is_file() {
        return Err("this project no longer has a .mallee/project.toml manifest".into());
    }
    std::process::Command::new("explorer.exe")
        .arg(manifest)
        .spawn()
        .map_err(display_error)?;
    Ok(())
}

#[tauri::command]
fn reorder_actions(
    project_id: String,
    action_ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<ProjectSummary, String> {
    let project = find_project(&state.core, &project_id)?;
    let manifest =
        ProjectManifest::reorder_actions(&project.root, &action_ids).map_err(display_error)?;
    Ok(ProjectSummary {
        root: project.root,
        manifest,
    })
}

#[tauri::command]
fn remove_action(
    project_id: String,
    action_id: String,
    state: State<'_, AppState>,
) -> Result<ProjectSummary, String> {
    if state
        .runtime
        .processes
        .lock()
        .map_err(|_| "process registry is unavailable".to_string())?
        .values()
        .any(|active| active.project_id == project_id && active.action_id == action_id)
    {
        return Err("stop the action before removing it".into());
    }
    let project = find_project(&state.core, &project_id)?;
    let manifest =
        ProjectManifest::remove_action(&project.root, &action_id).map_err(display_error)?;
    Ok(ProjectSummary {
        root: project.root,
        manifest,
    })
}

#[tauri::command]
fn update_action_presentation(
    project_id: String,
    action_id: String,
    label: String,
    icon: Option<String>,
    state: State<'_, AppState>,
) -> Result<ProjectSummary, String> {
    let project = find_project(&state.core, &project_id)?;
    let manifest =
        ProjectManifest::update_action_presentation(&project.root, &action_id, label, icon)
            .map_err(display_error)?;
    Ok(ProjectSummary {
        root: project.root,
        manifest,
    })
}

#[tauri::command]
fn get_history(project_id: String, state: State<'_, AppState>) -> Result<Vec<RunRecord>, String> {
    state
        .core
        .history
        .list(&project_id, 50)
        .map_err(display_error)
}

#[tauri::command]
fn get_artifacts(project_id: String, state: State<'_, AppState>) -> Result<Vec<Artifact>, String> {
    let project = find_project(&state.core, &project_id)?;
    discover_artifacts(&project.root, &project.manifest).map_err(display_error)
}

#[tauri::command]
fn open_artifact(
    project_id: String,
    artifact_path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let artifact = resolve_artifact(&state.core, &project_id, &artifact_path)?;
    std::process::Command::new("explorer.exe")
        .arg(&artifact)
        .spawn()
        .map_err(display_error)?;
    Ok(())
}

#[tauri::command]
fn open_artifact_folder(
    project_id: String,
    artifact_path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let artifact = resolve_artifact(&state.core, &project_id, &artifact_path)?;
    let folder = artifact
        .parent()
        .ok_or_else(|| "artifact does not have a parent folder".to_string())?;
    std::process::Command::new("explorer.exe")
        .arg(folder)
        .spawn()
        .map_err(display_error)?;
    Ok(())
}

fn resolve_artifact(
    core: &CoreService,
    project_id: &str,
    artifact_path: &str,
) -> Result<PathBuf, String> {
    let project = find_project(core, project_id)?;
    let requested = PathBuf::from(artifact_path);
    discover_artifacts(&project.root, &project.manifest)
        .map_err(display_error)?
        .into_iter()
        .find(|artifact| artifact.path == requested)
        .map(|artifact| artifact.path)
        .ok_or_else(|| "artifact is no longer available for this project".to_string())
}

#[tauri::command]
async fn start_action(
    project_id: String,
    action_id: String,
    confirmed: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let project = find_project(&state.core, &project_id)?;
    let action = project
        .manifest
        .action(&action_id)
        .cloned()
        .ok_or_else(|| {
            format!(
                "project '{}' has no action '{action_id}'",
                project.manifest.name
            )
        })?;

    if action.confirm && !confirmed {
        return Err("this action requires confirmation".into());
    }

    if let Some(operation) = action.operation.as_deref() {
        perform_operation(operation, &project)?;
        tracing::info!(
            subsystem = "actions",
            event = "action.operation.completed",
            status = "success",
            project_id = %project.manifest.id,
            action_id = %action.id,
            "[Action] Project operation completed"
        );
        return Ok(format!("operation-{}", Uuid::new_v4()));
    }

    let run_id = Uuid::new_v4().to_string();
    let transcript_path = state
        .core
        .data_dir
        .join("transcripts")
        .join(&project.manifest.id)
        .join(format!("{run_id}.log"));
    if let Some(parent) = transcript_path.parent() {
        std::fs::create_dir_all(parent).map_err(display_error)?;
    }
    let transcript = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&transcript_path)
        .map_err(display_error)?;
    let replaced_processes = {
        let mut processes = state
            .runtime
            .processes
            .lock()
            .map_err(|_| "process registry is unavailable".to_string())?;
        let existing = processes
            .iter()
            .filter(|(_, active)| {
                active.project_id == project.manifest.id && active.action_id == action.id
            })
            .map(|(run_id, active)| (run_id.clone(), active.process_id))
            .collect::<Vec<_>>();
        if !existing.is_empty() && action.concurrency == ConcurrencyPolicy::Reject {
            return Err(format!("action '{}' is already running", action.label));
        }
        processes.insert(
            run_id.clone(),
            ActiveProcess {
                process_id: None,
                project_id: project.manifest.id.clone(),
                action_id: action.id.clone(),
            },
        );
        existing
    };
    if action.concurrency == ConcurrencyPolicy::ReplaceSameAction {
        {
            let mut cancelled = state
                .runtime
                .cancelled
                .lock()
                .map_err(|_| "cancellation registry is unavailable".to_string())?;
            for (existing_run_id, _) in &replaced_processes {
                cancelled.insert(existing_run_id.clone());
            }
        }
        for (_, process_id) in replaced_processes {
            if let Some(process_id) = process_id
                && let Err(error) = terminate_process_tree(process_id).await
            {
                if let Ok(mut processes) = state.runtime.processes.lock() {
                    processes.remove(&run_id);
                }
                return Err(error);
            }
        }
    }
    if let Err(error) = state.core.history.start_run(
        &run_id,
        &project.manifest.id,
        &action.id,
        &action.label,
        &transcript_path,
    ) {
        if let Ok(mut processes) = state.runtime.processes.lock() {
            processes.remove(&run_id);
        }
        return Err(display_error(error));
    }

    let core = state.core.clone();
    let runtime = Arc::clone(&state.runtime);
    let run_id_for_task = run_id.clone();
    let app_for_task = app.clone();
    let transcript = Arc::new(Mutex::new(transcript));

    tauri::async_runtime::spawn(async move {
        let started = Instant::now();
        let _ = core.history.set_running(&run_id_for_task);
        emit_run_event(
            &app_for_task,
            RunEventPayload {
                run_id: run_id_for_task.clone(),
                project_id: project.manifest.id.clone(),
                action_id: action.id.clone(),
                kind: "started".into(),
                stream: None,
                line: None,
                status: Some("running".into()),
                exit_code: None,
                duration_ms: None,
            },
        );
        tracing::info!(
            subsystem = "runner",
            event = "action.run.started",
            session_id = %run_id_for_task,
            status = "running",
            project_id = %project.manifest.id,
            action_id = %action.id,
            "[Action] Run started"
        );

        let runtime_for_spawn = Arc::clone(&runtime);
        let run_id_for_spawn = run_id_for_task.clone();
        let app_for_output = app_for_task.clone();
        let run_id_for_output = run_id_for_task.clone();
        let project_id_for_output = project.manifest.id.clone();
        let action_id_for_output = action.id.clone();
        let transcript_for_output = Arc::clone(&transcript);
        let result = execute_action(
            &project.root,
            &project.manifest,
            &action,
            move |process_id| {
                let registered = if let Ok(mut processes) = runtime_for_spawn.processes.lock()
                    && let Some(active) = processes.get_mut(&run_id_for_spawn)
                {
                    active.process_id = Some(process_id);
                    true
                } else {
                    false
                };
                registered
                    && runtime_for_spawn
                        .cancelled
                        .lock()
                        .map(|cancelled| !cancelled.contains(&run_id_for_spawn))
                        .unwrap_or(false)
            },
            move |output: RunOutput| {
                if let Ok(mut file) = transcript_for_output.lock() {
                    let prefix = match output.stream {
                        OutputStream::Stdout => "",
                        OutputStream::Stderr => "[stderr] ",
                        OutputStream::System => "[mallee] ",
                    };
                    let _ = writeln!(file, "{prefix}{}", output.line);
                }
                emit_run_event(
                    &app_for_output,
                    RunEventPayload {
                        run_id: run_id_for_output.clone(),
                        project_id: project_id_for_output.clone(),
                        action_id: action_id_for_output.clone(),
                        kind: "output".into(),
                        stream: Some(output.stream),
                        line: Some(output.line),
                        status: None,
                        exit_code: None,
                        duration_ms: None,
                    },
                );
            },
        )
        .await;

        if let Ok(mut processes) = runtime.processes.lock() {
            processes.remove(&run_id_for_task);
        }
        let cancelled = runtime
            .cancelled
            .lock()
            .map(|mut values| values.remove(&run_id_for_task))
            .unwrap_or(false);

        match result {
            Ok(result) => {
                let status = if cancelled {
                    "cancelled"
                } else if result.success {
                    "success"
                } else {
                    "failed"
                };
                let _ = core.history.finish_run(
                    &run_id_for_task,
                    status,
                    result.duration_ms,
                    result.exit_code,
                );
                emit_run_event(
                    &app_for_task,
                    RunEventPayload {
                        run_id: run_id_for_task.clone(),
                        project_id: project.manifest.id.clone(),
                        action_id: action.id.clone(),
                        kind: "finished".into(),
                        stream: None,
                        line: None,
                        status: Some(status.into()),
                        exit_code: result.exit_code,
                        duration_ms: Some(result.duration_ms),
                    },
                );
                let event_name = if status == "success" {
                    "action.run.completed"
                } else if status == "cancelled" {
                    "action.run.cancelled"
                } else {
                    "action.run.failed"
                };
                tracing::info!(
                    subsystem = "runner",
                    event = event_name,
                    session_id = %run_id_for_task,
                    status = status,
                    duration_ms = result.duration_ms as f64,
                    exit_code = result.exit_code,
                    project_id = %project.manifest.id,
                    action_id = %action.id,
                    "[Action] Run finished"
                );
            }
            Err(error) => {
                let duration_ms = started.elapsed().as_millis() as i64;
                let status = if cancelled { "cancelled" } else { "failed" };
                let _ = core
                    .history
                    .finish_run(&run_id_for_task, status, duration_ms, None);
                emit_run_event(
                    &app_for_task,
                    RunEventPayload {
                        run_id: run_id_for_task.clone(),
                        project_id: project.manifest.id.clone(),
                        action_id: action.id.clone(),
                        kind: "finished".into(),
                        stream: None,
                        line: Some(error.to_string()),
                        status: Some(status.into()),
                        exit_code: None,
                        duration_ms: Some(duration_ms),
                    },
                );
                tracing::error!(
                    subsystem = "runner",
                    event = "action.run.failed",
                    session_id = %run_id_for_task,
                    status = "failed",
                    error_kind = "execution_error",
                    duration_ms = duration_ms as f64,
                    project_id = %project.manifest.id,
                    action_id = %action.id,
                    error = %error,
                    "[Action] Run failed"
                );
            }
        }
    });

    Ok(run_id)
}

#[tauri::command]
async fn stop_run(run_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let process_id = state
        .runtime
        .processes
        .lock()
        .map_err(|_| "process registry is unavailable".to_string())?
        .get(&run_id)
        .map(|active| active.process_id)
        .ok_or_else(|| format!("run '{run_id}' is not active"))?;
    state
        .runtime
        .cancelled
        .lock()
        .map_err(|_| "cancellation registry is unavailable".to_string())?
        .insert(run_id.clone());

    if let Some(process_id) = process_id {
        terminate_process_tree(process_id).await
    } else {
        Ok(())
    }
}

async fn terminate_process_tree(process_id: u32) -> Result<(), String> {
    let status = tokio::process::Command::new("taskkill.exe")
        .args(["/PID", &process_id.to_string(), "/T", "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .status()
        .await
        .map_err(display_error)?;
    if !status.success() {
        return Err(format!(
            "failed to stop process tree for process '{process_id}'"
        ));
    }
    Ok(())
}

#[tauri::command]
fn open_in_deebugee(project_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let project = find_project(&state.core, &project_id)?;
    if !project.manifest.logs.open_with_deebugee {
        return Err("this project has not enabled DeeBugee integration".into());
    }
    let executable = locate_deebugee().ok_or_else(|| {
        "DeeBugee was not found. Set DEEBUGEE_EXE or install dee-bugee.exe on PATH.".to_string()
    })?;
    let mut command = std::process::Command::new(executable);
    if project
        .root
        .join(".deebugee")
        .join("project.toml")
        .is_file()
    {
        command.arg("--project").arg(&project.root);
    } else if let Some(source) = project.manifest.logs.sources.first() {
        command.arg(expand_environment_path(source, &project.root));
    } else {
        return Err("this project does not declare any log sources".into());
    }
    command.spawn().map_err(display_error)?;
    Ok(())
}

fn perform_operation(operation: &str, project: &ProjectSummary) -> Result<(), String> {
    let path = match operation {
        "open_project_folder" => project.root.clone(),
        "open_log_folder" => resolve_log_folder(project)?,
        other => return Err(format!("unsupported operation '{other}'")),
    };
    std::process::Command::new("explorer.exe")
        .arg(path)
        .spawn()
        .map_err(display_error)?;
    Ok(())
}

fn resolve_log_folder(project: &ProjectSummary) -> Result<PathBuf, String> {
    let source = project
        .manifest
        .logs
        .sources
        .first()
        .map(|value| expand_environment_path(value, &project.root))
        .ok_or_else(|| "this project does not declare a log folder".to_string())?;

    // A log source can be either a JSONL file or a directory of application logs.
    // Keep configured directories intact instead of opening their parent folder.
    if source.is_dir() || source.extension().is_none() {
        Ok(source)
    } else {
        source
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| "this project does not declare a log folder".to_string())
    }
}

fn find_project(core: &CoreService, project_id: &str) -> Result<ProjectSummary, String> {
    core.projects()
        .map_err(display_error)?
        .into_iter()
        .find(|project| project.manifest.id == project_id)
        .ok_or_else(|| format!("project '{project_id}' is not registered"))
}

fn emit_run_event(app: &AppHandle, payload: RunEventPayload) {
    if let Err(error) = app.emit(RUN_EVENT, payload) {
        tracing::warn!(
            subsystem = "ipc",
            event = "run.event.emit.failed",
            error_kind = "ipc_error",
            error = %error,
            "[Terminal] Failed to emit run event"
        );
    }
}

fn locate_deebugee() -> Option<PathBuf> {
    if let Some(configured) = std::env::var_os("DEEBUGEE_EXE") {
        let path = PathBuf::from(configured);
        if path.is_file() {
            return Some(path);
        }
    }
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        let installed = installed_deebugee_path(Path::new(&local_app_data));
        if installed.is_file() {
            return Some(installed);
        }
    }
    let output = std::process::Command::new("where.exe")
        .arg("dee-bugee.exe")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(PathBuf::from)
}

fn installed_deebugee_path(local_app_data: &Path) -> PathBuf {
    local_app_data
        .join("Programs")
        .join("DeeBugee")
        .join("dee-bugee.exe")
}

fn display_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let core = CoreService::new().expect("failed to initialize Mallee data storage");
    let logging =
        init_logging(&core.data_dir, "desktop").expect("failed to initialize Mallee logging");

    #[cfg(debug_assertions)]
    {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
        if root.join(".mallee").join("project.toml").is_file() {
            let _ = core.add_project(root);
        }
    }

    let state = AppState {
        core,
        runtime: Arc::new(RuntimeSessions::default()),
        _logging: Arc::new(Mutex::new(Some(logging))),
    };

    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            list_projects,
            add_project,
            reorder_projects,
            project_icon,
            add_action,
            open_manifest,
            reorder_actions,
            remove_action,
            update_action_presentation,
            get_history,
            get_artifacts,
            open_artifact,
            open_artifact_folder,
            start_action,
            stop_run,
            open_in_deebugee,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Mallee");
}

#[cfg(test)]
mod tests {
    use super::*;
    use mallee_core::{ArtifactConfig, LogConfig, ProjectConfig};

    #[test]
    fn resolves_a_log_directory_without_stepping_up_to_its_parent() {
        let log_directory = std::env::temp_dir();
        let project = ProjectSummary {
            root: log_directory.clone(),
            manifest: ProjectManifest {
                schema_version: 1,
                id: "sample".into(),
                name: "Sample".into(),
                description: String::new(),
                project: ProjectConfig::default(),
                logs: LogConfig {
                    sources: vec![log_directory.to_string_lossy().into_owned()],
                    open_with_deebugee: false,
                },
                artifacts: ArtifactConfig::default(),
                actions: vec![],
            },
        };

        assert_eq!(resolve_log_folder(&project).unwrap(), log_directory);
    }

    #[test]
    fn resolves_the_standard_deebugee_installation_path() {
        assert_eq!(
            installed_deebugee_path(Path::new(r"C:\Users\Sample\AppData\Local")),
            PathBuf::from(r"C:\Users\Sample\AppData\Local\Programs\DeeBugee\dee-bugee.exe")
        );
    }
}
