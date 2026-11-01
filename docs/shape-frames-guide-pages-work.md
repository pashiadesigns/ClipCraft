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

**Add Page** offers a blank page or a copy of the current page. With multiple
pages, each page has a delete action with confirmation. At small zoom levels,
page actions become compact buttons outside the canvas to avoid covering content.
Section menus offer **Move to** and **Copy to** other pages.

Tool Settings has a **Step Guide** group with page dimensions, Letter/A4/landscape
presets and automatic overflow. Width changes scale content proportionally;
whole sections move to following pages when they no longer fit. Sections taller
than a page require a taller page or a shorter section; they are never silently
cut. **Distribute sections** repacks the reading order into pages. Page and section
operations support one-step Undo and persist in editable projects and saved
local templates.

PDF export and print preview honor explicit guide pages. PDF retains supported
vector content and frame outlines. PNG/JPEG represent the complete stacked canvas.

## Compatibility and verification

Shape frames and page metadata require project format 27. Older ClipCraft builds
reject these new projects instead of discarding their data. Existing projects
without these features retain their earlier format. The existing 25-million-pixel
canvas budget and 32768-pixel dimension limit also apply to paged documents, with
an additional upper bound of 128 pages.

Automated coverage includes clipping, independent opacity, crop geometry,
conversion, source-texture reuse, synthetic drops, click modes, page/section
operations, saved templates, legacy backgrounds, invalid input, persistence,
Undo, explicit PDF page boundaries and snapping popup interaction. Native
Explorer drag/drop remains a separate review check. No new native Snagit parity
or signing claim is made.
