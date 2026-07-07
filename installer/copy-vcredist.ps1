# Copies the VC++ runtime DLLs that $SourceExe actually depends on into $DestDir
# (side-by-side deployment next to the exe), so the packaged app runs without
# requiring the Visual C++ Redistributable to be pre-installed on the target
# machine. See: https://learn.microsoft.com/cpp/windows/redistributing-visual-cpp-files
#
# The set of DLLs is not hardcoded: it is derived from the actual `dumpbin
# /dependents` output cross-referenced against a list of known VC++ Redist
# DLL names, so it keeps working if Rust/Slint/Skia updates change the
# dependency set.
#
# Usage: powershell -File installer\copy-vcredist.ps1 -SourceExe <path> -DestDir <path>

param(
    [Parameter(Mandatory = $true)][string]$SourceExe,
    [Parameter(Mandatory = $true)][string]$DestDir
)

$ErrorActionPreference = "Stop"

$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
if (-not (Test-Path $vswhere)) { throw "vswhere.exe not found ($vswhere)" }
$vsPath = & $vswhere -latest -property installationPath
if (-not $vsPath) { throw "Visual Studio installation not found via vswhere" }

$msvcTools = Get-ChildItem "$vsPath\VC\Tools\MSVC" -Directory |
    Sort-Object Name -Descending | Select-Object -First 1
if (-not $msvcTools) { throw "VC\Tools\MSVC not found under $vsPath" }
$dumpbin = "$($msvcTools.FullName)\bin\Hostx64\x64\dumpbin.exe"
if (-not (Test-Path $dumpbin)) { throw "dumpbin.exe not found at $dumpbin" }

# DLL names shipped by the VC++ Redistributable. Anything else dumpbin reports
# (kernel32.dll, api-ms-win-crt-*.dll, ...) ships with Windows itself and must
# NOT be bundled.
$redistDllNames = @(
    "vcruntime140.dll", "vcruntime140_1.dll",
    "msvcp140.dll", "msvcp140_1.dll", "msvcp140_2.dll", "msvcp140_atomic_wait.dll",
    "concrt140.dll"
)

$dependents = & $dumpbin /dependents $SourceExe
if ($LASTEXITCODE -ne 0) { throw "dumpbin /dependents failed for $SourceExe" }

$needed = $dependents |
    Select-String -Pattern '^\s+(\S+\.dll)\s*$' |
    ForEach-Object { $_.Matches[0].Groups[1].Value } |
    Where-Object { $redistDllNames -contains $_.ToLower() }

if (-not $needed) {
    Write-Host "no VC++ redistributable DLLs required by $SourceExe"
    exit 0
}

New-Item -ItemType Directory -Force $DestDir | Out-Null
foreach ($dll in $needed) {
    $found = Get-ChildItem "$vsPath\VC\Redist\MSVC" -Recurse -Filter $dll -File |
        Where-Object { $_.FullName -match '\\x64\\' -and $_.FullName -notmatch '\\onecore\\' } |
        Sort-Object FullName -Descending | Select-Object -First 1
    if (-not $found) { throw "$dll is required by $SourceExe but was not found under $vsPath\VC\Redist\MSVC" }
    Copy-Item $found.FullName $DestDir -Force
    Write-Host "copied $dll from $($found.FullName)"
}
