param(
    [Parameter(Mandatory=$true)][string]$Package,
    [Parameter(Mandatory=$true)][string]$Installer,
    [Parameter(Mandatory=$true)][string]$Version
)
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted') { throw 'Install tests require an ephemeral GitHub-hosted Windows runner.' }
$root = Split-Path -Parent $PSScriptRoot
$installPath = Join-Path $env:LOCALAPPDATA 'Programs/ClipCraft'
$key = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\ClipCraft'
$start = [Environment]::GetFolderPath('Programs')
$links = @('ClipCraft Editor.lnk','ClipCraft Capture Controller.lnk')
if ((Test-Path -LiteralPath $installPath) -or (Test-Path -LiteralPath $key) -or @($links | Where-Object { Test-Path -LiteralPath (Join-Path $start $_) }).Count) { throw 'Refusing to test over an existing installation.' }
$evidence = Join-Path $root 'artifacts/installer-test'
New-Item -ItemType Directory -Path $evidence -Force | Out-Null
function RunSetup([string]$Path, [string]$Arguments, [int]$Expected = 0) {
    $process = Start-Process -FilePath $Path -ArgumentList $Arguments -WindowStyle Hidden -Wait -PassThru
    if ($process.ExitCode -ne $Expected) { throw "Unexpected installer exit code $($process.ExitCode); expected $Expected" }
}
function CheckPayload {
    foreach ($file in Get-ChildItem -LiteralPath $Package -Recurse -File) {
        $relative = $file.FullName.Substring((Resolve-Path -LiteralPath $Package).Path.Length + 1)
        if ((Get-FileHash -LiteralPath $file.FullName).Hash -ne (Get-FileHash -LiteralPath (Join-Path $installPath $relative)).Hash) { throw "Installed bytes differ: $relative" }
    }
}
# First exercise the exact candidate on a clean machine, before the upgrade case.
RunSetup $Installer '/S'
CheckPayload
RunSetup (Join-Path $installPath 'Uninstall.exe') '/S'
$freshDeadline = [DateTime]::UtcNow.AddSeconds(30)
while ((Test-Path -LiteralPath $key) -and [DateTime]::UtcNow -lt $freshDeadline) { Start-Sleep -Milliseconds 200 }
if ((Test-Path -LiteralPath $key) -or (Test-Path -LiteralPath (Join-Path $installPath 'ClipCraft.exe'))) { throw 'Fresh install removal failed.' }
$prior = Join-Path $evidence 'previous-version-fixture.exe'
& (Join-Path $PSScriptRoot 'build-installer.ps1') -Package $Package -Output $prior -Version '0.1.9-test.1'
RunSetup $prior '/S'
CheckPayload
if ((Get-ItemProperty -LiteralPath $key).DisplayVersion -ne '0.1.9-test.1') { throw 'Fresh install registry missing.' }
$sentinel = Join-Path $installPath 'user-created-keep.txt'
'keep user files' | Set-Content -LiteralPath $sentinel
$external = Join-Path $env:LOCALAPPDATA 'ClipCraft/installer-test-keep.txt'
New-Item -ItemType Directory -Path (Split-Path -Parent $external) -Force | Out-Null
'keep user data' | Set-Content -LiteralPath $external
RunSetup $Installer '/S'
CheckPayload
if ((Get-ItemProperty -LiteralPath $key).DisplayVersion -ne $Version) { throw 'Upgrade did not update the registered version.' }
$shell = New-Object -ComObject WScript.Shell
try {
    foreach ($entry in @(@{Name=$links[0];Arg='--editor';Icon=0},@{Name=$links[1];Arg='--capture-controller';Icon=1})) {
        $link = $shell.CreateShortcut((Join-Path $start $entry.Name))
        if ($link.TargetPath -ne (Join-Path $installPath 'ClipCraft.exe') -or $link.Arguments -ne $entry.Arg -or $link.IconLocation -notmatch (',\s*' + $entry.Icon + '$')) { throw "Start shortcut identity differs: target=$($link.TargetPath); arguments=$($link.Arguments); icon=$($link.IconLocation)" }
        [Runtime.InteropServices.Marshal]::FinalReleaseComObject($link) | Out-Null
    }
} finally { [Runtime.InteropServices.Marshal]::FinalReleaseComObject($shell) | Out-Null }
'repair fixture' | Set-Content -LiteralPath (Join-Path $installPath 'README.md')
RunSetup $Installer '/S'
CheckPayload
# Simulate a running application without launching/capturing the desktop.
$lock = [IO.File]::Open((Join-Path $installPath 'ClipCraft.exe'), 'Open', 'Read', 'Read')
try { RunSetup $Installer '/S' 2 } finally { $lock.Dispose() }
CheckPayload
$uninstall = Join-Path $installPath 'Uninstall.exe'
RunSetup $uninstall '/S'
# NSIS runs removal from a temporary copy so it can delete its own installed EXE.
$deadline = [DateTime]::UtcNow.AddSeconds(30)
while ((Test-Path -LiteralPath $key) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 200 }
if ((Test-Path -LiteralPath $key) -or (Test-Path -LiteralPath (Join-Path $installPath 'ClipCraft.exe'))) { throw 'Uninstall left registered application files.' }
foreach ($file in Get-ChildItem -LiteralPath $Package -Recurse -File) {
    $relative = $file.FullName.Substring((Resolve-Path -LiteralPath $Package).Path.Length + 1)
    if (Test-Path -LiteralPath (Join-Path $installPath $relative)) { throw "Uninstall left a managed file: $relative" }
}
foreach ($link in $links) { if (Test-Path -LiteralPath (Join-Path $start $link)) { throw 'Uninstall left a shortcut.' } }
if ((Get-Content -Raw -LiteralPath $sentinel).Trim() -ne 'keep user files' -or (Get-Content -Raw -LiteralPath $external).Trim() -ne 'keep user data') { throw 'User files were not preserved.' }
@{passed=$true;version=$Version;platform=$env:RUNNER_OS;checks=@('fresh install','upgrade from synthetic previous version','same-version repair','both shortcut resources','locked file refusal','silent uninstall','user data preserved');application_launched=$false;physical_desktop_test=$false} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $evidence 'result.json')
