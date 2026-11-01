# ClipCraft security review — 2026-11-01

This is an AI-assisted internal source review with reproducible regression checks
and a local antivirus scan. It found no confirmed exploitable vulnerability in the
reviewed paths. It is not an independent audit, penetration test, malware-free
certification or approval by Microsoft. The limits and open work below matter
when assessing suitability for an organization.

## Evaluated version

- Release: [0.1.0-preview.2](https://github.com/pashiadesigns/ClipCraft/releases/tag/v0.1.0-preview.2).
- Released source: `c25acbb2093a569c8da0035bd96d0048090bcbda`.
- Review baseline: documentation follow-up `b90af53370c4b67abd0b928b8dc2a7858301164b`.
- Downloaded ZIP SHA256: `4a53c410e06edaa0d42955f6cbf3ceb635e9513b837db2eeef010a7528dd7dd1`.
- Downloaded editor SHA256: `4dc1a90cf1cd76b4542d1eb78d770d4a9ea0fdb024d1f04d2add2f50412a5242`.

This review adds tests and documentation, without changing application behavior
or replacing the immutable unsigned release. The release's hosted source/SBOM
attestations and package checksums were previously verified. The separate
documentation follow-up CI run 41339712620 also completed successfully.

## Executed checks

| Check | Result and limits |
| --- | --- |
| Rust formatting, workspace tests and Clippy | Pass: 602 tests, including six new security regression tests. Eleven explicitly ignored native/environment tests were not enabled. |
| Dependency advisories | cargo-audit 0.22.2: zero known vulnerabilities; five unsuppressed maintenance warnings. RustSec revision `2295466c4e9df26647fd67d8fa1c705ac2fc7f81`. |
| Downloaded package antivirus scan | Microsoft Defender custom scan of the extracted hosted package: no threats found. Engine product 4.18.26100.4, definitions 1.463.73.0. This is one scanner at one time. |
| Release/source/website script regression tests | 13 pass; GitHub Actions workflow lint passes. |
| GitHub security configuration | Dependency security updates, secret scanning and push protection enabled. Zero open dependency or secret-scanning alerts at inspection. No branch protection or repository rulesets configured. |

The existing 43 optimized export/viewport comparisons belong to release
qualification; they were not rerun for this test/documentation-only review.
No personal content was submitted to an external scanner, and no native desktop
capture or operating-system clipboard test was run.

Reproduce the new synthetic checks with:

```powershell
cargo test --locked --offline -p capture-core --test security_inputs
./scripts/check.ps1
cargo audit --json
```

`cargo audit` requires the separately installed audit tool and a current advisory
database. Its result can change. The synthetic tests cover malformed project JSON,
truncated/mutated clipboard payloads, extreme PNG dimensions with a valid header
checksum, Library traversal identifiers, invalid vector path data, and partial
redaction in PNG/PDF. They do not constitute sustained fuzzing or prove every
possible input safe.

## Reviewed trust boundaries

**Projects and images.** `crates/core/src/lib.rs`, `bitmap.rs` and
`canvas_size.rs` validate dimensions, source/raster consistency, geometry and
document structure. Limits include 25 million canvas pixels, 160 MB input files,
10,000 annotations and 100,000 freehand points. Embedded image dimensions are
checked before full decoding. Writes use a temporary file in the destination
directory and an atomic replacement. These checks bound many inputs, but multiple
image buffers and complex valid documents can still consume substantial resources.

**Clipboard and vector stamps.** `clipboard_payload.rs` and desktop clipboard
adapters check schemas, lengths and decoded document structure. Editable clipboard
objects can include original pixels. Vector stamps contain bounded path geometry
and at most five colors; they are not arbitrary SVG documents with scripts,
external resources or XML entities. Imported images still reach native/library
code and decoders inside the application process.

**Library and recovery.** `history.rs` and its management/recovery modules constrain
identifiers and normal image/thumbnail locations, validate index metadata and use
atomic persistence. Synthetic traversal identifiers were rejected. This does not
establish protection against a malicious process running as the same Windows
user, or every filesystem link/race scenario.

**Redaction and export.** `export/vector_pdf.rs` masks covered source pixels before
embedding raster data and removes covered vector content through masked rendering.
The new partial-redaction test found no synthetic covered RGB pixels in the PNG
or decompressed PDF streams, while preserving the original editable document.
Existing release tests cover additional PDF vector/raster combinations. The result
applies to tested opaque Redact cases, not to every possible composition, blur or
pixelation as information removal.

**Windows integration.** Targeted review covered clipboard ownership, capture/OCR,
step mouse-hook cleanup, bounded UI Automation workers and print/export helpers.
Step context checks the UI Automation password flag; that flag is not a guarantee
that a captured screen contains no sensitive information. Export opening uses
Windows ShellExecute on the completed file or folder, without constructing shell
commands. First-party application source inspection found no network client,
telemetry or update-download path. This was not a runtime network or native-FFI
penetration test; OS services, drivers and third-party internals remain outside
that conclusion.

**Build and distribution.** The reviewed workflows pin action revisions, use
read-only default token permissions, avoid persisting checkout credentials, gate
release jobs on tests/advisories, and attach source/SBOM attestations. Packaging
requires a clean source checkout and produces file manifests. The separate
PC-hosted website serves generated assets and listed downloads; it is not an
application service. Synthetic website tests cover path containment and allowed
requests. Live infrastructure penetration testing was not performed.

## Findings and follow-up priorities

1. **Protect release sources before adding signing credentials — release-process
   gap.** `main` is currently unprotected and there are no repository rulesets.
   Existing CI checks do not prevent an authorized writer from bypassing them or
   rewriting a release tag. Configure required checks and restrictions on force
   pushes/deletions, immutable version-tag rules, and a protected signing release
   environment. Federated signing access should be narrowly scoped. The review
   did not change repository access policies or create signing credentials.

2. **Keep original-data retention visible — expected behavior with privacy risk.**
   Projects, editable clipboard objects, saved templates and recovery versions
   can retain pixels and text hidden in the current view. Empty template frames
   do not clear original/background data or other objects. These formats must not
   be distributed as sanitized redactions. Use an inspected opaque-redacted
   export and apply organization retention controls. The security model now
   documents this explicitly; no secure erasure claim is made.

3. **Replace or reassess unmaintained dependencies — maintenance risk.**
   `ttf-parser 0.25.1` (RUSTSEC-2026-0192) is directly used on Windows and is the
   highest-priority parser maintenance item. `paste 1.0.15` has
   RUSTSEC-2024-0436. The full lockfile also reports `zbus_names 4.3.5`,
   `zvariant 5.15.1` and `zvariant_derive 5.15.1` (RUSTSEC-2026-0371, -0370 and
   -0372); these are outside the resolved Windows inventory. These are
   maintenance warnings, not five confirmed vulnerabilities. No warning was
   suppressed and no dependency was upgraded in this review.

4. **Expand adversarial testing — assurance gap.** Add sustained fuzzing of
   project/clipboard/image/font inputs, memory/CPU resource tests, and independent
   Windows FFI/clipboard/capture review. File size checks followed by separate
   reads are not isolation from concurrent local modification. Parsers run in
   process; there is no parser sandbox. The current tests do not rule out
   denial-of-service, unknown dependency flaws or a compromised build host.

## Signing status

Microsoft Artifact Signing with individual verification was selected. The Basic
account was created in East US on 2026-11-01 after the maintainer upgraded Azure.
Account billing begins at creation; no invoice amount has been verified. Updated
terms were accepted and the maintainer's account-scoped Identity Verifier role was
assigned with explicit confirmation. Individual validation awaits the maintainer's
legal-identity review and submission. No certificate profile or signed artifact exists. Windows
installers and signed releases remain
future work. Signing establishes publisher identity and detects file modification;
it does not resolve these findings or certify the absence of backdoors.

See [security and data handling](security-model.md), [distribution and signing
workflow](github-releases.md), and [private vulnerability reporting](../SECURITY.md).
