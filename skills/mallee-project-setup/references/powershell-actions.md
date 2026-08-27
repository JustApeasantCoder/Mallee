# PowerShell action generation

Generate a Mallee-specific script under `.mallee/scripts/<action>.ps1` only when a
workflow has meaningful multi-step Windows orchestration. Reuse an established
project script when it already owns the workflow.

## Required script shape

```powershell
[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$projectRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
Push-Location $projectRoot
try {
    # Project-specific commands.
    & cargo test --workspace
    if ($LASTEXITCODE -ne 0) {
        throw "cargo test failed with exit code $LASTEXITCODE"
    }
}
finally {
    Pop-Location
}
```

Adapt parameters and commands to the repository. Do not copy the example blindly.

## Non-obvious requirements

- Before writing the action, locate a usable interpreter. Prefer PowerShell 7
  (`pwsh`); otherwise use Windows PowerShell (`powershell` or `powershell.exe`).
  A targeted probe such as
  `Get-Command pwsh -CommandType Application -ErrorAction SilentlyContinue`
  establishes whether the preferred executable is available. Do not assume a
  package is installed merely because the action would normally use it.
- Put the interpreter actually selected by that probe in the manifest's
  `program` field. A manifest cannot fall back from a missing `pwsh` executable
  after it has launched, so the fallback must be selected before the action is
  generated.
- When Windows PowerShell is selected, write PowerShell 5.1-compatible code:
  avoid `&&`/`||`, `??`, ternary expressions, `ForEach-Object -Parallel`, and
  other PowerShell-7-only features. If no PowerShell interpreter is installed,
  use a compatible existing project workflow when one exists; otherwise leave
  the action uncreated and report the prerequisite. Do not install PowerShell
  without explicit authorization.
- Derive the repository root from `$PSScriptRoot`; never embed the current
  checkout's absolute path.
- Invoke native programs with `&` and explicit arguments. Check
  `$LASTEXITCODE` immediately after each native command whose failure must stop
  the workflow; `$ErrorActionPreference` alone does not convert native nonzero
  exits into PowerShell exceptions.
- Use `[CmdletBinding()]` and explicit parameters for values users may change.
- Avoid `Invoke-Expression`, string-built commands, broad recursive deletion,
  hidden background launchers, and mutation outside resolved project paths.
- Do not print secret environment values or pass secrets in arguments when a
  safer tool-specific secret channel exists.
- Make output concise and useful in Mallee's terminal. Mallee owns structured
  action lifecycle logging; the script should not invent a parallel JSONL logger.
- Release, publish, signing, deployment, and destructive scripts must be explicit
  about their effect and paired with `confirm = true` in the manifest.

Reference the script with an argument array. Use the executable found by the
probe; this example assumes `pwsh` was available:

```toml
[[actions]]
id = "build-installer"
label = "Build Installer"
program = "pwsh"
args = ["-NoLogo", "-NoProfile", "-File", ".mallee/scripts/build-installer.ps1"]
kind = "task"
terminal = "captured"
sound_notification = true
```

`Build Installer` is one of the two default sound-enabled actions. Do not add
`sound_notification = true` to other generated PowerShell actions unless the
user explicitly requests it; the other default is `Push Release`.

Validate PowerShell parsing when possible, run the smallest safe path, and verify
that a deliberate native failure produces a nonzero script exit and a failed
Mallee history entry.
