<#
Prepares a Windows host for `cargo xtask check-all` (its windows host) and for the Windows host
proof: git, the Visual Studio 2022 Build Tools with the C++ workload, Rust with the MSVC toolchain
(rustfmt, clippy), cargo-nextest (Isoloom's `cargo xtask test`) and cargo-machete (the launcher's
`pnpm quality` runs it), Node 24 and pnpm for the launcher, then Vagrant, Terraform and
PowerShell 7 for the real-target runs, and long paths turned on. Everything comes from winget,
except cargo-nextest and cargo-machete (cargo install).

Run it once, as an administrator, in Windows PowerShell (`cargo xtask host-setup` runs it once
Rust is there):

  powershell -ExecutionPolicy Bypass -File xtask\host-setup\windows.ps1             install what is missing
  powershell -ExecutionPolicy Bypass -File xtask\host-setup\windows.ps1 -CheckOnly  report only

What is there already is left alone, so running it again changes nothing. check-all reaches
the host over the OpenSSH server (Settings > System > Optional features), with a key in
C:\ProgramData\ssh\administrators_authorized_keys for an administrator account.
#>
param([switch]$CheckOnly)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
# The launcher's Node 24, from winget's OpenJS.NodeJS.LTS.
$NodeVersion = '24.20.0'
$CargoMacheteVersion = '0.9.2'
$CargoNextestVersion = '0.9.148'
$VsWhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$VcTools = 'Microsoft.VisualStudio.Component.VC.Tools.x86.x64'

function Update-SessionPath {
    $env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' +
        [Environment]::GetEnvironmentVariable('Path', 'User') + ';' + (Join-Path $env:USERPROFILE '.cargo\bin')
}

function Get-Version([string]$Command, [string[]]$Arguments) {
    if (-not (Get-Command $Command -ErrorAction SilentlyContinue)) { return $null }
    try { return ((& $Command @Arguments 2>$null) | Select-Object -First 1) } catch { return $null }
}

function Install-Package([string]$Id, [string[]]$Extra = @()) {
    $arguments = @('install', '--id', $Id, '--exact', '--silent', '--accept-package-agreements',
        '--accept-source-agreements', '--disable-interactivity') + $Extra
    & winget @arguments
    if ($LASTEXITCODE -ne 0) { throw "winget install $Id failed (exit $LASTEXITCODE)" }
    Update-SessionPath
}

function Test-VcTools {
    if (-not (Test-Path $VsWhere)) { return $null }
    $path = & $VsWhere -products * -requires $VcTools -property installationPath | Select-Object -First 1
    if ($path) { return "C++ build tools in $path" }
    return $null
}

function Install-VcTools {
    $installed = if (Test-Path $VsWhere) { & $VsWhere -products * -property installationPath | Select-Object -First 1 }
    if ($installed) {
        # Build Tools without the C++ workload: add it.
        $setup = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\setup.exe'
        & $setup modify --installPath $installed --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended --quiet --norestart --wait
        if ($LASTEXITCODE -ne 0 -and $LASTEXITCODE -ne 3010) { throw "Visual Studio Installer failed (exit $LASTEXITCODE)" }
    } else {
        Install-Package 'Microsoft.VisualStudio.2022.BuildTools' @('--override',
            '--wait --quiet --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended')
    }
}

function Test-Rust {
    # Without rustup, `& rustup` throws (the preference is Stop) instead of returning nothing.
    if (-not (Get-Command rustup -ErrorAction SilentlyContinue)) { return $null }
    $components = (& rustup component list --installed 2>$null) -join ' '
    $default = (& rustup default 2>$null) -join ' '
    # Any MSVC toolchain as the default: stable, or the one rust-toolchain.toml pins.
    if ($default -like '*-x86_64-pc-windows-msvc*' -and $components -like '*rustfmt*' -and $components -like '*clippy*') {
        return Get-Version rustc @('--version')
    }
    return $null
}

