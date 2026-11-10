# Release verification

The 1.0.0-beta.2 release provides an unsigned Windows x64 **MSI installer**, a
**portable ZIP**, and a source archive for the exact release commit. It replaces
the previous development downloads; old project formats are not supported.

`SHA256SUMS.txt` identifies the published downloads. `PACKAGE-SHA256SUMS.txt`
identifies every portable payload file except itself. `SOURCE-SHA256SUMS.txt`
identifies committed Git blobs; compare with the source archive, since a checkout
may convert line endings. `build-info.json` records the exact commit, compiler,
target, lockfile hash and local build origin. This release is built locally and
does not claim a GitHub-hosted build attestation or an Authenticode signature.

```powershell
Get-FileHash ./clipcraft-1.0.0-beta.2-windows-x64.msi -Algorithm SHA256
Get-FileHash ./clipcraft-1.0.0-beta.2-windows-x64.zip -Algorithm SHA256
```

The SPDX inventory covers the resolved Windows build/test dependency graph and
bundled assets. It is not a runtime-only inventory. Packages retain license texts
and provenance in `THIRD-PARTY-NOTICES.txt`.

Release preparation runs Rust formatting, tests and strict Clippy, rendering and
export checks, Windows resource/import checks, a source publication audit and
payload inspection. The MSI is built with a SHA-256-pinned WiX 3.14.1 toolset;
its table validation and extracted payload are checked. These checks do not
substitute for installation on every supported Windows/device configuration.

Public packaging starts from an explicit source/payload allowlist. Local Library
files, recent items, settings, capture media, test launchers and internal review
tracking are excluded. Synthetic source test fixtures are intentional; they are
not loaded into the user's Library on startup.

See [installation and storage](installing.md), [known limitations](../README.md#beta-notes),
and [the security model](security-model.md). Checksums identify bytes; they do not
certify security or perceptual playback quality.
