//! Ordered display layers for a viewport renderer. Ordinary annotations retain
//! their geometry; only effects which read/erase earlier pixels need a raster
//! replacement patch. This is display data, never a flattened document edit.
use crate::{Annotation, Document, Error, Kind, MAX_PIXELS, Result};
use image::RgbaImage;

pub mod effect_patch;
pub mod vector;
mod viewport;
pub use viewport::Viewport;
pub use viewport::render_annotation_preview;
pub(crate) use viewport::{render_rotated, render_vector};

#[derive(Debug)]
// Keep vector annotations inline: adding a font identity must not add an allocation per viewport object.
#[allow(clippy::large_enum_variant)]
pub enum Layer {
    Vector(Annotation),
    Shadow(crate::shadow::Patch),
    /// Replace these pixels, including transparent pixels. Source-over drawing
    /// would incorrectly reveal older annotations beneath an eraser stroke.
    ReplacePixels {
        origin: [u32; 2],
        pixels: RgbaImage,
    },
}

pub struct DisplayList {
    pub(crate) base: RgbaImage,
    /// Immutable after construction, so a blank guide canvas can be filled once
    /// rather than sampled four times per display pixel at fractional zoom.
    base_color: Option<image::Rgba<u8>>,
    pub layers: Vec<Layer>,
}

impl Document {
    pub fn display_list(&self) -> Result<DisplayList> {
        self.display_list_limited(MAX_PIXELS)
    }

    fn display_list_limited(&self, patch_pixel_limit: u64) -> Result<DisplayList> {
        let visible: Vec<_> = self
            .annotations
            .iter()
            .filter(|a| self.object_visible(a.id))
            .cloned()
            .collect();
        layers_from_base(self.base_image()?, &visible, patch_pixel_limit)
    }
}

pub(crate) fn layers_from_base(
    base: RgbaImage,
    annotations: &[Annotation],
    patch_pixel_limit: u64,
) -> Result<DisplayList> {
    // No raster effects means no intermediate bitmap or annotation
    // rasterization is needed. The UI can tessellate vectors at its DPI.
    let last_effect = annotations.iter().rposition(needs_pixels);
    let mut composite = last_effect.map(|_| base.clone());
    let mut layers = Vec::with_capacity(annotations.len());
    let mut patch_pixels = 0u64;
    for (index, annotation) in annotations.iter().enumerate() {
        if annotation.analysis.is_some() {
            continue;
        }
        let shadow = crate::shadow::patch(annotation);
        if let Some(image) = &mut composite {
            if crate::shadow::above_pixel_effect(annotation.kind) {
                crate::render_annotation_ink(image, annotation);
                if let Some(patch) = &shadow {
                    patch.paint(image);
                }
            } else {
                if let Some(patch) = &shadow {
                    patch.paint(image);
                }
                crate::render_annotation_ink(image, annotation);
            }
        }
        if needs_pixels(annotation) {
            if let Some([left, top, width, height]) =
                effect_bounds(annotation, base.width(), base.height())
            {
                patch_pixels += u64::from(width) * u64::from(height);
                if patch_pixels > patch_pixel_limit {
                    return Err(Error::Invalid("Vector display effect patches exceed the memory limit; use the standard raster preview.".into()));
                }
                let pixels = image::imageops::crop_imm(
                    composite.as_ref().expect("raster effect has a composite"),
                    left,
                    top,
                    width,
                    height,
                )
                .to_image();
                layers.push(Layer::ReplacePixels {
                    origin: [left, top],
                    pixels,
                });
            }
        } else {
            if let Some(patch) = shadow {
                patch_pixels += u64::from(patch.pixels.width()) * u64::from(patch.pixels.height());
                if patch_pixels > patch_pixel_limit {
                    return Err(Error::Invalid(
                        "Object shadow patches exceed the display memory limit; use raster preview"
                            .into(),
                    ));
                }
                layers.push(Layer::Shadow(patch));
            }
            layers.push(Layer::Vector(annotation.clone()));
        }
        if Some(index) == last_effect {
            // Later vectors never read pixels; release the full-size
            // working composite before caching the display list.
            composite = None;
        }
    }
    let first = *base.get_pixel(0, 0);
    let base_color = base.pixels().all(|p| *p == first).then_some(first);
    Ok(DisplayList {
        base,
        base_color,
        layers,
    })
}

fn needs_pixels(annotation: &Annotation) -> bool {
    matches!(
        annotation.kind,
        Kind::Blur | Kind::Pixelate | Kind::Eraser | Kind::Redact
    )
}

