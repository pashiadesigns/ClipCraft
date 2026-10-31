use crate::{Annotation, Command, Document, Kind, Session};

#[test]
fn fractional_reverse_redactions_cover_every_touched_pixel_in_flattened_png() {
    let mut source = image::RgbaImage::new(9, 7);
    for (x, y, pixel) in source.enumerate_pixels_mut() {
        *pixel = image::Rgba([x as u8 * 20, y as u8 * 30, 91, ((x + y) % 4 * 85) as u8]);
    }
    for (start, end) in [
        ([1.2, 1.8], [5.1, 4.9]),
        ([5.1, 4.9], [1.2, 1.8]),
        ([0.0, 0.0], [9.0, 7.0]),
    ] {
        let mut session = Session::new(Document::from_image(source.clone()).unwrap()).unwrap();
        session
            .execute(Command::Add {
                annotation: Annotation {
                    image: None,
                    step_outline: None,
                    line_style: None,
                    shadow: None,
                    analysis: None,
                    highlighter_rect: false,
                    group: None,
                    rotation_degrees: 0.0,
                    id: 1,
                    kind: Kind::Redact,
                    start,
                    end,
                    // Redaction must ignore decorative color, alpha and stroke width.
                    color: [255, 90, 60, 0],
                    width: 32,
                    text: None,
                    corner_radius: 0.0,
                    fill: None,
                    points: Vec::new(),
                    callout: None,
                },
            })
            .unwrap();
        let flattened = session.document.render().unwrap();
        let left = start[0].min(end[0]).floor() as u32;
        let top = start[1].min(end[1]).floor() as u32;
        let right = start[0].max(end[0]).ceil() as u32;
        let bottom = start[1].max(end[1]).ceil() as u32;
        for (x, y, pixel) in flattened.enumerate_pixels() {
            let expected = if (left..right).contains(&x) && (top..bottom).contains(&y) {
                [0, 0, 0, 255]
            } else {
                source.get_pixel(x, y).0
            };
            assert_eq!(pixel.0, expected, "incorrect coverage at {x},{y}");
        }
        assert_eq!(session.document.base_image().unwrap(), source);
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("redaction.capture.json");
        let png = dir.path().join("redaction.png");
        session.save(&project).unwrap();
        let reopened = Document::load(&project).unwrap();
        reopened.export_png(&png).unwrap();
        assert_eq!(image::open(png).unwrap().into_rgba8(), flattened);
        session.execute(Command::Undo).unwrap();
        assert_eq!(session.document.render().unwrap(), source);
        session.execute(Command::Redo).unwrap();
        assert_eq!(session.document.render().unwrap(), flattened);
    }
}
