# Mallee Project Plan

## 1. Product definition

Mallee is a native Windows developer project manager. It is installed once and
provides one place to discover local applications, run their declared workflows,
watch terminal output, inspect prior runs and artifacts, open project/log folders,
and launch DeeBugee for structured diagnostics.

Mallee does not infer how a project should build or release at execution time.
Each repository declares its supported actions in a committed
`.mallee/project.toml` manifest. The desktop app and CLI execute the same manifest
through the same Rust core.

### Product principles

- Install Mallee once; bootstrap each repository with a small committed manifest.
- Keep all actions explicit, visible, editable, and project-scoped.
- Give every command an honest live state: queued, starting, running, stopping,
  succeeded, failed, cancelled, or timed out.
- Keep personal state, history, terminal transcripts, and UI preferences out of
  repositories.
- Use rectangular, information-dense native-tooling UI: 0-3 px corner radii,
  thin dividers, restrained color, and no decorative dashboard charts.
- The Rust backend owns files, child processes, PTYs, and logging. The renderer
  receives constrained commands and events; it never receives arbitrary file
  access.

## 2. Confirmed technology stack

### Desktop

- Tauri 2 application shell
- React, TypeScript, and Vite for the UI
- xterm.js for terminal rendering
- Zustand for small client-side UI state
- TanStack Query for backend queries and invalidation

### Rust

- A Cargo workspace shared by the desktop application and CLI
- `clap` for the CLI
- `serde` and `toml` for manifests
- `tokio` for asynchronous process and event handling
- `portable-pty` for a Windows ConPTY-backed interactive terminal
- Windows Job Objects for reliable child-process-tree termination
- SQLite through `rusqlite` for local run history and artifact metadata
- `tracing` plus `dee-bugee-rust` for Mallee's structured diagnostics

The core runner will be custom Rust code rather than a renderer-facing generic
shell plugin. This keeps validation, lifecycle, cancellation, output capture, and
history consistent between the desktop app and CLI.

## 3. Repository shape

```text
Mallee/
  Cargo.toml
  package.json
  apps/
    desktop/
      src/                    React UI
      src-tauri/              Tauri host
  crates/
    mallee-core/              application services and shared types
    mallee-manifest/          parse, validate, normalize, migrate
    mallee-runner/            PTY/process sessions and cancellation
    mallee-store/             registry, history, artifacts, settings
    mallee-logging/           DeeBugee-compatible tracing setup
    mallee-cli/               `mallee` executable
  schemas/
    project-v1.schema.json
  examples/
    project.toml
  docs/
```

The CLI and Tauri commands remain thin adapters over the core crates. Business
logic must not be duplicated in TypeScript or the CLI entry point.

## 4. Project manifest

The canonical path is `.mallee/project.toml` (two Ls, matching the product name).
It is committed to the repository. Machine-specific absolute paths should be
avoided; relative paths resolve from the repository root and selected environment
variables may be expanded.

### Proposed v1 example

```toml
schema_version = 1
id = "streamee"
name = "Streamee"
description = "Tauri streaming desktop application"

[project]
working_directory = "."

[logs]
sources = ["%TEMP%/streamee_logs/Streamee.jsonl"]
open_with_deebugee = true

[artifacts]
paths = [
  "src-tauri/target/release/bundle/msi/*.msi",
  "src-tauri/target/release/bundle/nsis/*.exe",
]

[[actions]]
id = "dev"
label = "Dev Server"
program = "pnpm"
args = ["tauri", "dev"]
kind = "long_running"
terminal = "interactive"
concurrency = "replace_same_action"

[[actions]]
id = "build"
label = "Build"
program = "pnpm"
args = ["tauri", "build", "--no-bundle"]
kind = "task"
terminal = "captured"

[[actions]]
id = "installer"
label = "Build Installer"
program = "pnpm"
args = ["tauri", "build"]
kind = "task"
terminal = "captured"

[[actions]]
id = "run"
label = "Run App"
program = "src-tauri/target/release/streamee.exe"
kind = "long_running"
terminal = "captured"

[[actions]]
id = "open_logs"
label = "Open Logs"
operation = "open_log_folder"

[[actions]]
id = "open_project"
label = "Open Project"
operation = "open_project_folder"
```

### Manifest rules

- Action IDs are stable, unique, lowercase identifiers.
- `program` and `args` are separate by default; Mallee does not parse a shell
  command string.
- An explicitly declared `runner = "shell"` may support commands that genuinely
  require PowerShell or cmd, and the UI must mark them as shell actions.
- Per-action options cover working directory, safe environment overrides,
  terminal mode, timeout, concurrency, confirmation, and expected artifacts.
