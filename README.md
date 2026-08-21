# Mallee

Mallee is a native Windows command center for local application projects. It
gives each repository a small, committed manifest that declares the commands it
is allowed to run, the build artifacts it produces, and the logs it owns. The
Tauri desktop app and the `mallee` CLI use the same Rust core, so a project has
one source of truth whichever interface you use.

Use Mallee when you want the routine work for several local repositories—start,
build, test, package, inspect output, and review run history—in one predictable
place without a collection of untracked helper scripts.

## What it does

- Registers projects containing `.mallee/project.toml`.
- Executes only actions explicitly declared in that manifest.
- Captures command output, exit status, duration, and a retained transcript for
  every CLI or desktop run.
- Stops running desktop actions, including their process trees.
- Lists files matched by each project's artifact globs and opens their files or
  folders from the desktop app.
- Detects candidate PowerShell, npm/pnpm, and Cargo workflows; candidates must
  still be reviewed and added to the manifest before Mallee can execute them.
- Adds, reorders, removes, and updates action cards from the desktop app while
  preserving the rest of the TOML document where possible. A backup is made
  before the Add Action workflow writes the manifest.
- Supports project and log-folder actions, plus optional opening of configured
  JSONL logs in DeeBugee.

Mallee is Windows-focused: the desktop app uses native Windows paths and
Explorer, and PowerShell actions normally use PowerShell 7 (`pwsh`).

## Agent skill

For AI-assisted repository onboarding, install the
[`mallee-project-setup`](skills/mallee-project-setup/SKILL.md) agent skill. It
inspects a repository, proposes and validates a focused manifest, registers the
project, and checks its declared commands.

```powershell
npx skills add JustApeasantCoder/Mallee --skill mallee-project-setup
```

