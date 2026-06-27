$ErrorActionPreference = "Stop"

$Repo = "ShiinaSaku/Hayate"
$BinaryName = "hayate.exe"

# Visual Header
Write-Host "`n    __  _______  _____  ____________" -ForegroundColor Cyan
Write-Host "   / / / /   \ \/ /   |/_  __/ ____/" -ForegroundColor Cyan
Write-Host "  / /_/ / /| |\  / /| | / / / __/   " -ForegroundColor Cyan
Write-Host " / __  / ___ |/ / ___ |/ / / /___   " -ForegroundColor Cyan
Write-Host "/_/ /_/_/  |_/_/_/  |_/_/ /_____/   `n" -ForegroundColor Cyan
Write-Host "  Swift, Secure, Encrypted & Compressed Local File Transfers`n" -ForegroundColor Magenta

Write-Host "[*] Detecting Windows environment..." -ForegroundColor DarkGray

$Arch = "amd64"
if ($env:PROCESSOR_ARCHITECTURE -match "ARM") {
    $Arch = "arm64"
}
Write-Host "[*] Architecture detected: windows-$Arch" -ForegroundColor DarkGray

Write-Host "[*] Fetching latest release version..." -ForegroundColor DarkGray
try {
    $Response = Invoke-WebRequest -Uri "https://github.com/$Repo/releases/latest" -UseBasicParsing -MaximumRedirection 0
    $LatestTag = ($Response.Headers.Location -split '/')[-1]
} catch {
    # Fallback to direct redirect query
    try {
        $Request = [System.Net.WebRequest]::Create("https://github.com/$Repo/releases/latest")
        $Request.AllowAutoRedirect = $false
        $Response = $Request.GetResponse()
        $LatestTag = ($Response.Headers["Location"] -split '/')[-1]
        $Response.Close()
    } catch {
        $LatestTag = ""
    }
}

if (-not $LatestTag -or $LatestTag -eq "latest") {
    Write-Host "[-] Failed to fetch latest release tag. Check internet connection." -ForegroundColor Red
    exit 1
}

# Short-circuit if already up-to-date
$LocalBinPath = Join-Path $env:USERPROFILE ".hayate\bin\hayate.exe"
if (Test-Path -Path $LocalBinPath) {
    try {
        $CurrentVersion = & $LocalBinPath --version | ForEach-Object { $_.Split(' ')[0] }
        if ($CurrentVersion -eq $LatestTag) {
            Write-Host "[+] Hayate is already up-to-date ($CurrentVersion)!" -ForegroundColor Green
            exit 0
        }
    } catch {}
}

$AssetName = "hayate-windows-${Arch}.exe"
$DownloadUrl = "https://github.com/$Repo/releases/download/$LatestTag/$AssetName"
$InstallDir = "$env:USERPROFILE\.hayate\bin"

if (-not (Test-Path -Path $InstallDir)) {
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
}

$DestPath = "$InstallDir\$BinaryName"

Write-Host "[*] Downloading $AssetName ($LatestTag)..." -ForegroundColor Cyan
try {
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    Invoke-WebRequest -Uri $DownloadUrl -OutFile $DestPath -UseBasicParsing
} catch {
    Write-Host "[-] Download failed. Check connection or verify release." -ForegroundColor Red
    exit 1
}

if (-not (Test-Path -Path $DestPath)) {
    Write-Host "[-] Installation failed: Binary not found." -ForegroundColor Red
    exit 1
}

# Update User PATH
$UserPath = [Environment]::GetEnvironmentVariable("PATH", "User")
if ($UserPath -notmatch [regex]::Escape($InstallDir)) {
    Write-Host "[*] Adding $InstallDir to user PATH environment variable..." -ForegroundColor DarkGray
    $NewPath = "$UserPath;$InstallDir"
    [Environment]::SetEnvironmentVariable("PATH", $NewPath, "User")
    $env:PATH = "$env:PATH;$InstallDir"
}

Write-Host "[+] Hayate $LatestTag installed successfully!" -ForegroundColor Green
Write-Host "[*] Restart your terminal for PATH changes to take effect." -ForegroundColor DarkGray
Write-Host "[*] Get started by running 'hayate help'`n" -ForegroundColor DarkGray
