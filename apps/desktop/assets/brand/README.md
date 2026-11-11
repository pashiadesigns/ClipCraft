# ClipDesigner brand assets

`clipdesigner.svg` is the exact user-supplied `clipcraft logo.svg` adopted on
2026-10-31, replacing the initial A / Capture vector. Its SHA-256 is
`82da9a0ae8e2e4695235ce381002da22315fa0056bd85b02b69071696eb49152`.
Preserve its square cyan background, white canvas and pointer, blue circle, and
black outlines/corner markers. The source file is copied byte for byte; the
exporter only bounds its percentage-sized viewport in memory. The prior master
is preserved in `artifacts/logo-source-update/previous-master.svg`.

Run `node scripts/build-brand-assets.cjs` from the workspace with the cached
`sharp` package available in `NODE_PATH` to regenerate all PNGs and the ICO.
Then run the Windows SDK resource compiler from this directory:

```powershell
rc.exe /nologo /fo clipdesigner.res clipdesigner.rc
```

The checked-in `.res` keeps normal offline builds independent of the resource
compiler. The desktop build links it as resource 101; egui windows and in-app
branding load the same 256px PNG. Native capture windows load the shared icon
resource. ICO sizes cover 16, 20, 24, 32, 40, 48, 64, 128 and 256 pixels; the 512px
PNG is available for larger previews.
