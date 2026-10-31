$ErrorActionPreference='Stop'
$clipcraftRoot=Split-Path -Parent $PSScriptRoot
& (Join-Path $PSScriptRoot 'start-site.ps1')
if ($LASTEXITCODE -ne 0) { throw 'Local website did not start.' }
$clipcraftState=Join-Path $clipcraftRoot 'artifacts/selfhost'
$clipcraftTunnelRecord=Join-Path $clipcraftState 'tunnel.json'
$clipcraftTunnelExe=Join-Path $clipcraftRoot 'artifacts/tools/cloudflared.exe'
if (Test-Path -LiteralPath $clipcraftTunnelRecord) {
  $clipcraftPrevious=Get-Content -LiteralPath $clipcraftTunnelRecord -Raw | ConvertFrom-Json
  $clipcraftExisting=Get-Process -Id $clipcraftPrevious.pid -ErrorAction SilentlyContinue
  if ($clipcraftExisting -and $clipcraftExisting.Path -eq $clipcraftTunnelExe -and $clipcraftPrevious.url) { Write-Output "Existing public link: $($clipcraftPrevious.url)"; exit 0 }
}
if (-not (Test-Path -LiteralPath $clipcraftTunnelExe)) { throw 'Missing verified official cloudflared binary. See docs/release-site.md.' }
$clipcraftStamp=(Get-Date).ToUniversalTime().ToString('yyyyMMdd-HHmmss')
$clipcraftLog=Join-Path $clipcraftState "tunnel-$clipcraftStamp.log"
$clipcraftProcess=Start-Process -FilePath $clipcraftTunnelExe -ArgumentList @('tunnel','--url','http://127.0.0.1:4173','--no-autoupdate','--protocol','http2','--logfile',('"'+$clipcraftLog+'"')) -WindowStyle Hidden -RedirectStandardOutput (Join-Path $clipcraftState "tunnel-$clipcraftStamp-stdout.log") -RedirectStandardError (Join-Path $clipcraftState "tunnel-$clipcraftStamp-stderr.log") -PassThru
$clipcraftPublicUrl=$null
for ($clipcraftAttempt=0;$clipcraftAttempt -lt 30;$clipcraftAttempt++) {
  if (Test-Path -LiteralPath $clipcraftLog) {
    $clipcraftMatch=[regex]::Match((Get-Content -LiteralPath $clipcraftLog -Raw),'https://[a-z0-9-]+\.trycloudflare\.com')
    if ($clipcraftMatch.Success) { $clipcraftPublicUrl=$clipcraftMatch.Value; break }
  }
  if ($clipcraftProcess.HasExited) { throw "Tunnel exited. Inspect $clipcraftLog" }
  Start-Sleep -Milliseconds 500
}
if (-not $clipcraftPublicUrl) { throw "Tunnel URL not reported yet. Inspect $clipcraftLog before starting another process." }
[PSCustomObject]@{pid=$clipcraftProcess.Id;started_at=(Get-Date).ToUniversalTime().ToString('o');kind='quick-tunnel';origin='http://127.0.0.1:4173';url=$clipcraftPublicUrl;log=$clipcraftLog} | ConvertTo-Json | Set-Content -LiteralPath $clipcraftTunnelRecord -Encoding utf8
Write-Output "New public URL: $clipcraftPublicUrl"
