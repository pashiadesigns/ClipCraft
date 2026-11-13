//! Windows installed codecs. Each decoder/encoder stays on its owning worker.
use capture_core::video::editor::Project;
use std::{os::windows::ffi::OsStrExt, path::Path};
use windows::{
    Win32::{
        Media::{Audio::*, MediaFoundation::*},
        System::{Com::StructuredStorage::PROPVARIANT, Com::*},
    },
    core::{GUID, PCWSTR},
};
pub type Result<T> = std::result::Result<T, String>;
pub(crate) use crate::capture::windows::video_encoder::{Encoder, Runtime};
fn err(e: windows::core::Error) -> String {
    format!("Windows media: {e}")
}
fn reader(path: &Path) -> Result<IMFSourceReader> {
    let text = path.to_string_lossy();
    let local = text.strip_prefix(r"\\?\").unwrap_or(&text);
    let drive = local.as_bytes();
    if local.contains("://")
        || local.starts_with(r"\\")
        || (text.starts_with(r"\\?\")
            && !(drive.len() >= 3
                && drive[0].is_ascii_alphabetic()
                && drive[1] == b':'
                && drive[2] == b'\\'))
        || !path.is_file()
    {
        return Err(format!(
            "Cannot open local media {}. Locate the original file or restore the project's media folder.",
            path.display()
        ));
    }
    unsafe {
        let mut attrs = None;
        MFCreateAttributes(&mut attrs, 2).map_err(err)?;
        let attrs = attrs.unwrap();
        attrs
            .SetUINT32(&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1)
            .map_err(err)?;
        let path: Vec<u16> = std::ffi::OsStr::new(local)
            .encode_wide()
            .chain(Some(0))
            .collect();
        MFCreateSourceReaderFromURL(PCWSTR(path.as_ptr()), &attrs).map_err(err)
    }
}
fn seek(reader: &IMFSourceReader, ms: u64) -> Result<()> {
    unsafe {
        reader
            .SetCurrentPosition(&GUID::zeroed(), &PROPVARIANT::from((ms * 10_000) as i64))
            .map_err(err)
    }
}
fn sample(reader: &IMFSourceReader, stream: u32) -> Result<Option<(u64, Vec<u8>)>> {
    unsafe {
        for _ in 0..4096 {
            let (mut sample, mut flags, mut time) = (None, 0, 0);
            reader
                .ReadSample(
                    stream,
                    0,
                    None,
                    Some(&mut flags),
                    Some(&mut time),
                    Some(&mut sample),
                )
                .map_err(err)?;
            if let Some(sample) = sample {
                let b = sample.ConvertToContiguousBuffer().map_err(err)?;
                let (mut p, mut len) = (std::ptr::null_mut(), 0);
                b.Lock(&mut p, None, Some(&mut len)).map_err(err)?;
                if len > 256 * 1024 * 1024 {
                    let _ = b.Unlock();
                    return Err("Decoded media sample is too large.".into());
                }
                let bytes = std::slice::from_raw_parts(p, len as usize).to_vec();
                b.Unlock().map_err(err)?;
                return Ok(Some((time.max(0) as u64, bytes)));
            }
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                return Ok(None);
            }
        }
        Err("Media did not return a usable sample.".into())
    }
}
pub struct Video {
    reader: IMFSourceReader,
    pub size: [u32; 2],
    pub duration: u64,
    pub fps: u32,
    pub has_audio: bool,
    last: Option<(u64, std::sync::Arc<image::RgbaImage>)>,
}
impl Video {
    pub fn open(path: &Path) -> Result<Self> {
        let reader = reader(path)?;
        unsafe {
            let v = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
            let media = reader.GetNativeMediaType(v, 0).map_err(err)?;
            let size = media.GetUINT64(&MF_MT_FRAME_SIZE).map_err(err)?;
            let size = [(size >> 32) as u32, size as u32];
            if size.contains(&0)
                || size.iter().any(|n| *n > 8192)
                || size[0] as u64 * size[1] as u64 > capture_core::MAX_PIXELS
            {
                return Err("Video dimensions exceed the supported 25 megapixels.".into());
            }
            let rate = media
                .GetUINT64(&MF_MT_FRAME_RATE)
                .unwrap_or((30u64 << 32) | 1);
            let fps = ((rate >> 32) as f64 / (rate as u32).max(1) as f64).round() as u32;
            let fps = if fps > 45 {
                60
            } else if fps < 23 {
                15
            } else {
                30
            };
            let duration = u64::try_from(
                &reader
                    .GetPresentationAttribute(
                        MF_SOURCE_READER_MEDIASOURCE.0 as u32,
                        &MF_PD_DURATION,
                    )
                    .map_err(err)?,
            )
            .map_err(err)?
                / 10_000;
            if duration == 0 || duration > 3_600_000 {
                return Err("Choose a video between one frame and one hour long.".into());
            }
            let has_audio = reader
                .GetNativeMediaType(MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32, 0)
                .is_ok();
            reader
                .SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)
                .map_err(err)?;
            reader.SetStreamSelection(v, true).map_err(err)?;
            let output = MFCreateMediaType().map_err(err)?;
            output
                .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
                .map_err(err)?;
            output
                .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)
                .map_err(err)?;
            output
                .SetUINT64(&MF_MT_FRAME_SIZE, ((size[0] as u64) << 32) | size[1] as u64)
                .map_err(err)?;
            reader.SetCurrentMediaType(v, None, &output).map_err(err)?;
            Ok(Self {
                reader,
                size,
                duration,
                fps,
                has_audio,
                last: None,
            })
        }
    }
    pub fn frame(&mut self, ms: u64) -> Result<image::RgbaImage> {
        self.frame_shared(ms).map(|p| (*p).clone())
    }
    pub fn frame_shared(&mut self, ms: u64) -> Result<std::sync::Arc<image::RgbaImage>> {
        let ms = ms.min(self.duration.saturating_sub(1));
        let sequential = self
            .last
            .as_ref()
            .is_some_and(|(t, _)| ms >= *t && ms < t + 1000);
        if let Some((t, p)) = &self.last
            && ms >= *t
            && ms < t + 1000 / self.fps as u64
        {
            return Ok(p.clone());
        }
        if !sequential {
            seek(&self.reader, ms)?;
            self.last = None;
        }
        let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
        for _ in 0..4096 {
            let Some((time, bytes)) = sample(&self.reader, stream)? else {
                break;
            };
            unsafe {
                let media = self.reader.GetCurrentMediaType(stream).map_err(err)?;
                let dims = media.GetUINT64(&MF_MT_FRAME_SIZE).map_err(err)?;
                if [(dims >> 32) as u32, dims as u32] != self.size {
                    return Err(format!(
                        "Video dimensions changed from {:?} to {:?}. Convert it to a fixed-size MP4 first.",
                        self.size,
                        [(dims >> 32) as u32, dims as u32]
                    ));
                }
                let stride = media
                    .GetUINT32(&MF_MT_DEFAULT_STRIDE)
                    .unwrap_or(self.size[0] * 4) as i32;
                let stride_abs = stride.unsigned_abs() as usize;
                if stride_abs < self.size[0] as usize * 4
                    || bytes.len() < stride_abs * self.size[1] as usize
                {
                    return Err("Decoder returned an incomplete video frame.".into());
                }
                let width = self.size[0] as usize * 4;
                let height = self.size[1] as usize;
                let mut pixels = vec![0u8; width * height];
                for (y, target) in pixels.chunks_exact_mut(width).enumerate() {
                    let row = if stride < 0 { height - 1 - y } else { y };
                    let start = row * stride_abs;
                    target.copy_from_slice(&bytes[start..start + width]);
                    for p in target.as_chunks_mut::<4>().0 {
                        *p = [p[2], p[1], p[0], 255];
                    }
                }
                let rgba = image::RgbaImage::from_raw(self.size[0], self.size[1], pixels)
                    .ok_or("Decoder returned an incomplete video frame.")?;
                let at = time / 10_000;
                self.last = Some((at, std::sync::Arc::new(rgba)));
                if at + 1000 / self.fps as u64 > ms {
                    return Ok(self.last.as_ref().unwrap().1.clone());
                }
            }
        }
        self.last
            .as_ref()
            .map(|(_, p)| p.clone())
            .ok_or_else(|| "No decodable video frame at this position.".into())
    }
}
pub struct Sound {
    reader: IMFSourceReader,
    at: u64,
    data: Vec<i16>,
    eof: bool,
}
impl Sound {
    pub fn duration(&self) -> Result<u64> {
        unsafe {
            let n = u64::try_from(
                &self
                    .reader
                    .GetPresentationAttribute(
                        MF_SOURCE_READER_MEDIASOURCE.0 as u32,
                        &MF_PD_DURATION,
                    )
                    .map_err(err)?,
            )
            .map_err(err)?
                / 10_000;
            if n == 0 || n > 3_600_000 {
                return Err("Choose audio up to one hour long.".into());
            }
            Ok(n)
        }
    }
    pub fn open(path: &Path) -> Result<Self> {
        let reader = reader(path)?;
        unsafe {
            let a = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;
            reader
                .SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)
                .map_err(err)?;
            reader.SetStreamSelection(a, true).map_err(err)?;
            let mt = MFCreateMediaType().map_err(err)?;
            mt.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)
                .map_err(err)?;
            mt.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM)
                .map_err(err)?;
            mt.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)
                .map_err(err)?;
            mt.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, 48_000)
                .map_err(err)?;
            mt.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, 2).map_err(err)?;
            reader.SetCurrentMediaType(a, None, &mt).map_err(err)?;
        }
        Ok(Self {
            reader,
            at: 0,
            data: vec![],
            eof: false,
        })
    }
    pub(super) fn value(&mut self, frame: u64) -> Result<[i16; 2]> {
        if frame < self.at || frame > self.at + self.data.len() as u64 / 2 + 48_000 {
            seek(&self.reader, frame / 48)?;
            self.data.clear();
            self.eof = false;
            self.at = frame;
        }
        for _ in 0..4096 {
            let end = self.at + self.data.len() as u64 / 2;
            if frame >= self.at && frame < end {
                let i = (frame - self.at) as usize * 2;
                return Ok([self.data[i], self.data[i + 1]]);
            }
            if self.eof {
                return Ok([0, 0]);
            }
            if let Some((pts, bytes)) =
                sample(&self.reader, MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32)?
            {
                self.at = pts * 48_000 / 10_000_000;
                self.data = bytes
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|b| i16::from_le_bytes([b[0], b[1]]))
                    .collect();
                if frame < self.at {
                    return Ok([0, 0]);
                }
            } else {
                self.eof = true;
            }
        }
        Err("Audio decoder could not reach this position.".into())
    }
}
pub struct Mixer {
    pub spectrum: Option<(capture_core::video::effects::Target, u64)>,
    decoders: std::collections::BTreeMap<(u64, usize), Sound>,
    processors: std::collections::BTreeMap<
        capture_core::video::effects::Target,
        capture_core::video::effects::Processor,
    >,
    pub meters: std::collections::BTreeMap<
        capture_core::video::effects::Target,
        Vec<capture_core::video::effects::Meter>,
    >,
}
impl Mixer {
    pub fn new(_project: &Project) -> Result<Self> {
        Ok(Self {
            decoders: Default::default(),
            processors: Default::default(),
            meters: Default::default(),
            spectrum: None,
        })
    }
    pub fn samples(
        &mut self,
        project: &Project,
        start_frame: u64,
        count: usize,
    ) -> Result<Vec<i16>> {
        let lo = start_frame / 48;
        let hi = (start_frame + count as u64).div_ceil(48);
        let clips: Vec<_> = project
            .segments
            .iter()
            .filter(|c| {
                c.at < hi && lo < c.until() && !c.audio.is_empty() && !project.track_muted(c.track)
            })
            .collect();
        self.decoders
            .retain(|(id, _), _| clips.iter().any(|c| c.id == *id));
        for c in &clips {
            for (i, a) in c.audio.iter().enumerate().filter(|(_, a)| !a.muted) {
                let key = (c.id, i);
                if let std::collections::btree_map::Entry::Vacant(e) = self.decoders.entry(key) {
                    e.insert(Sound::open(Path::new(&a.path))?);
                }
            }
        }
        if count > 48_000 {
            return Err("Audio block exceeds one second.".into());
        }
        let mut tracks: std::collections::BTreeMap<u64, Vec<[f64; 2]>> =
            clips.iter().map(|c| (c.id, vec![[0.; 2]; count])).collect();
        use capture_core::video::effects::Target;
        self.processors.retain(|target, _| match target {
            Target::Clip(id) => clips.iter().any(|c| c.id == *id),
            Target::Track(id) => project.media_tracks.contains(id),
        });
        self.meters
            .retain(|target, _| self.processors.contains_key(target));
        for target in self.processors.keys() {
            if let Target::Clip(id) = target {
                tracks.entry(*id).or_insert_with(|| vec![[0.; 2]; count]);
            }
        }
        for i in 0..count {
            let at = (start_frame + i as u64) as f64 / 48.;
            let ms = at as u64;
            for c in &clips {
                if !c.active(ms) {
                    continue;
                }
                let age = at - c.at as f64;
                let left = c.until() as f64 - at;
                let base = project.opacity(c, ms);
                for (index, a) in c.audio.iter().enumerate().filter(|(_, a)| !a.muted) {
                    let gain = a.volume as f64
                        * base
                        * if a.fade_in == 0 {
                            1.
                        } else {
                            (age / a.fade_in as f64).min(1.)
                        }
                        * if a.fade_out == 0 {
                            1.
                        } else {
                            (left / a.fade_out as f64).min(1.)
                        };
                    let source = c.start as f64 + age * c.speed;
                    let value = self
                        .decoders
                        .get_mut(&(c.id, index))
                        .unwrap()
                        .value((source * 48.) as u64)?;
                    let sum = &mut tracks.get_mut(&c.id).unwrap()[i];
                    for ch in 0..2 {
                        sum[ch] += if c.mono {
                            (value[0] as f64 + value[1] as f64) / 65536. * gain
                        } else {
                            value[ch] as f64 / 32768. * gain
                        };
                    }
                }
            }
        }
        let mut buses: std::collections::BTreeMap<u64, Vec<[f64; 2]>> =
            std::collections::BTreeMap::new();
        for (id, mut values) in tracks {
            self.process_effects(project, Target::Clip(id), &mut values)?;
            if let Some(c) = project.segments.iter().find(|c| c.id == id) {
                let bus = buses.entry(c.track).or_insert_with(|| vec![[0.; 2]; count]);
                for (out, v) in bus.iter_mut().zip(values) {
                    for ch in 0..2 {
                        out[ch] += v[ch];
                    }
                }
            }
        }
        for target in self.processors.keys() {
            if let Target::Track(id) = target {
                buses.entry(*id).or_insert_with(|| vec![[0.; 2]; count]);
            }
        }
        let mut mixed = vec![[0.; 2]; count];
        for (id, mut values) in buses {
            self.process_effects(project, Target::Track(id), &mut values)?;
            let gain = if project.track_muted(id) {
                0.
            } else {
                project.media_levels.get(&id).copied().unwrap_or(1.) as f64
            };
            for (out, v) in mixed.iter_mut().zip(values) {
                for ch in 0..2 {
                    out[ch] += v[ch] * gain;
                }
            }
        }
        let out = mixed
            .into_iter()
            .flatten()
            .map(|v| (v * 32768.).round().clamp(-32768., 32767.) as i16)
            .collect();
        Ok(out)
    }
    fn process_effects(
        &mut self,
        p: &Project,
        target: capture_core::video::effects::Target,
        values: &mut [[f64; 2]],
    ) -> Result<()> {
        if let Some(chain) = p.effect_chain(target) {
            if let std::collections::btree_map::Entry::Vacant(e) = self.processors.entry(target) {
                e.insert(
                    capture_core::video::effects::Processor::new(chain)
                        .map_err(|e| e.to_string())?,
                );
            }
            let processor = self.processors.get_mut(&target).unwrap();
            processor.reconfigure(chain).map_err(|e| e.to_string())?;
            processor.set_visualization(
                self.spectrum
                    .filter(|(t, _)| *t == target)
                    .map(|(_, id)| id),
            );
            processor.process(values);
            self.meters.insert(target, processor.meters.clone());
        } else {
            self.processors.remove(&target);
            self.meters.remove(&target);
        }
        Ok(())
    }
}
/// Bounded decoder cache shared by live preview, still export and MP4 export.
/// Only active visual clips are decoded; source pixels are independent of canvas zoom.
pub struct Visuals {
    pub preview: bool,
    frames: capture_core::video::frame_cache::PreviewCache,
    videos: std::collections::BTreeMap<(u64, String), Video>,
    images: std::collections::BTreeMap<String, std::sync::Arc<image::RgbaImage>>,
}
impl Visuals {
    #[cfg(test)]
    pub(crate) fn cached_decoders(&self) -> usize {
        self.videos.len()
    }
    pub fn new() -> Self {
        Self {
            preview: false,
            frames: Default::default(),
            videos: Default::default(),
            images: Default::default(),
        }
    }
    /// Decode source frames only. Live crop, masking, opacity and ordering are
    /// composed by the retained canvas renderer instead of rasterizing the entire
    /// project on the decoding worker for every property change.
    pub fn sources(
        &mut self,
        p: &Project,
        time: u64,
    ) -> Result<Vec<(u64, std::sync::Arc<capture_core::bitmap::Bitmap>)>> {
        use capture_core::video::editor::MediaKind;
        let active: Vec<_> = p
            .segments
            .iter()
            .filter(|c| c.active(time) && !c.hidden && c.kind() != MediaKind::Audio)
            .collect();
        // Retain a small decoder cache across audio-only gaps and clip edges.
        // Scrubbing back must not reopen the codec on every boundary crossing.
        self.videos.retain(|(id, path), _| {
            p.segments
                .iter()
                .any(|c| c.id == *id && c.source.as_ref().map_or(&p.source, |s| &s.path) == path)
        });
        if self.videos.len() > 8 {
            self.videos
                .retain(|(id, _), _| active.iter().any(|c| c.id == *id));
        }
        // Pictures that start within the next two seconds are decoded now,
        // so a crossfade into them has both pictures from its first frame
        // (BW15).
        let upcoming: Vec<&String> = p
            .segments
            .iter()
            .filter(|c| {
                c.kind() == MediaKind::Image
                    && !c.hidden
                    && c.at > time
                    && c.at <= time.saturating_add(2000)
            })
            .filter_map(|c| c.source.as_ref().map(|s| &s.path))
            .collect();
        self.images.retain(|path, _| {
            upcoming.contains(&path)
                || active
                    .iter()
                    .any(|c| c.source.as_ref().map_or(&p.source, |s| &s.path) == path)
        });
        for path in upcoming {
            if !self.images.contains_key(path)
                && let Ok(pixels) =
                    capture_core::Document::open_image(Path::new(path)).and_then(|d| d.render())
            {
                self.images
                    .insert(path.clone(), std::sync::Arc::new(pixels));
            }
        }
        active
            .into_iter()
            .map(|c| {
                let path = c.source.as_ref().map_or(&p.source, |s| &s.path);
                let pixels = if c.kind() == MediaKind::Image {
                    if !self.images.contains_key(path) {
                        self.images.insert(
                            path.clone(),
                            std::sync::Arc::new(
                                capture_core::Document::open_image(Path::new(path))
                                    .and_then(|d| d.render())
                                    .map_err(|e| e.to_string())?,
                            ),
                        );
                    }
                    self.images[path].clone()
                } else {
                    let key = (c.id, path.clone());
                    if !self.videos.contains_key(&key) {
                        self.videos
                            .insert(key.clone(), Video::open(Path::new(path))?);
                    }
                    self.videos
                        .get_mut(&key)
                        .unwrap()
                        .frame_shared(c.source_time(time))?
                };
                // Filters and color are drawn here, off the window thread; the
                // canvas then draws the frame's shape as usual.
                let pixels = match c.frame.as_ref().filter(|f| !f.color.is_neutral()) {
                    Some(f) => {
                        let mut colored = (*pixels).clone();
                        f.color.apply(&mut colored);
                        std::sync::Arc::new(colored)
                    }
                    None => pixels,
                };
                Ok((
                    c.id,
                    std::sync::Arc::new(
                        capture_core::bitmap::Bitmap::transient(pixels)
                            .map_err(|e| e.to_string())?,
                    ),
                ))
            })
            .collect()
    }
    pub fn layers(
        &mut self,
        p: &Project,
        size: [u32; 2],
        time: u64,
    ) -> Result<Vec<(u64, std::sync::Arc<capture_core::bitmap::Bitmap>)>> {
        use capture_core::video::editor::MediaKind;
        let sampled = p.sampled_media(time);
        let p = sampled.as_ref();
        let mut layers = vec![];
        let active: Vec<_> = p
            .segments
            .iter()
            .filter(|c| c.active(time) && !c.hidden && c.kind() != MediaKind::Audio)
            .collect();
        let paths: Vec<_> = active
            .iter()
            .map(|c| {
                c.source
                    .as_ref()
                    .map_or(p.source.as_str(), |s| s.path.as_str())
            })
            .collect();
        self.videos.retain(|(id, path), _| {
            active.iter().any(|c| c.id == *id) && paths.contains(&path.as_str())
        });
        self.images.retain(|path, _| paths.contains(&path.as_str()));
        for track in p.media_tracks.iter().rev() {
            if !active.iter().any(|c| c.track == *track) {
                continue;
            }
            let mut pixels = Vec::<f32>::new();
            let mut direct = None;
            for c in active.iter().filter(|c| c.track == *track) {
                let path = c.source.as_ref().map_or(&p.source, |s| &s.path);
                let frame = if c.kind() == MediaKind::Image {
                    if !self.images.contains_key(path) {
                        let document = capture_core::Document::open_image(Path::new(path))
                            .map_err(|e| e.to_string())?;
                        let decoded = document.render().map_err(|e| e.to_string())?;
                        self.images
                            .insert(path.clone(), std::sync::Arc::new(decoded));
                    }
                    self.images[path].clone()
                } else {
                    let key = (c.id, path.clone());
                    if !self.videos.contains_key(&key) {
                        self.videos
                            .insert(key.clone(), Video::open(Path::new(path))?);
                    }
                    self.videos
                        .get_mut(&key)
                        .unwrap()
                        .frame_shared(c.source_time(time))?
                };
                if active.iter().filter(|a| a.track == *track).count() == 1
                    && c.frame
                        .as_ref()
                        .is_none_or(capture_core::video::frame::Frame::unmodified_pixels)
                    && c.rect == [0., 0., 1., 1.]
                    && frame.dimensions() == (size[0], size[1])
                    && (c.source.is_some() || p.crop == [0, 0, size[0], size[1]])
                {
                    direct = Some(std::sync::Arc::new(
                        capture_core::bitmap::Bitmap::transient(frame.clone())
                            .map_err(|e| e.to_string())?
                            .with_frame_opacity((p.opacity(c, time) * 255.).round() as u8),
                    ));
                    break;
                }
                if active.iter().filter(|a| a.track == *track).count() == 1
                    && let Some(style) = &c.frame
                {
                    let rendered = if self.preview {
                        self.frames.render(style, frame.clone(), c.rect, size, c.id)
                    } else {
                        self.frames
                            .render_steady(style, frame.clone(), c.rect, size, c.id)
                    }
                    .map_err(|e| e.to_string())?;
                    direct = Some(std::sync::Arc::new(
                        capture_core::bitmap::Bitmap::transient(std::sync::Arc::new(rendered))
                            .map_err(|e| e.to_string())?
                            .with_frame_opacity((p.opacity(c, time) * 255.).round() as u8),
                    ));
                    break;
                }
                if pixels.is_empty() {
                    pixels.resize(size[0] as usize * size[1] as usize * 4, 0.);
                }
                if c.frame
                    .as_ref()
                    .is_none_or(capture_core::video::frame::Frame::unmodified_pixels)
                    && c.rect == [0., 0., 1., 1.]
                    && frame.dimensions() == (size[0], size[1])
                    && (c.source.is_some() || p.crop == [0, 0, size[0], size[1]])
                {
                    let opacity = p.opacity(c, time) as f32;
                    for (dst, src) in pixels
                        .as_chunks_mut::<4>()
                        .0
                        .iter_mut()
                        .zip(frame.as_raw().as_chunks::<4>().0)
                    {
                        let alpha = opacity * src[3] as f32 / 255.;
                        for channel in 0..3 {
                            dst[channel] += src[channel] as f32 * alpha;
                        }
                        dst[3] += alpha;
                    }
                    continue;
                }
                if let Some(style) = &c.frame {
                    let rendered = if self.preview {
                        self.frames.render(style, frame.clone(), c.rect, size, c.id)
                    } else {
                        self.frames
                            .render_steady(style, frame.clone(), c.rect, size, c.id)
                    }
                    .map_err(|e| e.to_string())?;
                    let opacity = p.opacity(c, time) as f32;
                    for (i, rgba) in rendered.pixels().enumerate() {
                        let alpha = opacity * rgba[3] as f32 / 255.;
                        for ch in 0..3 {
                            pixels[i * 4 + ch] += rgba[ch] as f32 * alpha;
                        }
                        pixels[i * 4 + 3] += alpha;
                    }
                    continue;
                }
                let (fw, fh) = frame.dimensions();
                let [x, y, w, h] = if c.source.is_none() {
                    p.crop
                } else {
                    [0, 0, fw, fh]
                };
                if w == 0 || h == 0 || x + w > fw || y + h > fh {
                    return Err("Media dimensions changed. Replace the source file.".into());
                }
                let dest = [
                    (c.rect[0] * size[0] as f32).round() as i64,
                    (c.rect[1] * size[1] as f32).round() as i64,
                    (c.rect[2] * size[0] as f32).round().max(1.) as i64,
                    (c.rect[3] * size[1] as f32).round().max(1.) as i64,
                ];
                let scale = if p.fill {
                    (dest[2] as f64 / w as f64).max(dest[3] as f64 / h as f64)
                } else {
                    (dest[2] as f64 / w as f64).min(dest[3] as f64 / h as f64)
                };
                let rw = (w as f64 * scale).round().max(1.) as u32;
                let rh = (h as f64 * scale).round().max(1.) as u32;
                let crop = image::imageops::crop_imm(frame.as_ref(), x, y, w, h).to_image();
                let fitted =
                    image::imageops::resize(&crop, rw, rh, image::imageops::FilterType::Triangle);
                let ox = dest[0] + (dest[2] - rw as i64) / 2;
                let oy = dest[1] + (dest[3] - rh as i64) / 2;
                let opacity = p.opacity(c, time) as f32;
                for yy in 0..dest[3] {
                    for xx in 0..dest[2] {
                        let dx = dest[0] + xx;
                        let dy = dest[1] + yy;
                        let sx = dx - ox;
                        let sy = dy - oy;
                        if dx < 0
                            || dy < 0
                            || dx >= size[0] as i64
                            || dy >= size[1] as i64
                            || sx < 0
                            || sy < 0
                            || sx >= rw as i64
                            || sy >= rh as i64
                        {
                            continue;
                        }
                        let rgba = fitted.get_pixel(sx as u32, sy as u32).0;
                        let alpha = opacity * rgba[3] as f32 / 255.;
                        let i = (dy as usize * size[0] as usize + dx as usize) * 4;
                        for ch in 0..3 {
                            pixels[i + ch] += rgba[ch] as f32 * alpha;
                        }
                        pixels[i + 3] += alpha;
                    }
                }
            }
            if let Some(bitmap) = direct {
                layers.push((*track, bitmap));
                continue;
            }
            let mut output = image::RgbaImage::new(size[0], size[1]);
            for (i, pixel) in output.pixels_mut().enumerate() {
                let alpha = pixels[i * 4 + 3].clamp(0., 1.);
                if alpha > 0. {
                    for ch in 0..3 {
                        pixel[ch] = (pixels[i * 4 + ch] / alpha).round().clamp(0., 255.) as u8;
                    }
                    pixel[3] = (alpha * 255.).round() as u8;
                }
            }
            layers.push((
                *track,
                std::sync::Arc::new(
                    capture_core::bitmap::Bitmap::transient(std::sync::Arc::new(output))
                        .map_err(|e| e.to_string())?,
                ),
            ));
        }
        Ok(layers)
    }
    #[cfg(test)]
    pub fn frame(&mut self, p: &Project, size: [u32; 2], time: u64) -> Result<image::RgbaImage> {
        let mut out = image::RgbaImage::from_pixel(size[0], size[1], image::Rgba(p.background));
        for (_, b) in self.layers(p, size, time)? {
            image::imageops::overlay(
                &mut out,
                b.pixels().map_err(|e| e.to_string())?.as_ref(),
                0,
                0,
            );
        }
        Ok(out)
    }
}
/// Shared-mode playback owns its bounded OS queue on the audio worker.
pub struct Output {
    client: IAudioClient,
    render: IAudioRenderClient,
    capacity: u32,
}
impl Output {
    pub fn open(id: &str) -> Result<Self> {
        unsafe {
            let e: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(err)?;
            let d = if id.is_empty() {
                e.GetDefaultAudioEndpoint(eRender, eConsole).map_err(err)?
            } else {
                let wide: Vec<_> = id.encode_utf16().chain(Some(0)).collect();
                e.GetDevice(PCWSTR(wide.as_ptr())).map_err(|e| {
                    format!("Audio output is unavailable. Choose another output device: {e}")
                })?
            };
            let client: IAudioClient = d.Activate(CLSCTX_ALL, None).map_err(err)?;
            let f = WAVEFORMATEX {
                wFormatTag: 1,
                nChannels: 2,
                nSamplesPerSec: 48_000,
                nAvgBytesPerSec: 192_000,
                nBlockAlign: 4,
                wBitsPerSample: 16,
                cbSize: 0,
            };
            client
                .Initialize(
                    AUDCLNT_SHAREMODE_SHARED,
                    AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                    200_000,
                    0,
                    &f,
                    None,
                )
                .map_err(err)?;
            let render = client.GetService().map_err(err)?;
            let capacity = client.GetBufferSize().map_err(err)?;
            Ok(Self {
                client,
                render,
                capacity,
            })
        }
    }
    pub fn room(&self) -> Result<u32> {
        unsafe {
            Ok(self
                .capacity
                .saturating_sub(self.client.GetCurrentPadding().map_err(err)?))
        }
    }
    pub fn is_empty(&self) -> Result<bool> {
        Ok(self.room()? == self.capacity)
    }
    pub fn write(&self, values: &[i16]) -> Result<()> {
        unsafe {
            let n = (values.len() / 2) as u32;
            if n > self.room()? {
                return Err("Playback queue is full.".into());
            }
            let p = self.render.GetBuffer(n).map_err(err)?;
            std::ptr::copy_nonoverlapping(values.as_ptr().cast::<u8>(), p, values.len() * 2);
            self.render.ReleaseBuffer(n, 0).map_err(err)
        }
    }
    pub fn start(&self) -> Result<()> {
        unsafe { self.client.Start().map_err(err) }
    }
}
impl Drop for Output {
    fn drop(&mut self) {
        unsafe {
            let _ = self.client.Stop();
            let _ = self.client.Reset();
        }
    }
}
