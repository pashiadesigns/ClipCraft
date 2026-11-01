# ClipCraft

[![Windows checks](https://github.com/pashiadesigns/ClipCraft/actions/workflows/windows.yml/badge.svg?branch=main)](https://github.com/pashiadesigns/ClipCraft/actions/workflows/windows.yml)

ClipCraft is a local Windows screenshot capture and annotation editor written in Rust.
It includes an editable document engine, desktop application, and command-line tools.
This is preview software; release notes identify tested behavior and open checks.

## Downloads

Open the [GitHub Releases page](https://github.com/pashiadesigns/ClipCraft/releases) for Windows x64 portable ZIPs. Extract the
whole archive and run `ClipCraft.exe`; no ClipCraft account is required. These
previews are **unsigned**. Signed Windows installers are planned separately.
Organizations can build from source or review the checksums, dependency inventory,
and GitHub build attestations before deployment.

## Features

- Region, window and monitor capture; configurable hotkeys and tray behavior.
- Steps recording and editable guides; local OCR through Windows APIs.
- Text, arrows, lines, shapes, callouts, numbered/lettered steps, pen and highlighter.
- Blur, pixelate, redact, eraser, simplification, groups and image-frame cropping.
- Local Library, range/drag selection, bulk operations, file details and imported image stamps.
- Original vector starter stamps with independently editable palette colors.
- PNG/JPEG/PDF export and a Windows print-dialog workflow.
- Editable image templates, empty guide sections and reusable local layouts.
- Image-frame replacement from files, clipboard or Library thumbnails; bounded Library version recovery.

Video and All-in-One capture are not implemented. PDF preserves vector paths and
outlined text; screenshots and pixel effects remain raster. PDF text is not searchable.
Installed fonts in editable projects are referenced, not embedded; missing
families use a bundled fallback. Preview releases are not an enterprise certification.
The source branch can include local review changes newer than the latest release;
use the release notes for the exact download's capabilities. General SVG/EPS stamp
import is not implemented yet.
Known issue: on some multi-monitor setups the right window border moves inward
while ClipCraft is inactive and recovers on focus. This cosmetic defect is deferred.
See the [feature inventory](sites/capture-workbench/ROADMAP.md) for scoped status;
historical local evidence links in it are not distributed verification records.

## Build and inspect

See [building.md](docs/building.md), the [security/data model](docs/security-model.md),
and the [release verification guide](docs/github-releases.md).

```powershell
cargo fetch --locked
./scripts/check.ps1
cargo build --release --workspace --locked --offline
./target/release/capture-cli.exe --help
```

The pinned Rust toolchain is in `rust-toolchain.toml`. Windows MSVC build tools
and the Windows SDK are required. CI does not opt into desktop capture, clipboard
access, print jobs, personal fixtures, or an AI service.

## Source layout

| Directory | Purpose |
| --- | --- |
| `crates/core` | Shared document, rendering, export, persistence and history engine |
| `apps/desktop` | Windows UI, capture, OCR, clipboard, printing and integration |
| `apps/cli` | Local automation using the same engine |
| `scripts` | Checks, packaging, dependency inventory and optional website hosting |
| `licenses` | Third-party asset and adapted-source notices |
| `sites/capture-workbench` | Companion website source and feature inventory |

All required application source, synthetic test fixtures, fonts and brand resources
are included. Machine-specific settings, personal captures, credentials, build
caches and private development journals are excluded. Binaries are release
attachments rather than Git objects.

ClipCraft uses original branding and implementation, with documented MIT-licensed
DesignCraft adaptations. Original ClipCraft code is [Apache-2.0 licensed](LICENSE).
It is independent of TechSmith and Snagit. Third-party
code and fonts retain their own licenses; see `licenses/` and release notices.
