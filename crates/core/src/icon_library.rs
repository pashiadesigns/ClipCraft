//! User-imported image icons. Source files are copied into managed storage;
//! placed icons are independent editable image objects with their own pixels.
use crate::{
    Document, Result,
    clipboard_payload::{ImagePayload, ObjectPayload},
    history::{Entry, History, Origin},
};
use std::path::Path;
pub mod builtin;
mod instruction;
pub mod vector;
pub struct Library(pub History);
impl Library {
    pub fn open(root: &Path) -> Result<Self> {
        Ok(Self(History::open(root)?))
    }
    pub fn import(&mut self, path: &Path) -> Result<Entry> {
        let svg = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("svg"));
        // BI11: an SVG icon keeps its shapes and editable colors when it fits
        // the vector icon format; the stored document holds that one object.
        let document = match svg
            .then(|| std::fs::read_to_string(path).ok())
            .flatten()
            .and_then(|text| crate::vector_import::svg_vector_icon(&text).ok())
            .and_then(|icon| crate::bitmap::Bitmap::from_vector(icon).ok())
        {
            Some(icon) => {
                let mut d = Document::canvas(128, 128, [0; 4])?;
                let mut a = std::sync::Arc::new(icon).annotation(1);
                a.start = [0., 0.];
                a.end = [128., 128.];
                d.annotations.push(a);
                d
            }
            None => Document::open_image(path)?,
        };
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Imported icon")
            .chars()
            .filter(|c| !c.is_control())
            .take(120)
            .collect::<String>();
        let name = if name.trim().is_empty() {
            "Imported icon"
        } else {
            &name
        };
        let entry = self.0.create(&document, name, Origin::Image)?;
        self.0.edit_metadata(&entry, name, &[])
    }
    pub fn payload(&self, id: &str) -> Result<ObjectPayload> {
        let document = self.0.load(id)?;
        let vector = (document.annotations.len() == 1)
            .then(|| document.annotations[0].image.clone())
            .flatten()
            .filter(|b| b.vector().is_some());
        let mut payload = from_pixels(document.render()?)?;
        if let Some(icon) = vector {
            payload.objects[0].image = Some(icon);
        }
        if let Some(entry) = self.0.entries().iter().find(|e| e.id == id) {
            let bitmap = payload.objects[0].image.as_ref().unwrap();
            payload.objects[0].image = Some(std::sync::Arc::new(
                (**bitmap).clone().named_icon(&entry.label),
            ));
            payload.document_version = crate::MAX_PROJECT_VERSION;
        }
        Ok(payload)
    }
}
fn from_pixels(pixels: image::RgbaImage) -> Result<ObjectPayload> {
    let mut payload = ImagePayload::from_rgba(pixels.width(), pixels.height(), pixels.into_raw())?
        .into_objects()?;
    for object in &mut payload.objects {
        if let Some(bitmap) = &object.image {
            object.image = Some(std::sync::Arc::new((**bitmap).clone().into_icon()));
        }
    }
    Ok(payload)
}
#[cfg(test)]
mod tests {
    use super::*;
    /// BI11: an SVG imported as an icon keeps editable colors.
    #[test]
    fn svg_icons_import_as_vector_icons() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Badge.svg");
        std::fs::write(
            &path,
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"><circle cx="32" cy="32" r="28" fill="#188b4e"/><path d="M 18 33 L 28 43 L 46 22" stroke="#ffffff" stroke-width="6" fill="none"/></svg>"##,
        )
        .unwrap();
        let mut lib = Library::open(&dir.path().join("icons")).unwrap();
        let entry = lib.import(&path).unwrap();
        let payload = lib.payload(&entry.id).unwrap();
        let icon = payload.objects[0].image.as_ref().unwrap();
        let vector = icon.vector().expect("kept as a vector icon");
        assert_eq!(vector.colors.len(), 2);
        assert_eq!(icon.icon_name(), Some("Badge"));
    }
    #[test]
    fn icon_import_is_portable_editable_and_one_undo() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Approved.png");
        let source =
            image::RgbaImage::from_fn(24, 24, |x, y| image::Rgba([x as u8, y as u8, 90, 128]));
        source.save(&path).unwrap();
        let mut lib = Library::open(&dir.path().join("icons")).unwrap();
        let entry = lib.import(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        let lib = Library::open(&dir.path().join("icons")).unwrap();
        assert_eq!(lib.0.entries()[0].label, "Approved");
        let payload = lib.payload(&entry.id).unwrap();
        let mut session = crate::Session::new(Document::blank(100, 100).unwrap()).unwrap();
        session
            .execute(crate::Command::PasteObjects {
                payload,
                center: [50., 50.],
            })
            .unwrap();
        let result = session.document.clone();
        assert_eq!(result.annotations[0].kind, crate::Kind::Image);
        assert!(result.annotations[0].image.as_ref().unwrap().is_icon());
        let saved = dir.path().join("icon.capture.json");
        result.save(&saved).unwrap();
        assert_eq!(
            Document::load(&saved).unwrap().render().unwrap(),
            result.render().unwrap()
        );
        session.execute(crate::Command::Undo).unwrap();
        assert!(session.document.annotations.is_empty());
        session.execute(crate::Command::Redo).unwrap();
        assert_eq!(
            serde_json::to_value(&session.document).unwrap(),
            serde_json::to_value(&result).unwrap()
        );
    }
}
