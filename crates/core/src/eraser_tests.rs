use crate::{Annotation, Command, Document, Kind, Session};
use image::{Rgba, RgbaImage};

fn eraser(opacity: u8) -> Annotation {
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
        kind: Kind::Eraser,
        start: [10.0, 20.0],
        end: [10.0, 20.0],
        points: vec![[10.0, 20.0], [50.0, 20.0], [10.0, 20.0]],
        color: [250, 20, 180, opacity], // RGB must not tint erasure.
        width: 8,
        corner_radius: 0.0,
        fill: None,
        text: None,
        callout: None,
    }
}

#[test]
fn eraser_alpha_coverage_is_once_per_stroke_and_preserves_outside_pixels() {
    for source_alpha in [0, 128, 255] {
        let source = RgbaImage::from_pixel(64, 48, Rgba([25, 100, 210, source_alpha]));
        for opacity in [0, 128, 255] {
            let mut document = Document::from_image(source.clone()).unwrap();
            document.annotations.push(eraser(opacity));
            let output = document.render().unwrap();
            let alpha = ((u16::from(source_alpha) * (255 - u16::from(opacity)) + 127) / 255) as u8;
            assert_eq!(output.get_pixel(30, 20)[3], alpha);
            if opacity != 0 {
                assert_eq!(
                    output.get_pixel(30, 20).0,
                    if alpha == 0 {
                        [0; 4]
                    } else {
                        [25, 100, 210, alpha]
                    }
                );
            } else {
                assert_eq!(output, source);
            }
            let edge_opacity = (u16::from(opacity) * 128 + 127) / 255;
            assert_eq!(
                output.get_pixel(30, 16)[3],
                ((u16::from(source_alpha) * (255 - edge_opacity) + 127) / 255) as u8
            );
            for y in 0..48 {
                for x in 0..64 {
                    if !(6..55).contains(&x) || !(16..25).contains(&y) {
                        assert_eq!(output.get_pixel(x, y), source.get_pixel(x, y));
                    }
                }
            }
        }
    }
}

#[test]
fn eraser_stacking_moves_history_and_real_project_roundtrip() {
    let mut document = Document::canvas(64, 48, [30, 80, 200, 255]).unwrap();
    let mut cover = eraser(255);
    cover.kind = Kind::Redact;
    cover.points.clear();
    cover.start = [20.0, 10.0];
    cover.end = [40.0, 30.0];
    document.annotations.push(cover);
    let before = document.render().unwrap();
    let source = document.source_png.clone();
    let mut session = Session::new(document).unwrap();
    let mut brush = eraser(255);
    brush.id = 2;
    session
        .execute(Command::Add {
            annotation: brush.clone(),
        })
        .unwrap();
    let erased = session.document.render().unwrap();
    assert_eq!(erased.get_pixel(30, 20).0, [0; 4]);
    assert_eq!(erased.get_pixel(30, 12).0, [0, 0, 0, 255]);
    session.execute(Command::Undo).unwrap();
    assert_eq!(session.document.render().unwrap(), before);
    assert!(!session.can_undo());
    session.execute(Command::Redo).unwrap();
    let moved = brush.translated([0.0, 16.0], [64, 48]).unwrap();
    session
        .execute(Command::Update { annotation: moved })
        .unwrap();
    assert_eq!(
        session.document.render().unwrap().get_pixel(30, 20).0,
        [0, 0, 0, 255]
    );
    session.execute(Command::Undo).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("eraser.capture.json");
    let png = dir.path().join("eraser.png");
    session.save(&project).unwrap();
    let loaded = Document::load(&project).unwrap();
    assert_eq!(loaded.source_png, source);
    assert_eq!(loaded.annotations[1], brush);
    assert_eq!(loaded.render().unwrap(), erased);
    loaded.export_png(&png).unwrap();
    assert_eq!(image::open(&png).unwrap().into_rgba8(), erased);
    let mut on_top = eraser(255);
    on_top.id = 3;
    on_top.kind = Kind::Pen;
    on_top.color = [90, 180, 50, 255];
    session
        .execute(Command::Add { annotation: on_top })
        .unwrap();
    assert_eq!(
        session.document.render().unwrap().get_pixel(30, 20).0,
        [90, 180, 50, 255]
    );
}

#[test]
fn eraser_dot_clips_edges_and_invalid_paths_preserve_history() {
    let mut session = Session::new(Document::blank(8, 8).unwrap()).unwrap();
    let mut dot = eraser(255);
    dot.points = vec![[0.0, 0.0]; 2];
    dot.start = [0.0, 0.0];
    dot.end = [0.0, 0.0];
    session
        .execute(Command::Add {
            annotation: dot.clone(),
        })
        .unwrap();
    assert_eq!(session.document.render().unwrap().get_pixel(0, 0).0, [0; 4]);
    assert_eq!(
        session.document.render().unwrap().get_pixel(7, 7).0,
        [255; 4]
    );
    session.execute(Command::Undo).unwrap();
    for bad in [
        vec![],
        vec![[0.0, 0.0]],
        vec![[0.0, 0.0], [f32::NAN, 1.0]],
        vec![[0.0, 0.0]; 8193],
    ] {
        dot.points = bad;
        assert!(
            session
                .execute(Command::Add {
                    annotation: dot.clone()
                })
                .is_err()
        );
        assert!(session.document.annotations.is_empty());
        assert!(session.can_redo());
    }
}
