[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidateNotNullOrEmpty()]
    [string]$RepositoryRoot
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$cargoTomlPath = Join-Path $RepositoryRoot "Cargo.toml"
$tauriConfigPath = Join-Path $RepositoryRoot "apps\desktop\src-tauri\tauri.conf.json"
$packageJsonPath = Join-Path $RepositoryRoot "apps\desktop\package.json"

foreach ($path in @($cargoTomlPath, $tauriConfigPath, $packageJsonPath)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Required version file was not found: $path"
    }
}

$cargoToml = Get-Content -LiteralPath $cargoTomlPath -Raw
$tauriConfig = Get-Content -LiteralPath $tauriConfigPath -Raw
$packageJson = Get-Content -LiteralPath $packageJsonPath -Raw

$cargoPattern = '(?ms)(\[workspace\.package\].*?^version\s*=\s*")(?<version>\d+\.\d+\.\d+)(")'
$jsonPattern = '(?m)("version"\s*:\s*")(?<version>\d+\.\d+\.\d+)(")'
$cargoMatch = [regex]::Match($cargoToml, $cargoPattern)
$tauriMatch = [regex]::Match($tauriConfig, $jsonPattern)
$packageMatch = [regex]::Match($packageJson, $jsonPattern)

if (-not $cargoMatch.Success -or -not $tauriMatch.Success -or -not $packageMatch.Success) {
    throw "Could not find a valid semantic version in every version file."
}

$currentVersion = $cargoMatch.Groups["version"].Value
if ($tauriMatch.Groups["version"].Value -ne $currentVersion -or $packageMatch.Groups["version"].Value -ne $currentVersion) {
    throw "Version files are out of sync. Cargo.toml is $currentVersion; synchronize the version files before building."
}

$parts = $currentVersion.Split('.')
try {
    $patch = [int]::Parse($parts[2])
} catch {
    throw "The patch number in $currentVersion cannot be incremented."
}
if ($patch -eq [int]::MaxValue) {
    throw "The patch number in $currentVersion cannot be incremented."
}
$nextPatch = $patch + 1
$nextVersion = "$($parts[0]).$($parts[1]).$nextPatch"

function Set-VersionInMatch {
    param(
        [Parameter(Mandatory)]
        [string]$Content,
        [Parameter(Mandatory)]
        [System.Text.RegularExpressions.Match]$Match,
        [Parameter(Mandatory)]
        [string]$Version
    )

    $versionGroup = $Match.Groups["version"]
    return $Content.Remove($versionGroup.Index, $versionGroup.Length).Insert($versionGroup.Index, $Version)
}

$cargoToml = Set-VersionInMatch -Content $cargoToml -Match $cargoMatch -Version $nextVersion
$tauriConfig = Set-VersionInMatch -Content $tauriConfig -Match $tauriMatch -Version $nextVersion
$packageJson = Set-VersionInMatch -Content $packageJson -Match $packageMatch -Version $nextVersion

$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
[System.IO.File]::WriteAllText($cargoTomlPath, $cargoToml, $utf8NoBom)
[System.IO.File]::WriteAllText($tauriConfigPath, $tauriConfig, $utf8NoBom)
[System.IO.File]::WriteAllText($packageJsonPath, $packageJson, $utf8NoBom)

Write-Host "Bumped Mallee version $currentVersion -> $nextVersion"
