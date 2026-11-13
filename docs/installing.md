# Installing ClipDesigner

Download **1.0.0-beta.5** from the [release page](https://github.com/pashiadesigns/ClipDesigner/releases/tag/v1.0.0-beta.5).
Windows 10/11 x64 is supported. Both downloads are unsigned; Windows may show an
unknown-publisher or reputation warning.

## Upgrading from ClipCraft

ClipDesigner was called ClipCraft through 1.0.0-beta.3; that release installs as
ClipCraft under `%LOCALAPPDATA%\Programs\ClipCraft` and ships `ClipCraft.exe`.
From 1.0.0-beta.4, releases use the ClipDesigner name:

- The MSI is an upgrade of the ClipCraft MSI. Installing it removes the ClipCraft
  installation, Start menu shortcut and file registrations and replaces them with
  ClipDesigner ones.
- On first start, ClipDesigner moves `%LOCALAPPDATA%\ClipCraft` to
  `%LOCALAPPDATA%\ClipDesigner`, keeping your Library, recordings, preferences,
  styles and presets. Exit ClipCraft first; anything still in use is moved on a
  later start. Storage folders you chose inside the old folder follow the move.
- Portable copies: run the new `register-project-files.ps1` to replace ClipCraft
  file registrations. If Windows asks which app should open `.iclip` or `.vclip`
  files, choose ClipDesigner.

## MSI installer

Run `clipdesigner-1.0.0-beta.5-windows-x64.msi`. It installs for the current user under
`%LOCALAPPDATA%\Programs\ClipDesigner` by default and creates one **ClipDesigner** Start
menu shortcut and an Installed Apps entry. It registers `.iclip` and `.vclip` in
Open With/Default Apps without modifying Windows' protected default-app choices.
It does not register ordinary `.json` files, launch the app, or enable startup at login.

Exit ClipDesigner through its tray menu before installation, repair or removal.
Remove any old preview/setup-EXE installation first; the new MSI uses Windows
Installer and does not claim those older installer registrations. Uninstall old
versions rather than deleting their installation folders. Old projects are not
compatible with this version; back up files before changing versions.

Windows Installed Apps provides repair/removal. Uninstall preserves Library,
settings, recordings, templates and your own files. The package contains no user
data. Silent installation uses `msiexec /i <package.msi> /qn /norestart`; managed
deployments must run in the intended user's context because this is a per-user MSI.

## Portable ZIP

Extract the whole ZIP and run `ClipDesigner.exe`. No installer or administrator access
is required. Optional `install-start-menu.ps1` adds a shortcut;
`register-project-files.ps1` registers image/video projects for that copy.
Portable means no installation is required; **data still uses the Windows user
profile**, not the extracted application folder.

## Default data locations

Open `%LOCALAPPDATA%\ClipDesigner` in Explorer. On a normal Windows profile this is
`C:\Users\<you>\AppData\Local\ClipDesigner`.

| Content | Default location below that folder |
| --- | --- |
| Library index and editable image/video projects | `history-v1` (project snapshots in `images`) |
| Image captures | Embedded in editable Library snapshots; thumbnails in `history-v1\thumbnails` |
| Video captures | `history-v1\recordings` |
| Narration audio and webcam takes | `history-v1\voiceovers` |
| Saved templates | `history-v1\templates` |
| Saved object, motion, frame and audio presets | `styles-v1` |
| Preferences, shortcuts, saved colors and output choices | `preferences-v1` |

Settings lets you choose folders for **Projects & recordings**, **Template
library** and **Preset library**. Restart after changing them; existing files are
not moved automatically. Export and Save As use the destination you choose.
Portable video projects keep a sibling `.vclip-media` folder; keep it with the
`.vclip` file when moving or sharing the project.

A fresh Library starts empty at the maximum configurable capacity: **1,000 items
and 16,000 MB (16 GB)**. This is a capacity ceiling, not reserved disk space.
Existing saved limits are preserved. Separate recording/media files and recovery
storage also consume disk space; the Library capacity is not a total disk quota.
Deleting a Library project currently does not remove all separate media dependencies.

## Package verification

Compare downloads with `SHA256SUMS.txt`. The MSI and ZIP share the same application
payload. See [release verification](github-releases.md) for provenance and checks.
