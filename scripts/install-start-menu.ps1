param(
    [string]$Executable = (Join-Path $PSScriptRoot 'ClipCraft.exe'),
    [string]$Destination = ([Environment]::GetFolderPath('Programs'))
)
$ErrorActionPreference = 'Stop'
$exe = (Resolve-Path -LiteralPath $Executable).Path
if ([IO.Path]::GetExtension($exe) -ne '.exe') { throw 'Choose the ClipCraft executable.' }
$destinationPath = [IO.Path]::GetFullPath($Destination)
New-Item -ItemType Directory -Path $destinationPath -Force | Out-Null
$shell = New-Object -ComObject WScript.Shell
try {
    foreach ($entry in @(
        @{Name='ClipCraft'; Arguments=''; Icon=101}
    )) {
        $path = Join-Path $destinationPath ($entry.Name + '.lnk')
        $shortcut = $shell.CreateShortcut($path)
        $shortcut.TargetPath = $exe
        $shortcut.Arguments = $entry.Arguments
        $shortcut.WorkingDirectory = Split-Path -Parent $exe
        $shortcut.IconLocation = "$exe,-$($entry.Icon)"
        $shortcut.Description = $entry.Name
        $shortcut.Save()
        [Runtime.InteropServices.Marshal]::FinalReleaseComObject($shortcut) | Out-Null
    }
} finally { [Runtime.InteropServices.Marshal]::FinalReleaseComObject($shell) | Out-Null }
Write-Output "Added ClipCraft shortcut in $destinationPath"
