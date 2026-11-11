use super::*;
use crate::{Command, Session, steps};
use image::Rgba;

fn fixture() -> Document {
    let mut d = Document::from_image(RgbaImage::from_fn(120, 80, |x, y| {
        Rgba([x as u8, y as u8, 70, if x % 3 == 0 { 0 } else { 180 }])
    }))
    .unwrap();
    d.annotations.push(
        steps::marker(
            1,
            [60.0, 40.0],
            40,
            [220, 20, 70, 255],
            (8, false),
            [120, 80],
        )
        .unwrap(),
    );
    d.annotations.push(
        steps::marker(
            2,
            [95.0, 50.0],
            24,
            [20, 90, 220, 255],
            (9, false),
            [120, 80],
        )
        .unwrap(),
    );
    d
}

#[test]
fn every_edge_pads_exact_transparent_pixels_preserves_geometry_and_undo() {
    for delta in [
        [-70.0, 0.0],
        [0.0, -50.0],
        [90.0, 0.0],
        [0.0, 70.0],
        [-70.0, -50.0],
        [90.0, 70.0],
    ] {
        let original = fixture();
        let original_bytes = serde_json::to_vec(&original).unwrap();
        let base = original.base_image().unwrap();
        let moved = translated(&original.annotations[..1], delta).unwrap();
        let plan = original.plan_expanding_move(&moved, false).unwrap();
        let preview = plan.display_list(&original).unwrap();
        let mut s = Session::new(original.clone()).unwrap();
        s.execute(Command::MoveAndExpand {
            annotations: moved,
            duplicate: false,
        })
        .unwrap();
        let padded = s.document.base_image().unwrap();
        assert_eq!(padded, preview.base);
        for (x, y, pixel) in padded.enumerate_pixels() {
            let expected = if x as i32 >= plan.offset[0]
                && y as i32 >= plan.offset[1]
                && (x as i32) < plan.offset[0] + 120
                && (y as i32) < plan.offset[1] + 80
            {
                *base.get_pixel(
                    (x as i32 - plan.offset[0]) as u32,
                    (y as i32 - plan.offset[1]) as u32,
                )
            } else {
                Rgba([0, 0, 0, 0])
            };
            assert_eq!(*pixel, expected);
        }
        assert_eq!(s.document.source_png, original.source_png);
        assert_eq!(
            s.document.annotations[1],
            translated(&original.annotations[1..], plan.offset.map(|n| n as f32)).unwrap()[0]
        );
        assert_eq!(s.document.annotations[0].text, original.annotations[0].text);
        let changed = serde_json::to_vec(&s.document).unwrap();
        s.execute(Command::Undo).unwrap();
        assert_eq!(serde_json::to_vec(&s.document).unwrap(), original_bytes);
        assert!(!s.can_undo());
        s.execute(Command::Redo).unwrap();
        assert_eq!(serde_json::to_vec(&s.document).unwrap(), changed);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("expanded.json");
        s.save(&path).unwrap();
        let reopened = Document::load(&path).unwrap();
        assert_eq!(reopened.base_image().unwrap(), padded);
        assert_eq!(reopened.render().unwrap(), s.document.render().unwrap());
        assert_eq!(reopened.annotations, s.document.annotations);
    }
}

#[test]
fn group_moves_and_duplicates_preserve_spacing_and_ids_in_one_transaction() {
    let original = fixture();
    let mut s = Session::new(original.clone()).unwrap();
    let copies = original.duplicate_selection(&[1, 2]).unwrap();
    let moved = translated(&copies, [-90.0, 70.0]).unwrap();
    let plan = original.plan_expanding_move(&moved, true).unwrap();
    s.execute(Command::MoveAndExpand {
        annotations: moved,
        duplicate: true,
    })
    .unwrap();
    assert_eq!(
        s.document
            .annotations
            .iter()
            .map(|a| a.id)
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4]
    );
    let [a, b, c, d] = s.document.annotations.as_slice() else {
        panic!()
    };
    for axis in 0..2 {
        assert_eq!(
            a.start[axis],
            original.annotations[0].start[axis] + plan.offset[axis] as f32
        );
        assert_eq!(d.start[axis] - c.start[axis], b.start[axis] - a.start[axis]);
    }
    s.execute(Command::Undo).unwrap();
    assert_eq!(s.document.annotations, original.annotations);
    assert_eq!(s.document.width, 120);
    assert_eq!(s.document.height, 80);
    let moved = translated(&original.annotations, [90.0, 60.0]).unwrap();
    s.execute(Command::MoveAndExpand {
        annotations: moved,
        duplicate: false,
    })
    .unwrap();
    assert_eq!(s.document.annotations.len(), 2);
    assert_eq!(
        s.document.annotations[1].start[0] - s.document.annotations[0].start[0],
        original.annotations[1].start[0] - original.annotations[0].start[0]
    );
}

