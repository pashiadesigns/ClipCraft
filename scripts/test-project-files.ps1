$ErrorActionPreference = 'Stop'
$root = 'Software\ClipDesignerAssociationTests\' + [guid]::NewGuid().ToString()
$exe = Join-Path $env:SystemRoot 'System32\notepad.exe'
$other = Join-Path $env:SystemRoot 'System32\cmd.exe'
$base = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey($root)
function ReadValue([string]$Path, [string]$Name) {
    $key=$base.OpenSubKey($Path)
    if ($null -eq $key) { return $null }
    try { $key.GetValue($Name) } finally { $key.Dispose() }
}
function SeedValue([string]$Path, [string]$Name, [string]$Value) {
    $key=$base.CreateSubKey($Path)
    try { $key.SetValue($Name, $Value) } finally { $key.Dispose() }
}
try {
    # A portable 1.0.0-beta.3 registration under the former ClipCraft name.
    SeedValue 'Classes\.iclip' '' 'ClipCraft.ImageClip'
    SeedValue 'Classes\.iclip\OpenWithProgids' 'ClipCraft.ImageClip' ''
    SeedValue 'Classes\ClipCraft.ImageClip\shell\open\command' '' ('"'+$exe+'" "%1"')
    SeedValue 'ClipCraft\Capabilities' 'ApplicationName' 'ClipCraft'
    SeedValue 'RegisteredApplications' 'ClipCraft' 'Software\ClipCraft\Capabilities'
    & "$PSScriptRoot/register-project-files.ps1" -Executable $exe -RegistryRoot $root
    if ($null -ne $base.OpenSubKey('Classes\ClipCraft.ImageClip')) { throw 'Legacy ProgID retained.' }
    if ($null -ne (ReadValue 'Classes\.iclip\OpenWithProgids' 'ClipCraft.ImageClip')) { throw 'Legacy Open With entry retained.' }
    if ($null -ne $base.OpenSubKey('ClipCraft')) { throw 'Legacy capabilities retained.' }
    if ($null -ne (ReadValue 'RegisteredApplications' 'ClipCraft')) { throw 'Legacy Default Apps entry retained.' }
    if ((ReadValue 'Classes\.iclip' '') -ne 'ClipDesigner.ImageClip') { throw 'Unclaimed extension not registered.' }
    if ((ReadValue 'Classes\ClipDesigner.ImageClip\shell\open\command' '') -ne ('"'+$exe+'" "%1"')) { throw 'Quoted file command differs.' }
    if ((ReadValue 'ClipDesigner\Capabilities\FileAssociations' '.iclip') -ne 'ClipDesigner.ImageClip') { throw 'Default Apps capability missing.' }
    if ((ReadValue 'Classes\.vclip' '') -ne 'ClipDesigner.VideoClip') { throw 'Video extension not registered.' }
    if ((ReadValue 'Classes\ClipDesigner.VideoClip\shell\open\command' '') -ne ('"'+$exe+'" "%1"')) { throw 'Video file command differs.' }
    & "$PSScriptRoot/register-project-files.ps1" -Executable $other -RegistryRoot $root
    & "$PSScriptRoot/register-project-files.ps1" -Executable $exe -RegistryRoot $root -Unregister
    if ((ReadValue 'Classes\ClipDesigner.ImageClip\shell\open\command' '') -ne ('"'+$other+'" "%1"')) { throw 'Old uninstall removed newer registration.' }
    $key=$base.OpenSubKey('Classes\.iclip',$true)
    try { $key.SetValue('','Other.ImageClip') } finally {$key.Dispose()}
    & "$PSScriptRoot/register-project-files.ps1" -Executable $other -RegistryRoot $root
    if ((ReadValue 'Classes\.iclip' '') -ne 'Other.ImageClip') { throw 'Existing association was overwritten.' }
    & "$PSScriptRoot/register-project-files.ps1" -Executable $other -RegistryRoot $root -Unregister
    if ((ReadValue 'Classes\.iclip' '') -ne 'Other.ImageClip') { throw 'Foreign association removed.' }
    if ($null -ne (ReadValue 'Classes\ClipDesigner.ImageClip\shell\open\command' '')) { throw 'Owned registration retained after uninstall.' }
    if ($null -ne (ReadValue 'Classes\ClipDesigner.VideoClip\shell\open\command' '')) { throw 'Owned video registration retained.' }
    Write-Output 'PASS: legacy ClipCraft cleanup, registration, quoted activation, Default Apps, update ownership and foreign association preservation.'
} finally {
    $base.Dispose()
    # Exact random test subtree; no project, app or user association deletion.
    [Microsoft.Win32.Registry]::CurrentUser.DeleteSubKeyTree($root, $false)
}
