param([ValidateSet('127.0.0.1','0.0.0.0')][string]$BindAddress='127.0.0.1',[ValidateRange(1024,65535)][int]$Port=4173)
$ErrorActionPreference='Stop'
$clipcraftRoot=Split-Path -Parent $PSScriptRoot
$clipcraftState=Join-Path $clipcraftRoot 'artifacts/selfhost'
if (-not (Test-Path -LiteralPath (Join-Path $clipcraftState 'current.json'))) { throw 'Run node scripts/publish-local-site.mjs first.' }
try {
  $clipcraftHealth=Invoke-RestMethod -Uri "http://127.0.0.1:$Port/__clipcraft/status" -TimeoutSec 2
  if ($clipcraftHealth.service -eq 'ClipCraft website') { Write-Output "ClipCraft already running at http://localhost:$Port/ ($($clipcraftHealth.latest))"; exit 0 }
} catch {}
$clipcraftNode=(Get-Command node -ErrorAction Stop).Source
$clipcraftProcess=Start-Process -FilePath $clipcraftNode -ArgumentList @(('"'+(Join-Path $PSScriptRoot 'self-host-site.mjs')+'"'),$BindAddress,$Port) -WorkingDirectory $clipcraftRoot -WindowStyle Hidden -RedirectStandardOutput (Join-Path $clipcraftState 'server.log') -RedirectStandardError (Join-Path $clipcraftState 'server-error.log') -PassThru
[PSCustomObject]@{pid=$clipcraftProcess.Id;host=$BindAddress;port=$Port;started_at=(Get-Date).ToUniversalTime().ToString('o')} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $clipcraftState 'server.json') -Encoding utf8
Write-Output "Started ClipCraft website on $BindAddress`:$Port (PID $($clipcraftProcess.Id))."
