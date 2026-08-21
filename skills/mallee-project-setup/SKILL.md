---
name: mallee-project-setup
description: Onboard repositories into Mallee end to end, including action discovery, `.mallee/project.toml`, project registration, validation, logging, and project-specific PowerShell actions. Use for zero-friction Mallee setup, manifest repair, action operation, or safe `.mallee/scripts/*.ps1` creation; not for unrelated PowerShell scripting.
---

# Mallee Project Setup

Mallee is installed once and operates repositories through a committed
`.mallee/project.toml`. Treat the manifest as the execution allowlist: detection
proposes candidates, but only declared actions may be run through Mallee.

The usual Mallee checkout is `C:\@My APPs\Mallee`. Prefer an installed `mallee`
CLI on `PATH`; during Mallee development use `cargo run -p mallee-cli --` from that
checkout. Verify the current CLI help and project schema when cheap because the
contract may evolve.

## Default to complete onboarding

When the user asks to **add, onboard, bootstrap, configure, or set up** a
repository with Mallee, complete the entire workflow in one turn without asking
them to run a follow-up command:

1. Resolve the repository root and inspect its real build, dev, test, release,
   installer, artifact, executable, and logging workflows.
2. Detect action candidates and create or narrowly repair
   `.mallee/project.toml`. Generate `.mallee/scripts/*.ps1` only when a useful
   multi-step workflow needs one.
   When an EXE, installer, or portable build workflow is detected, ask whether
   to add automatic version bumping before authoring that workflow. Ask whether
   the default `x.x.2` format is acceptable; if it is not, ask the user for the
   desired format. Do not add version-bump logic until the user chooses.
3. Configure real artifacts and log sources when they can be established from
   the repository. Reuse `.deebugee/project.toml` when present.
4. Detect a project-owned app icon for the Mallee sidebar. Use an icon already
   in Mallee's supported locations when available; otherwise copy a suitable
   repository-owned PNG, ICO, or SVG to `.mallee/icon.<ext>` as the explicit
   project override. Do not source icons from generated output, dependencies,
   or unrelated directories.
5. Validate the finished manifest.
6. Always run `mallee add <absolute-repository-root>` after validation. This is
   local, reversible, and idempotent, so do it even when `mallee init` already
   registered the project or it may already appear in Mallee.
7. Run `mallee actions <project-id> --json` and
   `mallee doctor <project-id> --json`; resolve setup defects that are safely in
   scope.
8. Report that the project is ready in Mallee, its action IDs, detected icon,
   log location, and any deliberate omissions.

Do not stop after merely writing the manifest. A successful onboarding request
ends with the project registered and loadable by ID. Registration is not implied
by a request that is explicitly limited to inspection, detection, or generating
one script.

## Choose the mode

- **Onboard, bootstrap, or repair a project:** perform complete onboarding as
  defined above. Inspect the repository's existing build,
  dev, test, release, installer, executable, artifact, and logging paths. Run
  `mallee detect <repo> --json`, compare the candidates with the real workflows,
  then initialize or edit the manifest. If an EXE, installer, or portable build
  is present, resolve the user's auto-bump and version-format choice before
  adding versioning logic. Read
  [references/manifest-workflow.md](references/manifest-workflow.md).
- **Generate a PowerShell action:** first determine whether a direct manifest
  command or an existing script already expresses the workflow. Generate
  `.mallee/scripts/<action>.ps1` only for useful multi-step Windows orchestration.
  Read [references/powershell-actions.md](references/powershell-actions.md).
- **Operate an existing project:** validate the manifest, list its declared
  actions, then invoke the named action. Do not substitute a different action or
  infer publishing permission from a build request.

## Resolve available tools before writing actions

Before proposing, generating, or running an action that depends on a program,
probe for that program first. Use the installed executable rather than assuming
the preferred package manager, shell, runtime, or compiler is available. If it
is absent, choose the next compatible tool already present and adapt the action
to it; do not install software unless the user authorizes installation.

For PowerShell actions, record the probed interpreter in the manifest's
`program` field. Keep scripts compatible with the selected interpreter. If no
interpreter is available, reuse a compatible existing project script or report
the missing prerequisite rather than emitting an action that cannot start. Read
[references/powershell-actions.md](references/powershell-actions.md).

## Preserve boundaries

- Inspect real repository scripts and configuration before proposing commands.
- Do not copy Mallee or DeeBugee executables into consuming repositories.
- Keep personal history, transcripts, registry state, and secrets out of the
  repository.
- Do not create a wrapper around a simple direct command such as `cargo test` or
  `pnpm tauri dev` without a concrete orchestration need.
- Name action labels as briefly and precisely as possible: lead with the verb
  and target (for example, `Run Tests`, `Build Installer`, or `Start Desktop`),
  and omit implementation detail already visible in the command preview.
- Never hard-code machine-specific repository paths, credentials, tokens, signing
  material, or private environment values.
- Preview new manifest actions and scripts. Release, publish, sign, deploy,
  installer deployment, and destructive cleanup require explicit authorization
  before execution even when their scripts already exist.
- Preserve unrelated files and edits. If a manifest exists, update it narrowly
  and retain comments and action order.

## Validate the result

1. Run `mallee validate <repo>`, `mallee add <absolute-repo>`, and
   `mallee doctor <project> --json` in that order for onboarding requests.
2. Confirm the action resolves to the intended program, arguments, and working
   directory.
3. Exercise a safe non-publishing success path and a focused failure path when the
   task authorizes execution.
4. Confirm nonzero native exit codes propagate as failed Mallee history entries.
5. If logs are configured, confirm the resolved log source exists or is allowed
   to be created later; use DeeBugee project mode when `.deebugee/project.toml`
  exists.
