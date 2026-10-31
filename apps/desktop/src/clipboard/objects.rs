//! Explicit clipboard transactions. No polling or process-local object cache.
use capture_core::{
    Document,
    clipboard_payload::{ClipboardPayload, Content, ImagePayload, ObjectPayload},
};
use image::RgbaImage;

pub trait Transfer {
    fn read(&mut self) -> Result<ClipboardPayload, String>;
    fn write(&mut self, payload: &ClipboardPayload, fallback: &RgbaImage) -> Result<(), String>;
}
pub struct SystemTransfer {
    pub owner: Option<isize>,
}

impl Transfer for SystemTransfer {
    fn read(&mut self) -> Result<ClipboardPayload, String> {
        #[cfg(windows)]
        if let Some(bytes) = windows::read_objects()? {
            return ClipboardPayload::decode(&bytes).map_err(|e| e.to_string());
        }
        let data = arboard::Clipboard::new()
            .and_then(|mut c| c.get_image())
            .map_err(|e| format!("Clipboard contains no usable image or ClipCraft objects: {e}"))?;
        let width = u32::try_from(data.width).map_err(|_| "Clipboard image too wide")?;
        let height = u32::try_from(data.height).map_err(|_| "Clipboard image too tall")?;
        let image = ImagePayload::from_rgba(width, height, data.bytes.into_owned())
            .map_err(|e| e.to_string())?;
        Ok(ClipboardPayload {
            schema_version: 1,
            content: Content::Image(image),
        })
    }
    fn write(&mut self, payload: &ClipboardPayload, fallback: &RgbaImage) -> Result<(), String> {
        let bytes = payload.encode().map_err(|e| e.to_string())?;
        #[cfg(windows)]
        {
            windows::write_objects(self.owner, &bytes, fallback)
        }
        #[cfg(not(windows))]
        {
            let _ = (bytes, fallback);
            Err("Editable object clipboard is currently available on Windows".into())
        }
    }
}

/// Deliberately omit the source screenshot, hidden/lock state and unrelated
/// annotations. Other apps receive just a cropped transparent object rendering.
pub fn selection(
    document: &Document,
    ids: &[u64],
) -> Result<(ClipboardPayload, RgbaImage), String> {
    let payload = ObjectPayload::capture_exact(document, ids).map_err(|e| e.to_string())?;
    let mut isolated =
        Document::canvas(document.width, document.height, [0; 4]).map_err(|e| e.to_string())?;
    isolated.version = payload.document_version;
    isolated.annotations = payload.objects.clone();
    isolated.promote_features();
    let render = isolated.render().map_err(|e| e.to_string())?;
    let mut low = [document.width as f32, document.height as f32];
    let mut high = [0.0f32; 2];
    for a in &payload.objects {
        let (min, max) = a.bounds();
        let margin = if a.kind == capture_core::Kind::Image {
            0.0
        } else {
            a.width as f32 * 4.0 + 2.0
        };
        for i in 0..2 {
            low[i] = low[i].min(min[i] - margin);
            high[i] = high[i].max(max[i] + margin);
        }
    }
    let [x, y] = low.map(|v| v.floor().max(0.0) as u32);
    let w = (high[0].ceil().min(document.width as f32) as u32)
        .saturating_sub(x)
        .max(1);
    let h = (high[1].ceil().min(document.height as f32) as u32)
        .saturating_sub(y)
        .max(1);
    let fallback = image::imageops::crop_imm(&render, x, y, w, h).to_image();
    Ok((
        ClipboardPayload {
            schema_version: 1,
            content: Content::Objects(payload),
        },
        fallback,
    ))
}

