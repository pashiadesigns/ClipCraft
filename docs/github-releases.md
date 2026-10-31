# GitHub releases and verification

The Windows workflow checks pushes/pull requests and builds packages for
`v<version>` tags. Use an immutable source commit and matching release notes.
Tags build a **draft prerelease**; review native limitations and workflow results
before publishing. There is no in-app updater or signed installer in this workflow.

Each package includes application/CLI binaries, a synthetic example, release notes,
third-party notices, an SPDX 2.3 dependency inventory, source SHA-256 manifest and
build information containing the source commit, toolchain and target. Checksums and
SBOM are also separate release assets. The inventory covers the resolved Windows
workspace build/test graph and bundled assets; it does not assert every listed
crate is linked into both executables.

GitHub-hosted builds attest the ZIP's provenance and SBOM. Local builds do not
receive a GitHub-hosted attestation. Locally uploaded previews must be identified
as local. Historical alpha downloads on the companion site predate the public
source import and cannot be attributed to its initial commit.

```powershell
Get-FileHash ./clipcraft-0.1.0-preview.1-windows-x64.zip -Algorithm SHA256
# Compare with the matching line in SHA256SUMS.txt.
gh attestation verify ./clipcraft-0.1.0-preview.1-windows-x64.zip --repo OWNER/ClipCraft
```

Check the attested repository, source commit/tag and workflow. Build information
states whether it ran on GitHub or locally. Checksums detect changed bytes;
attestations establish origin. Neither is Authenticode signing, a security
certification or a promise of bit-for-bit reproducible rebuilds. Runner/SDK and
installed fonts may vary.

RustSec dependency checks run on changes, manually and weekly. Dependabot proposes
Cargo/GitHub Actions updates. These are ordinary scripts with no Codex, model API,
or AI review calls.
