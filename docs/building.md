# Building ClipDesigner

The supported desktop target is Windows x64 (`x86_64-pc-windows-msvc`). Install
Git, Rust through rustup, Visual Studio Build Tools with Desktop development with
C++, and the Windows SDK. Rustup reads the exact compiler/components in
`rust-toolchain.toml`. Node.js 24 is used by release/verification scripts.

```powershell
cargo fetch --locked
$env:CARGO_BUILD_JOBS = '2'
$env:RUST_TEST_THREADS = '2'
./scripts/check.ps1
node --test scripts/source-audit.test.mjs scripts/release-metadata.test.mjs
node scripts/source-audit.mjs
cargo build --release --workspace --locked --offline
```

The first fetch needs crates.io access; subsequent locked/offline commands use that
cache. Dependencies are not vendored. Cargo.lock records versions and registry
checksums. Binaries appear in `target/release`. Running the portable app does not
require administrator elevation.

Tests use original fixtures under `crates/core/tests/fixtures`. They do not opt in
to the native clipboard test. Capture, printer drivers, hotkey conflicts and
multi-monitor behavior need separate Windows checks; a green CI job does not
establish those results.

For a portable package, write notes at `releases/<version>.json`, commit the
exact source, then run:

```powershell
./scripts/package-github.ps1 -Version 1.0.0-beta.3
```

The script checks source cleanliness, runs tests, builds binaries, checks PE
imports, exercises CLI save/reopen/render, generates notices/SBOM/source hashes,
and writes a new package beneath `artifacts/github/releases`. Existing packages
are never overwritten. It does not publish or modify the website or Library.

Brand PNGs/ICO and the Windows resource include SVG/RC inputs.
`scripts/verify-windows-resource.ps1` recompiles and compares the resource with the
Windows SDK. Optional PNG/ICO regeneration uses `sharp` and
`scripts/build-brand-assets.cjs` (editor) or `node scripts/build-brand-assets.cjs capture-controller`; normal builds do not require it. Optional
headless mesh-image inspection uses Python, Pillow and NumPy with
`scripts/render-egui-fixture.py`.

The release source excludes the separate website and internal review tracker.
The application build requires neither. Screenshots use synthetic test content.

Portable builds include `install-start-menu.ps1` for one ClipDesigner shortcut.
Run `scripts/build-msi.ps1 -Package <clean-payload-folder> -Output <new.msi>
-Version 1.0.0-beta.3` to build an unsigned per-user MSI with the pinned WiX toolset.
The script downloads and verifies its build tool; it never installs the application.
