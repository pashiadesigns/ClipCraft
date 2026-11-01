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
gh attestation verify ./clipcraft-0.1.0-preview.1-windows-x64.zip `
  --repo pashiadesigns/ClipCraft --deny-self-hosted-runners `
  --signer-workflow pashiadesigns/ClipCraft/.github/workflows/windows.yml `
  --source-ref refs/tags/v0.1.0-preview.1
```

Repeat with `--predicate-type https://spdx.dev/Document` to verify the SPDX inventory attestation.

Check the attested repository, source commit/tag and workflow. Build information
states whether it ran on GitHub or locally. Checksums detect changed bytes;
attestations establish origin. Neither is Authenticode signing, a security
certification or a promise of bit-for-bit reproducible rebuilds. Runner/SDK and
installed fonts may vary.

RustSec dependency checks run on changes, manually and weekly. Dependabot proposes
Cargo/GitHub Actions updates. These are ordinary scripts with no Codex, model API,
or AI review calls.

## Planned major-release installers

The intended distribution policy is signed Windows installers for designated major
releases, with source archives and portable ZIPs continuing for incremental releases.
This is a future packaging workflow: published preview.1 and preview.2 remain
unsigned, immutable portable releases. Creating an installer does not itself sign it.

The recommended enterprise package is an MSI with silent installation, upgrade and
uninstall support, separate Editor/Capture Controller shortcuts, and preservation of
user settings and Library data. Qualify fresh installation, upgrade, repair and
uninstall on clean Windows environments before release. Select and pin the installer
toolchain after reviewing its current distribution and maintenance terms.

Signing requires a validated publisher identity or an approved open-source signing
provider. Microsoft Artifact Signing currently offers a Basic plan at USD9.99/month
for 5,000 signatures. It requires a supported paid Azure subscription and identity
validation; individual developers must be in the US or Canada. The verified legal
publisher identity appears on certificates. SignPath Foundation is a possible free
option for qualifying open-source projects, subject to approval and its policies;
its certificate publisher is SignPath Foundation. On 2026-11-01 the maintainer
selected Microsoft Artifact Signing and individual publisher verification.
Registration is awaiting the maintainer's Microsoft/Azure sign-in; no signing
account, identity validation, subscription charge or certificate profile has
been confirmed. Published previews remain unsigned.

Before enabling signed releases, protect the release branch and version tags,
configure narrowly scoped federated signing permission, and qualify the signed
package. The dated [security review](security-review-2026-11-01.md) records the
current repository controls and remaining gaps.

Once configured, a protected release workflow should sign and timestamp application
binaries, build and sign the MSI, verify both signatures, then generate the final
checksums and attestations. Prefer federated GitHub identity over stored signing
credentials. Signing the executables inside incremental ZIPs is recommended as well;
installers need only be produced for designated major releases. Preserve the current
source manifests, SPDX inventories and provenance checks alongside signing.

Signing identifies the publisher and detects tampering; it is not a security audit
and does not guarantee immediate SmartScreen reputation for new releases.

For the current payload, signing both executables and the optional Start menu
PowerShell script would require three signing operations per new portable build.
An MSI using those same signed files adds one more, for four total across both
downloads. Extra setup wrappers, architectures, signing retries or new binaries
can increase that count. Downloading or installing an existing signed artifact
does not require another signing operation.

Microsoft states that deleting an Artifact Signing account does not affect
certificates already used to sign files. Timestamp every signature so verification
continues after certificate expiration, subject to revocation and platform trust.
An active service is needed to sign subsequent builds; it is not a recurring
validity fee for existing downloads. The Basic monthly fee is not prorated.

References checked 2026-11-01:

- [Artifact Signing pricing](https://azure.microsoft.com/en-us/products/artifact-signing)
- [Identity and setup requirements](https://learn.microsoft.com/en-us/azure/artifact-signing/quickstart)
- [Subscription requirements](https://learn.microsoft.com/en-us/azure/artifact-signing/faq)
- [Microsoft GitHub signing action](https://github.com/Azure/artifact-signing-action)
- [SignPath Foundation conditions](https://signpath.org/terms.html)
- [MSI deployment options](https://learn.microsoft.com/en-us/windows/win32/msi/standard-installer-command-line-options)
- [WiX maintenance terms](https://docs.firegiant.com/wix/osmf/)
- [Signing and SmartScreen](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options)
- [Certificate lifetime and timestamping](https://learn.microsoft.com/en-us/azure/artifact-signing/concept-certificate-management)
