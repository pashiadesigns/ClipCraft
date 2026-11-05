//! Original, redistributable business-guide symbols; no external packs required.
use crate::{Result, clipboard_payload::ObjectPayload};
pub struct Icon {
    pub id: &'static str,
    pub label: &'static str,
    keywords: &'static str,
    png: &'static [u8],
}
impl Icon {
    pub fn category(&self) -> &'static str {
        match self.id.trim_start_matches("builtin-") {
            "check-green" | "check-circle" | "cross-red" | "cross-circle" => "Status",
            "caution-yellow" | "exclamation" | "exclamation-circle" | "question"
            | "information" => "Notes & alerts",
            "raster-sample" | "five-color" => "Samples",
            id if id.starts_with("mouse-") || id.starts_with("key-") || id == "drag" => {
                "Input gestures"
            }
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
        let bitmap = crate::bitmap::Bitmap::from_vector(super::vector::VectorIcon::builtin(
            self.id.trim_start_matches("builtin-"),
        ))?;
        Ok((**bitmap.pixels()?).clone())
    }
    pub fn payload(&self) -> Result<ObjectPayload> {
        let mut payload = super::from_pixels(self.pixels()?)?;
        if self.id != "builtin-raster-sample" {
            payload.document_version = crate::MAX_PROJECT_VERSION;
            payload.objects[0].image =
                Some(std::sync::Arc::new(crate::bitmap::Bitmap::from_vector(
                    super::vector::VectorIcon::builtin(self.id.trim_start_matches("builtin-")),
                )?));
        }
        let bitmap = payload.objects[0].image.as_ref().unwrap();
        payload.objects[0].image = Some(std::sync::Arc::new(
            (**bitmap).clone().named_icon(self.label),
        ));
        payload.document_version = crate::MAX_PROJECT_VERSION;
        Ok(payload)
    }
}
macro_rules! icon {
    ($id:literal,$label:literal,$words:literal) => {
        Icon {
            id: concat!("builtin-", $id),
            label: $label,
            keywords: $words,
            png: &[],
        }
    };
}
pub const ALL: &[Icon] = &[
    icon!("mouse-left", "Left click", "mouse primary click input"),
    icon!(
        "mouse-right",
        "Right click",
        "mouse secondary context menu input"
    ),
    icon!("mouse-double", "Double click", "mouse twice open input"),
    icon!("mouse-scroll", "Scroll", "mouse wheel up down input"),
    icon!("drag", "Drag", "mouse hold move input"),
    icon!("key-enter", "Enter", "keyboard return confirm input"),
    icon!("key-tab", "Tab", "keyboard next field input"),
    icon!("key-ctrl", "Ctrl", "keyboard control modifier input"),
    icon!("key-alt", "Alt", "keyboard alternate modifier input"),
    icon!("key-shift", "Shift", "keyboard shift modifier input"),
    icon!("clock", "Time", "workflow clock wait duration deadline"),
    icon!(
        "lock",
        "Secure",
        "workflow security lock private permission"
    ),
    icon!(
        "document",
        "Document",
        "workflow file page attachment instructions"
    ),
    icon!("download", "Download", "workflow save receive import down"),
    icon!("upload", "Upload", "workflow send export up"),
    icon!("lightbulb", "Tip", "workflow idea suggestion light bulb"),
    icon!("keyboard", "Keyboard", "workflow shortcut type enter keys"),
    icon!("target", "Target", "workflow goal focus destination"),
    Icon {
        id: "builtin-five-color",
        label: "Five-color workflow",
        keywords: "palette test five color workflow process",
        png: &[],
    },
    icon!(
        "check-green",
        "Green check",
        "yes approved done correct success"
    ),
    icon!(
        "check-circle",
        "Check circle",
        "green white yes approved done correct success"
    ),
    icon!("cross-red", "Red X", "no cross incorrect failed error"),
    icon!(
        "cross-circle",
        "X circle",
        "red white no cross incorrect failed error"
    ),
    icon!(
        "caution-yellow",
        "Caution",
        "yellow warning attention triangle"
    ),
    icon!(
        "exclamation",
        "Exclamation",
        "black important attention alert"
    ),
    icon!(
        "exclamation-circle",
        "Exclamation circle",
        "red white important alert error"
    ),
    icon!("question", "Question", "blue help unsure question mark"),
    icon!("information", "Information", "blue info note tip"),
    Icon {
        id: "builtin-raster-sample",
        label: "Raster test check",
        keywords: "png raster test sample",
        png: include_bytes!("../../assets/icon_library/check-green.png"),
    },
];
pub fn find(id: &str) -> Option<&'static Icon> {
    ALL.iter().find(|s| s.id == id)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_five_color_icon_preserves_all_slots_and_backward_version_guards() {
        let mut doc =
            crate::Session::new(crate::Document::canvas(500, 200, [255; 4]).unwrap()).unwrap();
        doc.execute(crate::Command::PasteObjects {
            payload: find("builtin-five-color").unwrap().payload().unwrap(),
            center: [250., 100.],
        })
        .unwrap();
        assert_eq!(doc.document.version, crate::MAX_PROJECT_VERSION);
        let b = doc.document.annotations[0].image.as_ref().unwrap();
        assert_eq!(b.icon_name(), Some("Five-color workflow"));
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
    fn starter_art_is_transparent_searchable_and_persists_as_editable_icons() {
        let mut fixture =
            crate::Session::new(crate::Document::canvas(640, 840, [245, 245, 245, 255]).unwrap())
                .unwrap();
        for icon in ALL {
            let pixels = icon.pixels().unwrap();
            assert_eq!(pixels.dimensions(), (128, 128));
            assert_eq!(pixels.get_pixel(0, 0).0[3], 0);
            assert!(pixels.pixels().any(|p| p.0[3] > 0 && p.0[3] < 255));
            let payload = icon.payload().unwrap();
            payload.validate().unwrap();
            let copy: ObjectPayload =
                serde_json::from_slice(&serde_json::to_vec(&payload).unwrap()).unwrap();
            assert!(copy.objects[0].image.as_ref().unwrap().is_icon());
            assert_eq!(copy.objects, payload.objects);
            let n = fixture.document.annotations.len();
            fixture
                .execute(crate::Command::PasteObjects {
                    payload: copy,
                    center: [90. + (n % 4) as f32 * 150., 90. + (n / 4) as f32 * 150.],
                })
                .unwrap();
        }
        if let Ok(path) = std::env::var("CLIPCRAFT_ICON_FIXTURE") {
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
