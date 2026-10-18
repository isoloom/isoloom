# IIS with a page naming the machine.
$ErrorActionPreference = "Stop"
Install-WindowsFeature -Name Web-Server | Out-Null
Set-Content -Path C:\inetpub\wwwroot\index.html -Value "<p>hello from $env:COMPUTERNAME</p>"
New-NetFirewallRule -DisplayName "HTTP in" -Direction Inbound -Protocol TCP -LocalPort 80 -Action Allow | Out-Null
"IIS is serving"