fn effect_bounds(a: &Annotation, width: u32, height: u32) -> Option<[u32; 4]> {
    let (mut min, mut max) = if a.kind == Kind::Redact {
        a.bounds()
    } else {
        a.body_bounds()
    };
    if a.kind == Kind::Eraser {
        for p in &a.points {
            for axis in 0..2 {
                min[axis] = min[axis].min(p[axis]);
                max[axis] = max[axis].max(p[axis]);
            }
        }
        // Include the existing shared stroke renderer's antialias fringe.
        let reach = a.width as f32 * 0.5 + 1.0;
        min = min.map(|v| v - reach);
        max = max.map(|v| v + reach);
    }
    if a.kind == Kind::Redact && a.rotation_degrees != 0.0 {
        min = min.map(|v| v - 1.0);
        max = max.map(|v| v + 1.0);
    }
    if let Some((lo, hi)) = crate::shadow::bounds(a) {
        for axis in 0..2 {
            min[axis] = min[axis].min(lo[axis]);
            max[axis] = max[axis].max(hi[axis]);
        }
    }
    let left = min[0].floor().clamp(0.0, width as f32) as u32;
    let top = min[1].floor().clamp(0.0, height as f32) as u32;
    let right = max[0].ceil().clamp(left as f32, width as f32) as u32;
    let bottom = max[1].ceil().clamp(top as f32, height as f32) as u32;
    (right > left && bottom > top).then_some([left, top, right - left, bottom - top])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Session, StackOrder};
    use image::{GenericImage, Rgba};

    // CPU oracle for a future GPU consumer: vector geometry must remain in
    // order, and effect patches must REPLACE, rather than alpha-composite.
    fn replay(list: DisplayList) -> RgbaImage {
        let mut image = list.base;
        for layer in list.layers {
            match layer {
                Layer::Vector(a) => crate::render_annotation_ink(&mut image, &a),
                Layer::Shadow(patch) => patch.paint(&mut image),
                Layer::ReplacePixels { origin, pixels } => {
                    image.copy_from(&pixels, origin[0], origin[1]).unwrap();
                }
            }
        }
        image
    }
    fn sample() -> Document {
        let image = RgbaImage::from_fn(160, 100, |x, y| {
            Rgba([
                (x * 3 % 255) as u8,
                (y * 7 % 255) as u8,
                170,
                if x < 4 { 0 } else { 200 },
            ])
        });
        let mut doc = Document::from_image(image).unwrap();
        let mut step = crate::steps::marker(
            1,
            [48.0, 45.0],
            44,
            [190, 40, 80, 180],
            (3, false),
            [160, 100],
        )
        .unwrap();
        doc.annotations.push(step.clone());
        step.id = 2;
        step.kind = Kind::Pixelate;
        step.start = [30.5, 25.25];
        step.end = [72.1, 65.75];
        step.width = 7;
        step.text = None;
        doc.annotations.push(step.clone());
        step.id = 3;
        step.kind = Kind::Eraser;
        step.start = [32.0, 45.0];
        step.end = [75.0, 35.0];
        step.points = vec![step.start, [45.25, 65.5], step.end];
        step.width = 15;
        step.color = [0, 0, 0, 255];
        doc.annotations.push(step.clone());
        step.id = 4;
        step.kind = Kind::Blur;
        step.start = [12.25, 40.5];
        step.end = [89.0, 76.5];
        step.width = 3;
        step.points.clear();
        doc.annotations.push(step);
        doc.annotations.push(
            crate::steps::marker(
                5,
                [98.0, 65.0],
                30,
                [20, 130, 80, 190],
                (5, false),
                [160, 100],
            )
            .unwrap(),
        );
        doc
    }
    #[test]
    fn ordered_layers_match_export_with_transparency_effects_reordering_and_undo() {
        let mut s = Session::new(sample()).unwrap();
        let original = serde_json::to_vec(&s.document).unwrap();
        for command in [
            None,
            Some(Command::Arrange {
                id: 3,
                order: StackOrder::BringToFront,
            }),
            Some(Command::Undo),
        ] {
            if let Some(command) = command {
                s.execute(command).unwrap();
            }
            let list = s.document.display_list().unwrap();
            assert_eq!(list.layers.len(), 5);
            assert_eq!(
                list.layers
                    .iter()
                    .filter(|l| matches!(l, Layer::Vector(_)))
                    .count(),
                2
            );
            assert_eq!(replay(list), s.document.render().unwrap());
        }
        assert_eq!(serde_json::to_vec(&s.document).unwrap(), original);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("display.capture.json");
        s.save(&path).unwrap();
        let reopened = Document::load(&path).unwrap();
        assert_eq!(
            replay(reopened.display_list().unwrap()),
            s.document.render().unwrap()
        );
    }
    #[test]
    fn vectors_stay_geometry_and_effect_patches_are_clipped_to_the_changed_pixels() {
        let mut doc = sample();
        doc.annotations.retain(|a| !needs_pixels(a));
        let list = doc.display_list_limited(0).unwrap();
        assert_eq!(list.base, doc.base_image().unwrap());
        for (layer, a) in list.layers.iter().zip(&doc.annotations) {
            let Layer::Vector(vector) = layer else {
                panic!("vector was flattened")
            };
            assert_eq!(vector, a);
        }
        let mut eraser = sample().annotations[2].clone();
        eraser.start = [0.0, 0.0];
        eraser.end = [6.0, 3.0];
        eraser.points = vec![eraser.start, [2.5, 7.25], eraser.end];
        assert_eq!(effect_bounds(&eraser, 160, 100), Some([0, 0, 15, 16]));
        doc.annotations.push(eraser);
        let list = doc.display_list().unwrap();
        let Layer::ReplacePixels { origin, pixels } = list.layers.last().unwrap() else {
            panic!("missing eraser patch")
        };
        assert_eq!(*origin, [0, 0]);
        assert_eq!(pixels.dimensions(), (15, 16));
        assert_eq!(replay(list), doc.render().unwrap());
    }
    #[test]
    fn invalid_documents_and_patch_budget_fail_without_mutating_the_source() {
        let mut doc = sample();
        let bytes = serde_json::to_vec(&doc).unwrap();
        assert!(doc.display_list_limited(1).is_err());
        assert_eq!(serde_json::to_vec(&doc).unwrap(), bytes);
        doc.annotations[0].start[0] = f32::NAN;
        assert!(doc.display_list().is_err());
    }
}
