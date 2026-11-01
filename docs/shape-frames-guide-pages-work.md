# Image frames and guide pages

This batch is a review candidate. Published preview.2 remains unchanged.

## Image frames

Select a rectangle, ellipse or polygon and use **Convert to Image Frame** above
Delete object, or the same action in its context menu. The geometry, outline,
fill, rotation and shadow remain editable. The frame holds an image instead of
text; Undo restores the previous text when reversing a conversion.

Click once for shape properties. Double-click for crop, pan, zoom, replacement
and image opacity. Outline opacity and image opacity are independent. The frame
styles provide square, rounded rectangle and circle presets. **Convert to Basic
Shape** removes the image and restores ordinary text-capable shape behavior.

Drag a local image file over an editable frame to see a cyan outline and
**Drop image to fill frame**. Release to replace its contents in one Undo step.
Locked and hidden frames are excluded. Image pixels remain shared while moving
or zooming; shape masks are drawn separately from the retained source texture.
Windows OLE drag tracking uses client-space pointer coordinates while a drag is
active because the windowing backend omits pointer motion during DragOver.

## Guide pages

Pages are arranged left to right. The centered page-plus icon offers a blank page or a copy of the current page. With multiple
pages, each page has a red trash icon with confirmation. Left and right arrows
move a page before its previous neighbor or after its next neighbor.
Section menus offer **Move to** and **Copy to** other pages.

Objects lists collapsed pages containing sections and their objects. Drag page
rows to reorder them; these containers do not create canvas groups. An object
belongs to the section with the largest overlap of its bounds, with its previous
section retained on a tie. Choose Image opens the shared File/Clipboard/Library
source menu.

Tool Settings has a **Step Guide** group with page dimensions, Letter/A4/landscape
presets and automatic overflow. Width changes scale content proportionally;
whole sections move to following pages when they no longer fit. Sections taller
than a page require a taller page or a shorter section; they are never silently
cut. **Distribute sections** repacks the reading order into pages. Page and section
operations support one-step Undo and persist in editable projects and saved
local templates.

PDF export and print preview honor explicit guide pages. PDF retains supported
vector content and frame outlines. PNG/JPEG represent the complete side-by-side canvas.

## Compatibility and verification

Shape frames require project format 27; horizontal pages require format 28. Older ClipCraft builds
reject these new projects instead of discarding their data. Existing projects
without these features retain their earlier format. The existing 25-million-pixel
canvas budget and 32768-pixel dimension limit also apply to paged documents, with
an additional upper bound of 128 pages.

Automated coverage includes clipping, independent opacity, crop geometry,
conversion, source-texture reuse, synthetic drops, click modes, page/section
operations, saved templates, legacy backgrounds, invalid input, persistence,
Undo, explicit PDF page boundaries and snapping popup interaction. Native
Explorer drag/drop passed user check U1 on the preceding candidate. No new native Snagit parity
or signing claim is made.

## Follow-up rendering corrections

The retained frame renderer previously clipped outline ink to the image body.
Its clip now includes the stroke extent. Rotated and shadowed raster frames keep
the original source texture, geometry and a separate native-resolution shadow
texture. Zoom and fractional movement reuse those pixels. Other CPU previews are
bounded to the visible viewport rather than the full zoomed object extent.

Older vertical page projects are converted to a horizontal layout in memory on
open, preserving their files and object locks. If the horizontal extent would
exceed the existing canvas limit, the valid old layout remains accessible.

Automated regressions exercise 1920×1080 source images, ellipse and pentagon
outlines, shadows, rotation and reset, and zoom/drag through 1600%. Headless
texture reuse is evidence about rendering work, not a measured native GPU frame
rate. User review still determines whether the reported interaction lag is gone.

Follow-up qualification: 622 Rust tests, formatting and Clippy pass. The focused
1920×1080 frame benchmark reuses all source/shadow textures across nine warm
zoom/drag frames in 0.33–1.23 ms total (unoptimized headless CPU test on the
development PC). This excludes native presentation and is not a GPU FPS claim.
