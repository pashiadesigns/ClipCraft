# DesignCraft reuse audit

Inspected 2026-10-30: https://github.com/storytold/designcraft, pinned commit ea08e4c2ba290f092063c99d70c4bdd5afc45887, version 0.14.0. Local read-only checkout: artifacts/reference/designcraft-source. No upstream code is incorporated into ClipCraft alpha.29.

Source is dual MIT OR Apache-2.0 (LICENSE-MIT, LICENSE-APACHE, NOTICE). For selected adaptations use MIT, retain its copyright/license notice in source and distributed third-party notices, and record modified files and revision. Brand assets and third-party material have separate terms; no brand/assets are needed. Both apps currently use egui/eframe 0.37, Rust 2024 and image 0.25. This reduces frontend mismatch but does not make document commands interchangeable.

| Candidate | Exact source | ClipCraft fit / next action |
| --- | --- | --- |
| Shape generators and live corners | crates/geom/src/shapes.rs; crates/geom/src/corners.rs; crates/tools/src/corners.rs | Pure geometry is a strong candidate for more shapes. Live controls support per-corner editing, transformed frames and corner styles; adapt only after the basic toolset. Current radius implementation is original and already verified. |
| Snapping and smart guides | crates/geom/src/snap.rs; crates/tools/src/snap.rs | Small geometry helpers are easy to isolate. Tool-level target collection depends on DesignCraft pages/layers; replace with ClipCraft annotation bounds. UX-006. |
| Selection, grouping and transforms | crates/doc/src/selection.rs; crates/engine/src/cmd/object.rs | Reuse algorithms/tests selectively, not the document model. DesignCraft stores affine transforms, nested items and page spreads; ClipCraft stores image-coordinate annotations. EDIT-010 / EDIT-011. |
| Align and distribute | crates/engine/src/cmd/object.rs | Candidate bounds-based algorithms, but require multiple selection first. Keep each user action one ClipCraft Undo command. EDIT-022. |
| Keyboard/dialog infrastructure | Cargo.toml craft-keymap, craft-dialogs, craft-window declarations | Potential later shared crates, requiring their own pinned license/dependency review. Do not replace working infrastructure merely for consistency. |

DesignCraft primarily targets page layout; image editing overlap is strongest in annotation geometry and interactions. Avoid importing its PDF/EPUB/IDML, typography, page-spread or prepress stack into a screenshot editor. Evaluate PhotoCraft separately if a raster-specific gap warrants it; no additional repositories downloaded yet.

First adoption candidate after alpha.29: shape geometry for an editable ellipse/basic shape selector under EDIT-014. Inspect the isolated geometry and tests, choose a minimal adaptation boundary and preserve provenance. The research does not count as a shipped feature.

## First adaptation: alpha.30

Ellipse anchors and bounds/area regression from geom/src/shapes.rs are now adapted in crates/core/src/ellipse.rs; MIT attribution, exact revision and modifications are preserved in licenses/designcraft and release notices. Native drawing, selection, resizing, Undo and save/reopen exercised. DesignCraft ui-egui/src/app/view.rs held-preview/settle pattern and properties/tests.rs slider regression informed ClipCraft property_preview.rs; its engine was not imported. Live selected colors render before Apply, with cancel/history isolation. See docs/alpha30-live-verification.md. Next reuse candidates: polygon/star geometry, smart guides, then affine rotation after the annotation transform representation is defined.

## Polygon adaptation: alpha.31

Regular polygon vertices from the same pinned DesignCraft geom/src/shapes.rs revision are adapted in crates/core/src/polygon.rs. ClipCraft normalizes vertices to drag bounds, validates 3–64 sides and reuses shared selection/history/rendering. Bounds and triangle-area checks, real project persistence and a canvas-input regression accompany native filled-polygon checks. MIT attribution remains in licenses/designcraft and the package notices. No proprietary reference assets were used.

## Raster effects and eraser boundary: alpha.34/35

The pinned DesignCraft renderfx code is aimed at vector shadows rather than
bounded screenshot raster edits. Blur reuses the already licensed image crate.
The eraser reuses ClipCraft freehand sampling, coverage masks and property
transactions, adding destination-out alpha compositing. No new third-party code
or dependency was needed. Independent raster comparisons verify output.

## Core navigation and selection review, alpha.40 work

The pinned ui-egui/src/canvas/input.rs hand_pan and recent-primary-press handling
were inspected. ClipCraft's original implementation extends its existing view
transform with middle-button panning and a complete-drag event fallback. No
DesignCraft document or renderer was imported. This is behavior/architecture
reference, not a claim of native middle-drag parity; the current inspection API
cannot hold the middle button during a drag. Actual egui input tests cover it.

