# SPDX-License-Identifier: Apache-2.0
#
# Isoloom's configuration library for Windows: one function per step kind of the configuration
# model. A recipe dot-sources it, then calls one function per step, in order. Each function
# ensures a state behind its guard: a second apply changes nothing. Every change writes
# `isoloom: changed <what>` and marks the file $env:ISOLOOM_CHANGED names (when set), which a
# later service step reads to restart its service. A failure throws `isoloom: <why>`.

[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

# Reports a change: its line, and the mark the restart rule reads.
function Write-IsoloomChange {
    param([Parameter(Mandatory)][string] $What)
    Write-Output "isoloom: changed $What"
    if ($env:ISOLOOM_CHANGED) {
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $env:ISOLOOM_CHANGED) | Out-Null
        Set-Content -LiteralPath $env:ISOLOOM_CHANGED -Value 'changed'
    }
}

# Whether an earlier step changed something during this apply: a service restarts then.
function Test-IsoloomRestartDue {
    return [bool]($env:ISOLOOM_CHANGED -and (Test-Path -LiteralPath $env:ISOLOOM_CHANGED))
}

# The SHA-256 of a file, in lowercase hex.
function Get-IsoloomHash {
    param([Parameter(Mandatory)][string] $Path)
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

# The packages step: each Windows feature installed (a server's role or feature, or a client's
# optional feature).
function Install-IsoloomFeature {
    param([Parameter(Mandatory)][string[]] $Name)
    $server = [bool](Get-Command -Name Get-WindowsFeature -ErrorAction SilentlyContinue)
    foreach ($feature in $Name) {
        if ($server) {
            if ((Get-WindowsFeature -Name $feature).Installed) { continue }
            $result = Install-WindowsFeature -Name $feature -IncludeManagementTools
            if (-not $result.Success) { throw "isoloom: installing the feature $feature failed" }
        } else {
            if ((Get-WindowsOptionalFeature -Online -FeatureName $feature).State -eq 'Enabled') { continue }
            Enable-WindowsOptionalFeature -Online -FeatureName $feature -All -NoRestart | Out-Null
        }
        Write-IsoloomChange -What "feature $feature"
    }
}

# A file at To with From's content (To written when it differs).
function Copy-IsoloomFile {
    param([Parameter(Mandatory)][string] $From, [Parameter(Mandatory)][string] $To)
    if ((Test-Path -LiteralPath $To -PathType Leaf) -and ((Get-IsoloomHash -Path $From) -eq (Get-IsoloomHash -Path $To))) {
        return
    }
    $parent = Split-Path -Parent $To
    if ($parent) { New-Item -ItemType Directory -Force -Path $parent | Out-Null }
    Copy-Item -LiteralPath $From -Destination $To -Force
    Write-IsoloomChange -What $To
}

# The copy step: From (from the current folder) at To; a folder puts what it holds into To.
function Copy-IsoloomItem {
    param([Parameter(Mandatory)][string] $From, [Parameter(Mandatory)][string] $To)
    if (-not (Test-Path -LiteralPath $From -PathType Container)) {
        Copy-IsoloomFile -From $From -To $To
        return
    }
    $root = (Resolve-Path -LiteralPath $From).Path.TrimEnd('\', '/')
    if (-not (Test-Path -LiteralPath $To -PathType Container)) {
        New-Item -ItemType Directory -Force -Path $To | Out-Null
        Write-IsoloomChange -What $To
    }
    foreach ($entry in Get-ChildItem -LiteralPath $root -Recurse -Force) {
        $target = Join-Path -Path $To -ChildPath $entry.FullName.Substring($root.Length).TrimStart('\', '/')
        if ($entry.PSIsContainer) {
            if (Test-Path -LiteralPath $target -PathType Container) { continue }
            New-Item -ItemType Directory -Force -Path $target | Out-Null
            Write-IsoloomChange -What $target
        } else {
            Copy-IsoloomFile -From $entry.FullName -To $target
        }
    }
}

# The template step: the file Isoloom filled in (its bytes, in base64) at To.
function Install-IsoloomTemplate {
    param([Parameter(Mandatory)][string] $To, [Parameter(Mandatory)][AllowEmptyString()][string] $Base64)
    $temporary = [IO.Path]::GetTempFileName()
    try {
        [IO.File]::WriteAllBytes($temporary, [Convert]::FromBase64String($Base64))
        Copy-IsoloomFile -From $temporary -To $To
    } finally {
        Remove-Item -LiteralPath $temporary -Force -ErrorAction SilentlyContinue
    }
}

# The file step: Path made a folder, absent, or a download of Url pinned by Sha256.
function Install-IsoloomFile {
    param(
        [Parameter(Mandatory)][string] $Path,
        [Parameter(Mandatory)][ValidateSet('directory', 'absent', 'download')][string] $State,
        [string] $Url,
        [string] $Sha256
    )
    switch ($State) {
        'directory' {
            if (Test-Path -LiteralPath $Path -PathType Container) { return }
            New-Item -ItemType Directory -Force -Path $Path | Out-Null
        }
        'absent' {
            if (-not (Test-Path -LiteralPath $Path)) { return }
            Remove-Item -LiteralPath $Path -Recurse -Force
        }
        'download' {
            if ((Test-Path -LiteralPath $Path -PathType Leaf) -and ((Get-IsoloomHash -Path $Path) -eq $Sha256)) { return }
            $parent = Split-Path -Parent $Path
            if ($parent) { New-Item -ItemType Directory -Force -Path $parent | Out-Null }
            $temporary = "$Path.isoloom-download"
            Invoke-WebRequest -UseBasicParsing -Uri $Url -OutFile $temporary
            $got = Get-IsoloomHash -Path $temporary
            if ($got -ne $Sha256) {
                Remove-Item -LiteralPath $temporary -Force
                throw "isoloom: $Url has SHA-256 $got, not ${Sha256}: the file changed upstream, or the pin is wrong"
            }
            Move-Item -LiteralPath $temporary -Destination $Path -Force
        }
    }
    Write-IsoloomChange -What $Path
}

# A password as a SecureString, built a character at a time.
function ConvertTo-IsoloomSecure {
    param([Parameter(Mandatory)][string] $Phrase)
    $secure = New-Object -TypeName System.Security.SecureString
    foreach ($character in $Phrase.ToCharArray()) { $secure.AppendChar($character) }
    return $secure
}

# Whether local user Name's password is Phrase.
function Test-IsoloomPhrase {
    param([Parameter(Mandatory)][string] $Name, [Parameter(Mandatory)][string] $Phrase)
    Add-Type -AssemblyName System.DirectoryServices.AccountManagement
    $machine = New-Object -TypeName System.DirectoryServices.AccountManagement.PrincipalContext -ArgumentList 'Machine'
    try { return $machine.ValidateCredentials($Name, $Phrase) } finally { $machine.Dispose() }
}

# Local user Name is a member of local group Group.
function Add-IsoloomMember {
    param([Parameter(Mandatory)][string] $Group, [Parameter(Mandatory)][string] $Name)
    $members = Get-LocalGroupMember -Group $Group | ForEach-Object { ($_.Name -split '\\')[-1] }
    if ($members -contains $Name) { return }
    Add-LocalGroupMember -Group $Group -Member $Name
    Write-IsoloomChange -What "$Name in $Group"
}

# The user step: local user Name, with its password (Phrase, when given) and in Groups.
function Add-IsoloomUser {
    param([Parameter(Mandatory)][string] $Name, [string] $Phrase, [string[]] $Groups = @())
    if (-not (Get-LocalUser -Name $Name -ErrorAction SilentlyContinue)) {
        if ($Phrase) {
            New-LocalUser -Name $Name -Password (ConvertTo-IsoloomSecure -Phrase $Phrase) -PasswordNeverExpires | Out-Null
        } else {
            New-LocalUser -Name $Name -NoPassword | Out-Null
        }
        Write-IsoloomChange -What "user $Name"
    } elseif ($Phrase -and -not (Test-IsoloomPhrase -Name $Name -Phrase $Phrase)) {
        Set-LocalUser -Name $Name -Password (ConvertTo-IsoloomSecure -Phrase $Phrase)
        Write-IsoloomChange -What "password of $Name"
    }
    foreach ($group in $Groups) { Add-IsoloomMember -Group $group -Name $Name }
}

# The group step: local group Name.
function Add-IsoloomGroup {
    param([Parameter(Mandatory)][string] $Name)
    if (Get-LocalGroup -Name $Name -ErrorAction SilentlyContinue) { return }
    New-LocalGroup -Name $Name | Out-Null
    Write-IsoloomChange -What "group $Name"
}

# A service Isoloom defines: a startup task (as SYSTEM) running Run (a program and its
# arguments) in Directory, registered again when it changed.
function Register-IsoloomTask {
    param([Parameter(Mandatory)][string] $Name, [Parameter(Mandatory)][string[]] $Run, [string] $Directory)
    $arguments = ($Run | Select-Object -Skip 1 | ForEach-Object { '"' + ($_ -replace '"', '\"') + '"' }) -join ' '
    $settings = @{ Execute = $Run[0] }
    if ($arguments) { $settings.Argument = $arguments }
    if ($Directory) { $settings.WorkingDirectory = $Directory }
    $action = New-ScheduledTaskAction @settings
    $task = Get-ScheduledTask -TaskName "isoloom-$Name" -ErrorAction SilentlyContinue
    $same = $task -and $task.Actions[0].Execute -eq $action.Execute -and $task.Actions[0].Arguments -eq $action.Arguments -and
        $task.Actions[0].WorkingDirectory -eq $action.WorkingDirectory
    if ($same) { return }
    $trigger = New-ScheduledTaskTrigger -AtStartup
    Register-ScheduledTask -TaskName "isoloom-$Name" -Action $action -Trigger $trigger -User 'SYSTEM' -RunLevel Highest -Force | Out-Null
    Write-IsoloomChange -What "task isoloom-$Name"
}

# The service step for a service Isoloom defines: its startup task enabled and running, or
# disabled and stopped.
function Register-IsoloomTaskState {
    param([Parameter(Mandatory)][string] $Name, [Parameter(Mandatory)][bool] $Enabled)
    $task = Get-ScheduledTask -TaskName "isoloom-$Name"
    if (-not $Enabled) {
        if ($task.State -eq 'Running') { Stop-ScheduledTask -TaskName "isoloom-$Name"; Write-IsoloomChange -What "isoloom-$Name stopped" }
        if ($task.State -ne 'Disabled') { Disable-ScheduledTask -TaskName "isoloom-$Name" | Out-Null; Write-IsoloomChange -What "isoloom-$Name disabled" }
        return
    }
    if ($task.State -eq 'Disabled') { Enable-ScheduledTask -TaskName "isoloom-$Name" | Out-Null; Write-IsoloomChange -What "isoloom-$Name enabled" }
    if ((Get-ScheduledTask -TaskName "isoloom-$Name").State -ne 'Running') {
        Start-ScheduledTask -TaskName "isoloom-$Name"
        Write-IsoloomChange -What "isoloom-$Name started"
    } elseif (Test-IsoloomRestartDue) {
        Stop-ScheduledTask -TaskName "isoloom-$Name"
        Start-ScheduledTask -TaskName "isoloom-$Name"
        Write-Output "isoloom: restarted isoloom-$Name"
    }
}

# The service step: Windows service Name started automatically and running (restarted when an
# earlier step changed something), or disabled and stopped.
function Register-IsoloomService {
    param([Parameter(Mandatory)][string] $Name, [Parameter(Mandatory)][bool] $Enabled)
    $service = Get-Service -Name $Name -ErrorAction SilentlyContinue
    if (-not $service) { throw "isoloom: no Windows service ${Name}: install what provides it in an earlier step" }
    if (-not $Enabled) {
        if ($service.StartType -ne 'Disabled') { Set-Service -Name $Name -StartupType Disabled; Write-IsoloomChange -What "$Name disabled" }
        if ($service.Status -ne 'Stopped') { Stop-Service -Name $Name -Force; Write-IsoloomChange -What "$Name stopped" }
        return
    }
    if ($service.StartType -ne 'Automatic') { Set-Service -Name $Name -StartupType Automatic; Write-IsoloomChange -What "$Name enabled" }
    if ($service.Status -ne 'Running') {
        Start-Service -Name $Name
        Write-IsoloomChange -What "$Name started"
    } elseif (Test-IsoloomRestartDue) {
        Restart-Service -Name $Name -Force
        Write-Output "isoloom: restarted $Name"
    }
}

# The command step: Run (a script block's text) after its guard, unless the guard says it
# already ran: Creates (a path whose existence says so) or Unless (a check that succeeds).
function Invoke-IsoloomCommand {
    param([Parameter(Mandatory)][string] $Run, [string] $Creates, [string] $Unless)
    if ($Creates -and (Test-Path -LiteralPath $Creates)) { return }
    if ($Unless -and (Test-IsoloomScript -Text $Unless)) { return }
    if (-not (Test-IsoloomScript -Text $Run -Loud)) { throw "isoloom: the command failed: $Run" }
    Write-IsoloomChange -What 'command'
}

# Whether script Text succeeds: no error, and a native program's exit code 0. Its output is
# shown with Loud, else dropped.
function Test-IsoloomScript {
    param([Parameter(Mandatory)][string] $Text, [switch] $Loud)
    Set-Variable -Name LASTEXITCODE -Scope Global -Value 0
    try {
        if ($Loud) { & ([scriptblock]::Create($Text)) } else { & ([scriptblock]::Create($Text)) | Out-Null }
    } catch {
        if ($Loud) { Write-Error -Message $_ -ErrorAction Continue }
        return $false
    }
    return (Get-Variable -Name LASTEXITCODE -Scope Global -ValueOnly) -eq 0
}

# The firewall step: Port (with Protocol) open in Windows Firewall.
function Enable-IsoloomFirewallPort {
    param([Parameter(Mandatory)][int] $Port, [Parameter(Mandatory)][ValidateSet('TCP', 'UDP')][string] $Protocol)
    $name = "isoloom-$Port-$Protocol"
    if (Get-NetFirewallRule -Name $name -ErrorAction SilentlyContinue) { return }
    New-NetFirewallRule -Name $name -DisplayName "Isoloom $Port/$Protocol" -Direction Inbound -Action Allow -Protocol $Protocol -LocalPort $Port | Out-Null
    Write-IsoloomChange -What "firewall $Port/$Protocol"
}
