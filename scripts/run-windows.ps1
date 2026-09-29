$ErrorActionPreference = "Stop"

$repository = Split-Path -Parent $PSScriptRoot
$source = Join-Path $repository "target\x86_64-pc-windows-msvc\debug\lmukit.exe"
$runtimeDirectory = Join-Path $env:LOCALAPPDATA "LMUKit\development"
$destination = Join-Path $runtimeDirectory "lmukit.exe"

New-Item -ItemType Directory -Force $runtimeDirectory | Out-Null

$running = Get-Process -Name "lmukit" -ErrorAction SilentlyContinue |
    Where-Object { [string]::Equals($_.Path, $destination, [System.StringComparison]::OrdinalIgnoreCase) }
if ($running) {
    $running | Stop-Process -Force
    $running | Wait-Process -Timeout 5 -ErrorAction SilentlyContinue
}

for ($attempt = 0; $attempt -lt 10; $attempt++) {
    try {
        Copy-Item $source $destination -Force
        break
    } catch {
        if ($attempt -eq 9) { throw }
        Start-Sleep -Milliseconds 200
    }
}
Start-Process $destination
