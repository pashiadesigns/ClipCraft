//! Original path-based icons. Palette slots are independent of object opacity.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use vello_cpu::kurbo::{BezPath, Ellipse, Rect, Shape};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VectorIcon {
    pub colors: Vec<[u8; 4]>,
    pub paths: Vec<Path>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Path {
    pub data: String,
    pub color: usize,
    /// None means fill; a positive width means a round stroke.
    pub stroke: Option<f64>,
}
impl VectorIcon {
    pub fn validate(&self) -> Result<()> {
        if self.colors.is_empty()
            || self.colors.len() > 5
            || self.paths.is_empty()
            || self.paths.len() > 128
        {
            return Err(Error::Invalid(
                "Vector icons require 1–5 colors and 1–128 paths.".into(),
            ));
        }
        for path in &self.paths {
            if path.data.len() > 32768
                || path.color >= self.colors.len()
                || path
                    .stroke
                    .is_some_and(|w| !w.is_finite() || !(0.01..=128.).contains(&w))
            {
                return Err(Error::Invalid("Invalid icon path or color.".into()));
            }
            let curve = BezPath::from_svg(&path.data)
                .map_err(|_| Error::Invalid("Invalid vector icon path.".into()))?;
            let b = curve.bounding_box();
            if curve.elements().is_empty()
                || ![b.x0, b.y0, b.x1, b.y1]
                    .iter()
                    .all(|n| n.is_finite() && (0.0..=128.0).contains(n))
            {
                return Err(Error::Invalid(
                    "Icon geometry must be within its 128 pixel view box.".into(),
                ));
            }
        }
        Ok(())
    }
    pub fn builtin(id: &str) -> Self {
        if let Some(icon) = super::instruction::icon(id) {
            return icon;
        }
        let green = [24, 139, 78, 255];
        let red = [218, 49, 62, 255];
        let blue = [24, 145, 193, 255];
        let ink = [37, 39, 41, 255];
        let white = [255; 4];
        let circle = Ellipse::from_rect(Rect::new(8., 8., 120., 120.))
            .to_path(0.01)
            .to_svg();
        let dot = Ellipse::from_rect(Rect::new(58., 88., 70., 100.))
            .to_path(0.01)
            .to_svg();
        let mut result = Self {
            colors: vec![],
            paths: vec![],
        };
        let mut add = |data: String, color: [u8; 4], stroke: Option<f64>| {
            let slot = if let Some(i) = result.colors.iter().position(|c| *c == color) {
                i
            } else {
                result.colors.push(color);
                result.colors.len() - 1
            };
            result.paths.push(Path {
                data,
                color: slot,
                stroke,
            });
        };
        match id {
            "clock" => {
                add(circle.clone(), blue, Some(7.));
                add("M 64 30 L 64 65 L 85 78".into(), ink, Some(7.));
            }
            "lock" => {
                add(
                    "M 38 57 L 38 37 C 38 8 90 8 90 37 L 90 57".into(),
                    ink,
                    Some(8.),
                );
                add("M 23 52 L 105 52 L 105 114 L 23 114 Z".into(), blue, None);
                add("M 64 75 L 64 94".into(), white, Some(9.));
            }
            "document" => {
                add(
                    "M 28 10 L 80 10 L 104 34 L 104 116 L 28 116 Z".into(),
                    white,
                    None,
                );
                add(
                    "M 28 10 L 80 10 L 104 34 L 104 116 L 28 116 Z M 80 10 L 80 34 L 104 34".into(),
                    blue,
                    Some(5.),
                );
                add(
                    "M 44 56 L 88 56 M 44 74 L 88 74 M 44 92 L 76 92".into(),
                    ink,
                    Some(5.),
                );
            }
            "download" | "upload" => {
                add("M 20 86 L 20 110 L 108 110 L 108 86".into(), ink, Some(7.));
                add(
                    if id == "download" {
                        "M 64 16 L 64 84 M 40 61 L 64 86 L 88 61"
                    } else {
                        "M 64 87 L 64 19 M 40 42 L 64 17 L 88 42"
                    }
                    .into(),
                    blue,
                    Some(9.),
                );
            }
            "lightbulb" => {
                add(
                    "M 44 88 C 44 72 25 65 25 43 C 25 0 103 0 103 43 C 103 65 84 72 84 88 Z".into(),
                    [245, 190, 48, 255],
                    None,
                );
                add(
                    "M 45 94 L 83 94 M 48 106 L 80 106 M 56 116 L 72 116".into(),
                    ink,
                    Some(6.),
                );
                add(
                    "M 53 68 L 64 80 L 75 68 M 64 80 L 64 89".into(),
                    white,
                    Some(4.),
                );
            }
            "keyboard" => {
                add("M 10 30 L 118 30 L 118 99 L 10 99 Z".into(), blue, Some(6.));
                for y in [46., 62.] {
                    for x in [25., 44., 63., 82., 101.] {
                        add(
                            Rect::new(x - 4., y - 3., x + 4., y + 3.)
                                .to_path(0.01)
                                .to_svg(),
                            ink,
                            None,
                        );
                    }
                }
                add("M 36 83 L 92 83".into(), ink, Some(6.));
            }
            "target" => {
                add(circle.clone(), red, Some(7.));
                add(
                    Ellipse::from_rect(Rect::new(30., 30., 98., 98.))
                        .to_path(0.01)
                        .to_svg(),
                    red,
                    Some(7.),
                );
                add(
                    Ellipse::from_rect(Rect::new(52., 52., 76., 76.))
                        .to_path(0.01)
                        .to_svg(),
                    red,
                    None,
                );
            }
            _ => {}
        }
        if matches!(
            id,
            "clock"
                | "lock"
                | "document"
                | "download"
                | "upload"
                | "lightbulb"
                | "keyboard"
                | "target"
        ) {
            return result;
        }
        if id == "five-color" {
            for (i, color) in [blue, green, [245, 190, 48, 255], red, white]
                .into_iter()
                .enumerate()
            {
                let x = 12. + i as f64 * 21.;
                add(
                    Ellipse::from_rect(Rect::new(x, 44., x + 20., 84.))
                        .to_path(0.01)
                        .to_svg(),
                    color,
                    None,
                );
            }
            return result;
        }
        let circled = id.ends_with("circle") || matches!(id, "question" | "information");
        if circled {
            add(
                circle,
                if id.starts_with("check") {
                    green
                } else if id.starts_with("cross") || id.starts_with("exclamation") {
                    red
                } else {
                    blue
                },
                None,
            );
        }
        let fg = if circled { white } else { ink };
        if id.starts_with("check") {
            add(
                "M 25 65 L 50 89 L 102 38".into(),
                if circled { white } else { green },
                Some(12.),
            );
        } else if id.starts_with("cross") {
            add(
                "M 34 34 L 94 94 M 94 34 L 34 94".into(),
                if circled { white } else { red },
                Some(12.),
            );
        } else if id == "caution-yellow" {
            add(
                "M 64 10 L 121 112 L 7 112 Z".into(),
                [255, 202, 50, 255],
                None,
            );
            add("M 64 10 L 121 112 L 7 112 Z".into(), ink, Some(2.));
            add("M 64 48 L 64 78".into(), ink, Some(9.));
            add(dot, ink, None);
        } else if id == "question" {
            add(
                "M 43 44 C 43 19 86 19 86 46 C 86 64 64 64 64 78".into(),
                fg,
                Some(10.),
            );
            add(dot, fg, None);
        } else if id == "information" {
            add(
                Ellipse::from_rect(Rect::new(58., 28., 70., 40.))
                    .to_path(0.01)
                    .to_svg(),
                fg,
                None,
            );
            add("M 64 55 L 64 96".into(), fg, Some(10.));
        } else {
            add("M 64 30 L 64 75".into(), fg, Some(10.));
            add(dot, fg, None);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn vectors_keep_palette_geometry_and_output_through_edit_undo_and_reopen() {
        use crate::{
            Command, Document, Session,
            bitmap::Bitmap,
            display::{Viewport, render_annotation_preview},
        };
        let mut s = Session::new(Document::canvas(500, 500, [0; 4]).unwrap()).unwrap();
        let payload = crate::icon_library::builtin::find("builtin-check-circle")
            .unwrap()
            .payload()
            .unwrap();
        s.execute(Command::PasteObjects {
            payload,
            center: [150., 150.],
        })
        .unwrap();
        let original = s.document.clone();
        let mut a = original.annotations[0].clone();
        let mut vector = a.image.as_ref().unwrap().vector().unwrap().clone();
        assert_eq!(vector.colors.len(), 2);
        vector.colors[0] = [90, 40, 230, 255];
        a.image = Some(std::sync::Arc::new(
            Bitmap::from_vector(vector.clone()).unwrap(),
        ));
        s.execute(Command::Update {
            annotation: a.clone(),
        })
        .unwrap();
        let encoded = serde_json::to_vec(&s.document).unwrap();
        let decoded: Document = serde_json::from_slice(&encoded).unwrap();
        decoded.validate().unwrap();
        assert_eq!(decoded.render().unwrap(), s.document.render().unwrap());
        assert_ne!(decoded.render().unwrap(), original.render().unwrap());
        let preview = render_annotation_preview(
            &a,
            Viewport {
                size: [500, 500],
                origin: [0.; 2],
                scale: 1.,
            },
        )
        .unwrap();
        assert_eq!(preview, decoded.render().unwrap());
        a.start = [0., 0.];
        a.end = [512., 512.];
        let large = render_annotation_preview(
            &a,
            Viewport {
                size: [512, 512],
                origin: [0.; 2],
                scale: 1.,
            },
        )
        .unwrap();
        assert!(large.pixels().any(|p| p[3] > 0 && p[3] < 255));
        assert!(large.pixels().any(|p| p.0 == [90, 40, 230, 255]));
        s.execute(Command::Undo).unwrap();
        assert_eq!(s.document.render().unwrap(), original.render().unwrap());
        vector.colors.extend([[0; 4]; 4]);
        assert!(vector.validate().is_err());
        vector.colors.truncate(2);
        vector.paths[0].data = "M NaN 0".into();
        assert!(vector.validate().is_err());
    }
}
