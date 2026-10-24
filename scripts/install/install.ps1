# Installs or upgrades RunBay for the current user, without admin rights.
#
# Downloads RunBay.exe of the newest GitHub release (pre-releases included, as the
# auto updater does), verifies it against SHA256SUMS, places it in the install folder,
# adds a Start menu shortcut, and starts RunBay.
#
# Usage (PowerShell):
#   irm https://raw.githubusercontent.com/MatrixCode-ID/RunBay/main/scripts/install/install.ps1 | iex
# With options:
#   & ([scriptblock]::Create((irm https://raw.githubusercontent.com/MatrixCode-ID/RunBay/main/scripts/install/install.ps1))) -InstallDir 'D:\Tools\RunBay'
#
# RUNBAY_UPDATE_API replaces https://api.github.com, as in RunBay itself (tests).
param(
    # Folder that receives RunBay.exe; RunBay keeps its data/ folder next to it.
    [string] $InstallDir = (Join-Path $env:LOCALAPPDATA 'Programs\RunBay'),
    # Install this version (e.g. 1.0.0-alpha.1) instead of the newest one.
    [string] $Version,
    # Do not create the Start menu shortcut.
    [switch] $NoShortcut,
    # Do not start RunBay after installing.
    [switch] $NoStart
)

$ErrorActionPreference = 'Stop'
# Invoke-WebRequest is much slower with the progress bar on Windows PowerShell 5.1.
$ProgressPreference = 'SilentlyContinue'
# Windows PowerShell 5.1 may not offer TLS 1.2 by default.
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

$api = if ($env:RUNBAY_UPDATE_API) { $env:RUNBAY_UPDATE_API.TrimEnd('/') } else { 'https://api.github.com' }

# MAJOR.MINOR.PATCH or MAJOR.MINOR.PATCH-(alpha|beta).N; stages sort alpha < beta < release.
function Get-VersionKey([string] $text) {
    if ($text -notmatch '^v?(\d+)\.(\d+)\.(\d+)(?:-(alpha|beta)\.(\d+))?$') { return $null }
    $stage = switch ($Matches[4]) { 'alpha' { 0 } 'beta' { 1 } default { 2 } }
    $number = if ($Matches[5]) { [int] $Matches[5] } else { 0 }
    '{0:D6}.{1:D6}.{2:D6}.{3}.{4:D6}' -f [int] $Matches[1], [int] $Matches[2], [int] $Matches[3], $stage, $number
}

function Get-Asset($release, [string] $name) {
    $asset = @($release.assets | Where-Object name -eq $name) | Select-Object -First 1
    if (-not $asset) { throw "Release $($release.tag_name) has no $name." }
    $asset.browser_download_url
}

Write-Host 'Looking for the RunBay release...'
$releases = @(Invoke-RestMethod -UseBasicParsing "$api/repos/MatrixCode-ID/RunBay/releases?per_page=100" |
    ForEach-Object { $_ } |
    Where-Object { -not $_.draft -and (Get-VersionKey $_.tag_name) })
if ($Version) {
    $release = $releases | Where-Object { $_.tag_name -eq "v$($Version.TrimStart('v'))" } | Select-Object -First 1
    if (-not $release) { throw "RunBay $Version was not found in the releases." }
} else {
    $release = $releases | Sort-Object { Get-VersionKey $_.tag_name } | Select-Object -Last 1
    if (-not $release) { throw 'No RunBay release has been published yet.' }
}
$releaseVersion = $release.tag_name.TrimStart('v')

$exe = Join-Path $InstallDir 'RunBay.exe'
$running = @(Get-Process -Name RunBay -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $exe })
if ($running) {
    throw "RunBay is running from $exe. Exit it from the tray menu (Exit) and run the installer again, or use Restart to update in RunBay."
}

New-Item -ItemType Directory -Force $InstallDir | Out-Null
$download = Join-Path $InstallDir 'RunBay.exe.download'
try {
    Write-Host "Downloading RunBay $releaseVersion..."
    Invoke-WebRequest -UseBasicParsing (Get-Asset $release 'RunBay.exe') -OutFile $download
    $sums = (Invoke-WebRequest -UseBasicParsing (Get-Asset $release 'SHA256SUMS')).Content
    if ($sums -is [byte[]]) { $sums = [Text.Encoding]::UTF8.GetString($sums) }
    $line = @($sums -split "`r?`n" | Where-Object { $_ -match '^\s*([0-9a-fA-F]{64})\s+\*?RunBay\.exe\s*$' }) | Select-Object -First 1
    if (-not $line) { throw 'SHA256SUMS has no entry for RunBay.exe.' }
    $expected = ($line.Trim() -split '\s+')[0]
    $actual = (Get-FileHash $download -Algorithm SHA256).Hash
    if ($actual -ne $expected.ToUpperInvariant()) { throw 'The downloaded RunBay.exe does not match SHA256SUMS.' }
    Move-Item -Force $download $exe
} finally {
    if (Test-Path $download) { Remove-Item -Force $download }
}

if (-not $NoShortcut) {
    $shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) 'RunBay.lnk'
    $shell = New-Object -ComObject WScript.Shell
    $link = $shell.CreateShortcut($shortcut)
    $link.TargetPath = $exe
    $link.WorkingDirectory = $InstallDir
    $link.Save()
}

Write-Host "RunBay $releaseVersion is installed in $InstallDir."
if (-not $NoStart) { Start-Process -FilePath $exe -WorkingDirectory $InstallDir }
