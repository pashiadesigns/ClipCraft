//! Original, redistributable business-guide symbols; no external packs required.
use crate::{Result, clipboard_payload::ObjectPayload};
pub struct Stamp {
    pub id: &'static str,
    pub label: &'static str,
    keywords: &'static str,
    png: &'static [u8],
}
impl Stamp {
    pub fn category(&self) -> &'static str {
        match self.id.trim_start_matches("builtin-") {
            "check-green" | "check-circle" | "cross-red" | "cross-circle" => "Status",
            "caution-yellow" | "exclamation" | "exclamation-circle" | "question"
            | "information" => "Notes & alerts",
            "raster-sample" | "five-color" => "Samples",
            _ => "Workflow",
        }
    }
    pub fn matches_query(&self, query: &str) -> bool {
        let text = format!("{} {}", self.label, self.keywords).to_lowercase();
        query
            .split_whitespace()
            .all(|word| text.contains(&word.to_lowercase()))
    }
    pub fn pixels(&self) -> Result<image::RgbaImage> {
        if self.id == "builtin-raster-sample" {
            return Ok(image::load_from_memory(self.png)?.to_rgba8());
        }
        let bitmap = crate::bitmap::Bitmap::from_vector(super::vector::VectorStamp::builtin(
            self.id.trim_start_matches("builtin-"),
        ))?;
        Ok((**bitmap.pixels()?).clone())
    }
    pub fn payload(&self) -> Result<ObjectPayload> {
        let mut payload = super::from_pixels(self.pixels()?)?;
        if self.id != "builtin-raster-sample" {
            payload.document_version = 22;
            payload.objects[0].image =
                Some(std::sync::Arc::new(crate::bitmap::Bitmap::from_vector(
                    super::vector::VectorStamp::builtin(self.id.trim_start_matches("builtin-")),
                )?));
        }
        let bitmap = payload.objects[0].image.as_ref().unwrap();
        payload.objects[0].image = Some(std::sync::Arc::new(
            (**bitmap).clone().named_stamp(self.label),
        ));
        payload.document_version = 23;
        Ok(payload)
    }
}
macro_rules! stamp {
    ($id:literal,$label:literal,$words:literal) => {
        Stamp {
            id: concat!("builtin-", $id),
            label: $label,
            keywords: $words,
            png: &[],
        }
    };
}
pub const ALL: &[Stamp] = &[
    stamp!("clock", "Time", "workflow clock wait duration deadline"),
    stamp!(
        "lock",
        "Secure",
        "workflow security lock private permission"
    ),
    stamp!(
        "document",
        "Document",
        "workflow file page attachment instructions"
    ),
    stamp!("download", "Download", "workflow save receive import down"),
    stamp!("upload", "Upload", "workflow send export up"),
    stamp!("lightbulb", "Tip", "workflow idea suggestion light bulb"),
    stamp!("keyboard", "Keyboard", "workflow shortcut type enter keys"),
    stamp!("target", "Target", "workflow goal focus destination"),
    Stamp {
        id: "builtin-five-color",
        label: "Five-color workflow",
        keywords: "palette test five color workflow process",
        png: &[],
    },
    stamp!(
        "check-green",
        "Green check",
        "yes approved done correct success"
    ),
    stamp!(
        "check-circle",
        "Check circle",
        "green white yes approved done correct success"
    ),
    stamp!("cross-red", "Red X", "no cross incorrect failed error"),
    stamp!(
        "cross-circle",
        "X circle",
        "red white no cross incorrect failed error"
    ),
    stamp!(
        "caution-yellow",
        "Caution",
        "yellow warning attention triangle"
    ),
    stamp!(
        "exclamation",
        "Exclamation",
        "black important attention alert"
    ),
    stamp!(
        "exclamation-circle",
        "Exclamation circle",
        "red white important alert error"
    ),
    stamp!("question", "Question", "blue help unsure question mark"),
    stamp!("information", "Information", "blue info note tip"),
    Stamp {
        id: "builtin-raster-sample",
        label: "Raster test check",
        keywords: "png raster test sample",
        png: include_bytes!("../../assets/stamps/check-green.png"),
    },
];
pub fn find(id: &str) -> Option<&'static Stamp> {
    ALL.iter().find(|s| s.id == id)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_five_color_stamp_preserves_all_slots_and_backward_version_guards() {
        let mut doc =
            crate::Session::new(crate::Document::canvas(500, 200, [255; 4]).unwrap()).unwrap();
        doc.execute(crate::Command::PasteObjects {
            payload: find("builtin-five-color").unwrap().payload().unwrap(),
            center: [250., 100.],
        })
        .unwrap();
        assert_eq!(doc.document.version, 23);
        let b = doc.document.annotations[0].image.as_ref().unwrap();
        assert_eq!(b.stamp_name(), Some("Five-color workflow"));
        assert_eq!(b.vector().unwrap().colors.len(), 5);
        let bytes = serde_json::to_vec(&doc.document).unwrap();
        let mut reopened: crate::Document = serde_json::from_slice(&bytes).unwrap();
        reopened.validate().unwrap();
        assert_eq!(doc.document.render().unwrap(), reopened.render().unwrap());
        reopened.version = 22;
        assert!(reopened.validate().is_err());
        if let Ok(path) = std::env::var("CLIPCRAFT_FIVE_COLOR_FIXTURE") {
            doc.document.save(std::path::Path::new(&path)).unwrap();
        }
    }
    #[test]
    fn starter_art_is_transparent_searchable_and_persists_as_editable_stamps() {
        let mut fixture =
            crate::Session::new(crate::Document::canvas(640, 840, [245, 245, 245, 255]).unwrap())
                .unwrap();
        for stamp in ALL {
            let pixels = stamp.pixels().unwrap();
            assert_eq!(pixels.dimensions(), (128, 128));
            assert_eq!(pixels.get_pixel(0, 0).0[3], 0);
            assert!(pixels.pixels().any(|p| p.0[3] > 0 && p.0[3] < 255));
            let payload = stamp.payload().unwrap();
            payload.validate().unwrap();
            let copy: ObjectPayload =
                serde_json::from_slice(&serde_json::to_vec(&payload).unwrap()).unwrap();
            assert!(copy.objects[0].image.as_ref().unwrap().is_stamp());
            assert_eq!(copy.objects, payload.objects);
            let n = fixture.document.annotations.len();
            fixture
                .execute(crate::Command::PasteObjects {
                    payload: copy,
                    center: [90. + (n % 4) as f32 * 150., 90. + (n / 4) as f32 * 150.],
                })
                .unwrap();
        }
        if let Ok(path) = std::env::var("CLIPCRAFT_STAMP_FIXTURE") {
            fixture.document.save(std::path::Path::new(&path)).unwrap();
        }
        assert_eq!(
            ALL.iter()
                .filter(|s| s.matches_query("yellow warning"))
                .count(),
            1
        );
    }
}
