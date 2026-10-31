param([Parameter(Mandatory=$true)][ValidatePattern('^\d+\.\d+\.\d+(-[a-z0-9.]+)?$')][string]$Version)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $projectRoot
$notesPath = Join-Path $projectRoot "releases\$Version.json"
if (-not (Test-Path -LiteralPath $notesPath)) { throw "Write release notes first: releases/$Version.json" }
$notes = Get-Content -Raw -LiteralPath $notesPath | ConvertFrom-Json
if ($notes.version -ne $Version) { throw 'Version differs from release notes.' }
if ($notes.limitations -isnot [Array] -or @($notes.limitations | Where-Object { $_ -isnot [string] }).Count) {
    throw 'Release notes require a limitations array of strings. Nothing has been staged.'
}
$cargoManifest = Get-Content -Raw -LiteralPath (Join-Path $projectRoot 'Cargo.toml')
$baseVersion = $Version.Split('-')[0]
if ($cargoManifest -notmatch ('(?m)^version\s*=\s*"' + [regex]::Escape($baseVersion) + '"')) { throw 'Release version differs from Cargo workspace version.' }
$siteRoot = Join-Path $projectRoot 'sites\capture-workbench'
$downloadRoot = Join-Path $siteRoot 'release-assets'
$fileName = "clipcraft-$Version-windows-x64.zip"
$downloadPath = Join-Path $downloadRoot $fileName
if ((Test-Path -LiteralPath $downloadPath) -or (Test-Path -LiteralPath (Join-Path $projectRoot "artifacts\releases\$Version\$fileName"))) { throw 'Release artifacts are immutable. Reuse the existing prepared release or create a new version.' }
$cargoCommand = Get-Command cargo -ErrorAction SilentlyContinue
$cargoPath = if ($cargoCommand) { $cargoCommand.Source } else { Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe' }
$pwshPath = (Get-Process -Id $PID).Path
& $pwshPath -NoProfile -File (Join-Path $PSScriptRoot 'check.ps1')
if ($LASTEXITCODE -ne 0) { throw 'Local verification failed. No release was staged.' }
$releaseRoot = Join-Path $projectRoot "artifacts\releases\$Version"
New-Item -ItemType Directory -Path $releaseRoot -Force | Out-Null
& $cargoPath build --release --workspace --locked --offline *> (Join-Path $releaseRoot 'build.log')
if ($LASTEXITCODE -ne 0) { throw 'Release build failed. See the build log.' }
& node (Join-Path $PSScriptRoot 'check-windows-imports.mjs') (Join-Path $projectRoot 'target\release\capture-desktop.exe') *> (Join-Path $releaseRoot 'windows-imports.json')
if ($LASTEXITCODE -ne 0) { throw 'Windows startup import verification failed. No release was staged.' }
$packageRoot = Join-Path $releaseRoot 'package'
New-Item -ItemType Directory -Path $packageRoot,$downloadRoot -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $projectRoot 'target\release\capture-desktop.exe') -Destination (Join-Path $packageRoot 'ClipCraft.exe')
Copy-Item -LiteralPath (Join-Path $projectRoot 'target\release\capture-cli.exe') -Destination $packageRoot
$fixtureRoot = Join-Path $releaseRoot 'fixture'
& (Join-Path $packageRoot 'capture-cli.exe') demo --out-dir $fixtureRoot
if ($LASTEXITCODE -ne 0) { throw 'Release CLI fixture generation failed.' }
& (Join-Path $packageRoot 'capture-cli.exe') render (Join-Path $fixtureRoot 'demo.capture.json') --out (Join-Path $fixtureRoot 'roundtrip.png')
if ($LASTEXITCODE -ne 0) { throw 'Release CLI project reopen/render failed.' }
& (Join-Path $packageRoot 'capture-cli.exe') compare (Join-Path $fixtureRoot 'demo.png') (Join-Path $fixtureRoot 'roundtrip.png')
if ($LASTEXITCODE -ne 0) { throw 'Release PNG roundtrip comparison failed.' }
Copy-Item -LiteralPath (Join-Path $fixtureRoot 'demo.capture.json') -Destination (Join-Path $packageRoot 'example.capture.json')
$readme = @"
ClipCraft $Version - Windows x64 alpha

