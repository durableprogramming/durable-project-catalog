# PowerShell installation script for Durable Project Catalog
# Supports Windows 10 and later

param(
    [string]$InstallDir = "$env:LOCALAPPDATA\Programs\dpc",
    [switch]$AddToPath
)

$ErrorActionPreference = "Stop"

# Configuration
$Repo = "your-org/durable-project-catalog"
$BinaryName = "dpc.exe"

# Colors for output
function Write-ColorOutput {
    param(
        [string]$Message,
        [string]$Color = "White"
    )
    Write-Host $Message -ForegroundColor $Color
}

# Detect architecture
function Get-Architecture {
    $arch = $env:PROCESSOR_ARCHITECTURE
    switch ($arch) {
        "AMD64" { return "x86_64" }
        "ARM64" { return "aarch64" }
        default {
            Write-ColorOutput "Error: Unsupported architecture: $arch" "Red"
            exit 1
        }
    }
}

# Get latest release version
function Get-LatestVersion {
    Write-ColorOutput "Fetching latest release..." "Yellow"

    try {
        $response = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
        $version = $response.tag_name -replace '^v', ''
        Write-ColorOutput "Latest version: v$version" "Green"
        return $version
    }
    catch {
        Write-ColorOutput "Error: Could not fetch latest version" "Red"
        Write-ColorOutput $_.Exception.Message "Red"
        exit 1
    }
}

# Download and install binary
function Install-Binary {
    param(
        [string]$Version,
        [string]$Architecture
    )

    $platform = "windows-$Architecture"
    $downloadUrl = "https://github.com/$Repo/releases/download/v$Version/dpc-$platform.exe"
    $tmpFile = Join-Path $env:TEMP "dpc-installer-temp.exe"

    Write-ColorOutput "Downloading from: $downloadUrl" "Yellow"

    try {
        Invoke-WebRequest -Uri $downloadUrl -OutFile $tmpFile
    }
    catch {
        Write-ColorOutput "Error: Failed to download binary" "Red"
        Write-ColorOutput $_.Exception.Message "Red"
        exit 1
    }

    # Create install directory if it doesn't exist
    if (!(Test-Path $InstallDir)) {
        New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    }

    # Move binary to install directory
    $destination = Join-Path $InstallDir $BinaryName
    Move-Item -Path $tmpFile -Destination $destination -Force

    Write-ColorOutput "✓ Installed $BinaryName to $InstallDir" "Green"
}

# Verify installation
function Test-Installation {
    $binaryPath = Join-Path $InstallDir $BinaryName

    try {
        $version = & $binaryPath --version
        Write-ColorOutput "✓ Installation verified: $version" "Green"
    }
    catch {
        Write-ColorOutput "Error: Installation verification failed" "Red"
        exit 1
    }
}

# Add to PATH
function Add-ToPath {
    $currentPath = [Environment]::GetEnvironmentVariable("Path", "User")

    if ($currentPath -notlike "*$InstallDir*") {
        Write-ColorOutput "Adding $InstallDir to user PATH..." "Yellow"
        $newPath = "$currentPath;$InstallDir"
        [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
        Write-ColorOutput "✓ Added to PATH (restart terminal for changes to take effect)" "Green"
    }
    else {
        Write-ColorOutput "✓ $InstallDir is already in PATH" "Green"
    }
}

# Check PATH
function Test-PathContains {
    $currentPath = [Environment]::GetEnvironmentVariable("Path", "User")

    if ($currentPath -notlike "*$InstallDir*") {
        Write-ColorOutput "Warning: $InstallDir is not in your PATH" "Yellow"
        Write-ColorOutput "Run with -AddToPath switch to add it automatically, or add manually:" "Yellow"
        Write-ColorOutput "  [Environment]::SetEnvironmentVariable('Path', `$env:Path + ';$InstallDir', 'User')" "White"
        Write-ColorOutput ""
    }
    else {
        Write-ColorOutput "✓ $InstallDir is in your PATH" "Green"
    }
}

# Suggest shell integration
function Show-NextSteps {
    Write-ColorOutput "" "White"
    Write-ColorOutput "Installation complete!" "Green"
    Write-ColorOutput "" "White"
    Write-ColorOutput "Optional: Set up PowerShell integration for quick navigation" "Yellow"
    Write-ColorOutput "Add the following to your PowerShell profile:" "Yellow"
    Write-ColorOutput "" "White"
    Write-ColorOutput "  # PowerShell (~\Documents\PowerShell\Microsoft.PowerShell_profile.ps1)" "Green"
    Write-ColorOutput "  Invoke-Expression (& dpc shell init powershell)" "White"
    Write-ColorOutput "" "White"
    Write-ColorOutput "Then run: dpc scan ~\projects to start cataloging your projects" "Green"
}

# Main installation flow
function Main {
    Write-ColorOutput "=== Durable Project Catalog Installer ===" "Green"
    Write-ColorOutput "" "White"

    $arch = Get-Architecture
    Write-ColorOutput "Detected architecture: $arch" "Green"

    $version = Get-LatestVersion
    Install-Binary -Version $version -Architecture $arch
    Test-Installation

    if ($AddToPath) {
        Add-ToPath
    }
    else {
        Test-PathContains
    }

    Show-NextSteps
}

# Run main installation
Main
