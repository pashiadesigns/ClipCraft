//! Shared flattened output and page geometry. Export never edits the document.
use crate::{Document, Error, Result};
use image::{RgbImage, RgbaImage};
mod vector_pdf;
use std::{
    io::{Cursor, Write},
    path::Path,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Format {
    #[default]
    Png,
    Jpeg,
    Pdf,
}
impl Format {
    pub fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::Pdf => "PDF",
        }
    }
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Pdf => "pdf",
        }
    }
    pub fn from_path(p: &Path) -> Result<Self> {
        match p
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "png" => Ok(Self::Png),
            "jpg" | "jpeg" => Ok(Self::Jpeg),
            "pdf" => Ok(Self::Pdf),
            _ => Err(Error::Invalid("Choose PNG, JPEG or PDF output.".into())),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageSettings {
    pub size: [f64; 2],
    pub margin: f64,
    pub paginate: bool,
}
impl Default for PageSettings {
    fn default() -> Self {
        Self {
            size: [612., 792.],
            margin: 36.,
            paginate: false,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    pub source: [u32; 4],
    pub destination: [f64; 4],
}
impl PageSettings {
    pub fn document_pages(self, document: &Document) -> Result<Vec<Page>> {
        if let Some(layout) = document.guide.as_ref().filter(|g| g.pages.is_some()) {
            let size = layout.pages.as_ref().unwrap().size;
            let mut result = Vec::new();
            for index in 0..layout.page_count() {
                // Explicit guide pages are kept intact, even when the print setting paginates long images.
                for mut page in (Self {
                    paginate: false,
                    ..self
                })
                .pages(size)?
                {
                    page.source[1] += layout.page_origin(index) as u32;
                    result.push(page);
                }
            }
            Ok(result)
        } else {
            self.pages([document.width, document.height])
        }
    }
    pub fn pages(self, image: [u32; 2]) -> Result<Vec<Page>> {
        crate::validate_dimensions(image[0], image[1])?;
        if !self
            .size
            .iter()
            .all(|n| n.is_finite() && *n > 0. && *n <= 14400.)
            || !self.margin.is_finite()
            || self.margin < 0.
            || self.size.iter().any(|n| *n <= self.margin * 2.)
        {
            return Err(Error::Invalid(
                "Page size and margins leave no printable area.".into(),
            ));
        }
        let area = self.size.map(|n| n - self.margin * 2.);
        let scale = if self.paginate {
            area[0] / f64::from(image[0])
        } else {
            (area[0] / f64::from(image[0])).min(area[1] / f64::from(image[1]))
        };
        let rows = if self.paginate {
            (area[1] / scale).floor().max(1.) as u32
        } else {
            image[1]
        };
        let count = image[1].div_ceil(rows);
        if count > 1000 {
            return Err(Error::Invalid(
                "Output exceeds 1,000 pages; increase page size or use Fit on one page.".into(),
            ));
        }
        Ok((0..image[1])
            .step_by(rows as usize)
            .map(|y| {
                let h = rows.min(image[1] - y);
                let w = f64::from(image[0]) * scale;
                let height = f64::from(h) * scale;
                Page {
                    source: [0, y, image[0], h],
                    destination: [
                        (self.size[0] - w) * 0.5,
                        if self.paginate {
                            self.margin
                        } else {
                            (self.size[1] - height) * 0.5
                        },
                        w,
                        height,
                    ],
                }
            })
            .collect())
    }
}
#[derive(Clone, Debug)]
pub struct Options {
    pub format: Format,
    pub quality: u8,
    pub background: [u8; 3],
    pub page: PageSettings,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            format: Format::Png,
            quality: 92,
            background: [255; 3],
            page: PageSettings::default(),
        }
    }
}
pub fn opaque(pixels: &RgbaImage, background: [u8; 3]) -> RgbImage {
    RgbImage::from_fn(pixels.width(), pixels.height(), |x, y| {
        let p = pixels.get_pixel(x, y);
        image::Rgb([0, 1, 2].map(|i| {
            ((u32::from(p[i]) * u32::from(p[3])
                + u32::from(background[i]) * (255 - u32::from(p[3]))
                + 127)
                / 255) as u8
        }))
    })
}
pub fn encode(pixels: &RgbaImage, options: &Options) -> Result<Vec<u8>> {
    let mut bytes = Cursor::new(Vec::new());
    match options.format {
        Format::Png => pixels.write_to(&mut bytes, image::ImageFormat::Png)?,
        Format::Jpeg => {
            if !(1..=100).contains(&options.quality) {
                return Err(Error::Invalid("JPEG quality must be 1–100.".into()));
            }
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, options.quality)
                .encode_image(&opaque(pixels, options.background))?;
        }
        Format::Pdf => return pdf(pixels, options),
    }
    Ok(bytes.into_inner())
}
pub fn save(document: &Document, path: &Path, options: &Options) -> Result<()> {
    let bytes = encode_document(document, options)?;
    crate::atomic_write(path, &bytes)
}
pub fn encode_document(document: &Document, options: &Options) -> Result<Vec<u8>> {
    if options.format == Format::Pdf {
        vector_pdf::encode(document, options)
    } else {
        encode(&document.render()?, options)
    }
}
pub fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    crate::atomic_write(path, bytes)
}
fn pdf(pixels: &RgbaImage, options: &Options) -> Result<Vec<u8>> {
    let pages = options.page.pages([pixels.width(), pixels.height()])?;
    let mut objects: Vec<Vec<u8>> = vec![b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(), vec![]];
    let mut kids = vec![];
    for page in &pages {
        let id = objects.len() + 1;
        kids.push(format!("{id} 0 R"));
        let [x, y, w, h] = page.source;
        let rgb = opaque(
            &image::imageops::crop_imm(pixels, x, y, w, h).to_image(),
            options.background,
        );
        let mut compressed =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        compressed.write_all(rgb.as_raw())?;
        let compressed = compressed.finish()?;
        let [dx, dy, dw, dh] = page.destination;
        let content = format!(
            "q\n{dw:.6} 0 0 {dh:.6} {dx:.6} {:.6} cm\n/Im0 Do\nQ\n",
            options.page.size[1] - dy - dh
        );
        objects.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /XObject << /Im0 {} 0 R >> >> /Contents {} 0 R >>",options.page.size[0],options.page.size[1],id+1,id+2).into_bytes());
        let mut stream=format!("<< /Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n",compressed.len()).into_bytes();
        stream.extend(compressed);
        stream.extend(b"\nendstream");
        objects.push(stream);
        objects.push(
            format!(
                "<< /Length {} >>\nstream\n{content}endstream",
                content.len()
            )
            .into_bytes(),
        );
    }
    objects[1] = format!(
        "<< /Type /Pages /Count {} /Kids [{}] >>",
        pages.len(),
        kids.join(" ")
    )
    .into_bytes();
    finish_pdf(objects)
}
fn finish_pdf(objects: Vec<Vec<u8>>) -> Result<Vec<u8>> {
    let mut bytes = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = vec![0];
    for (index, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        writeln!(bytes, "{} 0 obj", index + 1)?;
        bytes.extend(object);
        bytes.extend(b"\nendobj\n");
    }
    let xref = bytes.len();
    write!(bytes, "xref\n0 {}\n0000000000 65535 f \n", offsets.len())?;
    for offset in offsets.iter().skip(1) {
        writeln!(bytes, "{offset:010} 00000 n ")?;
    }
    write!(
        bytes,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        offsets.len()
    )?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn formats_preserve_pixels_or_composite_alpha_and_never_mutate_document() {
        let d = Document::from_image(RgbaImage::from_pixel(
            48,
            32,
            image::Rgba([200, 100, 50, 128]),
        ))
        .unwrap();
        let before = serde_json::to_vec(&d).unwrap();
        let temp = tempfile::tempdir().unwrap();
        for format in [Format::Png, Format::Jpeg, Format::Pdf] {
            let options = Options {
                format,
                ..Default::default()
            };
            let path = temp.path().join(format!("output.{}", format.extension()));
            save(&d, &path, &options).unwrap();
            let bytes = std::fs::read(path).unwrap();
            if format == Format::Pdf {
                assert!(bytes.starts_with(b"%PDF-1.4"));
                assert!(bytes.ends_with(b"%%EOF\n"));
            } else {
                let actual = image::load_from_memory(&bytes).unwrap();
                assert_eq!((actual.width(), actual.height()), (48, 32));
                if format == Format::Png {
                    assert_eq!(actual.to_rgba8(), d.render().unwrap());
                } else {
                    let p = actual.to_rgb8().get_pixel(10, 10).0;
                    let expected = opaque(&d.render().unwrap(), [255; 3]).get_pixel(10, 10).0;
                    assert!(p.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 2));
                }
            }
        }
        assert_eq!(serde_json::to_vec(&d).unwrap(), before);
    }
    #[test]
    fn pagination_covers_every_source_row_once_and_rejects_bad_settings() {
        let settings = PageSettings {
            paginate: true,
            ..Default::default()
        };
        let pages = settings.pages([600, 6000]).unwrap();
        assert!(pages.len() > 1);
        let mut y = 0;
        for p in pages {
            assert_eq!(p.source[1], y);
            y += p.source[3];
            assert!(
                p.destination[1] + p.destination[3] <= settings.size[1] - settings.margin + 0.001
            );
        }
        assert_eq!(y, 6000);
        assert!(
            PageSettings {
                margin: f64::NAN,
                ..settings
            }
            .pages([600, 400])
            .is_err()
        );
        assert!(
            PageSettings {
                margin: 500.,
                ..settings
            }
            .pages([600, 400])
            .is_err()
        );
    }
}
