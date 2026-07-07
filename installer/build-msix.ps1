# Builds the MSIX package for Microsoft Store submission / local testing.
#
# Usage:
#   powershell -File installer\build-msix.ps1                       # pack only (unsigned, for Store upload)
#   powershell -File installer\build-msix.ps1 -CertThumbprint XXXX  # pack + sign (for local install testing)
#
# Prerequisites:
#   - built app:      target\release\gif-ide.exe (cargo build --release)
#   - ffmpeg bins:    resources\ffmpeg\{ffmpeg.exe, ffprobe.exe, LICENSE.txt}
#   - Windows SDK:    makeappx.exe / signtool.exe
#   - Visual Studio:  vswhere.exe / dumpbin.exe (used by copy-vcredist.ps1)
#
# Notes:
#   - The MSIX uploaded to Partner Center must be UNSIGNED (or the signature is
#     ignored); the Store signs it with a Microsoft certificate on publication.
#   - Signing locally requires the certificate Subject to equal the manifest
#     Publisher (CN=Flupinochan).

param(
    [string]$CertThumbprint = "",
    [string]$SourceExe = "target\release\gif-ide.exe",
    [string]$FfmpegDir = "resources\ffmpeg"
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot

# Locate Windows SDK tools (pick the newest installed kit)
$kitRoot = "C:\Program Files (x86)\Windows Kits\10\bin"
$kit = Get-ChildItem $kitRoot -Directory |
    Where-Object { Test-Path "$($_.FullName)\x64\makeappx.exe" } |
    Sort-Object Name -Descending | Select-Object -First 1
if (-not $kit) { throw "makeappx.exe not found under $kitRoot (install the Windows SDK)" }
$makeappx = "$($kit.FullName)\x64\makeappx.exe"
$signtool = "$($kit.FullName)\x64\signtool.exe"

# Version from Cargo.toml
$version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"' |
    Select-Object -First 1).Matches[0].Groups[1].Value
if (-not $version) { throw "version not found in Cargo.toml" }
Write-Host "version: $version"

# Assemble layout
$layout = "dist\msix-layout"
if (Test-Path $layout) { Remove-Item -Recurse -Force $layout }
New-Item -ItemType Directory -Force "$layout\ffmpeg", "$layout\Assets" | Out-Null

Copy-Item $SourceExe "$layout\"
Copy-Item "$FfmpegDir\ffmpeg.exe", "$FfmpegDir\ffprobe.exe", "$FfmpegDir\LICENSE.txt" "$layout\ffmpeg\"

# Side-by-side deploy the VC++ runtime DLLs so the app runs without the
# Visual C++ Redistributable pre-installed (see copy-vcredist.ps1 for details)
powershell -NoProfile -ExecutionPolicy Bypass -File "$PSScriptRoot\copy-vcredist.ps1" -SourceExe "$layout\gif-ide.exe" -DestDir $layout
if ($LASTEXITCODE -ne 0) { throw "copy-vcredist.ps1 failed" }

# Generate logo assets from the app icon (128x128) with ffmpeg
$ff = "$FfmpegDir\ffmpeg.exe"
& $ff -v error -y -i ui\ico\app.ico -vf "scale=44:44:flags=lanczos" "$layout\Assets\Square44x44Logo.png"
if ($LASTEXITCODE -ne 0) { throw "ffmpeg failed (44x44)" }
& $ff -v error -y -i ui\ico\app.ico -vf "scale=150:150:flags=lanczos" "$layout\Assets\Square150x150Logo.png"
if ($LASTEXITCODE -ne 0) { throw "ffmpeg failed (150x150)" }
& $ff -v error -y -i ui\ico\app.ico -vf "scale=50:50:flags=lanczos" "$layout\Assets\StoreLogo.png"
if ($LASTEXITCODE -ne 0) { throw "ffmpeg failed (50x50)" }

# Manifest with version substituted
(Get-Content installer\msix\AppxManifest.xml -Raw) -replace '\$VERSION\$', $version |
    Out-File "$layout\AppxManifest.xml" -Encoding utf8

# Pack
$msix = "dist\gif-ide-v$version-x64.msix"
if (Test-Path $msix) { Remove-Item -Force $msix }
& $makeappx pack /d $layout /p $msix /o
if ($LASTEXITCODE -ne 0) { throw "makeappx pack failed" }
Write-Host "packed: $msix"

# Optional local signing (required to install the MSIX on this machine)
if ($CertThumbprint) {
    & $signtool sign /fd SHA256 /sha1 $CertThumbprint $msix
    if ($LASTEXITCODE -ne 0) { throw "signtool sign failed" }
    Write-Host "signed with certificate $CertThumbprint"
}