Extract the entire ZIP and open ClipCraft.exe. No ClipCraft account is required.
Open example.capture.json or a PNG/JPEG. Original pixels and editable annotations
autosave to the local Library. Wait for Saved to Library before closing.
Save As writes an additional portable project; Export PNG creates flattened output.

Home holds the grouped tools; right-click a tool to favorite it.
Tool Settings holds font, justification, alignment, distribution and arrangement.
The property pane has Tool Properties and Objects tabs. Hover the circled question
marks for instructions. File opens the full workspace menu.

Draw objects with live previews. Click existing objects to select; Shift-click
toggles multi-selection. Drag selected objects together, or Alt-drag to duplicate.
Eight resize handles support Shift proportions and Alt expansion from center.
Double-click text-bearing objects to edit inline. Back to Object returns from
embedded text properties. Steps continue after the highest existing label.
Rounded-corner nodes stay near the corners; drag to adjust the actual radius.
Colors, width, font size and other supported properties update live.
Release commits a drag; one Undo restores it.

Quick Styles apply to selected objects and remember new-object formatting.
Use + to save a style; right-click saved styles or My Colors to update/remove.
Format Painter transfers shared formatting; double-click it for repeated use.
The Objects pane supports stacking, visibility and locks.

Middle-drag or Space-drag pans. Alt-wheel and footer controls change zoom.
Fit to Screen maximizes the image view; 1:1 shows native size.
Raster Preview shows actual export pixels while preserving pan and zoom.
Hide Annotations compares against the base image without changing exports.

Ctrl+N/O/S: New/Open/Save; Ctrl+Shift+S: Save As; Ctrl+Z/Y: Undo/Redo.
Ctrl+Shift+C: Copy All; Ctrl+Shift+N: New from Clipboard.
Capture or Ctrl+Alt+C while ClipCraft runs opens region capture.
Escape/right-click cancels capture. Captures are retained in the Library.
Arrow keys nudge; Shift nudges 10 pixels. Ctrl+Shift+F/B changes stacking.
Rectangular pixel Selection supports Copy/Cut/Clear and a flattening review.
Normal paste into the current canvas is not included in this release.

Editable projects retain original image data. Share flattened PNG output for
redactions. Library capacity is 50 images/512MB; full/failed saves are reported
without silently removing older images. Use Save As when additional storage is needed.
Fonts are two bundled families. See release-notes.json for current limitations.
capture-cli.exe uses the same document engine; run --help for local automation.