#[test]
fn invalid_moves_are_atomic_preserve_redo_and_reject_limits_before_raster_allocation() {
    let original = fixture();
    let mut s = Session::new(original.clone()).unwrap();
    s.execute(Command::Remove { id: 2 }).unwrap();
    s.execute(Command::Undo).unwrap();
    assert!(s.can_redo());
    for (mut moved, duplicate) in [
        (original.annotations[..1].to_vec(), true),
        (
            vec![
                original.annotations[0].clone(),
                original.annotations[0].clone(),
            ],
            false,
        ),
    ] {
        moved[0].start[0] = -30.0;
        assert!(
            s.execute(Command::MoveAndExpand {
                annotations: moved,
                duplicate
            })
            .is_err()
        );
    }
    for value in [f32::NAN, f32::INFINITY, -f32::MAX, 1_000_000.0] {
        let mut moved = original.annotations[..1].to_vec();
        moved[0].start[0] = value;
        assert!(
            s.execute(Command::MoveAndExpand {
                annotations: moved,
                duplicate: false
            })
            .is_err()
        );
    }
    assert_eq!(
        serde_json::to_vec(&s.document).unwrap(),
        serde_json::to_vec(&original).unwrap()
    );
    assert!(s.can_redo());
    assert!(!s.can_undo());
    s.execute(Command::MoveAndExpand {
        annotations: original.annotations.clone(),
        duplicate: false,
    })
    .unwrap();
    assert!(!s.can_undo());
    assert!(s.can_redo());
}

#[test]
fn callout_tail_and_path_coordinates_translate_with_the_same_offset() {
    let original: Document =
        serde_json::from_str(include_str!("../../tests/fixtures/properties.capture.json")).unwrap();
    let moved = translated(&original.annotations, [-300.0, -180.0]).unwrap();
    let plan = original.plan_expanding_move(&moved, false).unwrap();
    let changed = plan.apply(&original).unwrap();
    for (before, after) in original.annotations.iter().zip(&changed.annotations) {
        let expected = translated(
            std::slice::from_ref(before),
            [plan.offset[0] as f32 - 300.0, plan.offset[1] as f32 - 180.0],
        )
        .unwrap();
        assert_eq!(after, &expected[0]);
    }
    let preview = plan.display_list(&original).unwrap();
    assert_eq!(preview.base, changed.base_image().unwrap());
    assert_eq!(
        preview
            .render_viewport(crate::display::Viewport {
                size: [640, 400],
                origin: [0.0, 0.0],
                scale: 1.0
            })
            .unwrap(),
        changed
            .display_list()
            .unwrap()
            .render_viewport(crate::display::Viewport {
                size: [640, 400],
                origin: [0.0, 0.0],
                scale: 1.0
            })
            .unwrap()
    );
}

#[test]
fn automatic_margins_follow_each_edge_inward_and_reopen_with_exact_original_pixels() {
    for delta in [
        [-70.0, 0.0],
        [0.0, -50.0],
        [90.0, 0.0],
        [0.0, 70.0],
        [-70.0, -50.0],
        [90.0, 70.0],
    ] {
        let original = fixture();
        let pixels = original.base_image().unwrap();
        let mut s = Session::new(original.clone()).unwrap();
        s.execute(Command::MoveAndExpand {
            annotations: translated(&original.annotations[..1], delta).unwrap(),
            duplicate: false,
        })
        .unwrap();
        let expanded = serde_json::to_vec(&s.document).unwrap();
        let large = [s.document.width, s.document.height];
        let artifact = (delta == [-70.0, -50.0])
            .then(|| std::env::var("CLIPDESIGNER_HEADLESS_MARGIN_DIR").ok())
            .flatten()
            .map(std::path::PathBuf::from);
        if let Some(root) = &artifact {
            std::fs::create_dir_all(root).unwrap();
            original.export_png(&root.join("original.png")).unwrap();
            s.document.export_png(&root.join("expanded.png")).unwrap();
            s.document
                .save(&root.join("expanded.capture.json"))
                .unwrap();
        }

        assert!(s.document.auto_canvas_base.is_some());
        assert_eq!(s.document.version, crate::MAX_PROJECT_VERSION);
        let base = s.document.auto_canvas_base.unwrap();
        let half = translated(
            &original.annotations[..1],
            [
                base[0] as f32 + delta[0] * 0.8,
                base[1] as f32 + delta[1] * 0.8,
            ],
        )
        .unwrap();
        let plan = s.document.plan_expanding_move(&half, false).unwrap();
        let preview = plan.display_list(&s.document).unwrap().base;
        assert!(plan.dimensions[0] < large[0] || plan.dimensions[1] < large[1]);
        s.execute(Command::MoveAndExpand {
            annotations: half,
            duplicate: false,
        })
        .unwrap();
        assert_eq!(s.document.base_image().unwrap(), preview);
        if let Some(root) = &artifact {
            s.document.export_png(&root.join("partial.png")).unwrap();
        }

        s.execute(Command::Undo).unwrap();
        assert_eq!(serde_json::to_vec(&s.document).unwrap(), expanded);
        s.execute(Command::Redo).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("automatic.capture.json");
        s.save(&path).unwrap();
        let mut s = Session::new(Document::load(&path).unwrap()).unwrap();
        let before = serde_json::to_vec(&s.document).unwrap();
        let base = s.document.auto_canvas_base.unwrap();
        let home =
            translated(&original.annotations[..1], [base[0] as f32, base[1] as f32]).unwrap();
        s.execute(Command::MoveAndExpand {
            annotations: home,
            duplicate: false,
        })
        .unwrap();
        assert_eq!([s.document.width, s.document.height], [120, 80]);
        assert!(s.document.auto_canvas_base.is_none());
        assert_eq!(s.document.annotations, original.annotations);
        assert_eq!(s.document.base_image().unwrap(), pixels);
        assert_eq!(s.document.source_png, original.source_png);
        assert_eq!(s.document.render().unwrap(), original.render().unwrap());
        if let Some(root) = &artifact {
            s.document.export_png(&root.join("restored.png")).unwrap();
            s.document
                .save(&root.join("restored.capture.json"))
                .unwrap();
        }

        s.execute(Command::Undo).unwrap();
        assert_eq!(serde_json::to_vec(&s.document).unwrap(), before);
        assert!(!s.can_undo());
        s.execute(Command::Redo).unwrap();
        assert_eq!(s.document.base_image().unwrap(), pixels);
    }
}