[![skills.sh](https://skills.sh/b/JustApeasantCoder/Mallee)](https://skills.sh/JustApeasantCoder/Mallee)

## Quick start

### 1. Build or run the CLI

From a checkout of this repository, run commands through Cargo:

```powershell
cargo run -p mallee-cli -- --help
```

For regular use, install the CLI so `mallee` is available on your `PATH`:

```powershell
cargo install --path crates/mallee-cli
mallee --help
```

### 2. Create a project manifest

Initialize a repository. This creates `.mallee/project.toml`, detects action
candidates, and registers the project locally.

```powershell
mallee init C:\path\to\project
```

Review the generated manifest before running anything. Detection is advisory;
the manifest is the execution allowlist. A minimal explicit example is:

```toml
schema_version = 1
id = "sample-app"
name = "Sample App"
description = "A local application managed by Mallee"

[project]
working_directory = "."

[artifacts]
paths = ["target/release/*.exe"]

[[actions]]
id = "test"
label = "Run Tests"
program = "cargo"
args = ["test"]
kind = "task"
terminal = "captured"
concurrency = "reject"
```

Commit `.mallee/project.toml` with the project. It describes intentional,
shareable developer workflows. Keep credentials, machine-specific paths, and
personal working state out of it.

### 3. Validate and register an existing manifest

For a manifest you created or edited yourself:

```powershell
mallee validate C:\path\to\project
mallee add C:\path\to\project
mallee list
```

`add` is safe to repeat. It resolves the repository path, validates the
manifest, and adds it to Mallee's local registry only if it is not already
present.

### 4. Run an action

Use the project ID, project name, or registered absolute project path:

```powershell
mallee actions sample-app
mallee run sample-app test
mallee history sample-app
mallee artifacts sample-app
mallee doctor sample-app
```

Add `--json` to commands when another tool or script needs structured output:

```powershell
mallee --json actions sample-app
mallee --json doctor sample-app
```

The CLI runs program actions. Desktop-only folder operations are intentionally
not available through `mallee run`.

## Desktop app

Start the development app from this repository:

```powershell
pnpm --dir apps/desktop install
pnpm --dir apps/desktop tauri dev
```

In the app, select **Add Project** and enter the root folder that contains a
valid `.mallee/project.toml`. The dashboard displays the manifest's action
cards, captured terminal output, recent history, and discovered artifacts.

- Click an action card to run it. Click a running card to stop it.
- Use **Add Action** to append a validated command action to the manifest.
- Drag project and action cards to persist their display order.
- Right-click an action to change its displayed title or glyph; these are
  presentation-only changes to the action entry.
- Select a history entry to reopen its transcript; use the artifact controls to
  open a file or its containing folder.
- If the project enables DeeBugee integration, use its log control to open the
  configured log sources in DeeBugee.

Commands marked `confirm = true` require confirmation in the desktop app.
Use this for actions with meaningful side effects such as publishing, deploying,
or installing.

## Project manifest

The canonical manifest is `.mallee/project.toml`. See the
[JSON Schema](schemas/project-v1.schema.json) and the ready-to-copy
[example manifest](examples/project.toml) for reference.

### Project settings

| Field | Required | Meaning |
| --- | --- | --- |
| `schema_version` | Yes | Must be `1`. |
| `id` | Yes | Stable lowercase project identifier: letters, digits, and hyphens only. |
| `name` | Yes | Display name. |
| `description` | No | Short project description. |
| `project.working_directory` | No | Base directory for actions; defaults to `.`. Relative paths resolve from the repository root. |
| `logs.sources` | No | Log file paths. `%ENVIRONMENT_VARIABLE%` values are expanded. |
| `logs.open_with_deebugee` | No | Enables the desktop DeeBugee action for configured log sources. |
| `artifacts.paths` | No | File globs relative to the repository root, or absolute paths. |

### Action settings

Every action requires an `id`, a non-empty `label`, and exactly one of `program`
or `operation`.

| Field | Meaning |
| --- | --- |
| `program` | Executable to run. Keep it separate from arguments. |
| `args` | Argument array passed to `program`. |
| `operation` | Desktop-only native action: `open_project_folder` or `open_log_folder`. |
| `working_directory` | Per-action directory overriding `project.working_directory`. |
| `kind` | `task` (default) or `long_running`. |
| `terminal` | `captured` (default), `interactive`, or `hidden`. |
| `concurrency` | `allow` (default), `reject`, or `replace_same_action`. Use `replace_same_action` for a dev server. |
| `timeout_seconds` | Optional execution limit, in seconds. |
| `confirm` | Requires desktop confirmation before execution. |
| `icon` | Optional desktop presentation glyph; it does not change execution. |

For a Mallee-specific multi-step PowerShell workflow, store the script under
`.mallee/scripts/` and call it explicitly. For example:

```toml
[[actions]]
id = "build-installer"
label = "Build Installer"
program = "pwsh"
args = ["-NoLogo", "-NoProfile", "-File", ".mallee/scripts/build-installer.ps1"]
kind = "task"
terminal = "captured"
confirm = true
```

Do not create a wrapper script for a simple command such as `cargo test`.
Before adding an action, make sure its executable is available. `mallee doctor`
reports missing programs for registered projects.

## CLI reference

| Command | Purpose |
| --- | --- |
| `mallee init [path] [--name <name>]` | Create a manifest from detected candidates and register the project. The path defaults to the current directory. |
| `mallee detect [path]` | List detected action candidates without writing a manifest. |
| `mallee validate <path>` | Parse and validate a repository manifest. |
| `mallee add <path>` | Validate and register a repository. Idempotent. |
| `mallee list` | List registered, valid projects. |
| `mallee actions <project>` | List actions for a registered project. |
| `mallee run <project> <action>` | Run a declared program action and record its result. |
| `mallee history <project>` | Show the 50 most recent recorded runs. |
| `mallee artifacts <project>` | List files matching configured artifact paths. |
| `mallee doctor [project]` | Check registered projects for action programs unavailable on `PATH`. |

`<project>` matches an ID, a project name, or the registered absolute path.

## Local data and diagnostics

Mallee stores local, uncommitted application state under
`%LOCALAPPDATA%\Mallee`:

| Path | Contents |
| --- | --- |
| `registry.toml` | Registered project roots and their display order. |
| `mallee.db` | SQLite run history. |
| `transcripts\<project-id>\<run-id>.log` | Captured output for each run. |
| `logs\Mallee.jsonl` | Mallee's own DeeBugee-compatible JSONL diagnostics. |

The project manifest stays in the repository; the registry, history,
transcripts, and diagnostics remain local to the Windows user profile.

## Development

Requirements:

- Windows with the [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/)
  installed;
- Rust stable with the MSVC toolchain;
- Node.js and pnpm.

Run the full checks from the repository root:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm --dir apps/desktop build
```

Build the desktop executable without producing an installer bundle:

```powershell
pnpm --dir apps/desktop tauri build --no-bundle
```

`BumpVersion.ps1` increments the patch version consistently in the Cargo
workspace, desktop `package.json`, and Tauri configuration. Run it only when
you intend to change the release version:

```powershell
.\BumpVersion.ps1 -RepositoryRoot (Get-Location)
```