Engine cmd/object.rs align_reference/align/distribute were inspected as the next
reuse candidates. The compatible boundary is annotation bounds plus atomic
UpdateMany commands. DesignCraft's page/spread/key-object machinery is not needed.
The next controls will distinguish object bounds alignment/distribution from
text justification, and remember the last action independently for each button.

## Alignment/distribution adaptation (after alpha40)

crates/core/src/layout_selection.rs adapts the pinned engine object.rs align/distribute bounds algorithms under MIT. Provenance/license travels with existing DesignCraft notices. ClipCraft adds typed actions, atomic batch history, center ordering, combined centering and explicit clipping rejection. Seven alignments and four distributions have geometry/Undo/persistence tests. New atomic independent quick-control preferences and desktop split buttons are original code; native verification is in progress, not released in alpha40.

## Object templates after alpha41

Inspected pinned engine/cmd/style/object.rs create/apply and geometry-preserving fields. ClipCraft reuses its own ArrowStyle atomic storage and UpdateMany rather than importing page-layout styles, effects and text stories. New object_styles.rs and object_templates.rs are original implementation; no further upstream source copied. Current templates deliberately exclude labels, pointers and geometry. docs/object-template-verification.md.

Post-alpha44: closest-axis matching from pinned MIT geom/src/snap.rs adapted in crates/core/src/snapping.rs; annotation bounds/group anchors, selected-object exclusion, canvas clamping and guide verification added. Provenance retained in licenses/designcraft/PROVENANCE.txt. docs/step-snapping-verification.md.

Post-alpha46: pinned tools/src/select.rs resize_rect/finish_resize/aspect_scale
and geom/src/lib.rs constrain_angle were audited. Shift preserves aspect and Alt
anchors at the center, including their combination and midpoint handles. The
adaptation in resize_modifiers.rs adds ClipCraft annotation bounds, paths, labels,
pointer preservation and canvas constraints. No upstream UI assets or document
model imported. Input and persistence checks plus native plain-edge resizing are
recorded in docs/interaction-modifiers-verification.md; held-modifier native drag
coverage remains pending due to the inspection API capability limit.

Post-alpha49 vector work: reviewed pinned ui-egui/src/canvas/render.rs and render/src/lib.rs. Viewport-sized rendering with zoom/DPI transform and cached overscan is a useful boundary. Added original shared display-list composition; no upstream code or new dependency copied in this pass. docs/vector-display-work.md.

Subsequent background pass: original Vello CPU0.2.0 viewport implementation now
uses the already cached dependency directly under MIT OR Apache-2.0. No additional
DesignCraft code was copied. ClipCraft uses its own ordered effect replacement,
shared font outlines/layout, bounded viewport cache and raster fallback; no page,
spread or PDF machinery was imported.226Rust checks pass; native/user validation
is pending. docs/vector-display-work.md and artifacts/vector-display/candidate.json.

2026-10-30 rotation stage: audited select.rs rotate_zone and Drag::Rotate at the same pinned revision. Adapted isolated geometry in crates/core/src/rotation.rs under MIT; no page model or assets imported. Four pure geometry tests pass; metadata, shared export, desktop handle integration and native evidence are still outstanding. docs/object-rotation-work.md.

2026-10-30 clipboard stage: inspected pinned ui-egui/src/app/clipboard.rs and engine/cmd/edit.rs routing between text and selected-object payloads. Added original clipboard_payload.rs with ClipCraft schema, geometry and command history; no DesignCraft source or publishing/runtime stack imported. docs/clipboard-paste-work.md.

## Initial draw angle snapping, 2026-10-30

Pinned geom/src/lib.rs::constrain_angle and tools/src/frame.rs initial line handling inspected. crates/core/src/angle_snap.rs adapts the MIT angle/radius approach with exact cardinal directions and ray clipping to ClipCraft canvas bounds. Same helper feeds live preview and release for Line/Arrow. All-octant and actual egui initial-draw/modifier-toggle tests accompany it. Licensing notices retained.
2026-10-31 text outline continuation: adapted paint_glyphs from the pinned
render/src/text.rs into crates/core/src/text/outline.rs. MIT attribution is in
that file and licenses/designcraft/PROVENANCE.txt (included by release.ps1).
ClipCraft supplies its existing fitted glyphs and rule bands, and adds isolated
outer coverage, clipping and validated parameters. Actual raster tests cover
both fonts, opaque/transparent fill geometry, antialiased edges, decorated
glyph unions and preservation of underlying destination pixels. The helper is
not wired into documents or UI yet; do not claim editable outlines delivered.
