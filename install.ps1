# aiem one-line installer for Windows (PowerShell)
# Usage: irm https://raw.githubusercontent.com/Vaxspark/aiem/main/install.ps1 | iex

$ErrorActionPreference = "Stop"

$repo    = "Vaxspark/aiem"
$asset   = "aiem-windows-x86_64.zip"
$installDir = if ($env:AIEM_INSTALL_DIR) { $env:AIEM_INSTALL_DIR } else { "$env:LOCALAPPDATA\aiem" }

# ── fetch latest release ──────────────────────────────────────────────────────
Write-Host "Fetching latest release..."
$release = Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest"
$downloadUrl = ($release.assets | Where-Object { $_.name -eq $asset }).browser_download_url

if (-not $downloadUrl) {
    Write-Error "Could not find asset '$asset' in the latest release."
}

# ── download & extract ────────────────────────────────────────────────────────
$tmp = New-TemporaryFile | ForEach-Object { Remove-Item $_; New-Item -ItemType Directory $_.FullName }
try {
    $zipPath = Join-Path $tmp "aiem.zip"
    Write-Host "Downloading $asset ..."
    Invoke-WebRequest $downloadUrl -OutFile $zipPath -UseBasicParsing
    Expand-Archive $zipPath -DestinationPath $tmp -Force

    New-Item -ItemType Directory -Force -Path $installDir | Out-Null
    Copy-Item (Join-Path $tmp "aiem.exe") (Join-Path $installDir "aiem.exe") -Force
} finally {
    Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host "Installed to $installDir\aiem.exe"

# ── add to user PATH if not already present ───────────────────────────────────
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$installDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$installDir", "User")
    $env:Path += ";$installDir"
    Write-Host "Added $installDir to your user PATH."
} else {
    Write-Host "$installDir is already in PATH."
}

Write-Host ""
Write-Host "Run 'aiem init' to get started."
