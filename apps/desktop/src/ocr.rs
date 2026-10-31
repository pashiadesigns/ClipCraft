//! Local text recognition. Only explicitly supplied pixels reach the backend.
use crate::{Editor, egui};
use std::sync::mpsc::{self, Receiver, TryRecvError};
mod dialog;
mod formatting;

#[derive(Clone, Debug)]
pub(crate) struct Recognized {
    pub text: String,
    pub language: String,
    pub scaled: bool,
    spans: Vec<formatting::Span>,
}
#[derive(Default)]
pub(super) struct State {
    receiver: Option<Receiver<Result<Recognized, String>>>,
    result: Option<Result<Recognized, String>>,
    open: bool,
    copy_pending: bool,
    plain: bool,
    copy_status: Option<String>,
}
impl State {
    pub(super) fn busy(&self) -> bool {
        self.receiver.is_some()
    }
    pub(super) fn is_open(&self) -> bool {
        self.open
    }
    pub(super) fn present(&mut self, result: Result<Recognized, String>, open: bool, copy: bool) {
        self.result = Some(result);
        self.open = open;
        self.copy_pending = copy;
        self.plain = false;
        self.copy_status = None;
    }
}
impl Editor {
    pub(super) fn request_ocr(&mut self, ctx: &egui::Context) {
        if self.ocr.busy() {
            self.ocr.open = true;
            return;
        }
        let document = self.session.document.clone();
        let ctx = ctx.clone();
        let (tx, rx) = mpsc::channel();
        self.ocr.receiver = Some(rx);
        self.ocr.result = None;
        self.ocr.open = true;
        self.ocr.plain = false;
        self.ocr.copy_status = None;
        self.status = "Recognizing text locally…".into();
        std::thread::spawn(move || {
            let result = document
                .render()
                .map_err(|e| e.to_string())
                .and_then(|pixels| recognize(&pixels));
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }
    pub(super) fn show_ocr(&mut self, ctx: &egui::Context) {
        self.ocr.poll();
        self.ocr.show(ctx);
    }
}
impl State {
    fn poll(&mut self) {
        let result = match self.receiver.as_ref().map(|r| r.try_recv()) {
            Some(Ok(r)) => r,
            Some(Err(TryRecvError::Disconnected)) => {
                Err("Text recognition stopped unexpectedly".into())
            }
            _ => return,
        };
        self.receiver = None;
        self.result = Some(result);
    }
}

fn prepared(image: &image::RgbaImage, limit: u32) -> Result<(image::RgbaImage, bool), String> {
    capture_core::validate_dimensions(image.width(), image.height()).map_err(|e| e.to_string())?;
    if limit == 0 {
        return Err("Windows OCR reported an invalid image limit".into());
    }
    let scaled = image.width() > limit || image.height() > limit;
    let mut pixels = if scaled {
        image::DynamicImage::ImageRgba8(image.clone())
            .thumbnail(limit, limit)
            .into_rgba8()
    } else {
        image.clone()
    };
    for p in pixels.pixels_mut() {
        let a = p[3] as u32;
        for i in 0..3 {
            p[i] = ((p[i] as u32 * a + 255 * (255 - a) + 127) / 255) as u8;
        }
        p[3] = 255;
    }
    // Small UI glyphs are often missed at their original pixel height. Double
    // the OCR input when it fits the installed engine's limit. Composite alpha
    // first so transparent colors cannot create dark interpolation fringes.
    if !scaled
        && pixels.width() <= limit / 2
        && pixels.height() <= limit / 2
        && u64::from(pixels.width()) * u64::from(pixels.height()) <= 4_000_000
    {
        pixels = image::imageops::resize(
            &pixels,
            pixels.width() * 2,
            pixels.height() * 2,
            image::imageops::FilterType::CatmullRom,
        );
    }
    Ok((pixels, scaled))
}
#[cfg(windows)]
pub(crate) fn recognize(image: &image::RgbaImage) -> Result<Recognized, String> {
    recognize_impl(image, std::time::Duration::from_secs(30), true)
}
#[cfg(windows)]
pub(crate) fn recognize_bounded(
    image: &image::RgbaImage,
    timeout: std::time::Duration,
) -> Result<Recognized, String> {
    recognize_impl(image, timeout, false)
}
#[cfg(windows)]
fn recognize_impl(
    image: &image::RgbaImage,
    timeout: std::time::Duration,
    formatting: bool,
) -> Result<Recognized, String> {
    use windows::{
        Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap},
        Media::Ocr::OcrEngine,
        Storage::Streams::DataWriter,
        Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize},
    };
    unsafe { RoInitialize(RO_INIT_MULTITHREADED) }
        .map_err(|e| format!("Cannot initialize Windows text recognition: {e}"))?;
    struct Runtime;
    impl Drop for Runtime {
        fn drop(&mut self) {
            unsafe {
                RoUninitialize();
            }
        }
    }
    let _runtime = Runtime;
    let engine = OcrEngine::TryCreateFromUserProfileLanguages()
        .map_err(|e| format!("Windows OCR is unavailable for your installed languages: {e}"))?;
    let language = engine
        .RecognizerLanguage()
        .and_then(|l| l.LanguageTag())
        .map_err(|e| e.to_string())?
        .to_string();
    let limit = OcrEngine::MaxImageDimension().map_err(|e| e.to_string())?;
    let (mut pixels, scaled) = prepared(image, limit)?;
    for p in pixels.pixels_mut() {
        p.0.swap(0, 2);
    }
    let writer = DataWriter::new().map_err(|e| e.to_string())?;
    writer
        .WriteBytes(pixels.as_raw())
        .map_err(|e| e.to_string())?;
    let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
        &writer.DetachBuffer().map_err(|e| e.to_string())?,
        BitmapPixelFormat::Bgra8,
        pixels.width() as i32,
        pixels.height() as i32,
    )
    .map_err(|e| e.to_string())?;
    let operation = engine.RecognizeAsync(&bitmap).map_err(|e| e.to_string())?;
    let (tx, rx) = mpsc::channel();
    operation
        .when(move |r| {
            let _ = tx.send(r);
        })
        .map_err(|e| e.to_string())?;
    let result = match rx.recv_timeout(timeout) {
        Ok(r) => r.map_err(|e| e.to_string())?,
        Err(_) => {
            let _ = operation.Cancel();
            return Err("Windows text recognition timed out; the image is unchanged".into());
        }
    };
    let lines = result.Lines().map_err(|e| e.to_string())?;
    let mut text = String::new();
    let mut spans = Vec::new();
    let angle = result.TextAngle().and_then(|a| a.Value()).unwrap_or(0.);
    let scale = [
        image.width() as f32 / pixels.width() as f32,
        image.height() as f32 / pixels.height() as f32,
    ];
    for i in 0..lines.Size().map_err(|e| e.to_string())? {
        let source = lines.GetAt(i).map_err(|e| e.to_string())?;
        let line = source.Text().map_err(|e| e.to_string())?.to_string();
        if !text.is_empty() {
            text.push('\n');
        }
        let start = text.len();
        text.push_str(&line);
        // Steps captions deliberately bypass this work. Full results infer one
        // representative appearance per line, with a bounded 64-line budget.
        if formatting
            && i < 64
            && angle.abs() < 2.
            && let Ok(words) = source.Words()
        {
            let mut candidates = Vec::new();
            for j in 0..words.Size().unwrap_or(0) {
                if let Ok(word) = words.GetAt(j)
                    && let (Ok(value), Ok(b)) = (word.Text(), word.BoundingRect())
                {
                    let value = value.to_string();
                    if (3..=48).contains(&value.chars().count()) {
                        candidates.push((value, b));
                    }
                }
            }
            candidates.sort_by_key(|(word, _)| std::cmp::Reverse(word.len()));
            for (word, b) in candidates.into_iter().take(2) {
                if let Some(appearance) = capture_core::text::recognition::estimate(
                    image,
                    &word,
                    [
                        b.X * scale[0],
                        b.Y * scale[1],
                        b.Width * scale[0],
                        b.Height * scale[1],
                    ],
                ) {
                    spans.push(formatting::Span {
                        range: start..text.len(),
                        appearance,
                    });
                    break;
                }
            }
        }
        if text.len() > 131072 {
            return Err("Recognized text exceeds the result limit; select a smaller area".into());
        }
    }
    Ok(Recognized {
        text,
        language,
        scaled,
        spans,
    })
}
#[cfg(not(windows))]
pub(crate) fn recognize(_image: &image::RgbaImage) -> Result<Recognized, String> {
    Err("Local text recognition currently requires Windows".into())
}

#[cfg(test)]
mod tests;
