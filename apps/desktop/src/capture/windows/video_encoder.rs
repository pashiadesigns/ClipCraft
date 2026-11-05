//! Installed Windows H.264/AAC encoders. No bundled codec or network service.
use std::{os::windows::ffi::OsStrExt, path::Path};
use windows::{
    Win32::{Media::MediaFoundation::*, System::Com::*},
    core::{PCWSTR, Result},
};
pub(crate) struct Runtime;
impl Runtime {
    pub fn new() -> Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
            if let Err(e) = MFStartup(MF_VERSION, MFSTARTUP_FULL) {
                CoUninitialize();
                return Err(e);
            }
            Ok(Self)
        }
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        unsafe {
            let _ = MFShutdown();
            CoUninitialize();
        }
    }
}
pub(crate) struct Encoder {
    writer: IMFSinkWriter,
    video: u32,
    audio: Option<u32>,
    size: [u32; 2],
    duration: i64,
    finished: bool,
}
fn ratio(a: u32, b: u32) -> u64 {
    ((a as u64) << 32) | b as u64
}
impl Encoder {
    pub fn new(path: &Path, size: [u32; 2], fps: u32, audio: bool) -> Result<Self> {
        if size.iter().any(|n| *n < 64 || !n.is_multiple_of(2))
            || size[0] > 1920
            || size[1] > 1080
            || ![15, 30, 60].contains(&fps)
        {
            return Err(windows::core::Error::from_hresult(
                windows::Win32::Foundation::E_INVALIDARG,
            ));
        }
        unsafe {
            let mut attrs = None;
            MFCreateAttributes(&mut attrs, 3)?;
            let attrs = attrs.unwrap();
            attrs.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1)?;
            let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            let writer = MFCreateSinkWriterFromURL(PCWSTR(path.as_ptr()), None, &attrs)?;
            let output = MFCreateMediaType()?;
            output.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            output.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
            output.SetUINT32(&MF_MT_AVG_BITRATE, 8_000_000)?;
            output.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
            output.SetUINT64(&MF_MT_FRAME_SIZE, ratio(size[0], size[1]))?;
            output.SetUINT64(&MF_MT_FRAME_RATE, ratio(fps, 1))?;
            output.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, ratio(1, 1))?;
            let video = writer.AddStream(&output)?;
            let input = MFCreateMediaType()?;
            output.CopyAllItems(&input)?;
            input.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)?;
            input.SetUINT32(&MF_MT_DEFAULT_STRIDE, size[0] * 4)?;
            writer.SetInputMediaType(video, &input, None)?;
            let audio = if audio {
                let output = MFCreateMediaType()?;
                output.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
                output.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_AAC)?;
                output.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, 2)?;
                output.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, 48_000)?;
                output.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
                output.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, 24_000)?;
                let index = writer.AddStream(&output)?;
                let input = MFCreateMediaType()?;
                input.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
                input.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM)?;
                input.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, 2)?;
                input.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, 48_000)?;
                input.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
                input.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, 4)?;
                input.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, 192_000)?;
                input.SetUINT32(&MF_MT_ALL_SAMPLES_INDEPENDENT, 1)?;
                writer.SetInputMediaType(index, &input, None)?;
                Some(index)
            } else {
                None
            };
            writer.BeginWriting()?;
            Ok(Self {
                writer,
                video,
                audio,
                size,
                duration: 10_000_000 / fps as i64,
                finished: false,
            })
        }
    }
    pub fn video(&mut self, time: u64, pixels: &[u8]) -> Result<()> {
        if pixels.len() != self.size[0] as usize * self.size[1] as usize * 4 {
            return Err(windows::core::Error::from_hresult(
                windows::Win32::Foundation::E_INVALIDARG,
            ));
        }
        self.sample(self.video, time as i64, self.duration, pixels)
    }
    pub fn audio(&mut self, frame: u64, samples: &[i16]) -> Result<()> {
        if let Some(index) = self.audio {
            let bytes = samples
                .iter()
                .flat_map(|s| s.to_le_bytes())
                .collect::<Vec<_>>();
            self.sample(
                index,
                (frame * 10_000_000 / 48_000) as i64,
                ((samples.len() / 2) as u64 * 10_000_000 / 48_000) as i64,
                &bytes,
            )?;
        }
        Ok(())
    }
    fn sample(&mut self, index: u32, time: i64, duration: i64, bytes: &[u8]) -> Result<()> {
        unsafe {
            if self.finished {
                return Err(windows::core::Error::from_hresult(
                    windows::Win32::Foundation::E_UNEXPECTED,
                ));
            }
            let buffer = MFCreateMemoryBuffer(bytes.len() as u32)?;
            let mut data = std::ptr::null_mut();
            buffer.Lock(&mut data, None, None)?;
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), data, bytes.len());
            buffer.Unlock()?;
            buffer.SetCurrentLength(bytes.len() as u32)?;
            let sample = MFCreateSample()?;
            sample.AddBuffer(&buffer)?;
            sample.SetSampleTime(time)?;
            sample.SetSampleDuration(duration)?;
            self.writer.WriteSample(index, &sample)
        }
    }
    pub fn finish(&mut self) -> Result<()> {
        unsafe {
            self.writer.Finalize()?;
            self.finished = true;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn synthetic_mp4_roundtrip_preserves_orientation_audio_and_timeline() {
        let _runtime = Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.mp4");
        let mut encoder = Encoder::new(&path, [320, 240], 30, true).unwrap();
        let mut pixels = vec![0; 320 * 240 * 4];
        for y in 0..240 {
            for x in 0..320 {
                let p = (y * 320 + x) * 4;
                pixels[p..p + 4].copy_from_slice(if y < 120 {
                    &[20, 30, 220, 255]
                } else {
                    &[220, 40, 20, 255]
                });
            }
        }
        assert!(encoder.video(0, &pixels[..5]).is_err());
        for frame in 0..60u64 {
            encoder.video(frame * 10_000_000 / 30, &pixels).unwrap();
            let audio: Vec<i16> = (0..1600u64)
                .flat_map(|i| {
                    let s = (((frame * 1600 + i) as f64 * 440. * std::f64::consts::TAU / 48_000.)
                        .sin()
                        * 12_000.) as i16;
                    [s, s]
                })
                .collect();
            encoder.audio(frame * 1600, &audio).unwrap();
        }
        encoder.finish().unwrap();
        assert!(encoder.video(20_000_000, &pixels).is_err());
        drop(encoder);
        unsafe {
            let path_w: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            let mut attrs = None;
            MFCreateAttributes(&mut attrs, 1).unwrap();
            let attrs = attrs.unwrap();
            attrs
                .SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)
                .unwrap();
            let reader = MFCreateSourceReaderFromURL(PCWSTR(path_w.as_ptr()), &attrs).unwrap();
            let v = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
            let a = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;
            assert_eq!(
                reader
                    .GetNativeMediaType(v, 0)
                    .unwrap()
                    .GetGUID(&MF_MT_SUBTYPE)
                    .unwrap(),
                MFVideoFormat_H264
            );
            assert_eq!(
                reader
                    .GetNativeMediaType(a, 0)
                    .unwrap()
                    .GetGUID(&MF_MT_SUBTYPE)
                    .unwrap(),
                MFAudioFormat_AAC
            );
            let output = MFCreateMediaType().unwrap();
            output
                .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
                .unwrap();
            output
                .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)
                .unwrap();
            reader.SetCurrentMediaType(v, None, &output).unwrap();
            let mut count = 0;
            let mut last = 0;
            loop {
                let mut sample = None;
                let mut flags = 0;
                let mut time = 0;
                reader
                    .ReadSample(
                        v,
                        0,
                        None,
                        Some(&mut flags),
                        Some(&mut time),
                        Some(&mut sample),
                    )
                    .unwrap();
                if let Some(sample) = sample {
                    assert!(time >= last);
                    last = time;
                    count += 1;
                    if count == 1 {
                        let buffer = sample.ConvertToContiguousBuffer().unwrap();
                        let mut data = std::ptr::null_mut();
                        let mut len = 0;
                        buffer.Lock(&mut data, None, Some(&mut len)).unwrap();
                        let bytes = std::slice::from_raw_parts(data, len as usize);
                        let stride = reader
                            .GetCurrentMediaType(v)
                            .unwrap()
                            .GetUINT32(&MF_MT_DEFAULT_STRIDE)
                            .unwrap() as i32;
                        let row = if stride < 0 { 239 - 20 } else { 20 };
                        let top = &bytes[row * stride.unsigned_abs() as usize + 20 * 4..];
                        let row = if stride < 0 { 239 - 220 } else { 220 };
                        let bottom = &bytes[row * stride.unsigned_abs() as usize + 20 * 4..];
                        let colors = (top[2], top[0], bottom[2], bottom[0]);
                        buffer.Unlock().unwrap();
                        assert!(
                            colors.0 > 180 && colors.1 < 60 && colors.2 < 60 && colors.3 > 180,
                            "orientation/colors {colors:?}, stride {stride}"
                        );
                    }
                }
                if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                    break;
                }
            }
            assert_eq!(count, 60);
            assert!((last - 19_666_666).abs() < 1000);
            // Fresh reader avoids inter-stream buffering assumptions.
            let reader = MFCreateSourceReaderFromURL(PCWSTR(path_w.as_ptr()), None).unwrap();
            let output = MFCreateMediaType().unwrap();
            output
                .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)
                .unwrap();
            output.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM).unwrap();
            output.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16).unwrap();
            reader.SetCurrentMediaType(a, None, &output).unwrap();
            let mut samples = 0;
            let mut peak = 0i32;
            loop {
                let mut sample = None;
                let mut flags = 0;
                reader
                    .ReadSample(a, 0, None, Some(&mut flags), None, Some(&mut sample))
                    .unwrap();
                if let Some(sample) = sample {
                    let buffer = sample.ConvertToContiguousBuffer().unwrap();
                    let mut data = std::ptr::null_mut();
                    let mut len = 0;
                    buffer.Lock(&mut data, None, Some(&mut len)).unwrap();
                    let values = std::slice::from_raw_parts(data.cast::<i16>(), len as usize / 2);
                    samples += values.len() / 2;
                    peak = peak.max(values.iter().map(|v| (*v as i32).abs()).max().unwrap_or(0));
                    buffer.Unlock().unwrap();
                }
                if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                    break;
                }
            }
            assert!((94_000..99_000).contains(&samples), "{samples}");
            assert!((9_000..16_000).contains(&peak), "{peak}");
            if let Some(root) = std::env::var_os("CLIPCRAFT_VIDEO_EVIDENCE_DIR") {
                let root = std::path::PathBuf::from(root);
                std::fs::create_dir_all(&root).unwrap();
                std::fs::copy(&path, root.join("synthetic-av.mp4")).unwrap();
                std::fs::write(root.join("encoder.json"),format!("{{\"video_frames\":{count},\"audio_frames\":{samples},\"peak\":{peak},\"orientation_passed\":true,\"native_capture_tested\":false}}")).unwrap();
            }
        }
    }
}
