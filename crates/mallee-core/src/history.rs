use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRecord {
    pub id: String,
    pub project_id: String,
    pub action_id: String,
    pub action_label: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub duration_ms: Option<i64>,
    pub status: String,
    pub exit_code: Option<i32>,
    pub transcript_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct HistoryStore {
    path: PathBuf,
}

impl HistoryStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn initialize(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let connection = self.connection()?;
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS runs (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                action_id TEXT NOT NULL,
                action_label TEXT NOT NULL,
                started_at TEXT NOT NULL,
                finished_at TEXT,
                duration_ms INTEGER,
                status TEXT NOT NULL,
                exit_code INTEGER,
                transcript_path TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_runs_project_started
                ON runs(project_id, started_at DESC);",
        )?;
        Ok(())
    }

    pub fn start_run(
        &self,
        id: &str,
        project_id: &str,
        action_id: &str,
        action_label: &str,
        transcript_path: &Path,
    ) -> Result<RunRecord> {
        let record = RunRecord {
            id: id.to_string(),
            project_id: project_id.to_string(),
            action_id: action_id.to_string(),
            action_label: action_label.to_string(),
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            duration_ms: None,
            status: "starting".into(),
            exit_code: None,
            transcript_path: Some(transcript_path.to_path_buf()),
        };
        self.connection()?.execute(
            "INSERT INTO runs
             (id, project_id, action_id, action_label, started_at, status, transcript_path)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                record.id,
                record.project_id,
                record.action_id,
                record.action_label,
                record.started_at,
                record.status,
                transcript_path.to_string_lossy(),
            ],
        )?;
        Ok(record)
    }

    pub fn set_running(&self, id: &str) -> Result<()> {
        self.connection()?.execute(
            "UPDATE runs SET status = 'running' WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn finish_run(
        &self,
        id: &str,
        status: &str,
        duration_ms: i64,
        exit_code: Option<i32>,
    ) -> Result<()> {
        self.connection()?.execute(
            "UPDATE runs
             SET finished_at = ?2, duration_ms = ?3, status = ?4, exit_code = ?5
             WHERE id = ?1",
            params![id, Utc::now().to_rfc3339(), duration_ms, status, exit_code],
        )?;
        Ok(())
    }

    pub fn list(&self, project_id: &str, limit: usize) -> Result<Vec<RunRecord>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, project_id, action_id, action_label, started_at, finished_at,
                    duration_ms, status, exit_code, transcript_path
             FROM runs WHERE project_id = ?1
             ORDER BY started_at DESC LIMIT ?2",
        )?;
        let records = statement
            .query_map(params![project_id, limit as i64], |row| {
                let transcript: Option<String> = row.get(9)?;
                Ok(RunRecord {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    action_id: row.get(2)?,
                    action_label: row.get(3)?,
                    started_at: row.get(4)?,
                    finished_at: row.get(5)?,
                    duration_ms: row.get(6)?,
                    status: row.get(7)?,
                    exit_code: row.get(8)?,
                    transcript_path: transcript.map(PathBuf::from),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(records)
    }

    fn connection(&self) -> Result<Connection> {
        Connection::open(&self.path)
            .with_context(|| format!("failed to open history database {}", self.path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_project_scoped_history() {
        let directory = tempfile::tempdir().unwrap();
        let store = HistoryStore::new(directory.path().join("history.db"));
        store.initialize().unwrap();
        store
            .start_run("run-1", "alpha", "build", "Build", Path::new("run.log"))
            .unwrap();
        store.set_running("run-1").unwrap();
        store.finish_run("run-1", "success", 42, Some(0)).unwrap();
        assert_eq!(store.list("alpha", 20).unwrap().len(), 1);
        assert!(store.list("beta", 20).unwrap().is_empty());
    }
}
