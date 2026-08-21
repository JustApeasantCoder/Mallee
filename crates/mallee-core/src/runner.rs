use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Instant,
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
};

use crate::{Action, ProjectManifest, expand_environment_path};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputStream {
    Stdout,
    Stderr,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunOutput {
    pub stream: OutputStream,
    pub line: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunResult {
    pub exit_code: Option<i32>,
    pub duration_ms: i64,
    pub success: bool,
    pub process_id: u32,
}

pub async fn execute_action<F>(
    root: &Path,
    manifest: &ProjectManifest,
    action: &Action,
    on_spawn: impl FnOnce(u32),
    on_output: F,
) -> Result<RunResult>
where
    F: Fn(RunOutput) + Send + Sync + 'static,
{
    let program = action.program.as_deref().context("action has no program")?;
    let base_working_directory = expand_environment_path(&manifest.project.working_directory, root);
    let working_directory = action
        .working_directory
        .as_deref()
        .map(|value| expand_environment_path(value, root))
        .unwrap_or(base_working_directory);
    if !working_directory.is_dir() {
        bail!(
            "working directory does not exist: {}",
            working_directory.display()
        );
    }

    let started = Instant::now();
    let resolved_program = resolve_program(program, root)
        .with_context(|| format!("program not found: '{program}'"))?;
    let mut command = Command::new(&resolved_program);
    command
        .args(&action.args)
        .current_dir(&working_directory)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(false);

    #[cfg(windows)]
    {
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        command.creation_flags(CREATE_NEW_PROCESS_GROUP);
    }

    let mut child = command
        .spawn()
        .with_context(|| format!("failed to start '{}'", resolved_program.display()))?;
    let process_id = child
        .id()
        .context("spawned process did not expose a process id")?;
    on_spawn(process_id);

    let on_output = Arc::new(on_output);
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let stdout_task = stdout.map(|stdout| {
        let callback = Arc::clone(&on_output);
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                callback(RunOutput {
                    stream: OutputStream::Stdout,
                    line,
                });
            }
        })
    });
    let stderr_task = stderr.map(|stderr| {
        let callback = Arc::clone(&on_output);
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                callback(RunOutput {
                    stream: OutputStream::Stderr,
                    line,
                });
            }
        })
    });

    let status = if let Some(timeout_seconds) = action.timeout_seconds {
        match tokio::time::timeout(
            std::time::Duration::from_secs(timeout_seconds),
            child.wait(),
        )
        .await
        {
            Ok(result) => result?,
            Err(_) => {
                let _ = child.kill().await;
                bail!("action timed out after {timeout_seconds} seconds");
            }
        }
    } else {
        child.wait().await?
    };

    if let Some(task) = stdout_task {
        let _ = task.await;
    }
    if let Some(task) = stderr_task {
        let _ = task.await;
    }

    Ok(RunResult {
        exit_code: status.code(),
        duration_ms: started.elapsed().as_millis() as i64,
        success: status.success(),
        process_id,
    })
}

fn resolve_program(program: &str, root: &Path) -> Option<PathBuf> {
    let configured = PathBuf::from(program);
    if configured.is_absolute() && configured.is_file() {
        return Some(configured);
    }
    if program.contains('/') || program.contains('\\') {
        let candidate = root.join(configured);
        return candidate.is_file().then_some(candidate);
    }
    #[cfg(windows)]
    {
        let output = std::process::Command::new("where.exe")
            .arg(program)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(PathBuf::from)
            .find(|path| is_windows_launchable(path))
    }
    #[cfg(not(windows))]
    {
        Some(configured)
    }
}

#[cfg(windows)]
fn is_windows_launchable(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some(extension)
            if extension.eq_ignore_ascii_case("exe")
                || extension.eq_ignore_ascii_case("com")
                || extension.eq_ignore_ascii_case("bat")
                || extension.eq_ignore_ascii_case("cmd")
    )
}

#[cfg(all(test, windows))]
mod tests {
    use std::path::Path;

    use super::is_windows_launchable;

    #[test]
    fn accepts_native_executables_and_command_shims() {
        assert!(is_windows_launchable(Path::new("pnpm.cmd")));
        assert!(is_windows_launchable(Path::new("cargo.exe")));
        assert!(!is_windows_launchable(Path::new("pnpm")));
        assert!(!is_windows_launchable(Path::new("pnpm.ps1")));
    }
}