- Secrets are never stored in the manifest. Actions inherit the user's process
  environment or reference named environment variables without recording values.
- Unknown manifest versions and invalid actions fail validation before execution.
- Mallee displays the resolved program, arguments, and working directory before
  the first execution of a newly added or changed action.

### Script discovery

Mallee recognizes PowerShell scripts, but it does not treat every `.ps1` file in
a repository as a runnable action. Detection proposes candidates; only actions
accepted into `.mallee/project.toml` are executable from Mallee.

Candidate discovery uses this order:

1. Existing actions in `.mallee/project.toml` — always authoritative.
2. Mallee-owned scripts under `.mallee/scripts/*.ps1`.
3. Existing project scripts in the repository root and conventional `scripts/`,
   `tools/`, and `build/` directories.
4. Declared commands from `package.json`, Cargo metadata, .NET solution/project
   files, Tauri configuration, `Makefile`, `justfile`, and `Taskfile` when present.
5. `.cmd` and `.bat` candidates when they are already part of the project.

Discovery is shallow and excludes dependency, generated, output, cache, and VCS
directories. Mallee shows the source, resolved command, and confidence/reason for
each candidate. It never executes, imports, or rewrites a detected script without
the user accepting it.

PowerShell is the preferred format for newly generated multi-step Windows
workflows such as release preparation or installer assembly. Simple commands such
as `cargo test` or `pnpm tauri dev` stay as direct manifest actions; generating a
wrapper for every command would add indirection without value.

Generated Mallee-specific scripts live under `.mallee/scripts/` and are committed
with the manifest. They:

- target PowerShell 7 (`pwsh`) by default, with the runtime checked by
  `mallee doctor`;
- use `Set-StrictMode -Version Latest` and `$ErrorActionPreference = "Stop"`;
- resolve the repository root from `$PSScriptRoot` rather than hard-coded paths;
- use explicit parameters and native argument arrays where practical;
- propagate native nonzero exit codes instead of reporting false success;
- avoid secrets, private environment values, interactive prompts in unattended
  tasks, and destructive cleanup outside explicitly resolved project paths;
- print useful command progress to the terminal while leaving structured Mallee
  lifecycle logging to the Rust runner;
- declare publishing, signing, deletion, or other high-impact behavior clearly so
  the corresponding manifest action can require confirmation.

## 5. Desktop experience

### Sidebar

The sidebar is the project switcher, not a miniature dashboard. It shows:

- registered projects;
- selected-project state;
- a small status indicator when that project has a running action;
- Add Project, Settings, and Mallee diagnostics at the bottom.

### Selected-project dashboard

The main dashboard is always scoped to the selected project.

1. **Project header** — name, repository path, branch when available, manifest
   health, Open Project, and Edit Manifest.
2. **Actions** — rectangular action tiles generated in manifest order. Each tile
   shows its label, command summary, current state, and latest result. An
   **Add Action** control opens a form and writes a valid manifest update after a
   preview.
3. **Current Terminal** — one prominent xterm.js surface showing the selected
   run. Tabs may appear when the same project has multiple allowed concurrent
   actions. It supports ANSI colors, resize, copy, search, clear, input for PTY
   sessions, and Stop/Restart.
4. **History** — prior runs for this project only, including action, start time,
   duration, result, exit code, and Run Again. Selecting a row opens its saved
   transcript and produced artifacts.
5. **Artifacts** — files discovered from this project's configured artifact
   paths, with Open, Open Folder, Copy Path, size, and modified time.
6. **DeeBugee** — a compact status/action strip showing whether configured log
   sources exist and an **Open in DeeBugee** action. Mallee does not duplicate the
   DeeBugee viewer inside its dashboard.

### Empty and error states

- No manifest: explain the bootstrap command and offer to initialize one.
- Invalid manifest: show exact file/field errors and disable execution.
- No history/artifacts/logs: show a compact factual empty state.
- Missing executable/tool: report the resolved program and offer `mallee doctor`.
- Running action after UI restart: reconcile against the backend session registry
  rather than presenting stale state.

## 6. Terminal and process model

Each action execution creates a durable run record and a runtime session ID.

```text
manifest action
  -> validation and resolved execution preview
  -> create run record
  -> spawn process/ConPTY inside a Windows Job Object
  -> stream output bytes to xterm.js and transcript storage
  -> observe exit, stop, cancellation, timeout, or spawn failure
  -> discover artifacts
  -> finalize history and structured diagnostic events
```

