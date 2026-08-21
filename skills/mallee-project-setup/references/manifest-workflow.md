# Mallee manifest workflow

## Inspect before authoring

Start with the named repository root and inspect only relevant conventional
surfaces:

- `.mallee/project.toml` and `.mallee/scripts/`;
- root, `scripts/`, `tools/`, and `build/` PowerShell or batch files;
- `package.json`, Cargo manifests, Tauri config, .NET solutions/projects,
  Makefiles, Justfiles, and Taskfiles;
- existing start/build/release documentation and actual produced artifacts;
- existing application logging configuration and real log paths.

Use `mallee detect <repo> --json` as candidate evidence, not as authority. Exclude
dependency, cache, generated output, vendored, and VCS directories.

## Authoring rules

The canonical manifest path is `.mallee/project.toml`.

- Use schema version 1 until Mallee provides a migration.
- Use stable lowercase action IDs containing letters, digits, and hyphens.
- Keep action labels short and precise. Prefer a clear verb and target such as
  `Run Tests`, `Build Installer`, or `Open Logs`; leave program names, flags,
  and other implementation detail to the command preview.
- When an EXE, installer, or portable build is detected, ask before adding
  automatic version bumping. Offer `x.x.2` as the default version format and,
  if the user declines it, request their desired format. Do not infer a format
  or add version-bump behavior without that answer.
- Keep `program` separate from `args`; do not collapse normal commands into an
  opaque shell string.
- Resolve relative paths from the repository root.
- Mark long-lived dev servers as `kind = "long_running"` and normally use
  `concurrency = "replace_same_action"`.
- Add `confirm = true` to release, publish, sign, deployment, or destructive
  actions.
- Configure artifact globs to actual output locations, not hoped-for ones.
- Prefer environment-based or repository-relative log paths over machine-specific
  absolute paths.
- Use `operation = "open_project_folder"` or `"open_log_folder"` for built-in
  folder actions instead of shelling out to Explorer scripts.
- Detect an existing project-owned app icon in a Mallee-supported location.
  If none exists, copy a suitable repository-owned PNG, ICO, or SVG to
  `.mallee/icon.<ext>` so it becomes Mallee's explicit sidebar override. Do not
  copy generated-output, dependency, or unrelated assets.

## Finish onboarding

For an add, onboard, bootstrap, configure, or setup request, do not leave
registration as a follow-up for the user. After editing:

1. Run `mallee validate <repo>`.
2. Run `mallee add <absolute-repo>` unconditionally. The registry operation is
   idempotent and does not create duplicate entries for the same canonical root.
3. Run `mallee actions <project-id> --json`.
4. Run `mallee doctor <project-id> --json` and address safe setup defects.
5. Confirm the project appears in `mallee list --json` and report the ready
   actions, detected icon, and log location.

An explicitly inspection-only, detection-only, or script-only request does not
register the repository unless the user also asks to add or onboard it.
