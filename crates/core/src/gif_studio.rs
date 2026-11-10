//! Project-owned GIF ranges and bounded, streaming animated-GIF encoding.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Range {
    pub id: u64,
    pub name: String,
    pub start: u64,
    pub end: u64,
    pub settings: Settings,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub width: u32,
    pub fps: u32,
    pub colors: u16,
    /// Palette sampling effort: 1 is most accurate; 30 is fastest.
    pub speed: i32,
    pub repeat: bool,
    pub background: [u8; 3],
}
impl Default for Settings {
    fn default() -> Self {
        Self::preset(1)
    }
}
impl Settings {
    pub const PRESETS: [&'static str; 4] = [
        "Small / sharing",
        "Balanced",
        "Crisp instructions",
        "Smooth motion",
    ];
    pub fn preset(i: usize) -> Self {
        let (width, fps, colors, speed) = match i {
            0 => (480, 10, 128, 10),
            2 => (960, 15, 256, 1),
            3 => (1080, 25, 256, 3),
            _ => (720, 15, 256, 5),
        };
        Self {
            width,
            fps,
            colors,
            speed,
            repeat: true,
            background: [255; 3],
        }
    }
    pub fn validate(&self) -> Result<()> {
        if !(64..=1920).contains(&self.width)
            || !(1..=50).contains(&self.fps)
            || !(2..=256).contains(&self.colors)
            || !(1..=30).contains(&self.speed)
        {
            return Err(Error::Invalid(
                "Use width 64–1920 px, 1–50 fps, 2–256 colors, and palette speed 1–30.".into(),
            ));
        }
        Ok(())
    }
    pub fn dimensions(&self, source: [u32; 2]) -> [u32; 2] {
        let width = self.width.min(source[0]).max(1);
        [
            width,
            ((source[1] as u64 * width as u64 + source[0].max(1) as u64 / 2)
                / source[0].max(1) as u64)
                .clamp(1, 1920) as u32,
        ]
    }
}
impl Range {
    pub fn validate(&self) -> Result<()> {
        self.settings.validate()?;
        if self.id == 0
            || self.name.trim().is_empty()
            || self.name.len() > 120
            || self.name.chars().any(char::is_control)
            || self.start >= self.end
            || self.end > 3_600_000
            || self.end - self.start > 300_000
        {
            return Err(Error::Invalid("Name each GIF (1–120 characters) and set an end after its start, with at most five minutes per GIF.".into()));
        }
        Ok(())
    }
    pub fn validate_export(&self, duration: u64) -> Result<()> {
        self.validate()?;
        if self.end > duration {
            return Err(Error::Invalid(format!(
                "‘{}’ extends beyond Project End. Shorten its end time before exporting.",
                self.name
            )));
        }
        Ok(())
    }
    pub fn frames(&self) -> u64 {
        ((self.end - self.start) * self.settings.fps as u64).div_ceil(1000)
    }
    pub fn sample_ms(&self, i: u64) -> u64 {
        (self.start + i * 1000 / self.settings.fps as u64).min(self.end - 1)
    }
    /// The source frame visible at this playback time, including GIF's 10 ms clock.
    pub fn preview_ms(&self, ms: u64) -> u64 {
        let cs = ms
            .clamp(self.start, self.end - 1)
            .saturating_sub(self.start)
            / 10;
        let mut lo = 0;
        let mut hi = self.frames();
        while lo + 1 < hi {
            let mid = (lo + hi) / 2;
            if (mid * 1000 / self.settings.fps as u64 + 5) / 10 <= cs {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        self.sample_ms(lo)
    }
    /// Cumulative centisecond rounding avoids drift at 15/30 fps.
    pub fn delay_cs(&self, i: u64) -> u16 {
        let end = ((i + 1) * 1000 / self.settings.fps as u64).min(self.end - self.start);
        let start = i * 1000 / self.settings.fps as u64;
        ((end + 5) / 10).saturating_sub((start + 5) / 10).max(1) as u16
    }
}

fn palette_frame(
    pixels: &image::RgbaImage,
    settings: &Settings,
    size: [u32; 2],
) -> gif::Frame<'static> {
    let [w, h] = size;
    let mut pixels =
        image::imageops::resize(pixels, w, h, image::imageops::FilterType::Triangle).into_raw();
    for p in pixels.as_chunks_mut::<4>().0 {
        for (c, bg) in settings.background.iter().enumerate() {
            p[c] =
                ((p[c] as u32 * p[3] as u32 + *bg as u32 * (255 - p[3] as u32) + 127) / 255) as u8;
        }
        p[3] = 255;
    }
    let quant = color_quant::NeuQuant::new(settings.speed, settings.colors as usize, &pixels);
    let indices: Vec<u8> = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| quant.index_of(p) as u8)
        .collect();
    gif::Frame::from_palette_pixels(w as u16, h as u16, indices, quant.color_map_rgb(), None)
}
/// Uses exactly the export palette and scaling; preview does not alter project pixels.
pub fn preview(pixels: &image::RgbaImage, settings: &Settings) -> Result<image::RgbaImage> {
    settings.validate()?;
    let size = settings.dimensions([pixels.width(), pixels.height()]);
    let f = palette_frame(pixels, settings, size);
    let palette = f.palette.unwrap();
    Ok(image::RgbaImage::from_fn(size[0], size[1], |x, y| {
        let i = f.buffer[(y * size[0] + x) as usize] as usize * 3;
        image::Rgba([palette[i], palette[i + 1], palette[i + 2], 255])
    }))
}

