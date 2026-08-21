use std::path::{Path, PathBuf};

/// Finds a project-owned application icon without walking generated output or
/// unrelated source trees. A `.mallee/icon.*` file is an explicit override.
pub fn find_project_icon(root: &Path) -> Option<PathBuf> {
    const CANDIDATES: &[&str] = &[
        ".mallee/icon.ico",
        ".mallee/icon.png",
        ".mallee/icon.svg",
        "src-tauri/icons/icon.ico",
        "src-tauri/icons/icon.png",
        "src-tauri/icons/128x128.png",
        "apps/desktop/src-tauri/icons/icon.ico",
        "apps/desktop/src-tauri/icons/icon.png",
        "apps/desktop/src-tauri/icons/128x128.png",
        "resources/icon.ico",
        "resources/icon.png",
        "assets/icon.ico",
        "assets/icon.png",
        "public/icon.svg",
        "public/icon.png",
        "icon.ico",
        "icon.png",
        "icon.svg",
    ];

    CANDIDATES
        .iter()
        .map(|candidate| root.join(candidate))
        .find(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::find_project_icon;

    #[test]
    fn explicit_mallee_icon_wins_over_framework_icon() {
        let directory = tempfile::tempdir().unwrap();
        let override_icon = directory.path().join(".mallee/icon.png");
        let tauri_icon = directory.path().join("src-tauri/icons/icon.ico");
        fs::create_dir_all(override_icon.parent().unwrap()).unwrap();
        fs::create_dir_all(tauri_icon.parent().unwrap()).unwrap();
        fs::write(&override_icon, []).unwrap();
        fs::write(&tauri_icon, []).unwrap();

        assert_eq!(find_project_icon(directory.path()), Some(override_icon));
    }

    #[test]
    fn returns_none_when_no_known_icon_exists() {
        let directory = tempfile::tempdir().unwrap();
        assert_eq!(find_project_icon(directory.path()), None);
    }
}
