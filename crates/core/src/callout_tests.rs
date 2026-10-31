use crate::{
    Annotation, Command, Document, Kind, Session, callout::CalloutStyle, selection::Handle,
    text::TextStyle,
};

fn annotation() -> Annotation {
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
        kind: Kind::Callout,
        start: [20.0, 20.0],
        end: [100.0, 70.0],
        color: [20, 20, 20, 255],
        width: 2,
        text: Some(TextStyle {
            content: String::new(),
            ..TextStyle::default()
        }),
        corner_radius: 0.0,
        fill: None,
        points: Vec::new(),
        callout: Some(CalloutStyle {
            fill: [200, 40, 80, 128],
            outline: [10, 60, 120, 255],
            pointer: [50.0, 100.0],
            padding: 5.0,
        }),
    }
}
fn document() -> Document {
    let mut doc = Document::canvas(128, 128, [0; 4]).unwrap();
    doc.annotations.push(annotation());
    doc
}

#[test]
fn body_pointer_outline_and_alpha_render_without_filling_the_bounding_box() {
    let doc = document();
    let image = doc.render().unwrap();
    assert_eq!(image.get_pixel(40, 40).0, [200, 40, 80, 128]);
    assert_eq!(image.get_pixel(50, 85).0, [200, 40, 80, 128]);
    assert_eq!(image.get_pixel(20, 40).0, [10, 60, 120, 255]);
    assert_eq!(image.get_pixel(90, 90).0, [0; 4]);
    assert_eq!(image.get_pixel(0, 0).0, [0; 4]);
    assert!(doc.annotations[0].hit_test([50.0, 85.0], 0.0));
    assert!(!doc.annotations[0].hit_test([90.0, 90.0], 0.0));
}

#[test]
fn pointers_on_each_side_share_selection_and_render_geometry() {
    for (tip, middle) in [
        ([60.0, 0.0], [60.0, 10.0]),
        ([120.0, 45.0], [110.0, 45.0]),
        ([60.0, 100.0], [60.0, 85.0]),
        ([0.0, 45.0], [10.0, 45.0]),
    ] {
        let mut doc = document();
        doc.annotations[0].callout.as_mut().unwrap().pointer = tip;
        assert!(doc.annotations[0].hit_test(middle, 0.0));
        let image = doc.render().unwrap();
        assert_eq!(
            image.get_pixel(middle[0] as u32, middle[1] as u32).0,
            [200, 40, 80, 128]
        );
        assert!(
            doc.annotations[0]
                .handles()
                .contains(&(Handle::Pointer, tip))
        );
    }
}

#[test]
fn movement_clamps_body_and_pointer_together_and_resize_preserves_pointer() {
    let a = annotation();
    let moved = a.translated([100.0, 100.0], [128, 128]).unwrap();
    assert_eq!(moved.start, [48.0, 48.0]);
    assert_eq!(moved.end, [128.0, 98.0]);
    assert_eq!(moved.callout.as_ref().unwrap().pointer, [78.0, 128.0]);
    let resized = a
        .resized(Handle::TopLeft, [110.0, 80.0], [128, 128])
        .unwrap();
    assert_eq!((resized.start, resized.end), ([100.0, 70.0], [110.0, 80.0]));
    assert_eq!(resized.callout.as_ref().unwrap().pointer, [50.0, 100.0]);
    assert!(
        a.resized(Handle::TopLeft, [100.0, 70.0], [128, 128])
            .is_err()
    );
    let pointer = a
        .resized(Handle::Pointer, [-25.0, 150.0], [128, 128])
        .unwrap();
    assert_eq!(pointer.callout.as_ref().unwrap().pointer, [0.0, 128.0]);
    assert_eq!((pointer.start, pointer.end), (a.start, a.end));
}

#[test]
fn callout_text_style_pointer_and_one_step_history_survive_save_reopen_export() {
    let mut session = Session::new(document()).unwrap();
    let original = session.document.render().unwrap();
    let mut edited = session.document.annotations[0]
        .translated([5.0, 8.0], [128, 128])
        .unwrap();
    edited.text.as_mut().unwrap().content = "Editable\ncallout".into();
    edited.text.as_mut().unwrap().size_points = 10.0;
    edited.callout.as_mut().unwrap().fill = [255, 240, 180, 255];
    session
        .execute(Command::Update {
            annotation: edited.clone(),
        })
        .unwrap();
    let expected = session.document.render().unwrap();
    assert_ne!(expected, original);
    session.execute(Command::Undo).unwrap();
    assert!(!session.is_modified());
    assert_eq!(session.document.render().unwrap(), original);
    session.execute(Command::Redo).unwrap();
    assert_eq!(session.document.annotations, vec![edited]);
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("callout.capture.json");
    session.save(&project).unwrap();
    assert!(!session.is_modified());
    let reopened = Document::load(&project).unwrap();
    assert_eq!(reopened.annotations, session.document.annotations);
    assert_eq!(reopened.render().unwrap(), expected);
    let png = dir.path().join("callout.png");
    reopened.export_png(&png).unwrap();
    assert_eq!(image::open(png).unwrap().into_rgba8(), expected);
}

#[test]
fn invalid_styles_and_missing_payloads_do_not_mutate_history() {
    let mut variants = Vec::new();
    for pointer in [[f32::NAN, 50.0], [-1.0, 50.0], [130.0, 50.0]] {
        let mut a = annotation();
        a.callout.as_mut().unwrap().pointer = pointer;
        variants.push(a);
    }
    for padding in [f32::INFINITY, -1.0, 129.0] {
        let mut a = annotation();
        a.callout.as_mut().unwrap().padding = padding;
        variants.push(a);
    }
    let mut missing_style = annotation();
    missing_style.callout = None;
    variants.push(missing_style);
    let mut missing_text = annotation();
    missing_text.text = None;
    variants.push(missing_text);
    let mut wrong_kind = annotation();
    wrong_kind.kind = Kind::Text;
    variants.push(wrong_kind);
    let mut session = Session::new(document()).unwrap();
    for a in variants {
        assert!(session.execute(Command::Update { annotation: a }).is_err());
        assert!(!session.can_undo());
        assert!(!session.is_modified());
        assert_eq!(session.document.annotations, vec![annotation()]);
    }
}

#[test]
fn tiny_fractional_bodies_and_coincident_pointer_render_without_panics() {
    for delta in [0.001, 0.01, 0.1, 0.5, 1.0, 2.0] {
        for pointer in [[10.0, 10.0], [0.0, 0.0], [128.0, 128.0]] {
            let mut doc = document();
            doc.annotations[0].start = [10.0, 10.0];
            doc.annotations[0].end = [10.0 + delta, 10.0 + delta];
            doc.annotations[0].callout.as_mut().unwrap().pointer = pointer;
            doc.render().unwrap();
        }
    }
}
