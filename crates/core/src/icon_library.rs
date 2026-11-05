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
        let document = Document::open_image(path)?;
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
        let pixels = self.0.load(id)?.render()?;
        let mut payload = from_pixels(pixels)?;
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