#[cfg(windows)]
mod windows {
    use super::*;
    use clipboard_win::{Clipboard, formats, raw};
    const FORMAT: &str = "ClipCraft.EditableObjects.v1";
    const LIMIT: usize = 160_000_000;
    fn format(name: &str) -> Result<u32, String> {
        clipboard_win::register_format(name)
            .map(|v| v.get())
            .ok_or_else(|| "Cannot register clipboard format".into())
    }
    pub(super) fn read_objects() -> Result<Option<Vec<u8>>, String> {
        let id = format(FORMAT)?;
        let _guard = Clipboard::new_attempts(5).map_err(|e| format!("Clipboard is busy: {e}"))?;
        if !clipboard_win::is_format_avail(id) {
            return Ok(None);
        }
        let size = clipboard_win::size(id)
            .ok_or("Clipboard object data is empty")?
            .get();
        if !(8..=LIMIT + 32).contains(&size) {
            return Err("Clipboard object data is too large or truncated".into());
        }
        let mut bytes = vec![0; size];
        let read = raw::get(id, &mut bytes).map_err(|e| e.to_string())?;
        bytes.truncate(read);
        unpack(&bytes).map(|v| Some(v.to_vec()))
    }
    fn unpack(bytes: &[u8]) -> Result<&[u8], String> {
        let header: [u8; 8] = bytes
            .get(..8)
            .ok_or("Truncated clipboard header")?
            .try_into()
            .unwrap();
        let length =
            usize::try_from(u64::from_le_bytes(header)).map_err(|_| "Invalid clipboard length")?;
        if length > LIMIT {
            return Err("Clipboard payload too large".into());
        }
        bytes
            .get(8..8 + length)
            .ok_or_else(|| "Truncated clipboard object data".into())
    }
    pub(super) fn write_objects(
        owner: Option<isize>,
        bytes: &[u8],
        image: &RgbaImage,
    ) -> Result<(), String> {
        // Prepare every representation before acquiring/clearing the clipboard.
        let id = format(FORMAT)?;
        let png_id = format("PNG")?;
        let mut encoded = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut encoded, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        let dib = dib_v5(image);
        let mut objects = Vec::with_capacity(bytes.len() + 8);
        objects.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        objects.extend_from_slice(bytes);
        let owner = owner.ok_or("Editor window is not ready for clipboard transfer")?;
        let _guard = Clipboard::new_attempts_for(owner as _, 5)
            .map_err(|e| format!("Clipboard is busy: {e}"))?;
        clipboard_win::empty().map_err(|e| e.to_string())?;
        for (format, data) in [
            (formats::CF_DIBV5, dib.as_slice()),
            (png_id, encoded.get_ref().as_slice()),
            (id, objects.as_slice()),
        ] {
            raw::set_without_clear(format, data)
                .map_err(|e| format!("Could not copy objects: {e}"))?;
        }
        Ok(())
    }
    fn dib_v5(image: &RgbaImage) -> Vec<u8> {
        let mut out = vec![0; 124];
        for (offset, value) in [
            (0, 124),
            (4, image.width()),
            (8, (-(image.height() as i32)) as u32),
            (16, 3),
            (20, image.width() * image.height() * 4),
            (40, 0x00ff0000),
            (44, 0x0000ff00),
            (48, 0x000000ff),
            (52, 0xff000000),
            (56, 0x73524742),
            (108, 4),
        ] {
            out[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        out[12..14].copy_from_slice(&1u16.to_le_bytes());
        out[14..16].copy_from_slice(&32u16.to_le_bytes());
        for p in image.pixels() {
            out.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
        }
        out
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn framing_and_bitmap_transport_are_bounded_and_exact() {
            let mut framed = 3u64.to_le_bytes().to_vec();
            framed.extend_from_slice(b"abc\0\0\0");
            assert_eq!(unpack(&framed).unwrap(), b"abc");
            for bytes in [
                vec![],
                vec![0; 7],
                u64::MAX.to_le_bytes().to_vec(),
                [10u64.to_le_bytes().as_slice(), b"x"].concat(),
            ] {
                assert!(unpack(&bytes).is_err());
            }
            let image = RgbaImage::from_raw(2, 1, vec![10, 20, 30, 40, 50, 60, 70, 255]).unwrap();
            let dib = dib_v5(&image);
            // BMP file header wraps the identical DIB for an independent decoder.
            let mut bmp = b"BM".to_vec();
            bmp.extend_from_slice(&((14 + dib.len()) as u32).to_le_bytes());
            bmp.extend_from_slice(&[0; 4]);
            bmp.extend_from_slice(&138u32.to_le_bytes());
            bmp.extend_from_slice(&dib);
            assert_eq!(
                image::load_from_memory_with_format(&bmp, image::ImageFormat::Bmp)
                    .unwrap()
                    .into_rgba8(),
                image
            );
        }
    }
}