Interactive actions use ConPTY, so tools detect a terminal and retain colors and
prompts. Captured actions use piped stdout/stderr when interactivity is not needed.
Output transport is byte-oriented and sequenced; the UI must not reorder stdout
and stderr. Terminal buffers are bounded in memory while complete transcripts are
written locally with a configurable retention policy.

Stopping an action terminates its full Job Object process tree after a graceful
attempt and short timeout. Mallee must never leave hidden child dev servers after
reporting that an action stopped.

## 7. Local persistence

Per-user data belongs under `%LOCALAPPDATA%\Mallee`:

```text
%LOCALAPPDATA%\Mallee\
  registry.toml               registered repository roots
  mallee.db                   run history and artifact metadata
  logs\Mallee.jsonl           Mallee's DeeBugee v1 events
  transcripts\<project>\     retained command output
  settings.toml               UI and retention preferences
```

History is per project and keyed by the manifest's stable project ID plus its
canonical repository path. Repository manifests never accumulate personal run
history. Transcript and database retention are configurable by age and count.

## 8. CLI

The `mallee` CLI exposes the same operations as the desktop app:

```text
mallee init [path]
mallee add <path>
mallee list
mallee show [project]
mallee actions [project]
mallee run <project> <action>
mallee stop <project> [run-id]
mallee history <project>
mallee artifacts <project>
mallee logs <project>
mallee open <project>
mallee doctor [project]
mallee validate [path]
mallee ui [project]
```

Commands support human-readable output and `--json` for skills and automation.
Exit codes are stable and documented. `run` attaches to terminal output by
default; `--detach` returns a run ID.

## 9. DeeBugee diagnostics

Mallee writes append-only DeeBugee v1 JSONL directly. It does not introduce a
logging HTTP service, listener, or database pipeline.

- One stable `app_session_id` is created at Mallee startup.
- Every action run receives a `session_id` used across accepted, started,
  output-health, completed, failed, cancelled, and artifact-discovery events.
- Stable event names include `action.run.accepted`, `action.run.started`,
  `action.run.completed`, `action.run.failed`, `action.run.cancelled`,
  `manifest.validation.failed`, and `artifact.discovery.completed`.
- `status`, `duration_ms`, and `error_kind` are promoted where applicable.
- Command output is not blindly duplicated into structured JSONL. Mallee logs
  lifecycle facts and safe summaries; terminal transcripts remain separate.
- Arguments, environment variables, exception text, and output are reviewed and
  redacted so credentials, tokens, cookies, private keys, and sensitive URLs are
  never logged.

Mallee's repository will also contain `.deebugee/project.toml`, pointing to
`%LOCALAPPDATA%\Mallee\logs\Mallee.jsonl`. The viewer remains one shared portable
installation per developer.

For a selected external project, **Open in DeeBugee** launches project mode when
`.deebugee/project.toml` exists; otherwise it opens the log sources declared in
`.mallee/project.toml` directly.

## 10. Bootstrap and script-generation skill

After the manifest and CLI contracts stabilize, create a `mallee-project-setup`
skill. Script generation is a mode of this one skill rather than a separate broad
PowerShell skill, because the skill needs repository and manifest context to
produce safe, useful actions. Its responsibilities are deliberately constrained:

- inspect a repository's existing scripts, build tools, executable outputs, and
  real log locations;
- run `mallee init`, propose a manifest, and validate it;
- detect existing `.ps1`, package, Cargo, .NET, Tauri, Make, Just, Task, batch, and
  command workflows without treating detection as execution permission;
- reuse an existing project script when it already expresses the workflow;
- generate `.mallee/scripts/<action>.ps1` when a workflow needs multi-step Windows
  orchestration, then reference it explicitly from the manifest;
- generate direct manifest actions for simple commands that do not need wrappers;
- use `mallee doctor --json` to report missing tools or paths;
- list and invoke declared actions by ID;
- open the repository, artifacts, or configured logs;
- never invent release commands, expose secrets, or silently execute a newly
  generated destructive/publishing action.

The skill previews every proposed manifest and generated script, validates the
result, and exercises a safe non-publishing action when appropriate. Release,
signing, publishing, installer deployment, and destructive actions require the
user's explicit authorization before the skill runs them.

The skill bootstraps a project; it does not copy Mallee or DeeBugee executables
into that repository.

## 11. Delivery phases

### Phase 1 — Foundation and contracts

- Scaffold the Cargo workspace, Tauri app, React UI, and CLI.
- Implement project manifest v1 types, schema, parser, validation, and fixtures.
- Implement registry storage and project discovery.
- Add Mallee's DeeBugee-compatible logging at startup.
- Establish CI checks: format, Clippy, Rust tests, TypeScript checks, and frontend
  tests.

