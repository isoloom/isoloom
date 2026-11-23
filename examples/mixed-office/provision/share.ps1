# A share for the office: readable by everyone on the network.
$ErrorActionPreference = "Stop"
New-Item -ItemType Directory -Force C:\Shares\Public | Out-Null
Set-Content C:\Shares\Public\welcome.txt "Welcome to the office share."
if (-not (Get-SmbShare -Name Public -ErrorAction SilentlyContinue)) {
    New-SmbShare -Name Public -Path C:\Shares\Public -ReadAccess Everyone | Out-Null
}
Enable-NetFirewallRule -DisplayGroup "File and Printer Sharing"
