param([Parameter(Mandatory=$true)][ValidatePattern('^\d+\.\d+\.\d+-[a-z0-9.]+$')][string]$Version)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $root
$trust = 'safe.directory=' + $root.Replace('\','/')
$notesPath = Join-Path $root "releases/$Version.json"
$notes = Get-Content -Raw -LiteralPath $notesPath | ConvertFrom-Json
if ($notes.version -ne $Version -or $notes.limitations -isnot [Array]) { throw 'Release notes/version/limitations are required.' }
$manifest = Get-Content -Raw -LiteralPath Cargo.toml
if ($manifest -notmatch ('(?m)^version\s*=\s*"' + [regex]::Escape($Version.Split('-')[0]) + '"')) { throw 'Release and workspace base versions differ.' }
$dirty = & git -c $trust status --porcelain --untracked-files=normal
if ($LASTEXITCODE -ne 0 -or $dirty) { throw 'Commit the exact audited source before packaging.' }
$release = Join-Path $root "artifacts/github/releases/$Version"
if (Test-Path -LiteralPath $release) { throw 'Release staging already exists; inspect it or choose a new version. Never overwrite a published package.' }
$env:CARGO_BUILD_JOBS = '2'
$env:RUST_TEST_THREADS = '2'
$pwsh = (Get-Process -Id $PID).Path
$targetRoot = if ($env:CARGO_TARGET_DIR) { [IO.Path]::GetFullPath($env:CARGO_TARGET_DIR) } else { Join-Path $root 'target' }
$desktopExe = Join-Path $targetRoot 'release/capture-desktop.exe'
$cliExe = Join-Path $targetRoot 'release/capture-cli.exe'
& node scripts/source-audit.mjs
if ($LASTEXITCODE -ne 0) { throw 'Source audit failed.' }
& node --test scripts/source-audit.test.mjs scripts/release-metadata.test.mjs
if ($LASTEXITCODE -ne 0) { throw 'Release metadata tests failed.' }
& $pwsh -NoProfile -File scripts/verify-windows-resource.ps1
if ($LASTEXITCODE -ne 0) { throw 'Windows resources failed verification.' }
& $pwsh -NoProfile -File scripts/check.ps1
if ($LASTEXITCODE -ne 0) { throw 'Rust checks failed.' }
New-Item -ItemType Directory -Path $release -Force | Out-Null
& cargo build --release --workspace --locked --offline *> (Join-Path $release 'build.log')
if ($LASTEXITCODE -ne 0) { throw 'Release build failed; inspect build.log.' }
& node scripts/check-windows-imports.mjs $desktopExe *> (Join-Path $release 'windows-imports.json')
if ($LASTEXITCODE -ne 0) { throw 'Windows startup import verification failed.' }
$package = Join-Path $release 'package'
$public = Join-Path $release 'public'
New-Item -ItemType Directory -Path $package,$public -Force | Out-Null
Copy-Item -LiteralPath $desktopExe -Destination (Join-Path $package 'ClipCraft.exe')
Copy-Item -LiteralPath $cliExe -Destination $package
$fixture = Join-Path $release 'fixture'
& (Join-Path $package 'capture-cli.exe') demo --out-dir $fixture
if ($LASTEXITCODE -ne 0) { throw 'Demo generation failed.' }
& (Join-Path $package 'capture-cli.exe') render (Join-Path $fixture 'demo.capture.json') --out (Join-Path $fixture 'roundtrip.png')
if ($LASTEXITCODE -ne 0) { throw 'Project reopen/render failed.' }
& (Join-Path $package 'capture-cli.exe') compare (Join-Path $fixture 'demo.png') (Join-Path $fixture 'roundtrip.png')
if ($LASTEXITCODE -ne 0) { throw 'Export roundtrip differs.' }
Copy-Item -LiteralPath (Join-Path $fixture 'demo.capture.json') -Destination (Join-Path $package 'example.capture.json')
Copy-Item -LiteralPath LICENSE,NOTICE,SECURITY.md,README.md -Destination $package
Copy-Item -LiteralPath $notesPath -Destination (Join-Path $package 'release-notes.json')
$packageDocs = Join-Path $package 'docs'
New-Item -ItemType Directory -Path $packageDocs -Force | Out-Null
Copy-Item -LiteralPath docs/building.md,docs/security-model.md,docs/github-releases.md -Destination $packageDocs
@"
ClipCraft $Version - unsigned Windows x64 preview

Extract the entire ZIP and run ClipCraft.exe. No ClipCraft account is required.
Open example.capture.json for a synthetic editable document. Use the File menu
for exports, printing and options. See release-notes.md for pending checks.

The ZIP contains portable executables; it is not a signed Windows installer.
Library/settings are stored in your Windows user profile by default. See
docs/security-model.md for data handling and docs/github-releases.md for
checksums, source commit and build attestation verification.
"@ | Set-Content -LiteralPath (Join-Path $package 'READ-ME.txt') -Encoding utf8
& cargo metadata --format-version 1 --locked --offline --filter-platform x86_64-pc-windows-msvc | Set-Content -LiteralPath (Join-Path $release 'cargo-metadata.json') -Encoding utf8
if ($LASTEXITCODE -ne 0) { throw 'Dependency resolution metadata failed.' }
& node scripts/release-metadata.mjs (Join-Path $release 'cargo-metadata.json') $package $Version
if ($LASTEXITCODE -ne 0) { throw 'SBOM/notices/build provenance preparation failed.' }
$packageHashes = Get-ChildItem -LiteralPath $package -Recurse -File | Sort-Object FullName | ForEach-Object {
    (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant() + '  ' + $_.FullName.Substring($package.Length + 1).Replace('\','/')
}
$packageHashes | Set-Content -LiteralPath (Join-Path $package 'PACKAGE-SHA256SUMS.txt') -Encoding utf8
$zip = Join-Path $public "clipcraft-$Version-windows-x64.zip"
Compress-Archive -Path (Join-Path $package '*') -DestinationPath $zip -CompressionLevel Optimal
Copy-Item -LiteralPath (Join-Path $package 'sbom.spdx.json'),(Join-Path $package 'build-info.json'),(Join-Path $package 'release-notes.md') -Destination $public
$publicHashes = Get-ChildItem -LiteralPath $public -File | Sort-Object Name | ForEach-Object {
    (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant() + '  ' + $_.Name
}
$publicHashes | Set-Content -LiteralPath (Join-Path $public 'SHA256SUMS.txt') -Encoding utf8
@{version=$Version;sha256=(Get-FileHash $zip -Algorithm SHA256).Hash.ToLowerInvariant();state='prepared_not_published';public=$public} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $release 'prepared.json') -Encoding utf8
Write-Output "Prepared $Version at $public. Nothing was published."
