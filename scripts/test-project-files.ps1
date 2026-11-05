$ErrorActionPreference = 'Stop'
$root = 'Software\ClipCraftAssociationTests\' + [guid]::NewGuid().ToString()
$exe = Join-Path $env:SystemRoot 'System32\notepad.exe'
$other = Join-Path $env:SystemRoot 'System32\cmd.exe'
$base = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey($root)
function ReadValue([string]$Path, [string]$Name) {
    $key=$base.OpenSubKey($Path)
    if ($null -eq $key) { return $null }
    try { $key.GetValue($Name) } finally { $key.Dispose() }
}
try {
    & "$PSScriptRoot/register-project-files.ps1" -Executable $exe -RegistryRoot $root
    if ((ReadValue 'Classes\.iclip' '') -ne 'ClipCraft.ImageClip') { throw 'Unclaimed extension not registered.' }
    if ((ReadValue 'Classes\ClipCraft.ImageClip\shell\open\command' '') -ne ('"'+$exe+'" "%1"')) { throw 'Quoted file command differs.' }
    if ((ReadValue 'ClipCraft\Capabilities\FileAssociations' '.iclip') -ne 'ClipCraft.ImageClip') { throw 'Default Apps capability missing.' }
    if ((ReadValue 'Classes\.vclip' '') -ne 'ClipCraft.VideoClip') { throw 'Video extension not registered.' }
    if ((ReadValue 'Classes\ClipCraft.VideoClip\shell\open\command' '') -ne ('"'+$exe+'" "%1"')) { throw 'Video file command differs.' }
    & "$PSScriptRoot/register-project-files.ps1" -Executable $other -RegistryRoot $root
    & "$PSScriptRoot/register-project-files.ps1" -Executable $exe -RegistryRoot $root -Unregister
    if ((ReadValue 'Classes\ClipCraft.ImageClip\shell\open\command' '') -ne ('"'+$other+'" "%1"')) { throw 'Old uninstall removed newer registration.' }
    $key=$base.OpenSubKey('Classes\.iclip',$true)
    try { $key.SetValue('','Other.ImageClip') } finally {$key.Dispose()}
    & "$PSScriptRoot/register-project-files.ps1" -Executable $other -RegistryRoot $root
    if ((ReadValue 'Classes\.iclip' '') -ne 'Other.ImageClip') { throw 'Existing association was overwritten.' }
    & "$PSScriptRoot/register-project-files.ps1" -Executable $other -RegistryRoot $root -Unregister
    if ((ReadValue 'Classes\.iclip' '') -ne 'Other.ImageClip') { throw 'Foreign association removed.' }
    if ($null -ne (ReadValue 'Classes\ClipCraft.ImageClip\shell\open\command' '')) { throw 'Owned registration retained after uninstall.' }
    if ($null -ne (ReadValue 'Classes\ClipCraft.VideoClip\shell\open\command' '')) { throw 'Owned video registration retained.' }
    Write-Output 'PASS: registration, quoted activation, Default Apps, update ownership and foreign association preservation.'
} finally {
    $base.Dispose()
    # Exact random test subtree; no project, app or user association deletion.
    [Microsoft.Win32.Registry]::CurrentUser.DeleteSubKeyTree($root, $false)
}
