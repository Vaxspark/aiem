# aiem one-line installer for Windows (PowerShell)
# Usage: irm https://raw.githubusercontent.com/Vaxspark/aiem/main/install.ps1 | iex
#
# Installs the CLI, the native GUI app when present in the release archive,
# Start Menu / Desktop shortcuts, a user PATH entry, and an HKCU uninstall entry.

$ErrorActionPreference = "Stop"

$repo = "Vaxspark/aiem"
$asset = if ($env:AIEM_RELEASE_ASSET) { $env:AIEM_RELEASE_ASSET } else { "" }
$installDir = if ($env:AIEM_INSTALL_DIR) { $env:AIEM_INSTALL_DIR } else { "$env:LOCALAPPDATA\aiem" }
$startMenuDir = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\aiem"
$desktopDir = [Environment]::GetFolderPath("Desktop")
$uninstallKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\aiem"

function New-AiemShortcut {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Target,
        [string]$Arguments = "",
        [string]$Icon = ""
    )

    $shell = New-Object -ComObject WScript.Shell
    $shortcut = $shell.CreateShortcut($Path)
    $shortcut.TargetPath = $Target
    $shortcut.Arguments = $Arguments
    $shortcut.WorkingDirectory = Split-Path $Target
    if ($Icon -and (Test-Path $Icon)) {
        $shortcut.IconLocation = $Icon
    }
    $shortcut.Save()
}

Write-Host "Fetching latest aiem release..."
$release = Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest"
$assetInfo = if ($asset) {
    $release.assets | Where-Object { $_.name -eq $asset } | Select-Object -First 1
} else {
    $release.assets | Where-Object { $_.name -like "aiem-*-windows-x86_64.zip" -or $_.name -eq "aiem-windows-x86_64.zip" } | Select-Object -First 1
}
$downloadUrl = $assetInfo.browser_download_url

if (-not $downloadUrl) {
    Write-Error "Could not find a Windows x86_64 zip asset in the latest release."
}

$tmp = New-TemporaryFile | ForEach-Object {
    Remove-Item $_
    New-Item -ItemType Directory $_.FullName
}

try {
    $zipPath = Join-Path $tmp "aiem.zip"
    Write-Host "Downloading $($assetInfo.name) ..."
    Invoke-WebRequest $downloadUrl -OutFile $zipPath -UseBasicParsing
    Expand-Archive $zipPath -DestinationPath $tmp -Force

    New-Item -ItemType Directory -Force -Path $installDir | Out-Null
    Copy-Item (Join-Path $tmp "aiem.exe") (Join-Path $installDir "aiem.exe") -Force

    $guiSource = Join-Path $tmp "aiem-gui.exe"
    if (Test-Path $guiSource) {
        Copy-Item $guiSource (Join-Path $installDir "aiem-gui.exe") -Force
    }

    $iconSource = Join-Path $tmp "aiem.ico"
    if (Test-Path $iconSource) {
        Copy-Item $iconSource (Join-Path $installDir "aiem.ico") -Force
    }
} finally {
    Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
}

$cliPath = Join-Path $installDir "aiem.exe"
$guiPath = Join-Path $installDir "aiem-gui.exe"
$iconPath = Join-Path $installDir "aiem.ico"

Write-Host "Installed aiem to $installDir"

$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$installDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$installDir", "User")
    $env:Path += ";$installDir"
    Write-Host "Added $installDir to your user PATH."
} else {
    Write-Host "$installDir is already in PATH."
}

New-Item -ItemType Directory -Force -Path $startMenuDir | Out-Null

if (Test-Path $guiPath) {
    New-AiemShortcut `
        -Path (Join-Path $startMenuDir "aiem.lnk") `
        -Target $guiPath `
        -Icon $iconPath
    New-AiemShortcut `
        -Path (Join-Path $desktopDir "aiem.lnk") `
        -Target $guiPath `
        -Icon $iconPath
}

New-AiemShortcut `
    -Path (Join-Path $startMenuDir "aiem Web UI.lnk") `
    -Target $cliPath `
    -Arguments "serve --host 127.0.0.1 --port 8787 --open" `
    -Icon $iconPath

New-Item -Path $uninstallKey -Force | Out-Null
New-ItemProperty -Path $uninstallKey -Name "DisplayName" -Value "aiem" -PropertyType String -Force | Out-Null
New-ItemProperty -Path $uninstallKey -Name "DisplayVersion" -Value $release.tag_name -PropertyType String -Force | Out-Null
New-ItemProperty -Path $uninstallKey -Name "Publisher" -Value "aiem contributors" -PropertyType String -Force | Out-Null
New-ItemProperty -Path $uninstallKey -Name "InstallLocation" -Value $installDir -PropertyType String -Force | Out-Null
New-ItemProperty -Path $uninstallKey -Name "DisplayIcon" -Value $iconPath -PropertyType String -Force | Out-Null
New-ItemProperty -Path $uninstallKey -Name "UninstallString" -Value "powershell.exe -NoProfile -ExecutionPolicy Bypass -Command `"Remove-Item '$installDir' -Recurse -Force; Remove-Item '$startMenuDir' -Recurse -Force -ErrorAction SilentlyContinue; Remove-Item '$desktopDir\aiem.lnk' -Force -ErrorAction SilentlyContinue; Remove-Item '$uninstallKey' -Recurse -Force`"" -PropertyType String -Force | Out-Null
New-ItemProperty -Path $uninstallKey -Name "NoModify" -Value 1 -PropertyType DWord -Force | Out-Null
New-ItemProperty -Path $uninstallKey -Name "NoRepair" -Value 1 -PropertyType DWord -Force | Out-Null

Write-Host ""
Write-Host "Run 'aiem init' to get started."
if (Test-Path $guiPath) {
    Write-Host "Launch the desktop app from Start Menu: aiem"
}
