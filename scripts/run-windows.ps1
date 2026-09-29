$ErrorActionPreference = "Stop"

$repository = Split-Path -Parent $PSScriptRoot
$source = Join-Path $repository "target\x86_64-pc-windows-msvc\debug\lmukit.exe"
$runtimeDirectory = Join-Path $env:LOCALAPPDATA "LMUKit\development"
$destination = Join-Path $runtimeDirectory "lmukit.exe"

New-Item -ItemType Directory -Force $runtimeDirectory | Out-Null

Get-Process -Name "lmukit" -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -eq $destination } |
    Stop-Process -Force

Copy-Item $source $destination -Force
Start-Process $destination
