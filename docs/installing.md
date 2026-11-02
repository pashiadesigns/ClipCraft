# Installing ClipCraft

Download the Windows x64 setup EXE or portable ZIP from the matching
[GitHub release](https://github.com/pashiadesigns/ClipCraft/releases).
Both beta packages are unsigned. Windows may display an unknown-publisher or
reputation warning. Organizations should follow their own review policies.

## Installer

Run `clipcraft-<version>-windows-x64-setup.exe`. Setup installs for the current
user under `%LOCALAPPDATA%\Programs\ClipCraft`, without administrator access.
It adds separate **ClipCraft Editor** and **ClipCraft Capture Controller** Start
menu entries and an Installed Apps entry. It does not launch ClipCraft or enable
Windows-startup registration automatically. The fixed install location cannot be
changed in this first installer.

Exit ClipCraft using its tray menu before updating; closing its window may leave
capture shortcuts running. Run a newer installer to update, or the same installer
to replace damaged application files. Back up important editable projects before
updating; older releases may not read newer project formats. This first beta does
not provide automatic rollback, MSI deployment or an in-app updater.

Uninstall through Windows Installed Apps or `Uninstall.exe` in the install folder.
Uninstall removes application files, shortcuts and its registration. It preserves
Library/settings data and unrecognized user-created files. It does not recursively
erase the installation folder. Running setup/uninstall with `/S` is silent; close
ClipCraft first. Exit code 2 means an application file is locked or not writable.

The automated installer check runs on a clean GitHub-hosted Windows runner. It
checks fresh install, a synthetic prior-version upgrade, same-version reinstall,
shortcut targets/icons, locked-file refusal, uninstall and data preservation. It
does not cover every Windows desktop, visual wizard interaction or enterprise policy.

## Portable ZIP

Extract the entire archive to a writable folder and run `ClipCraft.exe`. Optional
`install-start-menu.ps1` creates the same two shortcuts for that folder. Portable
means no installer is required; settings and Library still use the Windows user
profile. See [the data model](security-model.md). Move or delete portable folders
yourself; the installer does not claim or remove them.

Use [release verification](github-releases.md) to check hashes, source and build
attestations. A verified hash identifies bytes, not an independent security audit.
