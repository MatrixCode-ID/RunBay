# Builds the release executable and publishes it to the local artifact folder
# ../.artefacts/RunBay/dist (relative to the repository root), with SHA256SUMS.
# The executable gets the version of the release note candidate being prepared.
#
# Usage: scripts\publish-local.cmd [-SkipTests]
#    or: pwsh -File scripts/publish-local/publish-local.ps1 [-SkipTests]
param(
    # Skip fmt, clippy, and tests before building.
    [switch] $SkipTests
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$artifacts = [System.IO.Path]::GetFullPath((Join-Path $repo '..\.artefacts\RunBay'))
$target = Join-Path $artifacts 'target'
$dist = Join-Path $artifacts 'dist'

# Use an MSVC toolset that has the x64 libraries when the shell has no build environment.
function Import-MsvcEnvironment {
    if ($env:LIB) { return }
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path $vswhere)) { return }
    foreach ($install in & $vswhere -all -products * -property installationPath) {
        $crt = Get-ChildItem (Join-Path $install 'VC\Tools\MSVC\*\lib\x64\msvcrt.lib') -ErrorAction SilentlyContinue
        $vcvars = Join-Path $install 'VC\Auxiliary\Build\vcvars64.bat'
        if ($crt -and (Test-Path $vcvars)) {
            Write-Host "Using the MSVC environment of $install"
            cmd /c "`"$vcvars`" >nul && set" | ForEach-Object {
                if ($_ -match '^([^=]+)=(.*)$') { Set-Item "env:$($Matches[1])" $Matches[2] }
            }
            return
        }
    }
}

# The version of the single release note candidate, so local builds carry the version
# being prepared; $null (keep Cargo.toml) when there is no single valid candidate.
function Get-CandidateVersion {
    $notes = @(Get-ChildItem (Join-Path $repo 'doc\ReleaseNote-Candidate') -Filter '*.md' -File -ErrorAction SilentlyContinue |
        Where-Object { $_.BaseName -match '^\d+\.\d+\.\d+(-(alpha|beta)\.\d+)?$' })
    if ($notes.Count -eq 1) { return $notes[0].BaseName }
    Write-Warning "Expected one release note candidate, found $($notes.Count); keeping the Cargo.toml version."
    return $null
}

# Runs the release build with the package version set to $Version. Cargo.toml and
# Cargo.lock are restored byte for byte afterwards, as CI only aligns them in its build.
function Invoke-ReleaseBuild([string] $Version) {
    $manifest = Join-Path $repo 'Cargo.toml'
    $lock = Join-Path $repo 'Cargo.lock'
    $savedManifest = [System.IO.File]::ReadAllBytes($manifest)
    $savedLock = [System.IO.File]::ReadAllBytes($lock)
    try {
        if ($Version) {
            $text = [System.IO.File]::ReadAllText($manifest)
            $updated = [regex]::new('(?m)^(version\s*=\s*)"[^"]*"').Replace($text, "`${1}`"$Version`"", 1)
            [System.IO.File]::WriteAllText($manifest, $updated, [System.Text.UTF8Encoding]::new($false))
        }
        Invoke-Cargo @('build', '--release')
    }
    finally {
        [System.IO.File]::WriteAllBytes($manifest, $savedManifest)
        [System.IO.File]::WriteAllBytes($lock, $savedLock)
    }
}

# Arguments are passed as an array: PowerShell would swallow a bare `--` given to a
# function as separate arguments.
function Invoke-Cargo([string[]] $Arguments) {
    & cargo @Arguments
    if ($LASTEXITCODE -ne 0) { throw "cargo $($Arguments -join ' ') failed." }
}

Push-Location $repo
try {
    # Keep all Cargo output outside the repository, including incremental caches.
    $env:CARGO_TARGET_DIR = $target
    Import-MsvcEnvironment
    if (-not $SkipTests) {
        Invoke-Cargo @('fmt', '--check')
        Invoke-Cargo @('clippy', '--all-targets', '--', '-D', 'warnings')
        Invoke-Cargo @('test')
    }
    Invoke-ReleaseBuild (Get-CandidateVersion)

    New-Item -ItemType Directory -Force $dist | Out-Null
    $exe = Join-Path $dist 'RunBay.exe'
    Copy-Item (Join-Path $target 'release\runbay.exe') $exe -Force
    $hash = (Get-FileHash $exe -Algorithm SHA256).Hash.ToLowerInvariant()
    Set-Content (Join-Path $dist 'SHA256SUMS') "$hash *RunBay.exe" -NoNewline
    $version = (Get-Item $exe).VersionInfo.ProductVersion
    Write-Host "Published RunBay $version to $((Resolve-Path $dist).Path)"
}
finally {
    Pop-Location
}
