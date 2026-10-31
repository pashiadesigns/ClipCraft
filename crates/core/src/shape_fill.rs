//! Shared optional shape fills. Reuses the existing callout mask/compositing
//! approach and the attributed ellipse outline rather than a second shape engine.
use crate::{Annotation, Kind};
use image::{GrayImage, Luma, RgbaImage};
use imageproc::{drawing::draw_polygon_mut, point::Point};

pub(crate) fn render(image: &mut RgbaImage, annotation: &Annotation) {
    let Some(color) = annotation.fill.filter(|color| color[3] != 0) else {
        return;
    };
    let (min, max) = annotation.body_bounds();
    if max[0] <= min[0] || max[1] <= min[1] {
        return;
    }
    let polygon = match annotation.kind {
        Kind::Rectangle => {
            annotation.rounded_polygon(vec![min, [max[0], min[1]], max, [min[0], max[1]]])
        }
        Kind::Ellipse => crate::ellipse::outline(min, max),
        Kind::Polygon => annotation.points.clone(),
        _ => return,
    };
    let left = min[0].floor().max(0.0) as u32;
    let top = min[1].floor().max(0.0) as u32;
    let right = max[0].ceil().min(image.width() as f32) as u32;
    let bottom = max[1].ceil().min(image.height() as f32) as u32;
    if right <= left || bottom <= top {
        return;
    }
    let mut points: Vec<_> = polygon
        .iter()
        .map(|p| {
            Point::new(
                (p[0] - left as f32).round() as i32,
                (p[1] - top as f32).round() as i32,
            )
        })
        .collect();
    points.dedup();
    if points.first() == points.last() {
        points.pop();
    }
    if points.len() < 3 {
        return;
    }
    // Allocate only the clipped body, not a second full-image RGBA surface.
    let mut mask = GrayImage::new(right - left, bottom - top);
    draw_polygon_mut(&mut mask, &points, Luma([255]));
    for (x, y, pixel) in mask.enumerate_pixels() {
        if pixel[0] != 0 {
            crate::text::composite(image.get_pixel_mut(left + x, top + y), color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Document, Session, selection::Handle};

    fn shape(kind: Kind) -> Annotation {
        Annotation {
            image: None,
            step_outline: None,
            line_style: None,
            shadow: None,
            analysis: None,
            highlighter_rect: false,
            group: None,
            rotation_degrees: 0.0,
            id: 1,
            kind,
            start: [20.0, 20.0],
            end: [120.0, 80.0],
            color: [20, 50, 80, 255],
            width: 3,
            corner_radius: 0.0,
            fill: Some([40, 100, 200, 128]),
            points: vec![],
            text: None,
            callout: None,
        }
    }

    #[test]
    fn fill_alpha_bounds_rounding_and_reverse_draw_have_shared_output() {
        for (kind, radius) in [
            (Kind::Rectangle, 0.0),
            (Kind::Rectangle, 20.0),
            (Kind::Ellipse, 0.0),
        ] {
            let mut document = Document::blank(160, 120).unwrap();
            let mut annotation = shape(kind);
            annotation.corner_radius = radius;
            document.annotations.push(annotation.clone());
            let pixels = document.render().unwrap();
            assert_eq!(pixels.get_pixel(70, 50).0, [147, 177, 227, 255]);
            if radius > 0.0 || kind == Kind::Ellipse {
                assert_eq!(pixels.get_pixel(20, 20).0, [255; 4]);
            }
            assert_eq!(pixels.get_pixel(0, 0).0, [255; 4]);
            std::mem::swap(&mut annotation.start, &mut annotation.end);
            document.annotations[0] = annotation.clone();
            assert_eq!(document.render().unwrap(), pixels);
            document.annotations[0].fill = Some([255, 0, 0, 0]);
            let transparent = document.render().unwrap();
            document.annotations[0].fill = None;
            assert_eq!(document.render().unwrap(), transparent);
            let mut transparent_document = Document::canvas(160, 120, [0; 4]).unwrap();
            transparent_document.annotations.push(annotation);
            assert_eq!(
                transparent_document.render().unwrap().get_pixel(70, 50).0,
                [40, 100, 200, 128]
            );
        }
    }

    #[test]
    fn filled_shapes_move_resize_undo_save_and_reject_invalid_kinds() {
        for kind in [Kind::Rectangle, Kind::Ellipse] {
            let mut document = Document::blank(160, 120).unwrap();
            let mut annotation = shape(kind);
            annotation.fill = None;
            document.annotations.push(annotation.clone());
            let json = serde_json::to_string(&document).unwrap();
            assert!(!json.contains("\"fill\""));
            assert_eq!(
                serde_json::from_str::<Document>(&json).unwrap().annotations[0].fill,
                None
            );
            let mut session = Session::new(document).unwrap();
            annotation.fill = Some([40, 100, 200, 128]);
            session
                .execute(Command::Update {
                    annotation: annotation.clone(),
                })
                .unwrap();
            let filled = session.document.render().unwrap();
            session.execute(Command::Undo).unwrap();
            assert_eq!(session.document.annotations[0].fill, None);
            assert!(!session.can_undo());
            session.execute(Command::Redo).unwrap();
            assert_eq!(session.document.render().unwrap(), filled);
            let moved = annotation.translated([10.0, 12.0], [160, 120]).unwrap();
            let resized = moved
                .resized(Handle::BottomRight, [145.0, 105.0], [160, 120])
                .unwrap();
            assert_eq!(resized.fill, annotation.fill);
            session
                .execute(Command::Update {
                    annotation: resized,
                })
                .unwrap();
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("filled.capture.json");
            session.save(&path).unwrap();
            let loaded = Document::load(&path).unwrap();
            assert_eq!(loaded.render().unwrap(), session.document.render().unwrap());
            let mut invalid = annotation.clone();
            invalid.kind = Kind::Arrow;
            let before = serde_json::to_vec(&session.document).unwrap();
            assert!(
                session
                    .execute(Command::Update {
                        annotation: invalid
                    })
                    .is_err()
            );
            assert_eq!(serde_json::to_vec(&session.document).unwrap(), before);
        }
    }
}
