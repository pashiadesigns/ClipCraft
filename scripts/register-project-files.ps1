param(
    [string]$Executable = (Join-Path $PSScriptRoot 'ClipDesigner.exe'),
    [switch]$Unregister,
    # Tests use an isolated registry subtree, never the real association keys.
    [string]$RegistryRoot = 'Software'
)
$ErrorActionPreference = 'Stop'
if ($RegistryRoot -ne 'Software' -and $RegistryRoot -notmatch '^Software\\ClipDesignerAssociationTests\\[a-f0-9-]{36}$') { throw 'Invalid registry test root.' }
$exe = [IO.Path]::GetFullPath($Executable)
if ([IO.Path]::GetExtension($exe) -ne '.exe') { throw 'Choose the ClipDesigner executable.' }
if (-not $Unregister -and -not (Test-Path -LiteralPath $exe -PathType Leaf)) { throw 'ClipDesigner executable is missing.' }
$command = '"' + $exe + '" "%1"'
$progid = 'ClipDesigner.ImageClip'
$base = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey($RegistryRoot)
function WriteValue([string]$Path, [string]$Name, [object]$Value, [Microsoft.Win32.RegistryValueKind]$Kind = 'String') {
    $key = $base.CreateSubKey($Path)
    try { $key.SetValue($Name, $Value, $Kind) } finally { $key.Dispose() }
}
function ReadValue([string]$Path, [string]$Name) {
    $key = $base.OpenSubKey($Path)
    if ($null -eq $key) { return $null }
    try { return $key.GetValue($Name) } finally { $key.Dispose() }
}
try {
    foreach ($type in @('Image','Video')) {
        $progid = "ClipDesigner.${type}Clip"
        $extension = if ($type -eq 'Image') { '.iclip' } else { '.vclip' }
        $friendly = "ClipDesigner $type Clip"
    if ($Unregister) {
        # An uninstall must not remove an association a newer/portable copy owns.
        if ((ReadValue "Classes\$progid\shell\open\command" '') -ne $command) { continue }
        if ((ReadValue "Classes\$extension" '') -eq $progid) {
            $key = $base.OpenSubKey("Classes\$extension", $true)
            try { $key.DeleteValue('', $false) } finally { $key.Dispose() }
        }
        $key = $base.OpenSubKey("Classes\$extension\OpenWithProgids", $true)
        if ($null -ne $key) { try { $key.DeleteValue($progid, $false) } finally { $key.Dispose() } }
        $base.DeleteSubKeyTree("Classes\$progid", $false)
        $base.DeleteSubKeyTree('ClipDesigner\Capabilities', $false)
        $key = $base.OpenSubKey('RegisteredApplications', $true)
        if ($null -ne $key) { try { $key.DeleteValue('ClipDesigner', $false) } finally { $key.Dispose() } }
    } else {
        # ClipDesigner was called ClipCraft through 1.0.0-beta.3. Retire that
        # registration so Explorer offers one current entry per file type.
        $legacy = "ClipCraft.${type}Clip"
        $claimed = (ReadValue "Classes\$extension" '') -eq $legacy
        $key = $base.OpenSubKey("Classes\$extension\OpenWithProgids", $true)
        if ($null -ne $key) { try { $key.DeleteValue($legacy, $false) } finally { $key.Dispose() } }
        $base.DeleteSubKeyTree("Classes\$legacy", $false)
        if ($claimed) { WriteValue "Classes\$extension" '' $progid }
        WriteValue "Classes\$progid" '' $friendly
        WriteValue "Classes\$progid\DefaultIcon" '' ('"' + $exe + '",0')
        WriteValue "Classes\$progid\shell\open\command" '' $command
        WriteValue "Classes\$progid\shell\open" 'FriendlyAppName' 'ClipDesigner'
        # Populate an unclaimed extension only. Windows UserChoice is untouched.
        if ([string]::IsNullOrEmpty((ReadValue "Classes\$extension" ''))) { WriteValue "Classes\$extension" '' $progid }
        WriteValue "Classes\$extension\OpenWithProgids" $progid ([byte[]]@()) 'None'
        WriteValue 'ClipDesigner\Capabilities' 'ApplicationName' 'ClipDesigner'
        WriteValue 'ClipDesigner\Capabilities' 'ApplicationDescription' 'Capture, annotate and share editable image clips.'
        WriteValue 'ClipDesigner\Capabilities' 'ApplicationIcon' ('"' + $exe + '",0')
        WriteValue 'ClipDesigner\Capabilities\FileAssociations' $extension $progid
        WriteValue 'RegisteredApplications' 'ClipDesigner' 'Software\ClipDesigner\Capabilities'
    }

    }
    if (-not $Unregister) {
        $base.DeleteSubKeyTree('ClipCraft', $false)
        $key = $base.OpenSubKey('RegisteredApplications', $true)
        if ($null -ne $key) { try { $key.DeleteValue('ClipCraft', $false) } finally { $key.Dispose() } }
    }
} finally { $base.Dispose() }
if ($RegistryRoot -eq 'Software') {
    Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public static class ClipDesignerAssociationNotify { [DllImport("shell32.dll")] public static extern void SHChangeNotify(uint e, uint f, IntPtr a, IntPtr b); }' -ErrorAction SilentlyContinue
    [ClipDesignerAssociationNotify]::SHChangeNotify(0x08000000, 0, [IntPtr]::Zero, [IntPtr]::Zero)
}
Write-Output 'ClipDesigner .iclip and .vclip associations updated. Existing Windows default-app choices are preserved.'
