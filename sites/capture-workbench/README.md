# ClipCraft companion website

Static product pages, changelog and feature inventory. The application does not
need this website or any hosting account to run.

- `src/` and `public/assets/`: original page code, branding and synthetic imagery.
- `roadmap.json`: canonical feature inventory; `ROADMAP.md` is generated from it.
- `release-data.json`: historical portable download metadata and checksums.
- `scripts/`: build, validation and preview utilities.
- `worker/`: optional hosting implementation, not used by the desktop application.

Run the dependency-free Node tests with:

```powershell
node --test scripts/release.test.mjs scripts/roadmap.test.mjs
```

The existing website build validates every historical ZIP and expects those files
under `release-assets/` plus the deployment-specific `.openai/hosting.json`.
Neither generated archives nor private deployment configuration are in Git. This
legacy hosting path is separate from the portable GitHub release workflow, which
uses the inventory JSON files without requiring a website build.

The original PC-hosted downloads are preserved. GitHub source-linked previews
have their own immutable tags and release attachments. See the repository's
[release guide](../../docs/github-releases.md). Historical inventory evidence
references describe their original verification scope; local artifact paths are
not public downloadable evidence.
