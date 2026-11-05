# ClipCraft

**Capture a moment. Explain a workflow. Make it editable.**

ClipCraft brings image capture, instructional guides and video editing into one
local Windows workspace. Capture your screen, point out what matters, add narration,
and share a polished image, PDF or video. No ClipCraft account is required.

**[Download 1.0.0-beta.1](https://github.com/pashiadesigns/ClipCraft/releases/tag/v1.0.0-beta.1)**
· [Install & storage locations](docs/installing.md) · [Build from source](docs/building.md)
· [Known limitations](#beta-notes)

Windows 10/11, x64. Choose the **MSI installer** or extract the **portable ZIP**.
This beta is unsigned. The source is available under Apache-2.0 with retained
third-party notices. Older development project formats are not supported.

## Images that explain

![Editable image guide with Focus controls](docs/images/image-workflow.png)

Capture a region, window or fullscreen; stitch a scrolling region manually or
automatically; or record actions as an editable **Step by Step** guide. Captures
are saved to your local Library even when you send the result only to the clipboard.

- **Annotate:** Lines and arrows, rich text, shapes and callouts, Icons, numbered
  Steps, Penlighter, and editable image frames.
- **Direct attention:** Focus windows keep an area clear while blurring or
  darkening the content beneath them. Combine effects, choose a frame shape and
  keep labels and arrows above the effect.
- **Build guides:** reusable templates, sections, multipage layouts, Page Setup,
  physical page sizes, margins, headers/footers, numbering, bleed and crop marks.
- **Refine:** layers, groups, alignment, snapping, crop, Cut Out, Eraser, rotation,
  flips, text styles, lists and presets. Original content stays editable.
- **Review privacy:** local OCR and sensitive-information suggestions for still
  images; opaque redaction, blur and pixelation with optional redaction labels.

Export PNG, JPEG or PDF; print from Windows; save an editable `.iclip` project.

## Video with context

![Video timeline with annotations, narration and motion properties](docs/images/video-workflow.png)

Record your screen or a selected window with microphone and system audio, then
continue in the editor. Add recordings to an existing project, import media, or
record narration with an optional webcam.

- Arrange video, images, audio, voiceover and annotations on separate tracks.
- Trim, split, duplicate, link and move clips together; adjust fades, volume and
  timing, and generate a held still frame from a video.
- Add animated text, shapes, arrows, Icons and Steps. Use entrance/exit presets,
  path drawing, typewriter text and compatible animation combinations.
- Add editable **Zoom** and **Focus** objects: their position in the track stack
  determines which underlying content they affect.
- Crop, pan and zoom inside video frames; change frame shapes, strokes and shadows.

Export MP4 or save an editable `.vclip` with its accompanying media folder.

## Narration and audio, in the same workflow

![Effects Workstation with EQ, live spectrum and an editable vocal chain](docs/images/effects-workstation.png)

Record and review takes, choose input/output devices, monitor through headphones,
and switch stereo/mono. The Effects Workstation provides an ordered, editable
chain of **parametric EQ, compression, clarity, de-essing, leveling and limiting**.
Apply effects to a clip or a whole track, adjust them during playback, and compare
original and processed sound. Local **Analyze Voice** suggests settings that you
can audition and refine; the original recording is preserved.

## Built around your workflow

![Image and video capture controls](docs/images/capture-workflow.png)

The Library includes search, tags and application/website/date filters. Save
templates and presets, customize shortcuts, capture from the tray, and switch
between image and video projects without leaving ClipCraft.

Screenshots above are rendered from the application with original synthetic
demonstration content. Release packages contain no personal projects, recordings,
saved settings or prepopulated Recent items.

## Beta notes

- **Framed crossfades:** the missing-preview/transparent-canvas defect is repaired.
  Transitions involving rotated, transparent, differently framed or fit-mode
  video can still play less smoothly. Export uses the full-resolution compositor.
- **Video privacy:** redactions are editable, timed objects; they do not
  automatically follow moving text or people. Review every frame. Focus is visual
  emphasis, not a redaction tool. Blur/pixelation can leave information inferable.
- **Share exports for redaction:** editable projects and their media retain
  original content. Do not share them as sanitized documents. See
  [the security and data model](docs/security-model.md).
- Scrolling capture is limited to a selected region. Exclude fixed headers and
  footers from that region. Automatic cursor choreography and tracked video
  redaction are planned for a later revision.
- Complex effects and high-resolution webcam previews may be slower on some
  systems. Capture devices, drivers and Windows audio routing can affect results.
- PDF text is exported as vector outlines, not searchable text. Installed fonts
  are referenced by editable projects; missing fonts use a fallback.
- This first v1 beta does not support old development projects or automatic
  updates. Back up important work. The installer and portable binaries are unsigned.

## Source, licenses and reporting

ClipCraft is written in Rust. See [building](docs/building.md),
[release verification](docs/github-releases.md), [licensing](docs/licensing.md),
[contributing](CONTRIBUTING.md) and [security reporting](SECURITY.md).

Report a bug through [GitHub Issues](https://github.com/pashiadesigns/ClipCraft/issues)
with the version, reproduction steps and a non-sensitive example when possible.
