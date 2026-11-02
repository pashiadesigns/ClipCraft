# Releases and verification

Protected-branch pull requests run Rust formatting, tests, strict Clippy and a
RustSec dependency scan. Immutable `v<version>` tags build an unsigned Windows x64
portable ZIP and per-user NSIS setup EXE. The tag workflow tests the installer
lifecycle on a fresh GitHub-hosted Windows runner, then creates a draft prerelease.
Maintainers inspect its results and exact artifacts before publication.

Each package contains application/CLI executables, a synthetic example, release
notes, third-party notices, an SPDX 2.3 inventory, committed-source SHA-256 manifest
and build information. The inventory covers the resolved Windows workspace
build/test graph and bundled assets, including the installer engine; not every
component is linked into both executables. GitHub also provides source archives.

SHA256SUMS.txt covers the public downloads. PACKAGE-SHA256SUMS.txt covers files
inside the portable ZIP, excluding itself. SOURCE-SHA256SUMS.txt hashes committed
Git blobs, so compare against an archive of the recorded commit rather than a
checkout with Windows line-ending conversion.

GitHub-hosted builds attest both setup EXE and ZIP provenance and their SPDX
inventory. For either downloaded artifact:

```powershell
Get-FileHash ./clipcraft-0.2.0-beta.1-windows-x64-setup.exe -Algorithm SHA256
gh attestation verify ./clipcraft-0.2.0-beta.1-windows-x64-setup.exe `
  --repo pashiadesigns/ClipCraft --deny-self-hosted-runners `
  --signer-workflow pashiadesigns/ClipCraft/.github/workflows/windows.yml `
  --source-ref refs/tags/v0.2.0-beta.1
```

Repeat with `--predicate-type https://spdx.dev/Document` for the inventory.
Check the repository, source commit/tag and workflow. Checksums identify bytes;
attestations establish origin. Neither is Authenticode, a security certification
or a promise of bit-for-bit reproducible rebuilds. Runner/SDK and fonts may vary.

## Installer and signing policy

See [installation](installing.md) for setup, update, removal and the exact scope
of automated lifecycle tests. Setup uses pinned NSIS 3.13, verifies the compiler
archive SHA-256 before execution and uses zlib compression. All installer source,
scripts and required notices are in this repository.

This beta includes an **unsigned** per-user installer. Microsoft signing is paused;
there is no verified publisher certificate on these binaries. A signed installer
is intended for later designated major releases; incremental source and portable
ZIP releases can continue separately. MSI/all-user enterprise deployment and an
in-app updater are future work. Existing releases remain immutable.

A future signing workflow must use an approved publisher identity, narrowly scoped
permissions, signatures and timestamps on the application and setup, signature
verification, and checksums generated after signing. Signing identifies a publisher
and detects tampering; it does not certify absence of vulnerabilities or guarantee
immediate Windows reputation. The [dated security review](security-review-2026-11-01.md)
records the previous preview's bounded findings, not a certification of this beta.

Historical alpha downloads predate the public source import. Their assets and
checksums are preserved; editorial release-note cleanup does not assert that
those binaries came from a later public commit.
