param([string]$OutputDirectory, [switch]$Installer)

$ErrorActionPreference = 'Stop'

$buildRoot = $PSScriptRoot
$cargoBin = Join-Path ([Environment]::GetFolderPath('UserProfile')) '.cargo\bin'
$targetRoot = Join-Path $buildRoot 'src-tauri\target'
$workerSource = Join-Path $buildRoot 'src-tauri\resources\whisper_worker.py'
$agySource = Join-Path $buildRoot 'vendor\agy.exe'
if (-not $OutputDirectory) { $OutputDirectory = if ($Installer) { 'installer-dist' } else { 'desktop-dist' } }
$outputRoot = [IO.Path]::GetFullPath((Join-Path $buildRoot $OutputDirectory))
if (-not $outputRoot.StartsWith($buildRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'The output directory must stay inside the project.'
}
$originalPath = $env:PATH
$originalCargoTarget = $env:CARGO_TARGET_DIR

if (-not (Test-Path -LiteralPath (Join-Path $cargoBin 'cargo.exe') -PathType Leaf)) {
    throw 'Rust Cargo was not found in the current user .cargo\bin directory.'
}
if (-not (Test-Path -LiteralPath $workerSource -PathType Leaf)) {
    throw 'The local Whisper worker is missing: src-tauri\resources\whisper_worker.py'
}

Push-Location -LiteralPath $buildRoot
try {
    $env:PATH = $cargoBin + [IO.Path]::PathSeparator + $originalPath
    # Keep the expected output path even when another project set CARGO_TARGET_DIR.
    $env:CARGO_TARGET_DIR = $targetRoot
    $npmCommand = (Get-Command npm.cmd -ErrorAction Stop).Source

    if (-not (Test-Path -LiteralPath (Join-Path $buildRoot 'node_modules') -PathType Container)) {
        & $npmCommand ci
        if ($LASTEXITCODE -ne 0) { throw "npm ci failed with exit code $LASTEXITCODE" }
    }

    if ($Installer) {
        if (-not (Test-Path -LiteralPath $agySource -PathType Leaf)) {
            throw 'Antigravity CLI is missing: vendor\agy.exe'
        }
        & $npmCommand run tauri -- build --bundles nsis --config src-tauri/tauri.installer.conf.json --ci
        if ($LASTEXITCODE -ne 0) { throw "Tauri installer build failed with exit code $LASTEXITCODE" }

        $config = Get-Content -Raw -LiteralPath 'src-tauri\tauri.conf.json' | ConvertFrom-Json
        $package = Join-Path $targetRoot "release\bundle\nsis\$($config.productName)_$($config.version)_x64-setup.exe"
        if (-not (Test-Path -LiteralPath $package -PathType Leaf)) { throw "The Windows x64 installer was not found: $package" }
        New-Item -ItemType Directory -Path $outputRoot -Force | Out-Null
        $setup = Join-Path $outputRoot "WhileWeTalk-$($config.version)-x64-setup.exe"
        Copy-Item -LiteralPath $package -Destination $setup -Force
        Write-Host "Installer ready: $setup"
        return
    }

    & $npmCommand run tauri -- build --no-bundle
    if ($LASTEXITCODE -ne 0) { throw "Tauri build failed with exit code $LASTEXITCODE" }

    $executableSource = Join-Path $targetRoot 'release\while-we-talk.exe'
    if (-not (Test-Path -LiteralPath $executableSource -PathType Leaf)) {
        throw 'The build finished without src-tauri\target\release\while-we-talk.exe'
    }

    $resourcesOutput = Join-Path $outputRoot 'resources'
    New-Item -ItemType Directory -Path $resourcesOutput -Force | Out-Null
    Copy-Item -LiteralPath $executableSource -Destination (Join-Path $outputRoot 'WhileWeTalk.exe') -Force
    Copy-Item -LiteralPath $workerSource -Destination (Join-Path $resourcesOutput 'whisper_worker.py') -Force
    foreach ($guideName in @('README.md', 'README-build.md')) {
        $guideSource = Join-Path $buildRoot $guideName
        if (Test-Path -LiteralPath $guideSource -PathType Leaf) {
            Copy-Item -LiteralPath $guideSource -Destination (Join-Path $outputRoot $guideName) -Force
        }
    }

    if (Test-Path -LiteralPath $agySource -PathType Leaf) {
        $vendorOutput = Join-Path $outputRoot 'vendor'
        New-Item -ItemType Directory -Path $vendorOutput -Force | Out-Null
        Copy-Item -LiteralPath $agySource -Destination (Join-Path $vendorOutput 'agy.exe') -Force
    } else {
        Write-Warning 'Antigravity CLI was not copied. Set the path to an installed official client in the app.'
    }

    Write-Host "Build ready: $(Join-Path $outputRoot 'WhileWeTalk.exe')"
    Write-Host 'This folder still requires the configured local Python, Whisper model and CUDA libraries.'
} finally {
    $env:PATH = $originalPath
    $env:CARGO_TARGET_DIR = $originalCargoTarget
    Pop-Location
}
