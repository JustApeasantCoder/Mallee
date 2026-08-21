use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use regex::Regex;

pub fn app_data_dir() -> Result<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")
        .context("LOCALAPPDATA is not set; Mallee cannot resolve its data directory")?;
    Ok(PathBuf::from(local).join("Mallee"))
}

pub fn expand_environment_path(value: &str, root: &Path) -> PathBuf {
    let pattern = Regex::new(r"%([A-Za-z_][A-Za-z0-9_]*)%").expect("valid regex");
    let expanded = pattern.replace_all(value, |captures: &regex::Captures<'_>| {
        std::env::var(&captures[1]).unwrap_or_else(|_| captures[0].to_string())
    });
    let path = PathBuf::from(expanded.replace('/', "\\"));
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

pub fn canonical_project_root(path: impl AsRef<Path>) -> Result<PathBuf> {
    let path = path.as_ref();
    if !path.exists() {
        bail!("project path does not exist: {}", path.display());
    }
    let root = path
        .canonicalize()
        .with_context(|| format!("failed to resolve project path: {}", path.display()))?;
    #[cfg(windows)]
    let root = without_verbatim_prefix(root);
    if !root.join(".mallee").join("project.toml").is_file() {
        bail!("no .mallee/project.toml found under {}", root.display());
    }
    Ok(root)
}

#[cfg(windows)]
fn without_verbatim_prefix(path: PathBuf) -> PathBuf {
    let value = path.to_string_lossy();
    if let Some(unc) = value.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{unc}"));
    }
    value
        .strip_prefix(r"\\?\")
        .map(PathBuf::from)
        .unwrap_or(path)
}
