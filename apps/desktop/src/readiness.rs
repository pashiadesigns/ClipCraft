//! Build-local readiness: the canonical inventory supplies targets, while an
//! explicit, scoped verification list gates green labels. This is UI metadata,
//! never part of a document, command, or rendered image.
use serde::Deserialize;
use std::sync::LazyLock;

#[derive(Deserialize)]
struct Inventory {
    features: Vec<Feature>,
}
#[derive(Deserialize)]
struct Feature {
    id: String,
    title: String,
    description: String,
    status: String,
    evidence: Vec<String>,
    released_in: Option<String>,
    target_version: Option<String>,
    missed_target_version: Option<String>,
}
impl Feature {
    // A Ready badge describes behavior verified in this binary. Canonical
    // "shipped" separately requires publication; qualified candidates retain
    // needs_verification until their exact package/public route is checked.
    fn build_release(&self) -> Option<&str> {
        match self.status.as_str() {
            "shipped" => self.released_in.as_deref(),
            "needs_verification" => self.target_version.as_deref(),
            _ => None,
        }
    }
}
static INVENTORY: LazyLock<Inventory> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../sites/capture-workbench/roadmap.json"
    ))
    .expect("validated feature inventory")
});

// These scopes have native evidence in selection-, text-, and import-verification.
// An inventory status alone must never make a newly added control green.
const VERIFIED: &[(&str, &str)] = &[
    (
        "UX-027",
        "New Step press-drag-release placement, retained selection and one Undo with sequence restoration; native save/reopen and exact clipboard verified.",
    ),
    (
        "UX-028",
        "Object/group movement and new Step edge/center snapping. Native toggle and exact saved positions verified; held-guide painting is input-tested. Drawing/resize snapping remains separate.",
    ),
    (
        "UX-029",
        "Compatible-object cyan hover borders/nodes and retained selection after creation. Native Step hover/selection and all supported drawing input regressions verified.",
    ),
    (
        "EDIT-023",
        "Direct canvas text input with shared export wrapping, sidebar formatting, keyboard selection, outside/Ctrl+Enter commit and one Undo. Native five-type input and borderline wrapping verified. Rich text remains separate.",
    ),
    (
        "EDIT-046",
        "Centered labels in Text, Callout, Rectangle, Ellipse and Polygon. Legacy saved alignment is preserved; native entry, independent text color and shared wrapping verified.",
    ),
    (
        "UX-024",
        "Canvas tool glyphs and text/move/resize/pan pointers; native visual checks and input regressions. UI indicators never enter exported pixels.",
    ),
    (
        "UX-026",
        "Hide/Show annotations compares the current underlying raster without changing editable objects, selection, history or clipboard/export. Previously flattened image operations remain separate.",
    ),
    (
        "UX-023",
        "Existing-object presets and local formatting templates. Native Text/Line/Shape/Step/Blur/Pixelate presets and custom callout save/batch application, active borders, one Undo and exact saved output verified. New-object defaults and missing tools remain unfinished.",
    ),
    (
        "UX-022",
        "Seven object alignments, four distributions and nine text justifications with independently remembered split buttons. Native restart, repeat, one Undo, editable persistence and exact alpha41 output verified. Inline shape text and complete Arrange parity remain separate.",
    ),
    (
        "EDIT-043",
        "Rectangular pixel selection: native reverse drag, movement/eight handles, Copy/Cut/Clear, review/Cancel, one Undo and exact packaged pixels verified in alpha.37. Other masks, Auto-Fill and moving selected pixel content remain planned.",
    ),
    (
        "EDIT-042",
        "Connected color Fill: native review/Cancel, color/opacity/tolerance, one Undo, save/reopen and exact packaged clipboard pixels verified in alpha.36. Global replacement and antialiased fill remain planned.",
    ),
    (
        "EDIT-041",
        "Editable smooth eraser: native dots and strokes, width/opacity, Move, one Undo, saved projects and exact packaged transparency/clipboard output verified in alpha.35. Brush presets and hard-edge toggle remain unfinished.",
    ),
    (
        "EDIT-040",
        "Rectangular Blur and Pixelation: native creation, type and live intensity, Move/resize, one Undo, persistence and exact packaged clipboard output verified in alpha.34. Ellipse effects, presets and Smart Redact remain unfinished.",
    ),
    (
        "EDIT-039",
        "Interactive rectangular crop: reverse drag, eight handles, movement, explicit flattening, Cancel, one-step Undo, exact saved pixels and clipboard verified in alpha.33. Non-destructive off-canvas object editing remains planned.",
    ),
    (
        "EDIT-037",
        "Whole-image clockwise, counterclockwise, half-turn and horizontal/vertical flips. Native menus, flatten confirmation, one-step Undo, exact saved pixels and clipboard verified. Custom angles and selected-object rotation remain planned.",
    ),
    (
        "EDIT-036",
        "Filled rectangles, ellipses and editable polygons: native Fill/No Fill, live opacity and side-count edits, one-step Undo/Redo, resizing, persistence and output verified. Additional shape families, rotation and shadows remain planned.",
    ),
    (
        "EDIT-009",
        "Editable outlined rectangles: native drawing, movement, corner resizing, uniform rounding, Undo, persistence and output verified. General fills ship in EDIT-036; additional styles remain planned.",
    ),
    (
        "EDIT-035",
        "Editable outlined ellipses with native creation, handle resizing, stroke RGBA/width, Undo and saved output. Adapted DesignCraft geometry is MIT-attributed. General fills and polygons ship in EDIT-036; additional styles remain planned.",
    ),
    (
        "UX-016",
        "Live selected-object stroke, radius and color/fill previews. Release or Apply commits one edit; Escape, focus loss and Cancel restore the original. Rotation remains planned.",
    ),
    (
        "EDIT-013",
        "Editable Pen, Line and Highlighter: RGBA/width, movement and resizing, one-gesture Undo, persistence and flattened output. Advanced styles and pen pressure remain planned.",
    ),
    (
        "EDIT-034",
        "Basic circular number/letter markers with next/reset, size/fill, editable labels, Undo and persistence. Automatic text contrast; alternative shapes and independent text color remain planned.",
    ),
    (
        "EDIT-033",
        "Drag blue corner circles inward to round rectangles/callouts and outward to square them. One radius, preserved bounds/pointer, Undo and saved output verified. Per-corner styles remain planned.",
    ),
    (
        "CAP-006",
        "Session delays of 3/5/10 seconds; native countdown Cancel preserves the document. Final delayed capture matches synthetic pixels without countdown/editor controls.",
    ),
    (
        "UX-014",
        "Original arrow presets, local custom save/remove/restart, selected-arrow application with one Undo and unchanged geometry. Reapplication is a no-op. RGBA/width and alpha compositing are verified; themes, favorites, other tools and stroke-quality refinement remain separate.",
    ),
    (
        "LIB-009",
        "Local image counts, UTC year/search filtering, deterministic sorting and 50–200% thumbnail sizing. Filtered reopen preserves pixels and unsaved protection. Full source groups, date hierarchy, batch export and reference parity remain separate.",
    ),
    (
        "LIB-008",
        "Compact recent thumbnails below the status row, active-snapshot border, unsaved marker and image/project labels. Reselecting the active item preserves edits and Undo; canceled switches keep selection. Pin/close, exact source-format labels and full reference parity remain separate.",
    ),
    (
        "LIB-005",
        "Instant literal search across snapshot names, source labels and dimensions; all words must match, ignoring case. Clear restores all entries; filtered reopen and refresh preserve snapshots. Tags, folders and full reference Library parity remain separate.",
    ),
    (
        "LIB-001",
        "Local snapshots and thumbnails on open, create, paste, capture and save. Library, recent tray and Open Recent restore editable copies across restarts with unsaved protection. Last 50 snapshots / 512 MB retained; ongoing edits require saving. Search, bulk import, storage settings and full reference layout remain separate.",
    ),
    (
        "EDIT-029",
        "Resize confirmation with Cancel/Escape, one-step flatten-and-resize Undo, saved original retention and exact native clipboard output. Candidate preview, print sizing and complete reference parity remain separate.",
    ),
    (
        "EDIT-031",
        "Image > Flatten All and Ctrl+Shift+T merge all objects into the current raster without changing output pixels. Undo restores editability; version2 projects retain original pixels for Revert. Selected-object flatten and resize confirmation remain separate.",
    ),
    (
        "EDIT-030",
        "Resize Canvas with absolute/relative pixel sizes, nine placements, transparent padding, exact raster translation/cropping, Undo/Redo and original restoration. Cropping across editable objects is rejected; interactive crop and off-canvas editing remain planned.",
    ),
    (
        "UX-012",
        "View/Ctrl+G pixel grid at 600% zoom and above, aligned to image pixels during panning. Workspace only: saved images, clipboard output and history are unchanged. Saved preferences and reference styling remain planned.",
    ),
    (
        "EDIT-027",
        "Pixels/Percent image sizing with aspect locking, flatten confirmation for editable objects, Undo/Redo and retained original dimensions. Print units, DPI and exact reference dialog parity remain planned.",
    ),
    (
        "EDIT-003",
        "Solid-black editable redactions with verified opaque flattened PNG and clipboard pixels. Editable project files retain the original image; share flattened output to conceal it.",
    ),
    (
        "UX-011",
        "Canvas zoom, actual size, fit modes and bounded wheel panning. Native drawing and movement at 25% and panned 200% preserve image coordinates. Pixel grid, snapping and saved preferences remain planned.",
    ),
    (
        "CAP-004",
        "Windows Ctrl+Alt+C from other apps, minimized wake, conflict reporting and a session toggle. Custom shortcuts and persistent preferences remain planned.",
    ),
    (
        "CAP-001",
        "Windows region capture from the editor. Verified on this PC's display configuration; global hotkeys and extended display qualification remain planned.",
    ),
    (
        "EDIT-001",
        "Basic straight arrows; advanced arrow styles remain planned.",
    ),
    (
        "EDIT-002",
        "Undo and redo for the current document commands.",
    ),
    (
        "EDIT-004",
        "Multiline text with two bundled fonts. Type directly inside boxes and edit font, alignment and color in the sidebar. Advanced font formatting remains planned.",
    ),
    (
        "EDIT-005",
        "Single-object selection, movement, resizing, and keyboard nudges. Pixel selection and Smart Move remain planned.",
    ),
    ("DOC-001", "Save and reopen portable editable projects."),
    (
        "FILE-001",
        "Flattened PNG export through the shared document renderer.",
    ),
    ("FILE-004", "Open PNG, JPEG, or a ClipCraft project."),
];

