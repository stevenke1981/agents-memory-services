param(
    [string]$Version = "v0.1.0",
    [switch]$FromSource,
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"

# Ensure security protocols support TLS 1.2 for download
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$ServerName = "ams"
$BinaryName = "memory-mcp-server.exe"

# Resolve target directory
$UserProfile = if ($env:USERPROFILE) { $env:USERPROFILE } else { [System.Environment]::GetFolderPath('UserProfile') }
$TargetDir = Join-Path $UserProfile ".config\$ServerName\bin"
$StableExe = Join-Path $TargetDir $BinaryName

Write-Host "Installing $ServerName..."

$SourceExe = $null

# If not forcing build from source, attempt download first
if (-not $FromSource) {
    # Normalize version tags (always prefix with 'v' if it looks like a version number and doesn't have it)
    $TagVersion = $Version
    if (-not $TagVersion.StartsWith("v")) {
        $TagVersion = "v$TagVersion"
    }

    $ZipName = "$ServerName-$TagVersion-x86_64-pc-windows-msvc.zip"
    $Url = "https://github.com/stevenke1981/agents-memory-services/releases/download/$TagVersion/$ZipName"
    
    $TempDir = Join-Path $env:TEMP "ams-install-$(Get-Random)"
    New-Item -ItemType Directory -Force -Path $TempDir | Out-Null
    $ZipPath = Join-Path $TempDir $ZipName

    Write-Host "Attempting to download release version $TagVersion from $Url..."
    try {
        if (Get-Command "curl.exe" -ErrorAction SilentlyContinue) {
            curl.exe -L -o $ZipPath $Url
        } else {
            Invoke-WebRequest -Uri $Url -OutFile $ZipPath -UseBasicParsing
        }
        
        if (Test-Path $ZipPath) {
            Write-Host "Extracting $ZipName..."
            $ExtractDir = Join-Path $TempDir "extracted"
            Expand-Archive -Path $ZipPath -DestinationPath $ExtractDir -Force
            
            # Find the binary in the extracted files
            $ExtractedExe = Get-ChildItem -Path $ExtractDir -Filter "*.exe" -Recurse | Select-Object -First 1
            if ($ExtractedExe) {
                $SourceExe = $ExtractedExe.FullName
                Write-Host "Successfully downloaded and extracted binary: $SourceExe"
            } else {
                Write-Warning "Could not find any executable in the downloaded archive."
            }
        }
    } catch {
        Write-Warning "Failed to download release asset: $($_.Exception.Message)"
    }

    if (-not $SourceExe) {
        throw "Could not download release asset for $TagVersion. Re-run with -FromSource only on machines that have a Rust toolchain."
    }
}

# Source build path
if ($FromSource) {
    if (-not $SkipBuild) {
        Write-Host "Building $ServerName from source..."
        Write-Host "Running: cargo build --release"
        & cargo build --release
        if ($LASTEXITCODE -ne 0) {
            throw "Cargo build failed"
        }
    } else {
        Write-Host "Skipping cargo build as requested (-SkipBuild)."
    }

    # Verify built binary
    $BuiltExe = "target/release/memory-mcp-server.exe"
    if (-not (Test-Path $BuiltExe)) {
        throw "Built binary not found at $BuiltExe. Please ensure you built it first or run without -SkipBuild."
    }
    $SourceExe = (Resolve-Path $BuiltExe).Path
    Write-Host "Using compiled binary at $SourceExe"
}

# Ensure target folder exists
if (-not (Test-Path $TargetDir)) {
    New-Item -ItemType Directory -Force -Path $TargetDir | Out-Null
}

# Step 3: Handle locked binary side-by-side versioning fallback
$InstalledExe = $StableExe
$IsStableCopied = $false

Write-Host "Installing executable to target path..."
try {
    Copy-Item -Path $SourceExe -Destination $StableExe -Force -ErrorAction Stop
    $IsStableCopied = $true
    Write-Host "Copied binary to stable path: $StableExe"
} catch {
    # Copy failed (e.g. locked or access denied)
    $ErrMessage = $_.Exception.Message
    Write-Warning "Could not copy to stable path $StableExe ($ErrMessage)."
    
    $TagVersion = $Version
    if (-not $TagVersion.StartsWith("v")) {
        $TagVersion = "v$TagVersion"
    }
    
    $VersionedName = "$ServerName-$TagVersion.exe"
    $VersionedExe = Join-Path $TargetDir $VersionedName
    
    Write-Warning "Installing versioned binary side-by-side: $VersionedExe"
    try {
        Copy-Item -Path $SourceExe -Destination $VersionedExe -Force -ErrorAction Stop
        $InstalledExe = $VersionedExe
        Write-Host "Copied binary to versioned path: $InstalledExe"
    } catch {
        $Timestamp = (Get-Date).ToString("yyyyMMdd-HHmmss")
        $TimestampedName = "$ServerName-$TagVersion-$Timestamp.exe"
        $TimestampedExe = Join-Path $TargetDir $TimestampedName
        Write-Warning "Installing timestamped binary side-by-side: $TimestampedExe"
        Copy-Item -Path $SourceExe -Destination $TimestampedExe -Force
        $InstalledExe = $TimestampedExe
        Write-Host "Copied binary to timestamped path: $InstalledExe"
    }
}

# Step 4: Configure OpenCode/Codex by running the installed binary
Write-Host "Running configuration installer from $InstalledExe..."
$InstallOutput = & $InstalledExe install --json
$InstallExitCode = $LASTEXITCODE
if ($InstallExitCode -ne 0) {
    throw "Installed binary failed to configure system settings (exit code $InstallExitCode): $InstallOutput"
}

try {
    $InstallReport = $InstallOutput | Out-String | ConvertFrom-Json
} catch {
    throw "Installed binary did not return a valid JSON install report: $InstallOutput"
}

if (-not $InstallReport.binary_path) {
    throw "Install report is missing binary_path."
}

$ReportedExe = (Resolve-Path -LiteralPath $InstallReport.binary_path).Path
$ResolvedInstalledExe = (Resolve-Path -LiteralPath $InstalledExe).Path
if ($ReportedExe -ne $ResolvedInstalledExe) {
    throw "Install report binary path mismatch. Expected $ResolvedInstalledExe but got $ReportedExe."
}

Write-Host "=========================================================" -ForegroundColor Green
Write-Host "$ServerName has been installed successfully!" -ForegroundColor Green
Write-Host "Installed Executable Path: $ReportedExe" -ForegroundColor Green
foreach ($Client in $InstallReport.configured_clients) {
    Write-Host "Configured $($Client.client): $($Client.path) [$($Client.status)]" -ForegroundColor Green
}
if (-not $IsStableCopied) {
    Write-Host "Note: Installed as a side-by-side versioned binary because the stable executable was in use." -ForegroundColor Yellow
}
Write-Host "Please restart your OpenCode or Codex agent to reload changes." -ForegroundColor Green
Write-Host "=========================================================" -ForegroundColor Green
