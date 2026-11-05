param(
    [string]$Executable = (Join-Path $PSScriptRoot 'ClipCraft.exe'),
    [switch]$Unregister,
    # Tests use an isolated registry subtree, never the real association keys.
    [string]$RegistryRoot = 'Software'
)
$ErrorActionPreference = 'Stop'
if ($RegistryRoot -ne 'Software' -and $RegistryRoot -notmatch '^Software\\ClipCraftAssociationTests\\[a-f0-9-]{36}$') { throw 'Invalid registry test root.' }
$exe = [IO.Path]::GetFullPath($Executable)
if ([IO.Path]::GetExtension($exe) -ne '.exe') { throw 'Choose the ClipCraft executable.' }
if (-not $Unregister -and -not (Test-Path -LiteralPath $exe -PathType Leaf)) { throw 'ClipCraft executable is missing.' }
$command = '"' + $exe + '" "%1"'
$progid = 'ClipCraft.ImageClip'
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
        $progid = "ClipCraft.${type}Clip"
        $extension = if ($type -eq 'Image') { '.iclip' } else { '.vclip' }
        $friendly = "ClipCraft $type Clip"
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
        $base.DeleteSubKeyTree('ClipCraft\Capabilities', $false)
        $key = $base.OpenSubKey('RegisteredApplications', $true)
        if ($null -ne $key) { try { $key.DeleteValue('ClipCraft', $false) } finally { $key.Dispose() } }
    } else {
        WriteValue "Classes\$progid" '' $friendly
        WriteValue "Classes\$progid\DefaultIcon" '' ('"' + $exe + '",0')
        WriteValue "Classes\$progid\shell\open\command" '' $command
        WriteValue "Classes\$progid\shell\open" 'FriendlyAppName' 'ClipCraft'
        # Populate an unclaimed extension only. Windows UserChoice is untouched.
        if ([string]::IsNullOrEmpty((ReadValue "Classes\$extension" ''))) { WriteValue "Classes\$extension" '' $progid }
        WriteValue "Classes\$extension\OpenWithProgids" $progid ([byte[]]@()) 'None'
        WriteValue 'ClipCraft\Capabilities' 'ApplicationName' 'ClipCraft'
        WriteValue 'ClipCraft\Capabilities' 'ApplicationDescription' 'Capture, annotate and share editable image clips.'
        WriteValue 'ClipCraft\Capabilities' 'ApplicationIcon' ('"' + $exe + '",0')
        WriteValue 'ClipCraft\Capabilities\FileAssociations' $extension $progid
        WriteValue 'RegisteredApplications' 'ClipCraft' 'Software\ClipCraft\Capabilities'
    }

    }
} finally { $base.Dispose() }
if ($RegistryRoot -eq 'Software') {
    Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public static class ClipCraftAssociationNotify { [DllImport("shell32.dll")] public static extern void SHChangeNotify(uint e, uint f, IntPtr a, IntPtr b); }' -ErrorAction SilentlyContinue
    [ClipCraftAssociationNotify]::SHChangeNotify(0x08000000, 0, [IntPtr]::Zero, [IntPtr]::Zero)
}
Write-Output 'ClipCraft .iclip and .vclip associations updated. Existing Windows default-app choices are preserved.'
