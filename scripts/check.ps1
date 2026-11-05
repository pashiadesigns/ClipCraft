param([switch]$Watch)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $projectRoot
$cargoCommand = Get-Command cargo -ErrorAction SilentlyContinue
$cargoPath = if ($cargoCommand) { $cargoCommand.Source } else { Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe' }
if (-not (Test-Path -LiteralPath $cargoPath)) { throw 'Install Rust before running local checks.' }
$artifactPath = Join-Path $projectRoot 'artifacts'
New-Item -ItemType Directory -Path $artifactPath -Force | Out-Null
function Get-SourceFingerprint {
    $files = Get-ChildItem -LiteralPath (Join-Path $projectRoot 'crates'),(Join-Path $projectRoot 'apps') -Recurse -File | Where-Object Extension -In '.rs','.toml'
    $files += Get-Item -LiteralPath (Join-Path $projectRoot 'Cargo.toml'),(Join-Path $projectRoot 'Cargo.lock')
    return (($files | Sort-Object FullName | ForEach-Object { $_.FullName + ':' + (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash }) -join '|')
}
function Invoke-LocalChecks {
    $checks = @(@('fmt','--all','--','--check'),@('test','--workspace','--locked','--offline'),@('clippy','--workspace','--all-targets','--locked','--offline','--','-D','warnings'))
    $results = @()
    foreach ($arguments in $checks) {
        $name = $arguments[0]
        $log = Join-Path $artifactPath ($name + '.log')
        & $cargoPath @arguments *> $log
        $exitCode = $LASTEXITCODE
        $results += [PSCustomObject]@{check=$name;exit_code=$exitCode;log=$log}
        if ($exitCode -ne 0) { break }
    }
    $passed = @($results | Where-Object exit_code -ne 0).Count -eq 0
    $report = [PSCustomObject]@{time=(Get-Date).ToUniversalTime().ToString('o');passed=$passed;checks=$results}
    $report | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $artifactPath 'checks.json') -Encoding utf8
    Write-Output ('Local checks: ' + $(if ($passed) {'PASS'} else {'FAIL'}) + '. Report: artifacts/checks.json')
    return $passed
}
$lastFingerprint = ''
do {
    $fingerprint = Get-SourceFingerprint
    if ($fingerprint -ne $lastFingerprint) {
        $output = @(Invoke-LocalChecks)
        $passed = $output[-1]
        $output | Select-Object -SkipLast 1 | Write-Output
        $lastFingerprint = $fingerprint
        if (-not $Watch) { if ($passed) { exit 0 } else { exit 1 } }
    }
    if ($Watch) { Start-Sleep -Seconds 10 }
} while ($Watch)
