# Builds the release executable and publishes it to the local artifact folder
# ../.artefacts/RunBay/dist (relative to the repository root), with SHA256SUMS.
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
    Invoke-Cargo @('build', '--release')

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