#[derive(Clone, Debug)]
pub struct Estimate {
    pub bytes: u64,
    pub low: u64,
    pub high: u64,
    pub sampled: u64,
    pub frames: u64,
}
impl Estimate {
    pub fn exact(&self) -> bool {
        self.sampled == self.frames
    }
}
#[derive(Default)]
struct ByteCounter(u64);
impl std::io::Write for ByteCounter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 += bytes.len() as u64;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
/// Measure real palette + LZW sizes at evenly spread frames. The exporter writes
/// independent full frames, so extrapolation needs no assumed compression ratio.
/// At most twelve frames are decoded; no file is created and no frames retained.
pub fn estimate(
    range: &Range,
    source_size: [u32; 2],
    mut frame: impl FnMut(u64) -> std::result::Result<image::RgbaImage, String>,
    mut cancelled: impl FnMut() -> bool,
) -> std::result::Result<Estimate, String> {
    range.validate().map_err(|e| e.to_string())?;
    let frames = range.frames();
    let count = frames.min(12);
    let [w, h] = range.settings.dimensions(source_size);
    let mut encoder = gif::Encoder::new(ByteCounter::default(), w as u16, h as u16, &[])
        .map_err(|e| e.to_string())?;
    if range.settings.repeat {
        encoder
            .set_repeat(gif::Repeat::Infinite)
            .map_err(|e| e.to_string())?;
    }
    let header = encoder.get_ref().0;
    let mut sizes = Vec::new();
    for n in 0..count {
        if cancelled() {
            return Err("Estimate cancelled".into());
        }
        let i = if count == 1 {
            0
        } else {
            n * (frames - 1) / (count - 1)
        };
        let pixels = frame(range.sample_ms(i))?;
        let mut f = palette_frame(&pixels, &range.settings, [w, h]);
        f.delay = range.delay_cs(i);
        f.dispose = gif::DisposalMethod::Keep;
        if cancelled() {
            return Err("Estimate cancelled".into());
        }
        let before = encoder.get_ref().0;
        encoder.write_frame(&f).map_err(|e| e.to_string())?;
        sizes.push(encoder.get_ref().0 - before);
    }
    let sum: u64 = sizes.iter().sum();
    let end = encoder.into_inner().map_err(|e| e.to_string())?.0;
    let overhead = end - sum;
    debug_assert!(overhead >= header);
    let bytes = overhead + (sum * frames + count / 2) / count;
    let (low, high) = if count == frames {
        (bytes, bytes)
    } else {
        // A content-based range, deliberately not a confidence guarantee.
        (
            overhead + sizes.iter().min().unwrap() * frames * 9 / 10,
            overhead + sizes.iter().max().unwrap() * frames * 11 / 10,
        )
    };
    Ok(Estimate {
        bytes,
        low,
        high,
        sampled: count,
        frames,
    })
}

