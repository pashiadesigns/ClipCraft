//! Straight strip removal, with exact retained RGBA and original-image retention.
use crate::{Document, Error, RasterImage, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CutAxis {
    Horizontal,
    Vertical,
}

impl CutAxis {
    pub fn dimension(self) -> usize {
        if self == Self::Horizontal { 1 } else { 0 }
    }
    pub fn output_size(self, size: [u32; 2], start: u32, end: u32) -> Result<[u32; 2]> {
        let axis = self.dimension();
        if start >= end || end > size[axis] || end - start == size[axis] || size.contains(&0) {
            return Err(Error::Invalid("Cut Out must remove a nonempty strip while keeping at least one pixel row or column.".into()));
        }
        let mut result = size;
        result[axis] -= end - start;
        Ok(result)
    }
}

impl Document {
    pub(crate) fn cut_out(&mut self, axis: CutAxis, start: u32, end: u32) -> Result<()> {
        let size = axis.output_size([self.width, self.height], start, end)?;
        let source = self.render()?;
        let result = image::RgbaImage::from_fn(size[0], size[1], |x, y| {
            let mut p = [x, y];
            if p[axis.dimension()] >= start {
                p[axis.dimension()] += end - start;
            }
            *source.get_pixel(p[0], p[1])
        });
        let raster = std::sync::Arc::new(RasterImage::from_image(&result)?);
        self.source_size = Some(self.source_size.unwrap_or([self.width, self.height]));
        [self.width, self.height] = size;
        self.raster = Some(raster);
        self.annotations.clear();
        self.auto_canvas_base = None;
        self.version = 2;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Session};
    use image::{Rgba, RgbaImage};
    #[test]
    fn strip_seams_and_edges_preserve_exact_rgba_history_persistence_and_original() {
        let image = RgbaImage::from_fn(17, 11, |x, y| {
            Rgba([x as u8 * 13, y as u8 * 21, 89, ((x + y) % 3 * 100) as u8])
        });
        let doc = Document::from_image(image.clone()).unwrap();
        for axis in [CutAxis::Horizontal, CutAxis::Vertical] {
            let length = [17, 11][axis.dimension()];
            for (start, end) in [
                (3, 7),
                (0, 4),
                (length - 4, length),
                (0, length - 1),
                (1, length),
            ] {
                let mut session = Session::new(doc.clone()).unwrap();
                session
                    .execute(Command::CutOut { axis, start, end })
                    .unwrap();
                let actual = session.document.render().unwrap();
                for (x, y, p) in actual.enumerate_pixels() {
                    let src = if axis == CutAxis::Horizontal {
                        [x, if y < start { y } else { y + end - start }]
                    } else {
                        [if x < start { x } else { x + end - start }, y]
                    };
                    assert_eq!(p, image.get_pixel(src[0], src[1]));
                }
                assert_eq!(session.document.source_png, doc.source_png);
                let dir = tempfile::tempdir().unwrap();
                let path = dir.path().join("cut.capture.json");
                session.save(&path).unwrap();
                assert_eq!(Document::load(&path).unwrap().render().unwrap(), actual);
                session.execute(Command::Undo).unwrap();
                assert_eq!(session.document.render().unwrap(), image);
                assert!(!session.can_undo());
                session.execute(Command::Redo).unwrap();
                assert_eq!(session.document.render().unwrap(), actual);
                session.execute(Command::RevertOriginal).unwrap();
                assert_eq!(session.document.render().unwrap(), image);
            }
        }
    }
    #[test]
    fn composite_cut_flattens_once_and_invalid_strips_preserve_redo_and_objects() {
        let doc: Document =
            serde_json::from_str(include_str!("../tests/fixtures/translation.capture.json"))
                .unwrap();
        let original = serde_json::to_vec(&doc).unwrap();
        let pixels = doc.render().unwrap();
        let mut session = Session::new(doc).unwrap();
        let command: Command = serde_json::from_str(
            r#"{"command":"cut_out","axis":"horizontal","start":100,"end":180}"#,
        )
        .unwrap();
        session.execute(command).unwrap();
        assert!(session.document.annotations.is_empty());
        let actual = session.document.render().unwrap();
        assert_eq!(actual.dimensions(), (640, 320));
        for (x, y, p) in actual.enumerate_pixels() {
            assert_eq!(p, pixels.get_pixel(x, if y < 100 { y } else { y + 80 }));
        }
        session.execute(Command::Undo).unwrap();
        assert!(!session.can_undo());
        for (axis, start, end) in [
            (CutAxis::Horizontal, 5, 5),
            (CutAxis::Horizontal, 6, 5),
            (CutAxis::Horizontal, 0, 400),
            (CutAxis::Vertical, 0, 641),
            (CutAxis::Vertical, u32::MAX, 0),
            (CutAxis::Horizontal, 0, u32::MAX),
        ] {
            assert!(
                session
                    .execute(Command::CutOut { axis, start, end })
                    .is_err()
            );
            assert_eq!(serde_json::to_vec(&session.document).unwrap(), original);
            assert!(session.can_redo());
        }
    }
}
