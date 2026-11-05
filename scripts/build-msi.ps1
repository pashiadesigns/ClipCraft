param(
    [Parameter(Mandatory=$true)][string]$Package,
    [Parameter(Mandatory=$true)][string]$Output,
    [Parameter(Mandatory=$true)][ValidatePattern('^\d+\.\d+\.\d+-[a-z0-9.]+$')][string]$Version
)
$ErrorActionPreference='Stop'
$root=Split-Path -Parent $PSScriptRoot
$payload=(Resolve-Path -LiteralPath $Package).Path
$destination=[IO.Path]::GetFullPath($Output)
if (Test-Path -LiteralPath $destination) { throw 'Do not overwrite an existing release package.' }
if (!(Test-Path -LiteralPath (Join-Path $payload 'ClipCraft.exe'))) { throw 'Missing application.' }
$cache=Join-Path $root 'artifacts/installer-tools/wix314'
New-Item -ItemType Directory -Force $cache | Out-Null
$archive=Join-Path $cache 'wix314-binaries.zip'
if (!(Test-Path -LiteralPath $archive)) {
    Invoke-WebRequest 'https://github.com/wixtoolset/wix3/releases/download/wix3141rtm/wix314-binaries.zip' -OutFile $archive
}
# Pin the upstream archive, also recorded by WireGuard's Windows build.
if ((Get-FileHash -LiteralPath $archive).Hash.ToLowerInvariant() -ne '6ac824e1642d6f7277d0ed7ea09411a508f6116ba6fae0aa5f2c7daa2ff43d31') { throw 'WiX archive checksum mismatch.' }
Expand-Archive -LiteralPath $archive -DestinationPath (Join-Path $cache 'bin') -Force
$work=Join-Path $cache ([guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force $work | Out-Null
function Xml([string]$value) { [Security.SecurityElement]::Escape($value) }
function Identifier([string]$value) {
    'I'+[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($value))).Substring(0,24)
}
function ComponentGuid([string]$value) {
    $bytes=[Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes('ClipCraft/MSI/components/v1/'+$value.ToLowerInvariant()))
    ([Guid]::new([byte[]]$bytes[0..15])).ToString('D')
}
$components=[Collections.Generic.List[string]]::new()
function DirectoryContents([string]$directory) {
    $parts=[Collections.Generic.List[string]]::new()
    foreach ($file in Get-ChildItem -LiteralPath $directory -File | Sort-Object Name) {
        $relative=[IO.Path]::GetRelativePath($payload,$file.FullName).Replace('\','/')
        if ($relative -match '(?i)(^|/)(history|preferences|recordings|voiceovers|styles)(/|$)|\.(iclip|vclip|wav|mp4|pdb)$') { throw "Unexpected private/media file: $relative" }
        $id=Identifier $relative
        $components.Add($id)
        $parts.Add('<Component Id="'+$id+'" Guid="'+(ComponentGuid $relative)+'" Win64="yes"><File Id="F'+$id+'" Source="'+(Xml $file.FullName)+'" Name="'+(Xml $file.Name)+'" />')
        $parts.Add('<RegistryValue Root="HKCU" Key="Software\ClipCraft\Installer\Files" Name="'+$id+'" Type="integer" Value="1" KeyPath="yes" />')
        if ($relative -eq 'ClipCraft.exe') {
            $parts.Add('<RegistryValue Root="HKCU" Key="Software\Classes\Applications\ClipCraft.exe\shell\open\command" Type="string" Value="&quot;[#F'+$id+']&quot; &quot;%1&quot;" />')
            foreach ($kind in @(@('Image','iclip'),@('Video','vclip'))) {
                $progid='ClipCraft.'+$kind[0]+'Clip'
                $parts.Add('<RegistryValue Root="HKCU" Key="Software\Classes\'+$progid+'" Type="string" Value="ClipCraft '+$kind[0]+' Clip" />')
                $parts.Add('<RegistryValue Root="HKCU" Key="Software\Classes\'+$progid+'\DefaultIcon" Type="string" Value="&quot;[#F'+$id+']&quot;,0" />')
                $parts.Add('<RegistryValue Root="HKCU" Key="Software\Classes\'+$progid+'\shell\open\command" Type="string" Value="&quot;[#F'+$id+']&quot; &quot;%1&quot;" />')
                $parts.Add('<RegistryValue Root="HKCU" Key="Software\Classes\.'+$kind[1]+'\OpenWithProgids" Name="'+$progid+'" Type="string" Value="" />')
                $parts.Add('<RegistryValue Root="HKCU" Key="Software\ClipCraft\Capabilities\FileAssociations" Name=".'+$kind[1]+'" Type="string" Value="'+$progid+'" />')
            }
            $parts.Add('<RegistryValue Root="HKCU" Key="Software\ClipCraft\Capabilities" Name="ApplicationName" Type="string" Value="ClipCraft" />')
            $parts.Add('<RegistryValue Root="HKCU" Key="Software\ClipCraft\Capabilities" Name="ApplicationDescription" Type="string" Value="Capture and edit image and video clips" />')
            $parts.Add('<RegistryValue Root="HKCU" Key="Software\RegisteredApplications" Name="ClipCraft" Type="string" Value="Software\ClipCraft\Capabilities" />')
            $parts.Add('<Shortcut Id="StartClipCraft" Directory="ProgramMenuFolder" Name="ClipCraft" Target="[#F'+$id+']" WorkingDirectory="INSTALLFOLDER" Icon="AppIcon" IconIndex="0" Advertise="no" />')
        }
        $parts.Add('</Component>')
    }
    foreach ($child in Get-ChildItem -LiteralPath $directory -Directory | Sort-Object Name) {
        $relative=[IO.Path]::GetRelativePath($payload,$child.FullName)
        $id=Identifier ('dir/'+$relative)
        $components.Add('C'+$id)
        $parts.Add('<Directory Id="'+$id+'" Name="'+(Xml $child.Name)+'">'+(DirectoryContents $child.FullName))
        $parts.Add('<Component Id="C'+$id+'" Guid="*" Win64="yes"><RegistryValue Root="HKCU" Key="Software\ClipCraft\Installer\Folders" Name="'+$id+'" Type="integer" Value="1" KeyPath="yes" /><RemoveFolder Id="R'+$id+'" On="uninstall" /></Component></Directory>')
    }
    $parts -join "`n"
}
$contents=DirectoryContents $payload
$refs=($components | ForEach-Object {'<ComponentRef Id="'+$_+'" />'}) -join "`n"
$base=$Version.Split('-')[0]
$source=@"
<?xml version="1.0" encoding="utf-8"?>
<Wix xmlns="http://schemas.microsoft.com/wix/2006/wi">
<Product Id="*" Name="ClipCraft $Version" Language="1033" Version="$base" Manufacturer="Pashia Designs" UpgradeCode="F36D3075-D037-49EB-BB02-041CBFB2E5CC">
<Package InstallerVersion="500" Compressed="yes" InstallScope="perUser" InstallPrivileges="limited" Platform="x64" />
<MajorUpgrade AllowSameVersionUpgrades="yes" DowngradeErrorMessage="A newer ClipCraft version is already installed." />
<MediaTemplate EmbedCab="yes" CompressionLevel="high" />
<Condition Message="ClipCraft requires 64-bit Windows 10 or newer.">Installed OR (VersionNT64 AND WindowsBuild &gt;= 10240)</Condition>
<Property Id="ARPPRODUCTICON" Value="AppIcon" /><Property Id="ARPURLINFOABOUT" Value="https://github.com/pashiadesigns/ClipCraft" />
<Property Id="MSIRESTARTMANAGERCONTROL" Value="Disable" /><Property Id="WIXUI_INSTALLDIR" Value="INSTALLFOLDER" />
<Icon Id="AppIcon" SourceFile="$(Xml (Join-Path $payload 'ClipCraft.exe'))" />
<Directory Id="TARGETDIR" Name="SourceDir"><Directory Id="LocalAppDataFolder"><Directory Id="ProgramsFolder" Name="Programs"><Directory Id="INSTALLFOLDER" Name="ClipCraft">
$contents
<Component Id="InstallFolderCleanup" Guid="*" Win64="yes"><RegistryValue Root="HKCU" Key="Software\ClipCraft\Installer" Name="Installed" Type="integer" Value="1" KeyPath="yes" /><RemoveFolder Id="RemoveInstallFolder" On="uninstall" /><RemoveFolder Id="RemoveEmptyProgramsParent" Directory="ProgramsFolder" On="uninstall" /></Component>
</Directory></Directory></Directory><Directory Id="ProgramMenuFolder" /></Directory>
<Feature Id="Complete" Title="ClipCraft" Level="1">$refs<ComponentRef Id="InstallFolderCleanup" /></Feature>
<UIRef Id="WixUI_InstallDir" /><UIRef Id="WixUI_ErrorProgressText" />
<WixVariable Id="WixUILicenseRtf" Value="$(Xml (Join-Path $PSScriptRoot 'installer/license.rtf'))" />
</Product></Wix>
"@
$wxs=Join-Path $work 'clipcraft.wxs'
$source | Set-Content -LiteralPath $wxs -Encoding utf8
& (Join-Path $cache 'bin/candle.exe') -nologo -arch x64 -out (Join-Path $work 'clipcraft.wixobj') $wxs
if ($LASTEXITCODE) { throw 'MSI compilation failed.' }
# This package is deliberately per-user. Same-version major upgrades allow a
# later beta at the same three-part product version; the public payload is immutable.
& (Join-Path $cache 'bin/light.exe') -nologo -sice:ICE91 -sice:ICE61 -ext (Join-Path $cache 'bin/WixUIExtension.dll') -out $destination (Join-Path $work 'clipcraft.wixobj')
if ($LASTEXITCODE -or !(Test-Path -LiteralPath $destination)) { throw 'MSI linking/validation failed.' }
Write-Output "Built unsigned per-user MSI: $destination"
