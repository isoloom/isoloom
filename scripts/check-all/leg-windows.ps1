<#
The Windows host's part of scripts/check-all.sh: the platform gates of the selected repositories,
on the copies check-all.sh put under <Root>/src, with one build folder per repository under
<Root>/target. It prints what leg.sh prints: "@@begin<TAB>repo<TAB>gate", the gate's output,
then "@@result<TAB>repo<TAB>gate<TAB>status<TAB>seconds<TAB>note".

Windows PowerShell 5.1 is enough; scripts/windows-host-setup.ps1 installs the tools.
#>
param(
    [Parameter(Mandatory = $true)][string]$Root,
    [Parameter(Mandatory = $true)][string[]]$Repos
)

$ErrorActionPreference = 'Continue'
$ProgressPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$OutputEncoding = [Text.Encoding]::UTF8
if (-not $env:CARGO_BUILD_JOBS) { $env:CARGO_BUILD_JOBS = '4' }
# Tests run the shell scripts Isoloom renders for Linux machines: git's sh, on PATH as on
# GitHub's Windows runners.
$gitBin = Join-Path $env:ProgramFiles 'Git\bin'
if (-not (Get-Command sh -ErrorAction SilentlyContinue) -and (Test-Path (Join-Path $gitBin 'sh.exe'))) {
    $env:Path = "$gitBin;$env:Path"
}

$Folders = @{ isoloom = 'isoloom'; isothrum = 'isothrum'; launcher = 'cyber-ctf-launcher' }

function Write-Result([string]$Repo, [string]$Gate, [string]$Status, [int]$Seconds, [string]$Note) {
    Write-Output "@@result`t$Repo`t$Gate`t$Status`t$Seconds`t$Note"
}

# Runs one gate through cmd, so stderr joins stdout as plain text. Sets $script:GateOk.
function Invoke-Gate([string]$Repo, [string]$Gate, [string]$Command) {
    Write-Output "@@begin`t$Repo`t$Gate"
    Write-Output "> $Command"
    $watch = [Diagnostics.Stopwatch]::StartNew()
    Push-Location (Join-Path $Root "src/$($Folders[$Repo])")
    cmd /d /c "$Command 2>&1 <NUL" | ForEach-Object { "$_" }
    $code = $LASTEXITCODE
    Pop-Location
    $script:GateOk = ($code -eq 0)
    if ($script:GateOk) { $status = 'pass'; $note = '' } else { $status = 'FAIL'; $note = "exit $code" }
    Write-Result $Repo $Gate $status ([int]$watch.Elapsed.TotalSeconds) $note
}

function Skip-Gate([string]$Repo, [string]$Gate, [string]$Status, [string]$Note) {
    Write-Output "@@begin`t$Repo`t$Gate"
    Write-Result $Repo $Gate $Status 0 $Note
}

function Test-Launcher {
    $version = (cmd /d /c "node --version 2>NUL") -join ''
    if ($version -notlike 'v24.*') {
        Skip-Gate launcher check FAIL "Node 24 needed, found '$version'"
        return
    }
    # The app embeds ../out at compile time; the tests don't need the real one (as in its CI).
    $out = Join-Path $Root 'src/cyber-ctf-launcher/out'
    if (-not (Test-Path (Join-Path $out 'index.html'))) {
        New-Item -ItemType Directory -Force $out | Out-Null
        Set-Content -Path (Join-Path $out 'index.html') -Value '<!doctype html>' -Encoding ASCII
    }
    Invoke-Gate launcher install 'pnpm install --frozen-lockfile'
    if ($script:GateOk) { Invoke-Gate launcher check 'pnpm check' } else { Skip-Gate launcher check skip 'install failed' }
}

$rustc = (cmd /d /c "rustc --version 2>&1") -join ''
$node = (cmd /d /c "node --version 2>&1") -join ''
Write-Output "windows: $([Environment]::OSVersion.VersionString); $rustc; node $node"

foreach ($repo in $Repos) {
    if (-not $Folders.ContainsKey($repo)) { Skip-Gate $repo '-' FAIL 'unknown repository'; continue }
    $env:CARGO_TARGET_DIR = Join-Path $Root "target/$repo"
    switch ($repo) {
        'isoloom' { Invoke-Gate isoloom test 'cargo test --workspace --no-fail-fast' }
        'isothrum' { Invoke-Gate isothrum test 'cargo test --workspace --no-fail-fast' }
        'launcher' { Test-Launcher }
    }
}
