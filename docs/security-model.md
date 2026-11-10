# Security and local data model

ClipDesigner runs as the current Windows user. It captures screen pixels and may read
window/UI Automation labels while recording steps. Global hotkeys and step capture
use Windows input APIs. Capture/OCR/print code is in `apps/desktop/src/capture`,
`ocr`, and `export_ui`; the shared file/image parser and renderer are in `crates/core`.

## Storage and original pixels

Library documents and imported icons are stored beneath
`%LOCALAPPDATA%/ClipDesigner/history-v1` by default. Preferences and saved colors use
`%LOCALAPPDATA%/ClipDesigner/preferences-v1`; style storage is also local. Environment
overrides such as `CLIPDESIGNER_HISTORY_DIR` and `CLIPDESIGNER_PREFERENCES_DIR` isolate
test/managed runs. Saved projects retain original image data and editable objects.
These files are not encrypted by ClipDesigner. Apply organization access controls
and retention rules to the Library, exports and backups.

Cropping, blurring and redacting editable objects do not remove original pixels
from a project. Share a flattened export when concealing content, and inspect that
export. Do not distribute the editable project as a secure redaction.

Use opaque Redact for information that must be removed from the shared image;
blur and pixelation can leave information inferable. PNG/JPEG are flattened, and
PDF masks covered original image/vector content before writing the replacement
pixels for every Obfuscate mode. Optional redaction labels are decorative and do
not enable or disable this sanitization. Canvas OCR reads the visible composite,
including masks and crop, rather than hidden embedded originals. The Library may keep up to
three previous revisions per item within a global 256 MiB recovery budget.
Recovery copies, backups and editable clipboard payloads can therefore retain
content no longer visible in the current canvas. Saved templates are editable
documents too: replacing image frames with placeholders is not a data-sanitizing
operation and does not clear background/original pixels, text, icons or cursors.

## System integration

Clipboard commands exchange images, text and editable object payloads with the OS
clipboard; clipboard managers may retain them. OCR uses Windows OCR. Printing
uses the selected Windows printer and spooler after the user accepts the dialog;
printer/driver network behavior is outside ClipDesigner's control. Closing to tray
can keep hotkeys active; use Exit to stop the process.

The application requires no ClipDesigner cloud account and includes no application
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

Beta packages are unsigned and require no ClipDesigner installer. No central
enterprise policy service, managed updater, signed distribution, or support SLA is claimed.
See SECURITY.md for vulnerability reporting and the actual CI evidence for each
tag.

## Video privacy

Find Sensitive Info analyzes still images and videos on this PC with Windows OCR.
In a video, tracked redactions follow detected text as it moves or scrolls. When a
track loses its text, it holds the last position for one second, then ends and is
flagged **Needs review**; Export video lists every redaction that still needs
review. Detection assists review and never guarantees full coverage: names, faces
and text that OCR misreads are not found. Check every flagged range and the whole
exported video. A missed frame can reveal the original information. Focus and
Magnify are visual emphasis, not a privacy feature.

The MSI and portable ZIP ship without a Library, recent projects, preferences or
capture media. Both use the current user's profile. See [storage locations](installing.md).
Raw recordings and imported originals remain editable; deleting a Library entry
does not currently remove all separate media dependencies.