**Exit:** a repository can be initialized, registered, validated, listed in the
sidebar, and inspected from both CLI and desktop.

### Phase 2 — Runner and terminal

- Implement captured and ConPTY execution modes.
- Stream terminal output with ordering, resize, input, and bounded buffering.
- Implement Windows Job Object lifecycle and reliable Stop.
- Add concurrency, timeout, cancellation, and restart policies.
- Persist run records and transcripts.

**Exit:** Dev Server, Build, and a failing command can be run, observed, stopped,
and revisited without orphan processes or false success states.

### Phase 3 — Project dashboard

- Build the sidebar and selected-project header.
- Render manifest-driven action tiles and live states.
- Build terminal, per-project history, and Run Again.
- Implement manifest-aware Add/Edit Action flow with validation preview.
- Validate wide and compact window layouts.

**Exit:** the approved Command Grid workflow is complete and usable without the
CLI.

### Phase 4 — Artifacts, logs, and integrations

- Discover and persist configured artifacts.
- Add project/log/artifact folder actions.
- Add DeeBugee project/direct-log launch behavior.
- Add missing-path diagnostics and `mallee doctor`.
- Exercise log rotation, replacement, and live-reader compatibility.

**Exit:** a project's build output and diagnostics can be reached from one screen.

### Phase 5 — CLI parity and bootstrap skill

- Complete CLI parity and stable JSON output.
- Add manifest migration support before changing v1.
- Create and test the `mallee-project-setup` skill, including `.ps1` detection and
  generation, against representative Tauri/Rust, Node, and .NET repositories.
- Document authoring, automation, and safety behavior.

**Exit:** a new repository can be inspected, bootstrapped, validated, and operated
through either UI or CLI without copying binaries.

### Phase 6 — Packaging and release

- Build the Mallee installer and portable CLI distribution.
- Add versioning, upgrade, and rollback behavior.
- Test clean-machine first run, paths with spaces, missing runtimes, Unicode
  output, large output, process crashes, sleep/resume, and abrupt Mallee exit.
- Perform visual QA at wide and compact window sizes.
- Validate fresh Mallee JSONL with DeeBugee and the schema validator.

**Exit:** Mallee installs once, survives real Windows development workflows, and
can diagnose its own failures.

## 12. MVP boundary

The first shippable release includes:

- manual project registration and `.mallee/project.toml`;
- manifest-driven custom actions;
- captured and interactive terminal execution;
- truthful start/stop/result state;
- per-project history and transcripts;
- artifact discovery;
- Open Project, Open Logs, and Open in DeeBugee;
- CLI parity for init, list, actions, run, stop, history, logs, doctor, and
  validate;
- candidate detection for existing `.ps1` and common project task definitions;
- safe `.mallee/scripts/*.ps1` generation through the setup skill when a
  multi-step workflow needs it;
- Mallee's own DeeBugee v1 structured log;
- an installer for Windows.

Deferred until after the MVP:

- automatic filesystem-wide repository scanning;
- remote execution and cloud sync;
- team/shared history;
- scheduling and dependency graphs between actions;
- plugin marketplace;
- embedded source control UI;
- parsing terminal prose into structured diagnostics;
- macOS/Linux support beyond keeping core abstractions portable.

## 13. Acceptance criteria

The MVP is ready when all of the following are demonstrated on a clean Windows
machine:

1. Install Mallee and its CLI once.
2. Bootstrap and register at least three representative repositories.
3. Render exactly the actions and order declared in each manifest.
4. Run a long-lived dev action, interact with its terminal, and stop its entire
   process tree.
5. Run successful and failing build/installer actions and record accurate exit
   state, duration, transcript, and artifacts.
6. Switch projects without mixing actions, history, artifacts, or terminal state.
7. Re-run a historical action and open its prior transcript.
8. Open configured project/log folders and launch the correct log sources in
   DeeBugee.
9. Restart Mallee without losing registry/history and without showing stale
   running state.
10. Validate Mallee's fresh JSONL as complete DeeBugee v1 records with no secrets,
    then verify live tailing, filtering, correlation, rotation, and shutdown flush.

## 14. First implementation slice

Begin with the smallest end-to-end vertical slice:

1. Scaffold the Cargo/Tauri/React workspace.
2. Define and test manifest v1.
3. Register one example project.
4. Render its sidebar entry and manifest actions.
5. Execute one captured action and stream it into xterm.js.
6. Persist and display its result in per-project history.
7. Emit and validate the corresponding DeeBugee lifecycle events.

Only after this slice is verified should interactive ConPTY sessions, Add Action,
artifacts, and packaging be layered on.