pub fn encode(
    range: &Range,
    source_size: [u32; 2],
    writer: impl std::io::Write,
    mut frame: impl FnMut(u64) -> std::result::Result<image::RgbaImage, String>,
    mut progress: impl FnMut(f32) -> std::result::Result<(), String>,
) -> std::result::Result<(), String> {
    range.validate().map_err(|e| e.to_string())?;
    let [w, h] = range.settings.dimensions(source_size);
    let mut encoder =
        gif::Encoder::new(writer, w as u16, h as u16, &[]).map_err(|e| e.to_string())?;
    if range.settings.repeat {
        encoder
            .set_repeat(gif::Repeat::Infinite)
            .map_err(|e| e.to_string())?;
    }
    for i in 0..range.frames() {
        progress(i as f32 / range.frames() as f32)?;
        let pixels = frame(range.sample_ms(i))?;
        let mut f = palette_frame(&pixels, &range.settings, [w, h]);
        f.delay = range.delay_cs(i);
        f.dispose = gif::DisposalMethod::Keep;
        encoder.write_frame(&f).map_err(|e| e.to_string())?;
    }
    progress(1.)?;
    encoder.into_inner().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn range() -> Range {
        Range {
            id: 1,
            name: "Demo".into(),
            start: 1000,
            end: 2000,
            settings: Settings {
                width: 64,
                ..Default::default()
            },
        }
    }
    #[test]
    fn comparison_pixels_custom_palette_and_estimates_match_encoded_output() {
        for colors in [2, 48, 256] {
            let mut r = range();
            r.settings.colors = colors;
            r.settings.fps = 10;
            r.end = r.start + 400;
            let frame = |ms| {
                Ok(image::RgbaImage::from_fn(128, 64, |x, y| {
                    image::Rgba([
                        ((x * 7 + ms as u32) % 256) as u8,
                        (y * 4) as u8,
                        (x + y) as u8,
                        255,
                    ])
                }))
            };
            let estimate = estimate(&r, [128, 64], frame, || false).unwrap();
            let mut bytes = vec![];
            encode(&r, [128, 64], &mut bytes, frame, |_| Ok(())).unwrap();
            assert_eq!(estimate.bytes, bytes.len() as u64);
            assert!(estimate.exact());
            let mut options = gif::DecodeOptions::new();
            options.set_color_output(gif::ColorOutput::RGBA);
            let mut decoder = options.read_info(std::io::Cursor::new(bytes)).unwrap();
            let mut elapsed = 0;
            for i in 0..r.frames() {
                let decoded = decoder.read_next_frame().unwrap().unwrap();
                let original = frame(r.sample_ms(i)).unwrap();
                let preview = preview(&original, &r.settings).unwrap();
                assert_eq!(decoded.buffer.as_ref(), preview.as_raw());
                assert_eq!(r.preview_ms(r.start + elapsed), r.sample_ms(i));
                elapsed += decoded.delay as u64 * 10;
            }
        }
    }
    #[test]
    fn representative_estimates_are_bounded_cancellable_and_stale_ranges_fail() {
        let mut r = range();
        r.end = 11000;
        let mut calls = 0;
        let estimate = estimate(
            &r,
            [64, 64],
            |ms| {
                calls += 1;
                Ok(image::RgbaImage::from_pixel(
                    64,
                    64,
                    image::Rgba([(ms / 10) as u8, 80, 120, 255]),
                ))
            },
            || false,
        )
        .unwrap();
        assert_eq!(calls, 12);
        assert!(!estimate.exact());
        assert!(estimate.low <= estimate.bytes && estimate.bytes <= estimate.high);
        assert!(
            super::estimate(
                &r,
                [64, 64],
                |_| panic!("Cancelled work must not decode"),
                || true
            )
            .is_err()
        );
        for fps in [15, 30, 50] {
            r.settings.fps = fps;
            let mut elapsed = 0;
            for i in 0..r.frames() {
                assert_eq!(r.preview_ms(r.start + elapsed), r.sample_ms(i));
                elapsed += r.delay_cs(i) as u64 * 10;
            }
        }
    }
    #[test]
    fn encoded_frames_have_correct_trim_timing_size_colors_and_looping() {
        let r = range();
        let mut bytes = vec![];
        let mut samples = vec![];
        encode(
            &r,
            [128, 64],
            &mut bytes,
            |ms| {
                samples.push(ms);
                Ok(image::RgbaImage::from_pixel(
                    128,
                    64,
                    image::Rgba([
                        if ms < 1500 { 255 } else { 0 },
                        0,
                        if ms < 1500 { 0 } else { 255 },
                        255,
                    ]),
                ))
            },
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(samples.len(), 15);
        assert_eq!(samples[0], 1000);
        assert!(samples.last().unwrap() < &2000);
        let mut options = gif::DecodeOptions::new();
        options.set_color_output(gif::ColorOutput::RGBA);
        let mut decoder = options.read_info(std::io::Cursor::new(&bytes)).unwrap();
        assert_eq!((decoder.width(), decoder.height()), (64, 32));
        let mut delay = 0;
        let mut count = 0;
        while let Some(frame) = decoder.read_next_frame().unwrap() {
            delay += frame.delay as u64;
            assert_eq!(frame.buffer.len(), 64 * 32 * 4);
            if count == 0 {
                assert!(frame.buffer[0] > 240);
            }
            count += 1;
        }
        assert_eq!(delay, 100);
        assert_eq!(count, 15);
        assert!(bytes.windows(11).any(|w| w == b"NETSCAPE2.0"));
    }
    #[test]
    fn ranges_persist_without_extending_project_and_survive_undo() {
        let mut d = crate::Document::blank(320, 180).unwrap();
        d.video = Some(crate::video::editor::Project::new(
            "local.mp4".into(),
            [320, 180],
            3000,
            30,
        ));
        d.video.as_mut().unwrap().gif_ranges = vec![range()];
        d.promote_features();
        d.validate().unwrap();
        let mut s = crate::Session::new(d.clone()).unwrap();
        let mut after = d.clone();
        after.video.as_mut().unwrap().gif_ranges[0].end = 2500;
        s.execute(crate::Command::Video {
            project: after.video.clone().unwrap(),
        })
        .unwrap();
        s.execute(crate::Command::Undo).unwrap();
        assert_eq!(s.document.video, d.video);
        let json = serde_json::to_string(&after).unwrap();
        let read: crate::Document = serde_json::from_str(&json).unwrap();
        assert_eq!(read.video, after.video);
        assert_eq!(read.video.unwrap().duration(), 3000);
        let mut r = range();
        r.end = 4000;
        assert!(r.validate().is_ok());
        assert!(
            r.validate_export(3000)
                .unwrap_err()
                .to_string()
                .contains("Shorten")
        );
    }
    #[test]
    fn cancelled_encoding_stops_before_requesting_more_frames_and_bad_ranges_rejected() {
        let r = range();
        let mut calls = 0;
        let err = encode(
            &r,
            [128, 64],
            Vec::new(),
            |_| {
                calls += 1;
                Ok(image::RgbaImage::new(128, 64))
            },
            |_| Err("cancelled".into()),
        )
        .unwrap_err();
        assert_eq!(calls, 0);
        assert_eq!(err, "cancelled");
        for (start, end) in [(5, 5), (6, 5), (0, 300001)] {
            let mut r = range();
            r.start = start;
            r.end = end;
            assert!(r.validate().is_err());
        }
    }
}
