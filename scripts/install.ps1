$ErrorActionPreference = "Stop"

$Repo = "ShiinaSaku/Hayate"
$BinaryName = "hayate.exe"

Write-Host "`n  __   __     _____    __  __    _____    _______     _____  " -ForegroundColor Cyan
Write-Host " /\_\ /_/\   /\___/\ /\  /\  /\ /\___/\ /\_______)\ /\_____\ " -ForegroundColor Cyan
Write-Host "( ( (_) ) ) / / _ \ \\ \ \/ / // / _ \ \\(___  __\/( (_____/ " -ForegroundColor Cyan
Write-Host " \ \___/ /  \ \(_)/ / \ \__/ / \ \(_)/ /  / / /     \ \__\   " -ForegroundColor Cyan
Write-Host " / / _ \ \  / / _ \ \  \__/ /  / / _ \ \ ( ( (      / /__/_  " -ForegroundColor Cyan
Write-Host "( (_( )_) )( (_( )_) ) / / /  ( (_( )_) ) \ \ \    ( (_____\ " -ForegroundColor Cyan
Write-Host " \/_/ \_\/  \/_/ \_\/  \/_/    \/_/ \_\/  /_/_/     \/_____/ " -ForegroundColor Cyan
Write-Host "  Swift File Transfer | Secure, Encrypted, & Compressed`n" -ForegroundColor Green

Write-Host "[*] Detecting Windows environment..." -ForegroundColor DarkGray

$Arch = "amd64"
if ($env:PROCESSOR_ARCHITECTURE -match "ARM") {
    $Arch = "arm64"
}
Write-Host "[*] Architecture detected: windows-$Arch" -ForegroundColor DarkGray

Write-Host "[*] Fetching latest release metadata..." -ForegroundColor DarkGray

try {
    $Response = Invoke-WebRequest -Uri "https://github.com/$Repo/releases/latest" -UseBasicParsing
    $LatestTag = ($Response.BaseResponse.ResponseUri.AbsoluteUri -split '/')[-1]
} catch {
    $LatestTag = ""
}

if (-not $LatestTag -or $LatestTag -eq "latest") {
    Write-Host "[ERR] Failed to fetch latest release tag." -ForegroundColor Red
    exit 1
}

$AssetName = "hayate-windows-${Arch}.exe"
$DownloadUrl = "https://github.com/$Repo/releases/download/$LatestTag/$AssetName"

$InstallDir = "$env:USERPROFILE\.hayate\bin"
if (-not (Test-Path -Path $InstallDir)) {
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
}

$DestPath = "$InstallDir\$BinaryName"

Write-Host "[*] Downloading $AssetName ($LatestTag)..." -ForegroundColor DarkGray
Invoke-WebRequest -Uri $DownloadUrl -OutFile $DestPath -UseBasicParsing

if (-not (Test-Path -Path $DestPath)) {
    Write-Host "[ERR] Download failed." -ForegroundColor Red
    exit 1
}

$UserPath = [Environment]::GetEnvironmentVariable("PATH", "User")
if ($UserPath -notmatch [regex]::Escape($InstallDir)) {
    Write-Host "[*] Adding $InstallDir to user PATH..." -ForegroundColor DarkGray
    $NewPath = "$UserPath;$InstallDir"
    [Environment]::SetEnvironmentVariable("PATH", $NewPath, "User")
    $env:PATH = "$env:PATH;$InstallDir"
}

Write-Host "[+] Hayate $LatestTag installed successfully!" -ForegroundColor Green
Write-Host "[*] You may need to restart your terminal for the PATH changes to take effect." -ForegroundColor DarkGray
Write-Host "[*] Run 'hayate help' to get started.`n" -ForegroundColor DarkGray