#[test]
fn outermost_remaining_object_controls_margin_and_delete_shrinks_in_one_undo() {
    let original = fixture();
    let mut s = Session::new(original.clone()).unwrap();
    let copies = original.duplicate_selection(&[1, 2]).unwrap();
    s.execute(Command::MoveAndExpand {
        annotations: translated(&copies, [-100.0, -70.0]).unwrap(),
        duplicate: true,
    })
    .unwrap();
    let expanded = serde_json::to_vec(&s.document).unwrap();
    let width = s.document.width;
    let height = s.document.height;
    let base = s.document.auto_canvas_base.unwrap();
    // Move only the furthest duplicate back; the second still requires a margin.
    let mut home = original.annotations[0].clone();
    home.id = 3;
    let home = translated(&[home], [base[0] as f32, base[1] as f32]).unwrap();
    s.execute(Command::MoveAndExpand {
        annotations: home,
        duplicate: false,
    })
    .unwrap();
    assert!(s.document.width < width && s.document.height < height);
    assert!(s.document.auto_canvas_base.is_some());
    let partial = serde_json::to_vec(&s.document).unwrap();
    s.execute(Command::Remove { id: 4 }).unwrap();
    assert_eq!([s.document.width, s.document.height], [120, 80]);
    assert!(s.document.auto_canvas_base.is_none());
    assert_eq!(
        &s.document.annotations[..2],
        original.annotations.as_slice()
    );
    assert_eq!(
        s.document.base_image().unwrap(),
        original.base_image().unwrap()
    );
    s.execute(Command::Undo).unwrap();
    assert_eq!(serde_json::to_vec(&s.document).unwrap(), partial);
    s.execute(Command::Undo).unwrap();
    assert_eq!(serde_json::to_vec(&s.document).unwrap(), expanded);
}

#[test]
fn explicit_canvas_and_raster_changes_retire_auto_margins() {
    let original = fixture();
    let mut expanded = Session::new(original.clone()).unwrap();
    expanded
        .execute(Command::MoveAndExpand {
            annotations: translated(&original.annotations[..1], [80.0, 50.0]).unwrap(),
            duplicate: false,
        })
        .unwrap();
    let expanded = expanded.document;
    for action in [
        Command::ResizeCanvas {
            width: 240,
            height: 180,
            placement: crate::Placement::TopLeft,
        },
        Command::ResizeImage {
            width: 240,
            height: 180,
        },
        Command::FlattenAll,
        Command::TransformImage {
            transform: crate::ImageTransform::Clockwise,
        },
    ] {
        let mut s = Session::new(expanded.clone()).unwrap();
        s.execute(action).unwrap();
        assert!(s.document.auto_canvas_base.is_none());
        let size = [s.document.width, s.document.height];
        if !s.document.annotations.is_empty() {
            let ids = s.document.annotations.iter().map(|a| a.id).collect();
            s.execute(Command::RemoveMany { ids }).unwrap();
            assert_eq!([s.document.width, s.document.height], size);
            s.execute(Command::Undo).unwrap();
        }
        s.execute(Command::Undo).unwrap();
        assert_eq!(s.document.auto_canvas_base, expanded.auto_canvas_base);
    }
}