Release: $($notes.title)
$($notes.summary)
"@
Set-Content -LiteralPath (Join-Path $packageRoot 'READ-ME.txt') -Value $readme -Encoding utf8
Copy-Item -LiteralPath $notesPath -Destination (Join-Path $packageRoot 'release-notes.json')
# Collect license files shipped with the cached crates used by this Windows build.
& $cargoPath metadata --format-version 1 --locked --offline --filter-platform x86_64-pc-windows-msvc > (Join-Path $releaseRoot 'cargo-metadata.json')
if ($LASTEXITCODE -ne 0) { throw 'Cannot read dependency metadata for release notices.' }
$metadata = Get-Content -Raw -LiteralPath (Join-Path $releaseRoot 'cargo-metadata.json') | ConvertFrom-Json
$licenseLines = [System.Collections.Generic.List[string]]::new()
$licenseLines.Add('Third-party dependency notices for ClipCraft')
foreach ($package in ($metadata.packages | Where-Object source | Sort-Object name,version)) {
    $licenseLines.Add("`n===== $($package.name) $($package.version) [$($package.license)] =====")
    $directory = Split-Path -Parent $package.manifest_path
    $licenseFiles = @(Get-ChildItem -LiteralPath $directory -File | Where-Object Name -Match '^(LICENSE|LICENCE|COPYING|NOTICE|UNLICENSE)([.-]|$)')
    if ($package.license_file) {
        $declared = Join-Path $directory $package.license_file
        if (Test-Path -LiteralPath $declared -PathType Leaf) { $licenseFiles += Get-Item -LiteralPath $declared }
    }
    if ($package.name -eq 'epaint_default_fonts') {
        foreach ($fontNotice in @('UFL.txt','OFL.txt','Hack-Regular.txt','emoji-icon-font-mit-license.txt')) {
            $fontNoticePath = Join-Path $directory ('fonts/' + $fontNotice)
            if (-not (Test-Path -LiteralPath $fontNoticePath -PathType Leaf)) { throw "Missing bundled font notice: $fontNotice" }
            $licenseFiles += Get-Item -LiteralPath $fontNoticePath
        }
    }
    foreach ($licenseFile in ($licenseFiles | Sort-Object FullName -Unique)) {
        $licenseLines.Add("--- $($licenseFile.Name) ---")
        $licenseLines.Add((Get-Content -Raw -LiteralPath $licenseFile.FullName))
    }
}
# Notices for source adaptations are required alongside dependency notices.
$adaptedNotices = Join-Path $projectRoot 'licenses\designcraft\LICENSE-MIT'
if (Test-Path -LiteralPath $adaptedNotices) {
    $licenseLines.Add("`n===== Adapted DesignCraft geometry =====")
    $licenseLines.Add((Get-Content -Raw -LiteralPath $adaptedNotices))
    $licenseLines.Add((Get-Content -Raw -LiteralPath (Join-Path $projectRoot 'licenses\designcraft\PROVENANCE.txt')))
}
foreach ($fontNotice in (Get-ChildItem -LiteralPath (Join-Path $projectRoot 'licenses\text-fonts') -File)) {
    $licenseLines.Add("`n===== Text font $($fontNotice.Name) =====")
    $licenseLines.Add((Get-Content -Raw -LiteralPath $fontNotice.FullName))
}
Set-Content -LiteralPath (Join-Path $packageRoot 'THIRD-PARTY-NOTICES.txt') -Value $licenseLines -Encoding utf8
$archivePath = Join-Path $releaseRoot $fileName
Compress-Archive -Path (Join-Path $packageRoot '*') -DestinationPath $archivePath -CompressionLevel Optimal -Force
$hash = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash.ToLowerInvariant()
$bytes = (Get-Item -LiteralPath $archivePath).Length
if ($bytes -gt 24MB) { throw 'Artifact exceeds the current immutable storage upload budget.' }
$notes | Add-Member -NotePropertyName asset -NotePropertyValue ([PSCustomObject]@{file=$fileName;bytes=$bytes;sha256=$hash}) -Force
$feedPath = Join-Path $siteRoot 'release-data.json'
$releases = @()
if (Test-Path -LiteralPath $feedPath) { $releases = @((Get-Content -Raw -LiteralPath $feedPath | ConvertFrom-Json).releases) }
if (@($releases | Where-Object version -EQ $Version).Count) { throw 'Version is already in the release feed.' }
Copy-Item -LiteralPath $archivePath -Destination $downloadPath
Copy-Item -LiteralPath (Join-Path $fixtureRoot 'demo.png') -Destination (Join-Path $siteRoot 'public\assets\annotated.png')
Copy-Item -LiteralPath (Join-Path $fixtureRoot 'source.png') -Destination (Join-Path $siteRoot 'public\assets\original.png')
$notes | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $releaseRoot 'site-entry.json') -Encoding utf8
node (Join-Path $siteRoot 'scripts\build.mjs')
if ($LASTEXITCODE -ne 0) { throw 'Website validation failed. Nothing has been published.' }
[PSCustomObject]@{version=$Version;prepared_at=(Get-Date).ToUniversalTime().ToString('o');artifact=$downloadPath;sha256=$hash;bytes=$bytes;state='prepared_not_published'} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $releaseRoot 'prepared.json') -Encoding utf8
Write-Output "Prepared $Version. After optimized native verification, publish with node scripts/publish-local-site.mjs $Version, then verify the public URL."
