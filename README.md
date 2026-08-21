# Mallee

Mallee is a native Windows command center for local application projects. Each
repository declares its actions, logs, and artifacts in `.mallee/project.toml`;
the desktop app and `mallee` CLI execute the same manifest through a shared Rust
core.

The current implementation includes:

- Tauri 2 + React desktop dashboard;
- rectangular project sidebar and manifest-driven action grid;
- captured stdout/stderr terminal output with cancellable process trees;
- per-project SQLite run history and retained transcripts;
- artifact discovery and project/log folder operations;
- format-preserving Add Action workflow with manifest validation and backup;
- `.ps1`, package script, and Cargo action detection;
- CLI project initialization, validation, execution, history, artifacts, and
  diagnostics;
- direct DeeBugee v1 JSONL diagnostics under
  `%LOCALAPPDATA%\Mallee\logs\Mallee.jsonl`.

## Development

Requirements: Rust stable MSVC, Node.js, pnpm, and the Tauri 2 Windows
prerequisites.

```powershell
pnpm --dir apps/desktop install
pnpm --dir apps/desktop tauri dev
```

Run the checks:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm --dir apps/desktop build
```

Build the native executable without packaging:

```powershell
pnpm --dir apps/desktop tauri build --no-bundle
```

## CLI

```powershell
cargo run -p mallee-cli -- init C:\path\to\project
cargo run -p mallee-cli -- detect C:\path\to\project --json
cargo run -p mallee-cli -- add C:\path\to\project
cargo run -p mallee-cli -- actions <project-id>
cargo run -p mallee-cli -- run <project-id> <action-id>
cargo run -p mallee-cli -- history <project-id>
cargo run -p mallee-cli -- doctor <project-id> --json
```

Detection only proposes candidates. Mallee executes actions only after they are
present in the committed project manifest. Newly detected PowerShell workflows
target `pwsh` and are expected to live under `.mallee/scripts/` when they are
Mallee-specific.

See [PROJECT_PLAN.md](PROJECT_PLAN.md) for architecture, phases, and acceptance
criteria.

## Agent skill

Mallee's repository-onboarding workflow is available as the
[`mallee-project-setup`](skills/mallee-project-setup/SKILL.md) agent skill.
Install it with the Skills CLI:

```powershell
npx skills add JustApeasantCoder/Mallee --skill mallee-project-setup
```

[![skills.sh](https://skills.sh/b/JustApeasantCoder/Mallee)](https://skills.sh/JustApeasantCoder/Mallee)
