[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$projectRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$cargoTomlPath = Join-Path $projectRoot "Cargo.toml"
$tauriConfigPath = Join-Path $projectRoot "apps\desktop\src-tauri\tauri.conf.json"

function Get-SemanticVersion {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Value,
        [Parameter(Mandatory = $true)]
        [string]$Source
    )

    $match = [regex]::Match($Value, "^(?<major>\d+)\.(?<minor>\d+)\.(?<patch>\d+)$")
    if (-not $match.Success) {
        throw "$Source version '$Value' is not in x.x.2 format."
    }

    return $match
}

Push-Location $projectRoot
try {
    $cargoToml = Get-Content -LiteralPath $cargoTomlPath -Raw
    $cargoMatch = [regex]::Match($cargoToml, '(?m)^version = "(?<version>\d+\.\d+\.\d+)"$')
    if (-not $cargoMatch.Success) {
        throw "Could not find [workspace.package] version in Cargo.toml."
    }

    $tauriConfig = Get-Content -LiteralPath $tauriConfigPath -Raw
    $tauriMatch = [regex]::Match($tauriConfig, '(?m)^  "version": "(?<version>\d+\.\d+\.\d+)",$')
    if (-not $tauriMatch.Success) {
        throw "Could not find the Tauri version in tauri.conf.json."
    }

    $cargoVersion = $cargoMatch.Groups["version"].Value
    $tauriVersion = $tauriMatch.Groups["version"].Value
    if ($cargoVersion -ne $tauriVersion) {
        throw "Cargo.toml ($cargoVersion) and tauri.conf.json ($tauriVersion) must match before building."
    }

    $versionMatch = Get-SemanticVersion -Value $cargoVersion -Source "Cargo.toml"
    $nextVersion = "{0}.{1}.{2}" -f $versionMatch.Groups["major"].Value, $versionMatch.Groups["minor"].Value, ([int]$versionMatch.Groups["patch"].Value + 1)

    $cargoToml = [regex]::Replace($cargoToml, '(?m)^version = "\d+\.\d+\.\d+"$', "version = `"$nextVersion`"", 1)
    $tauriConfig = [regex]::Replace($tauriConfig, '(?m)^  "version": "\d+\.\d+\.\d+",$', "  `"version`": `"$nextVersion`",", 1)
    Set-Content -LiteralPath $cargoTomlPath -Value $cargoToml -Encoding utf8NoBOM
    Set-Content -LiteralPath $tauriConfigPath -Value $tauriConfig -Encoding utf8NoBOM

    Write-Host "Version bumped to $nextVersion."
    & pnpm --dir apps/desktop tauri build --no-bundle
    if ($LASTEXITCODE -ne 0) {
        throw "Desktop build failed with exit code $LASTEXITCODE."
    }
}
finally {
    Pop-Location
}
