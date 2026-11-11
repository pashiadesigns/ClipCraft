$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$brand = Join-Path $root 'apps/desktop/assets/brand'
$resourceCompiler = Get-Command rc.exe -ErrorAction SilentlyContinue
$compiler = if ($resourceCompiler) { $resourceCompiler.Source } else {
    $sdk = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10/bin'
    Get-ChildItem -Path (Join-Path $sdk '*/x64/rc.exe') -File | Sort-Object FullName -Descending | Select-Object -First 1 -ExpandProperty FullName
}
if (-not $compiler) { throw 'Windows SDK resource compiler rc.exe is required to verify the checked-in resource.' }
$output = Join-Path $root 'artifacts/github/resource'
New-Item -ItemType Directory -Path $output -Force | Out-Null
Push-Location -LiteralPath $brand
try {
    & $compiler /nologo /fo (Join-Path $output 'clipdesigner.res') clipdesigner.rc
    if ($LASTEXITCODE -ne 0) { throw 'Resource compilation failed.' }
} finally { Pop-Location }
$expected = (Get-FileHash -LiteralPath (Join-Path $brand 'clipdesigner.res') -Algorithm SHA256).Hash
$actual = (Get-FileHash -LiteralPath (Join-Path $output 'clipdesigner.res') -Algorithm SHA256).Hash
if ($expected -ne $actual) { throw 'Checked-in Windows resource does not match its RC/ICO inputs.' }
Write-Output 'Windows resource matches its source inputs.'
