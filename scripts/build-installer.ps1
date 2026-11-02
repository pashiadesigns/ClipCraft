param(
    [Parameter(Mandatory=$true)][string]$Package,
    [Parameter(Mandatory=$true)][string]$Output,
    [Parameter(Mandatory=$true)][ValidatePattern('^\d+\.\d+\.\d+-[a-z0-9.]+$')][string]$Version
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$packagePath = (Resolve-Path -LiteralPath $Package).Path
$outputPath = [IO.Path]::GetFullPath($Output)
if (Test-Path -LiteralPath $outputPath) { throw 'Installer output already exists; do not replace it.' }
$cache = Join-Path $root 'artifacts/installer-tools'
New-Item -ItemType Directory -Path $cache -Force | Out-Null
$archive = Join-Path $cache 'nsis-3.13.zip'
$expected = 'ba63dffc4410ee89193e1cb5a41989991bd77c61068da17e3156d136b7b0b3d8'
if (!(Test-Path -LiteralPath $archive)) {
    $url = 'https://downloads.sourceforge.net/project/nsis/NSIS%203/3.13/nsis-3.13.zip'
    Invoke-WebRequest -Uri $url -OutFile $archive
    if ((Get-FileHash -LiteralPath $archive).Hash.ToLowerInvariant() -ne $expected) {
        $html = Get-Content -Raw -LiteralPath $archive
        if ($html -match 'http-equiv="refresh" content="\d+; url=([^"]+)"') {
            $mirror = [Net.WebUtility]::HtmlDecode($Matches[1])
            if (!$mirror.StartsWith($url + '?', [StringComparison]::Ordinal)) { throw 'Unexpected NSIS mirror redirect.' }
            Invoke-WebRequest -Uri $mirror -OutFile $archive
        }
    }
}
if ((Get-FileHash -LiteralPath $archive).Hash.ToLowerInvariant() -ne $expected) { throw 'NSIS archive checksum mismatch; no extracted code was run.' }
# Re-extract the verified archive; do not execute a stale modified compiler cache.
Expand-Archive -LiteralPath $archive -DestinationPath $cache -Force
$compiler = Join-Path $cache 'nsis-3.13/makensis.exe'
$staging = Join-Path $cache ([guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $staging -Force | Out-Null
$files = @(Get-ChildItem -LiteralPath $packagePath -Recurse -File | Sort-Object FullName)
if (!$files.Count -or !(Test-Path -LiteralPath (Join-Path $packagePath 'ClipCraft.exe'))) { throw 'Incomplete application package.' }
function NsisLiteral([string]$Value) { return $Value.Replace('$','$$').Replace('"','$\"') }
$install = @()
$remove = @()
foreach ($file in $files) {
    $relative = $file.FullName.Substring($packagePath.Length + 1)
    $parent = Split-Path -Parent $relative
    $install += 'SetOutPath "$INSTDIR' + $(if ($parent) { '\' + (NsisLiteral $parent) } else { '' }) + '"'
    $install += 'File "' + (NsisLiteral $file.FullName) + '"'
    $remove += 'Delete "$INSTDIR\' + (NsisLiteral $relative) + '"'
}
$directories = @(Get-ChildItem -LiteralPath $packagePath -Recurse -Directory | Sort-Object { $_.FullName.Length } -Descending)
foreach ($dir in $directories) { $remove += 'RMDir "$INSTDIR\' + (NsisLiteral $dir.FullName.Substring($packagePath.Length + 1)) + '"' }
$installFile = Join-Path $staging 'install.nsh'
$removeFile = Join-Path $staging 'remove.nsh'
$install | Set-Content -LiteralPath $installFile -Encoding utf8
$remove | Set-Content -LiteralPath $removeFile -Encoding utf8
$size = [math]::Ceiling(($files | Measure-Object Length -Sum).Sum / 1024)
& $compiler /NOCONFIG /V2 "/DVERSION=$Version" "/DBASE_VERSION=$($Version.Split('-')[0])" "/DREPO=$root" "/DOUTPUT=$outputPath" "/DINSTALL_FILES=$installFile" "/DREMOVE_FILES=$removeFile" "/DSIZE_KB=$size" (Join-Path $PSScriptRoot 'installer/clipcraft.nsi')
if ($LASTEXITCODE -ne 0 -or !(Test-Path -LiteralPath $outputPath)) { throw 'Installer compilation failed.' }
Write-Output "Built unsigned installer: $outputPath"