pub struct Badge {
    pub ready: bool,
    pub label: String,
    pub detail: String,
}
pub fn badge(id: &str) -> Badge {
    let Some(feature) = INVENTORY.features.iter().find(|f| f.id == id) else {
        return Badge {
            ready: false,
            label: "Unmapped".into(),
            detail: format!("{id}: no verified feature mapping"),
        };
    };
    let verified = VERIFIED.iter().find(|(key, _)| *key == id);
    let ready =
        verified.is_some() && feature.build_release().is_some() && !feature.evidence.is_empty();
    let label = if ready {
        "Ready".into()
    } else {
        feature
            .missed_target_version
            .as_ref()
            .map(|v| format!("Overdue {}", v.trim_start_matches("0.1.0-")))
            .or_else(|| {
                feature
                    .target_version
                    .as_ref()
                    .map(|v| format!("Target {}", v.trim_start_matches("0.1.0-")))
            })
            .unwrap_or_else(|| {
                if feature.status == "shipped" {
                    "Needs check".into()
                } else {
                    "Unscheduled".into()
                }
            })
    };
    Badge {
        ready,
        detail: if ready {
            format!(
                "{} · {}\n{}",
                feature.id,
                feature.title,
                verified.unwrap().1
            )
        } else {
            format!(
                "{} · {}\n{} — this scope is unfinished.\n{}\n{}",
                feature.id,
                feature.title,
                label,
                feature.description,
                feature
                    .target_version
                    .as_ref()
                    .map(|v| format!("Current provisional target: {v}"))
                    .unwrap_or_else(|| {
                        "No new release date promised. Original missed targets are retained.".into()
                    })
            )
        },
        label,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn badges_require_native_evidence_and_a_release_definition() {
        let mut ids = std::collections::HashSet::new();
        assert!(
            INVENTORY.features.iter().all(|f| ids.insert(&f.id)),
            "feature IDs must be unique"
        );
        let releases: serde_json::Value = serde_json::from_str(include_str!(
            "../../../sites/capture-workbench/release-data.json"
        ))
        .unwrap();
        for (id, _) in VERIFIED {
            let feature = INVENTORY.features.iter().find(|f| f.id == *id).unwrap();
            assert!(badge(id).ready, "verified scope lost evidence: {id}");
            let in_feed = releases["releases"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["version"].as_str() == feature.build_release());
            // release.ps1 validates the binary before adding its immutable ZIP
            // to the feed. A new native scope may refer to that release's exact
            // notes during staging; a deployment receipt remains separate proof
            // of publication, which cannot be inferred by a local unit test.
            if !in_feed {
                let version = feature.build_release().unwrap();
                let notes = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../releases")
                    .join(format!("{version}.json"));
                let notes: serde_json::Value = serde_json::from_slice(
                    &std::fs::read(notes).expect("verified scope needs staged release notes"),
                )
                .unwrap();
                assert_eq!(notes["version"].as_str(), Some(version));
                assert!(notes["changes"].is_object());
                assert!(notes["limitations"].is_array());
            }
        }
        assert!(
            !badge("SHIP-001").ready,
            "shipped metadata alone is not a verified control"
        );
        for id in ["EDIT-008", "EDIT-012", "EDIT-047", "CAP-005"] {
            let badge = badge(id);
            assert!(!badge.ready);
            assert!(badge.label.starts_with("Overdue alpha.") || badge.label == "Unscheduled");
        }
        assert!(!badge("unknown").ready);
        assert!(
            !badge("UX-018").ready,
            "partial parent must not inherit readiness"
        );
    }
}
