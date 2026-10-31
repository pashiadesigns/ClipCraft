# Security and local data model

ClipCraft runs as the current Windows user. It captures screen pixels and may read
window/UI Automation labels while recording steps. Global hotkeys and step capture
use Windows input APIs. Capture/OCR/print code is in `apps/desktop/src/capture`,
`ocr`, and `export_ui`; the shared file/image parser and renderer are in `crates/core`.

## Storage and original pixels

Library documents and imported stamps are stored beneath
`%LOCALAPPDATA%/ClipCraft/history-v1` by default. Preferences and saved colors use
`%LOCALAPPDATA%/ClipCraft/preferences-v1`; style storage is also local. Environment
overrides such as `CLIPCRAFT_HISTORY_DIR` and `CLIPCRAFT_PREFERENCES_DIR` isolate
test/managed runs. Saved projects retain original image data and editable objects.
These files are not encrypted by ClipCraft. Apply organization access controls
and retention rules to the Library, exports and backups.

Cropping, blurring and redacting editable objects do not remove original pixels
from a project. Share a flattened export when concealing content, and inspect that
export. Do not distribute the editable project as a secure redaction.

## System integration

Clipboard commands exchange images, text and editable object payloads with the OS
clipboard; clipboard managers may retain them. OCR uses Windows OCR. Printing
uses the selected Windows printer and spooler after the user accepts the dialog;
printer/driver network behavior is outside ClipCraft's control. Closing to tray
can keep hotkeys active; use Exit to stop the process.

The application requires no ClipCraft cloud account and includes no application
telemetry or automatic update client. Source/build/release scripts download
dependencies and communicate with GitHub when invoked. Optional website/tunnel
scripts are separate tooling, not launched by the application. Audit OS services,
printer drivers and CI network access according to your environment.

## Review scope and limitations

Review unsafe Windows FFI, parsers, image/font handling, clipboard, hooks and
filesystem writes as trust boundaries. Projects and imported images are untrusted
inputs. Automated tests cover validation, persistence, Undo and rendering but are
not a penetration test. Dependency advisories may change after a release.
The SPDX inventory covers the Windows build/test graph and bundled assets, not
Windows system libraries or a certified runtime-only binary dependency inventory.

Portable previews are unsigned and require no ClipCraft installer. No central
enterprise policy service, managed updater, signed MSI, or support SLA is claimed.
See SECURITY.md for vulnerability reporting and the actual CI evidence for each
tag.

## Initial dependency review (2026-10-31)

`cargo-audit 0.22.2` reported zero known vulnerabilities using RustSec database
commit `2295466c4e9df26647fd67d8fa1c705ac2fc7f81`. Five unmaintained-package
warnings remain visible: `paste 1.0.15` (RUSTSEC-2024-0436), `ttf-parser 0.25.1`
(RUSTSEC-2026-0192), `zbus_names 4.3.5` (RUSTSEC-2026-0371), `zvariant 5.15.1`
(RUSTSEC-2026-0370), and `zvariant_derive 5.15.1` (RUSTSEC-2026-0372).
The scan covers the full cross-platform lockfile; the zbus/zvariant entries are
outside the resolved Windows dependency inventory. Maintenance warnings have not
been suppressed. Reassess replacements in a dependency-maintenance batch and
consult the latest scan; this dated result is not a continuing guarantee.
