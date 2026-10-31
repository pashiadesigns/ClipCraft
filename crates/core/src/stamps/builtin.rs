//! Original, redistributable business-guide symbols; no external packs required.
use crate::{Result, clipboard_payload::ObjectPayload};
pub struct Stamp {
    pub id: &'static str,
    pub label: &'static str,
    keywords: &'static str,
    png: &'static [u8],
}
impl Stamp {
    pub fn matches_query(&self, query: &str) -> bool {
        let text = format!("{} {}", self.label, self.keywords).to_lowercase();
        query
            .split_whitespace()
            .all(|word| text.contains(&word.to_lowercase()))
    }
    pub fn pixels(&self) -> Result<image::RgbaImage> {
        Ok(image::load_from_memory(self.png)?.to_rgba8())
    }
    pub fn payload(&self) -> Result<ObjectPayload> {
        super::from_pixels(self.pixels()?)
    }
}
macro_rules! stamp {
    ($id:literal,$label:literal,$words:literal) => {
        Stamp {
            id: concat!("builtin-", $id),
            label: $label,
            keywords: $words,
            png: include_bytes!(concat!("../../assets/stamps/", $id, ".png")),
        }
    };
}
pub const ALL: &[Stamp] = &[
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
];
pub fn find(id: &str) -> Option<&'static Stamp> {
    ALL.iter().find(|s| s.id == id)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn starter_art_is_transparent_searchable_and_persists_as_editable_stamps() {
        for stamp in ALL {
            let pixels = stamp.pixels().unwrap();
            assert_eq!(pixels.dimensions(), (128, 128));
            assert_eq!(pixels.get_pixel(0, 0).0[3], 0);
            assert!(pixels.pixels().any(|p| p.0[3] > 0 && p.0[3] < 255));
            let payload = stamp.payload().unwrap();
            let copy: ObjectPayload =
                serde_json::from_slice(&serde_json::to_vec(&payload).unwrap()).unwrap();
            assert!(copy.objects[0].image.as_ref().unwrap().is_stamp());
            assert_eq!(copy.objects, payload.objects);
        }
        assert_eq!(
            ALL.iter()
                .filter(|s| s.matches_query("yellow warning"))
                .count(),
            1
        );
    }
}
