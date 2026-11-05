param([Parameter(Mandatory=$true)][string]$Package,[Parameter(Mandatory=$true)][string]$Installer)
$ErrorActionPreference='Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted') { throw 'Lifecycle tests require a disposable GitHub-hosted Windows runner.' }
$install=Join-Path $env:LOCALAPPDATA 'Programs/ClipCraft'
$data=Join-Path $env:LOCALAPPDATA 'ClipCraft'
if ((Test-Path -LiteralPath $install) -or (Test-Path -LiteralPath $data)) { throw 'Refusing to test over existing ClipCraft data.' }
$payload=(Resolve-Path -LiteralPath $Package).Path
$msi=(Resolve-Path -LiteralPath $Installer).Path
$logRoot=Join-Path (Get-Location) 'artifacts/installer-test'
New-Item -ItemType Directory -Force $logRoot | Out-Null
function RunMsi([string]$Arguments,[string]$Name) {
    $log=Join-Path $logRoot "$Name.log"
    $process=Start-Process msiexec.exe -ArgumentList "$Arguments /qn /norestart /l*v `"$log`"" -WindowStyle Hidden -Wait -PassThru
    if ($process.ExitCode -notin @(0,3010)) { throw "$Name failed with code $($process.ExitCode)." }
}
function CheckPayload {
    foreach ($file in Get-ChildItem -LiteralPath $payload -File -Recurse) {
        $relative=[IO.Path]::GetRelativePath($payload,$file.FullName)
        if ((Get-FileHash -LiteralPath $file.FullName).Hash -ne (Get-FileHash -LiteralPath (Join-Path $install $relative)).Hash) { throw "Payload differs: $relative" }
    }
    foreach ($kind in @('Image','Video')) {
        $command=Get-Item -LiteralPath "HKCU:\Software\Classes\ClipCraft.${kind}Clip\shell\open\command"
        if ($command.GetValue('') -ne ('"'+(Join-Path $install 'ClipCraft.exe')+'" "%1"')) { throw 'Project open command differs.' }
    }
    $link=Join-Path ([Environment]::GetFolderPath('Programs')) 'ClipCraft.lnk'
    $shell=New-Object -ComObject WScript.Shell
    try { $shortcut=$shell.CreateShortcut($link); if ($shortcut.TargetPath -ne (Join-Path $install 'ClipCraft.exe')) {throw 'Shortcut target differs.'} } finally { [Runtime.InteropServices.Marshal]::FinalReleaseComObject($shell) | Out-Null }
}
RunMsi "/i `"$msi`"" 'install'
CheckPayload
if (Test-Path -LiteralPath $data) { throw 'Installer populated a user Library or preferences.' }
New-Item -ItemType Directory -Force $data | Out-Null
'user data must survive' | Set-Content -LiteralPath (Join-Path $data 'keep.txt')
'user file must survive' | Set-Content -LiteralPath (Join-Path $install 'keep.txt')
'damaged fixture' | Set-Content -LiteralPath (Join-Path $install 'README.md')
RunMsi "/fa `"$msi`"" 'repair'
CheckPayload
RunMsi "/x `"$msi`"" 'uninstall'
foreach ($file in Get-ChildItem -LiteralPath $payload -File -Recurse) {
    if (Test-Path -LiteralPath (Join-Path $install ([IO.Path]::GetRelativePath($payload,$file.FullName)))) { throw 'Uninstall left a managed payload file.' }
}
foreach ($path in @((Join-Path $data 'keep.txt'),(Join-Path $install 'keep.txt'))) { if (!(Test-Path -LiteralPath $path)) {throw 'Uninstall removed user data.'} }
foreach ($kind in @('Image','Video')) {if (Test-Path -LiteralPath "HKCU:\Software\Classes\ClipCraft.${kind}Clip") {throw 'Uninstall left project registration.'}}
if (Test-Path -LiteralPath (Join-Path ([Environment]::GetFolderPath('Programs')) 'ClipCraft.lnk')) {throw 'Uninstall left shortcut.'}
@{passed=$true; checks=@('fresh install','exact MSI/ZIP payload','empty user profile','image/video associations','Start shortcut','repair','uninstall','user data preservation'); time=[DateTime]::UtcNow.ToString('o')} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $logRoot 'result.json') -Encoding utf8
Write-Output 'MSI install, repair, removal and data-preservation checks passed.'
