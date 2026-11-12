//! Portable clipboard values, independent of OS clipboard access and the UI.
//! Object payloads omit the source screenshot; paste retains editable geometry.
use crate::{
    Annotation, Document, Error, RasterImage, Result, canvas_expansion, validate_dimensions,
};
use image::RgbaImage;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, sync::Arc};

const MAX_PAYLOAD_BYTES: usize = 160_000_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "content", rename_all = "snake_case")]
pub enum Content {
    Objects(ObjectPayload),
    Image(ImagePayload),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClipboardPayload {
    pub schema_version: u32,
    #[serde(flatten)]
    pub content: Content,
}

impl ClipboardPayload {
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)?;
        if bytes.len() > MAX_PAYLOAD_BYTES {
            return Err(Error::Invalid(
                "What you copied is over 160 MB. Copy less at once.".into(),
            ));
        }
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_PAYLOAD_BYTES {
            return Err(Error::Invalid(
                "What you copied is over 160 MB. Copy less at once.".into(),
            ));
        }
        let payload: Self = serde_json::from_slice(bytes)?;
        payload.validate()?;
        Ok(payload)
    }
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1 {
            return Err(Error::Invalid(
                "This was copied from a different ClipDesigner version. Copy it again in this one."
                    .into(),
            ));
        }
        match &self.content {
            Content::Objects(objects) => objects.validate(),
            Content::Image(image) => image.decode().map(|_| ()),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImagePayload {
    /// Embedded PNG is shared by cheap payload/history clones, never a file path.
    pub image: Arc<RasterImage>,
}

impl ImagePayload {
    pub fn from_rgba(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self> {
        validate_dimensions(width, height)?;
        if pixels.len() as u64 != u64::from(width) * u64::from(height) * 4 {
            return Err(Error::Invalid("Invalid clipboard RGBA buffer".into()));
        }
        let image = RgbaImage::from_raw(width, height, pixels)
            .ok_or_else(|| Error::Invalid("Invalid clipboard image".into()))?;
        Ok(Self {
            image: Arc::new(RasterImage::from_image(&image)?),
        })
    }
    pub fn into_objects(self) -> Result<ObjectPayload> {
        let bitmap = Arc::new(crate::bitmap::Bitmap::new(self.image)?);
        let size = bitmap.size();
        Ok(ObjectPayload {
            document_version: crate::MAX_PROJECT_VERSION,
            source_canvas: size,
            objects: vec![bitmap.annotation(1)],
        })
    }
    pub fn decode(&self) -> Result<RgbaImage> {
        validate_dimensions(self.image.width, self.image.height)?;
        if self.image.png.len() > MAX_PAYLOAD_BYTES {
            return Err(Error::Invalid("Clipboard PNG exceeds 160 MB".into()));
        }
        self.image.render(self.image.width, self.image.height)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObjectPayload {
    pub document_version: u32,
    pub source_canvas: [u32; 2],
    /// Original stacking order, independent of the order of selection clicks.
    pub objects: Vec<Annotation>,
}

impl ObjectPayload {
    pub fn capture(document: &Document, ids: &[u64]) -> Result<Self> {
        if ids
            .iter()
            .any(|id| !document.annotations.iter().any(|a| a.id == *id))
        {
            return Err(Error::Invalid("Select existing objects to copy".into()));
        }
        Self::capture_exact(document, &document.expand_groups(ids))
    }
    /// Copy the explicit selection, including an individually selected group child.
    /// Group expansion belongs to the selection model, not the transfer transport.
    pub fn capture_exact(document: &Document, ids: &[u64]) -> Result<Self> {
        document.validate()?;
        if ids
            .iter()
            .any(|id| !document.annotations.iter().any(|a| a.id == *id))
        {
            return Err(Error::Invalid("Select existing objects to copy".into()));
        }
        let ids: BTreeSet<_> = ids.iter().copied().collect();
        if ids.is_empty()
            || ids
                .iter()
                .any(|id| !document.annotations.iter().any(|a| a.id == *id))
        {
            return Err(Error::Invalid("Select existing objects to copy".into()));
        }
        let mut payload = Self {
            document_version: crate::MAX_PROJECT_VERSION,
            source_canvas: [document.width, document.height],
            objects: document
                .annotations
                .iter()
                .filter(|a| ids.contains(&a.id))
                .cloned()
                .collect(),
        };
        let counts = payload.objects.iter().filter_map(|a| a.group).fold(
            std::collections::BTreeMap::<u64, usize>::new(),
            |mut counts, group| {
                *counts.entry(group).or_default() += 1;
                counts
            },
        );
        for object in &mut payload.objects {
            if object.group.is_some_and(|group| counts[&group] < 2) {
                object.group = None;
            }
        }
        payload.validate()?;
        Ok(payload)
    }
    pub fn validate(&self) -> Result<()> {
        // Version4 only adds document-local canvas provenance, omitted on copy.
        // Version5 preserves underline/strikethrough on editable labels.
        if self.document_version != crate::MAX_PROJECT_VERSION {
            return Err(Error::Invalid(
                "Unsupported clipboard object version".into(),
            ));
        }

        if self.objects.is_empty() {
            return Err(Error::Invalid("Clipboard object selection is empty".into()));
        }
        let mut source = Document::blank(self.source_canvas[0], self.source_canvas[1])?;
        source.version = self.document_version;
        source.annotations = self.objects.clone();
        source.validate()
    }
    /// Prepare fresh IDs and a shared translation without changing either document.
    pub fn place(&self, target: &Document, center: [f32; 2]) -> Result<Vec<Annotation>> {
        self.validate()?;
        target.validate()?;
        if !center
            .iter()
            .all(|v| v.is_finite() && v.abs() <= 1_000_000.0)
        {
            return Err(Error::Invalid("Invalid clipboard insertion point".into()));
        }
        let mut min = [f32::INFINITY; 2];
        let mut max = [f32::NEG_INFINITY; 2];
        for a in &self.objects {
            let (lo, hi) = a.bounds();
            for i in 0..2 {
                min[i] = min[i].min(lo[i]);
                max[i] = max[i].max(hi[i]);
            }
        }
        let delta = [0, 1].map(|i| center[i] - (min[i] + max[i]) * 0.5);
        let mut objects = canvas_expansion::translated(&self.objects, delta)?;
        let mut id = target.annotations.iter().map(|a| a.id).max().unwrap_or(0);
        for a in &mut objects {
            id = id
                .checked_add(1)
                .ok_or_else(|| Error::Invalid("No more object identifiers are available".into()))?;
            a.id = id;
        }
        crate::groups::freshen(&mut objects, target)?;
        Ok(objects)
    }
}

impl Document {
    pub(crate) fn paste_objects(&mut self, payload: ObjectPayload, center: [f32; 2]) -> Result<()> {
        let objects = payload.place(self, center)?;
        let mut candidate = self.clone();
        candidate.version = candidate.version.max(payload.document_version);
        let plan = candidate.plan_expanding_move(&objects, true)?;
        let result = plan.apply(&candidate)?;
        *self = result;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Kind, Session};

    #[test]
    fn copied_objects_keep_order_geometry_and_editability_across_documents() {
        let mut source: Document =
            serde_json::from_str(include_str!("../tests/fixtures/properties.capture.json"))
                .unwrap();
        let ids: Vec<_> = source
            .annotations
            .iter()
            .filter(|a| matches!(a.kind, Kind::Step | Kind::Callout | Kind::Rectangle))
            .map(|a| a.id)
            .rev()
            .collect();
        let id = ids[0];
        source.rotate_annotation(id, 37.0).unwrap();
        let original = serde_json::to_vec(&source).unwrap();
        let objects = ObjectPayload::capture(&source, &ids).unwrap();
        let bytes = ClipboardPayload {
            schema_version: 1,
            content: Content::Objects(objects.clone()),
        }
        .encode()
        .unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("source_png"));
        let ClipboardPayload {
            content: Content::Objects(payload),
            ..
        } = ClipboardPayload::decode(&bytes).unwrap()
        else {
            panic!()
        };
        let mut target =
            Session::new(Document::canvas(640, 400, [20, 30, 40, 255]).unwrap()).unwrap();
        target.document.annotations.push(
            crate::steps::marker(
                77,
                [20.0, 20.0],
                24,
                [255, 50, 50, 255],
                (1, false),
                [640, 400],
            )
            .unwrap(),
        );
        let before = serde_json::to_vec(&target.document).unwrap();
        target
            .execute(Command::PasteObjects {
                payload,
                center: [320.0, 200.0],
            })
            .unwrap();
        assert_eq!(target.document.version, crate::MAX_PROJECT_VERSION);
        let pasted = &target.document.annotations[1..];
        assert_eq!(
            pasted.iter().map(|a| a.id).collect::<Vec<_>>(),
            vec![78, 79, 80]
        );
        for (a, b) in objects.objects.iter().zip(pasted) {
            assert_eq!(a.kind, b.kind);
            assert_eq!(a.text, b.text);
            assert_eq!(a.rotation_degrees, b.rotation_degrees);
            assert_eq!(a.end[0] - a.start[0], b.end[0] - b.start[0]);
            assert_eq!(a.end[1] - a.start[1], b.end[1] - b.start[1]);
        }
        assert_eq!(serde_json::to_vec(&source).unwrap(), original);
        let image = target.document.render().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pasted.capture.json");
        target.save(&path).unwrap();
        assert_eq!(Document::load(&path).unwrap().render().unwrap(), image);
        target.execute(Command::Undo).unwrap();
        assert_eq!(serde_json::to_vec(&target.document).unwrap(), before);
        assert!(!target.can_undo());
        target.execute(Command::Redo).unwrap();
        assert_eq!(target.document.render().unwrap(), image);
    }

    #[test]
    fn pasted_objects_expand_transparently_without_replacing_original_pixels() {
        let mut source = Document::blank(200, 100).unwrap();
        source.annotations.push(
            crate::steps::marker(
                1,
                [100.0, 50.0],
                80,
                [40, 80, 180, 255],
                (8, false),
                [200, 100],
            )
            .unwrap(),
        );
        let payload = ObjectPayload::capture(&source, &[1]).unwrap();
        let base = RgbaImage::from_pixel(30, 20, image::Rgba([25, 75, 125, 180]));
        let mut target = Session::new(Document::from_image(base.clone()).unwrap()).unwrap();
        let original = serde_json::to_vec(&target.document).unwrap();
        target
            .execute(Command::PasteObjects {
                payload,
                center: [15.0, 10.0],
            })
            .unwrap();
        assert_eq!([target.document.width, target.document.height], [80, 80]);
        let padded = target.document.base_image().unwrap();
        assert_eq!(
            image::imageops::crop_imm(&padded, 25, 30, 30, 20).to_image(),
            base
        );
        assert_eq!(padded.get_pixel(0, 0).0, [0; 4]);
        target.execute(Command::Undo).unwrap();
        assert_eq!(serde_json::to_vec(&target.document).unwrap(), original);
    }

    #[test]
    fn invalid_object_paste_is_atomic_and_preserves_redo() {
        let mut doc = Document::blank(100, 100).unwrap();
        doc.annotations.push(
            crate::steps::marker(
                1,
                [50.0, 50.0],
                40,
                [40, 80, 180, 255],
                (8, false),
                [100, 100],
            )
            .unwrap(),
        );
        let payload = ObjectPayload::capture(&doc, &[1]).unwrap();
        let mut session = Session::new(doc).unwrap();
        session.execute(Command::Remove { id: 1 }).unwrap();
        session.execute(Command::Undo).unwrap();
        let before = serde_json::to_vec(&session.document).unwrap();
        for center in [[f32::NAN, 0.0], [1_000_001.0, 0.0], [32768.0, 32768.0]] {
            assert!(
                session
                    .execute(Command::PasteObjects {
                        payload: payload.clone(),
                        center
                    })
                    .is_err()
            );
            assert_eq!(serde_json::to_vec(&session.document).unwrap(), before);
            assert!(session.can_redo());
        }
        let mut bad = payload.clone();
        bad.objects.push(bad.objects[0].clone());
        assert!(bad.validate().is_err());
        let mut bad = payload;
        bad.document_version = 4;
        assert!(bad.validate().is_err());
        assert!(ObjectPayload::capture(&session.document, &[999]).is_err());
        assert!(ObjectPayload::capture(&session.document, &[]).is_err());
    }

    #[test]
    fn image_payload_retains_rgba_and_rejects_malformed_or_future_data() {
        let pixels = vec![17, 85, 201, 0, 18, 52, 171, 128, 250, 124, 9, 255];
        let image = ImagePayload::from_rgba(3, 1, pixels.clone()).unwrap();
        let bytes = ClipboardPayload {
            schema_version: 1,
            content: Content::Image(image.clone()),
        }
        .encode()
        .unwrap();
        let ClipboardPayload {
            content: Content::Image(decoded),
            ..
        } = ClipboardPayload::decode(&bytes).unwrap()
        else {
            panic!()
        };
        assert_eq!(decoded.decode().unwrap().into_raw(), pixels);
        assert!(ImagePayload::from_rgba(32769, 1, vec![]).is_err());
        assert!(ImagePayload::from_rgba(1, 1, vec![0; 3]).is_err());
        assert!(ImagePayload::from_rgba(1, 1, vec![0; 5]).is_err());
        let mut wrong = image.clone();
        Arc::make_mut(&mut wrong.image).width = 2;
        assert!(wrong.decode().is_err());
        Arc::make_mut(&mut wrong.image).png = "not a PNG".into();
        assert!(wrong.decode().is_err());
        assert!(
            ClipboardPayload {
                schema_version: 2,
                content: Content::Image(image)
            }
            .encode()
            .is_err()
        );
    }
}
