use std::{
    io::Write,
    path::{Path, PathBuf},
    time::Instant,
};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use mallee_core::{
    CoreService, OutputStream, ProjectManifest, RunOutput, detect_actions, discover_artifacts,
    execute_action, init_logging, initialize_project,
};
use uuid::Uuid;

#[derive(Debug, Parser)]
#[command(name = "mallee", version, about = "Developer project command center")]
struct Cli {
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Init {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long)]
        name: Option<String>,
    },
    Add {
        path: PathBuf,
    },
    Detect {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    List,
    Validate {
        path: PathBuf,
    },
    Actions {
        project: String,
    },
    Run {
        project: String,
        action: String,
    },
    History {
        project: String,
    },
    Artifacts {
        project: String,
    },
    Doctor {
        project: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let core = CoreService::new()?;
    let _logging = init_logging(&core.data_dir, "cli")?;

    match cli.command {
        Command::Init { path, name } => {
            let manifest = initialize_project(&path, name.as_deref())?;
            let project = core.add_project(path)?;
            if cli.json {
                print_value(true, &project)?;
            } else {
                println!(
                    "Initialized {} with {} detected actions",
                    manifest.name,
                    manifest.actions.len()
                );
            }
        }
        Command::Add { path } => print_value(cli.json, &core.add_project(path)?)?,
        Command::Detect { path } => print_value(cli.json, &detect_actions(&path)?)?,
        Command::List => print_value(cli.json, &core.projects()?)?,
        Command::Validate { path } => {
            let root = canonical_or_original(&path);
            let manifest = ProjectManifest::load(&root)?;
            if cli.json {
                print_value(
                    true,
                    &serde_json::json!({ "valid": true, "manifest": manifest }),
                )?;
            } else {
                println!("Valid: {} ({})", manifest.name, manifest.id);
            }
        }
        Command::Actions { project } => {
            let selected = find_project(&core, &project)?;
            print_value(cli.json, &selected.manifest.actions)?;
        }
        Command::Run { project, action } => {
            let selected = find_project(&core, &project)?;
            let configured = selected
                .manifest
                .action(&action)
                .cloned()
                .with_context(|| {
                    format!(
                        "project '{}' has no action '{}'",
                        selected.manifest.name, action
                    )
                })?;
            if configured.operation.is_some() {
                bail!("operation actions are currently available from the desktop app only");
            }
            let run_id = Uuid::new_v4().to_string();
            let transcript = core
                .data_dir
                .join("transcripts")
                .join(&selected.manifest.id)
                .join(format!("{run_id}.log"));
            if let Some(parent) = transcript.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let transcript_file =
                std::sync::Arc::new(std::sync::Mutex::new(std::fs::File::create(&transcript)?));
            core.history.start_run(
                &run_id,
                &selected.manifest.id,
                &configured.id,
                &configured.label,
                &transcript,
            )?;
            core.history.set_running(&run_id)?;
            tracing::info!(
                subsystem = "runner",
                event = "action.run.started",
                session_id = %run_id,
                status = "running",
                project_id = %selected.manifest.id,
                action_id = %configured.id,
                "[Action] Run started"
            );
            let started = Instant::now();
            let file = transcript_file.clone();
            let result = execute_action(
                &selected.root,
                &selected.manifest,
                &configured,
                |_| {},
                move |output: RunOutput| {
                    let prefix = match output.stream {
                        OutputStream::Stdout => "",
                        OutputStream::Stderr => "[stderr] ",
                        OutputStream::System => "[mallee] ",
                    };
                    println!("{prefix}{}", output.line);
                    if let Ok(mut file) = file.lock() {
                        let _ = writeln!(file, "{prefix}{}", output.line);
                    }
                },
            )
            .await;
            match result {
                Ok(result) => {
                    let status = if result.success { "success" } else { "failed" };
                    core.history.finish_run(
                        &run_id,
                        status,
                        result.duration_ms,
                        result.exit_code,
                    )?;
                    if result.success {
                        tracing::info!(
                            subsystem = "runner",
                            event = "action.run.completed",
                            session_id = %run_id,
                            status = "success",
                            duration_ms = result.duration_ms as f64,
                            exit_code = result.exit_code,
                            project_id = %selected.manifest.id,
                            action_id = %configured.id,
                            "[Action] Run completed"
                        );
                    } else {
                        tracing::error!(
                            subsystem = "runner",
                            event = "action.run.failed",
                            session_id = %run_id,
                            status = "failed",
                            error_kind = "nonzero_exit",
                            duration_ms = result.duration_ms as f64,
                            exit_code = result.exit_code,
                            project_id = %selected.manifest.id,
                            action_id = %configured.id,
                            "[Action] Run failed"
                        );
                    }
                    if cli.json {
                        print_value(true, &result)?;
                    }
                    if result.success {
                        Ok(())
                    } else {
                        bail!("action exited with status {:?}", result.exit_code)
                    }
                }
                Err(error) => {
                    let duration_ms = started.elapsed().as_millis() as i64;
                    core.history
                        .finish_run(&run_id, "failed", duration_ms, None)?;
                    tracing::error!(
                        subsystem = "runner",
                        event = "action.run.failed",
                        session_id = %run_id,
                        status = "failed",
                        error_kind = "execution_error",
                        duration_ms = duration_ms as f64,
                        project_id = %selected.manifest.id,
                        action_id = %configured.id,
                        error = %error,
                        "[Action] Run failed"
                    );
                    Err(error)
                }
            }?
        }
        Command::History { project } => {
            let selected = find_project(&core, &project)?;
            print_value(cli.json, &core.history.list(&selected.manifest.id, 50)?)?;
        }
        Command::Artifacts { project } => {
            let selected = find_project(&core, &project)?;
            print_value(
                cli.json,
                &discover_artifacts(&selected.root, &selected.manifest)?,
            )?;
        }
        Command::Doctor { project } => {
            let projects = core.projects()?;
            let checked: Vec<_> = projects
                .into_iter()
                .filter(|entry| {
                    project
                        .as_ref()
                        .is_none_or(|query| matches_project(entry, query))
                })
                .map(|entry| {
                    let missing: Vec<_> = entry
                        .manifest
                        .actions
                        .iter()
                        .filter_map(|action| action.program.as_ref())
                        .filter(|program| !program_available(program))
                        .cloned()
                        .collect();
                    serde_json::json!({
                        "id": entry.manifest.id,
                        "name": entry.manifest.name,
                        "valid": true,
                        "missingPrograms": missing,
                    })
                })
                .collect();
            print_value(cli.json, &checked)?;
        }
    }
    Ok(())
}

fn find_project(core: &CoreService, query: &str) -> Result<mallee_core::ProjectSummary> {
    core.projects()?
        .into_iter()
        .find(|entry| matches_project(entry, query))
        .with_context(|| format!("no registered project matches '{query}'"))
}

fn matches_project(entry: &mallee_core::ProjectSummary, query: &str) -> bool {
    entry.manifest.id.eq_ignore_ascii_case(query)
        || entry.manifest.name.eq_ignore_ascii_case(query)
        || entry.root.to_string_lossy().eq_ignore_ascii_case(query)
}

fn print_value<T: serde::Serialize + std::fmt::Debug>(json: bool, value: &T) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(value)?);
    } else {
        println!("{value:#?}");
    }
    Ok(())
}

fn canonical_or_original(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn program_available(program: &str) -> bool {
    std::process::Command::new("where.exe")
        .arg(program)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}