function Install-Rust {
    if (-not (Get-Command rustup -ErrorAction SilentlyContinue)) { Install-Package 'Rustlang.Rustup' }
    & rustup toolchain install stable-x86_64-pc-windows-msvc --profile minimal -c rustfmt -c clippy
    & rustup default stable-x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) { throw 'rustup failed' }
}

function Test-Machete {
    $version = Get-Version cargo @('machete', '--version')
    if ($version -like "$CargoMacheteVersion*") { return "cargo-machete $version" }
    return $null
}

function Test-Nextest {
    $version = Get-Version cargo @('nextest', '--version')
    if ($version -like "cargo-nextest $CargoNextestVersion*") { return $version }
    return $null
}

function Test-Node {
    $version = Get-Version node @('--version')
    if ($version -like 'v24.*') { return $version }
    return $null
}

function Test-LongPaths {
    $windows = (Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem').LongPathsEnabled
    $git = (& git config --system --get core.longpaths 2>$null)
    if ($windows -eq 1 -and $git -eq 'true') { return 'Windows and git' }
    return $null
}

function Enable-LongPaths {
    Set-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem' -Name LongPathsEnabled -Value 1 -Type DWord
    & git config --system core.longpaths true
}

$steps = @(
    @{ Name = 'git'; Test = { Get-Version git @('--version') }; Install = { Install-Package 'Git.Git' } },
    @{ Name = 'Visual Studio 2022 Build Tools, C++ workload'; Test = { Test-VcTools }; Install = { Install-VcTools } },
    @{ Name = 'Rust, MSVC toolchain, rustfmt and clippy'; Test = { Test-Rust }; Install = { Install-Rust } },
    @{ Name = "cargo-machete $CargoMacheteVersion"; Test = { Test-Machete }; Install = { & cargo install --locked "cargo-machete@$CargoMacheteVersion"; if ($LASTEXITCODE -ne 0) { throw 'cargo install failed' } } },
    @{ Name = "cargo-nextest $CargoNextestVersion"; Test = { Test-Nextest }; Install = { & cargo install --locked "cargo-nextest@$CargoNextestVersion"; if ($LASTEXITCODE -ne 0) { throw 'cargo install failed' } } },
    @{ Name = "Node $NodeVersion"; Test = { Test-Node }; Install = { Install-Package 'OpenJS.NodeJS.LTS' @('--version', $NodeVersion) } },
    @{ Name = 'pnpm'; Test = { Get-Version pnpm @('--version') }; Install = { Install-Package 'pnpm.pnpm' } },
    @{ Name = 'Vagrant'; Test = { Get-Version vagrant @('--version') }; Install = { Install-Package 'Hashicorp.Vagrant' } },
    @{ Name = 'Terraform'; Test = { Get-Version terraform @('version') }; Install = { Install-Package 'Hashicorp.Terraform' } },
    @{ Name = 'PowerShell 7 (pwsh)'; Test = { Get-Version pwsh @('--version') }; Install = { Install-Package 'Microsoft.PowerShell' } },
    @{ Name = 'Long paths'; Test = { Test-LongPaths }; Install = { Enable-LongPaths } }
)

$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $CheckOnly -and -not $admin) { throw 'Run it as an administrator (the Build Tools, the MSIs and long paths need it).' }

Update-SessionPath
$failed = $false
foreach ($step in $steps) {
    $found = & $step.Test
    if ($found) { Write-Output ('ok        {0} ({1})' -f $step.Name, $found); continue }
    if ($CheckOnly) { Write-Output ('missing   {0}' -f $step.Name); continue }
    Write-Output ('install   {0}' -f $step.Name)
    try {
        & $step.Install
        Write-Output ('          {0}' -f (& $step.Test))
    } catch {
        Write-Output ('failed    {0}: {1}' -f $step.Name, $_.Exception.Message)
        $failed = $true
    }
}
if ($failed) { exit 1 }
if (-not $CheckOnly) { Write-Output 'Done. A new SSH session picks up the new PATH; Vagrant may ask for a restart.' }
