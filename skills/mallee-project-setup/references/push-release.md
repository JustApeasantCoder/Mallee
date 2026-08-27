# Push Release action

Use this workflow only after the user opts in to adding a `Push Release` action.
The action publishes externally, so adding it does not authorize running it.

## Derive the real release workflow

Inspect the repository's existing release scripts, CI configuration, package
metadata, version files, Tauri or installer configuration, artifact names, Git
tags, and GitHub release conventions. Reuse a project-owned release command when
it already performs the complete guarded workflow. Otherwise create a narrowly
scoped `.mallee/scripts/push-release.ps1` that:

1. verifies required build tools and GitHub CLI availability;
2. verifies `gh auth status` without printing credentials;
3. requires an appropriate clean Git state and the expected branch/remote state;
4. applies only the version-bump behavior and format the user previously chose;
5. runs the project's actual release build and fails on any native nonzero exit;
6. resolves and checks every expected artifact before publishing;
7. creates or updates the repository's intended Git tag and GitHub release; and
8. uploads only the verified project artifacts produced by that release build.

Do not invent a universal artifact list. Depending on the project, the release
may include an EXE, MSI, NSIS installer, portable archive, checksums, symbols, or
another established distributable. Keep artifact paths repository-relative and
derive version and filenames from authoritative project metadata.

## Manifest and safety requirements

Use the stable action ID `push-release`, label it `Push Release`, and set
`confirm = true` and `sound_notification = true`. Its description or
confirmation text should state that it builds release artifacts and publishes a
GitHub release. Prefer captured terminal output unless the real project workflow
requires interaction. `Push Release` is one of the only two actions that receive
sound notifications by default; the other is `Build Installer`.

Never embed tokens, credentials, signing material, owner names, repository
names, branch names, or version numbers when they can be resolved safely from
Git and project configuration. Do not install `gh`, authenticate it, push a
release, create a tag, or modify remote state while merely configuring the
action. Run the action only after the user separately authorizes that publish.

Validate PowerShell parsing, manifest resolution, tool probes, and a safe
pre-publish failure path. Do not use a test that can create a tag, push a commit,
create a release, or upload an artifact.
