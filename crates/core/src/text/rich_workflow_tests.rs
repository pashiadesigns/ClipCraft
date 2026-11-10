use super::*;
use crate::{Command, Document, Kind, Session};
fn fixture() -> Document {
    let mut d = Document::canvas(900, 560, [255; 4]).unwrap();
    let mut a = crate::steps::marker(
        1,
        [100., 100.],
        50,
        [30, 40, 50, 255],
        (1, false),
        [900, 560],
    )
    .unwrap();
    a.kind = Kind::Text;
    a.start = [36., 26.];
    a.end = [864., 532.];
    let mut s=TextStyle{content:"A clearer way to share\nMixed fonts, color and emphasis in one text box.\nReview the guide\nCheck your work\nShare with your team\nContact alex@example.com or +1 (312) 555-0182\nRead the online guide".into(),size_points:20.,color:Some([40,50,60,255]),horizontal:HorizontalAlignment::Left,vertical:VerticalAlignment::Top,fit:TextFit::Shrink,..Default::default()};
    rich::format_range(&mut s, 0..22, |f| {
        f.size = 34.;
        f.bold = true;
        f.color = Some([15, 145, 185, 255]);
    });
    rich::format_range(&mut s, 23..34, |f| {
        f.font = TextFont::HackRegular;
        f.color = Some([165, 35, 95, 255]);
        f.underline = true;
    });
    rich::format_range(&mut s, 36..41, |f| {
        f.typography.highlight = Some([255, 230, 70, 200]);
    });
    let starts = rich::paragraph_starts(&s.content);
    let n = s.content.chars().count();
    rich::paragraph_range(&mut s, 0..n, |p| {
        p.line_spacing = 1.15;
        p.after = 10.;
    });
    rich::paragraph_range(&mut s, starts[2]..starts[5], |p| {
        p.list = rich::List::Numbered;
        p.indent = 12.;
    });
    rich::paragraph_range(&mut s, starts[3]..starts[4], |p| p.level = 1);
    rich::format_range(&mut s, starts[6]..starts[6] + 21, |f| {
        f.link = Some("https://example.com/guide".into());
        f.underline = true;
        f.color = Some([15, 145, 185, 255]);
    });
    a.text = Some(s);
    d.annotations.push(a);
    d.validate().unwrap();
    d
}
#[test]
fn rich_text_render_save_resize_and_undo_share_geometry() {
    let d = fixture();
    let before = d.render().unwrap();
    let mut s = Session::new(d.clone()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mixed.iclip");
    s.save(&path).unwrap();
    assert_eq!(Document::load(&path).unwrap().render().unwrap(), before);
    s.execute(Command::ResizeImage {
        width: 1080,
        height: 672,
    })
    .unwrap();
    assert!(
        (rich::format_at(s.document.annotations[0].text.as_ref().unwrap(), 0).size - 40.8).abs()
            < 0.01
    );
    s.execute(Command::Undo).unwrap();
    assert_eq!(s.document.render().unwrap(), before);
    if let Ok(root) = std::env::var("CLIPDESIGNER_RICH_FIXTURE_DIR") {
        let path = std::path::Path::new(&root);
        std::fs::create_dir_all(path).unwrap();
        d.save(&path.join("rich-text.capture.json")).unwrap();
        before.save(path.join("rich-text.png")).unwrap();
    }
}
#[test]
fn malformed_interchange_is_bounded_and_never_panics() {
    let base = TextStyle::default();
    let fragments = [
        "é",
        "\\'é",
        "\\bin1 é",
        "{",
        "}",
        "\\u-10240?",
        "<İ a='z'>",
        "<p style='color:€abc'>hello</p>",
        "<script>x</script>",
        "&invalid;",
    ];
    for (i, a) in fragments.iter().enumerate() {
        for b in &fragments {
            let value = format!("{{\\rtf1 {a} {b} }}");
            let _ = clipboard::from_rtf(&value, &base);
            let _ = clipboard::from_html(&format!("<p>{a}{b}</p>"), &base);
            assert!(clipboard::from_rtf(&"{".repeat(70), &base).is_err(), "{i}");
        }
    }
    assert!(clipboard::from_html(&format!("<p>{}</p>", "x".repeat(5000)), &base).is_err());
}
#[test]
fn privacy_suggestions_apply_atomically_and_keep_originals_only_in_project() {
    let mut d = fixture();
    let s = d.annotations[0].text.as_mut().unwrap();
    s.content = "alex@example.com".into();
    s.rich = None;
    let words = vec![crate::privacy::Word {
        text: s.content.clone(),
        line: 0,
        bounds: [40., 30., 200., 35.],
    }];
    let findings = crate::privacy::detect(&words, &Default::default(), [900, 560]).unwrap();
    assert_eq!(findings.len(), 1);
    let original = serde_json::to_vec(&d).unwrap();
    let mut session = Session::new(d.clone()).unwrap();
    session
        .execute(crate::privacy::cover(&d, findings, crate::Kind::Redact, 1))
        .unwrap();
    assert_eq!(
        session.document.annotations.last().unwrap().kind,
        Kind::Redact
    );
    assert_eq!(
        session.document.render().unwrap().get_pixel(50, 40).0,
        [0, 0, 0, 255]
    );
    session.execute(Command::Undo).unwrap();
    assert_eq!(serde_json::to_vec(&session.document).unwrap(), original);
    assert!(!session.can_undo());
}
