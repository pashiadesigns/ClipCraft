//! Asynchronous Windows camera input. One pending sample; cancellation never waits
//! for a device's blocking ReadSample. Files and COM objects stay on the worker.
use super::*;
use windows::{
    Win32::{Media::MediaFoundation::*, System::Com::CoTaskMemFree},
    core::{HRESULT, Interface, PCWSTR, Ref, Result as WinResult, implement},
};
type Packet = Result<Option<(u64, Vec<u8>)>, String>;
#[implement(IMFSourceReaderCallback)]
struct Callback {
    tx: mpsc::SyncSender<Packet>,
}
impl IMFSourceReaderCallback_Impl for Callback_Impl {
    fn OnReadSample(
        &self,
        status: HRESULT,
        _stream: u32,
        flags: u32,
        time: i64,
        sample: Ref<IMFSample>,
    ) -> WinResult<()> {
        let value = (|| -> Result<_, String> {
            status.ok().map_err(|e| e.to_string())?;
            if flags & (MF_SOURCE_READERF_ENDOFSTREAM.0 | MF_SOURCE_READERF_ERROR.0) as u32 != 0 {
                return Err("Camera disconnected. Reconnect it and record a new take.".into());
            }
            let Some(sample) = sample.as_ref() else {
                return Ok(None);
            };
            unsafe {
                let stamp = sample
                    .GetUINT64(&MFSampleExtension_DeviceTimestamp)
                    .unwrap_or(time.max(0) as u64);
                let b = sample
                    .ConvertToContiguousBuffer()
                    .map_err(|e| e.to_string())?;
                let (mut ptr, mut len) = (std::ptr::null_mut(), 0);
                b.Lock(&mut ptr, None, Some(&mut len))
                    .map_err(|e| e.to_string())?;
                if len == 0 || ptr.is_null() {
                    let _ = b.Unlock();
                    return Ok(None);
                }
                if len > 1920 * 1080 * 8 {
                    let _ = b.Unlock();
                    return Err("Camera returned an oversized frame.".into());
                }
                let bytes = std::slice::from_raw_parts(ptr, len as usize).to_vec();
                b.Unlock().map_err(|e| e.to_string())?;
                Ok(Some((stamp, bytes)))
            }
        })();
        let _ = self.tx.try_send(value);
        Ok(())
    }
    fn OnFlush(&self, _: u32) -> WinResult<()> {
        Ok(())
    }
    fn OnEvent(&self, _: u32, _: Ref<IMFMediaEvent>) -> WinResult<()> {
        Ok(())
    }
}
unsafe fn string(a: &IMFAttributes, key: &windows::core::GUID) -> WinResult<String> {
    unsafe {
        let mut value = windows::core::PWSTR::null();
        let mut n = 0;
        a.GetAllocatedString(key, &mut value, &mut n)?;
        let text = if n == 0 || value.is_null() {
            String::new()
        } else {
            String::from_utf16_lossy(std::slice::from_raw_parts(value.0, n as usize))
        };
        CoTaskMemFree(Some(value.0.cast()));
        Ok(text)
    }
}
pub(crate) fn devices() -> Result<Vec<(String, String)>, String> {
    unsafe {
        let mut attrs = None;
        MFCreateAttributes(&mut attrs, 1).map_err(|e| e.to_string())?;
        let attrs = attrs.unwrap();
        attrs
            .SetGUID(
                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
            )
            .map_err(|e| e.to_string())?;
        let (mut ptr, mut count) = (std::ptr::null_mut(), 0);
        MFEnumDeviceSources(&attrs, &mut ptr, &mut count).map_err(|e| e.to_string())?;
        let mut out = vec![];
        for item in if count == 0 {
            &mut []
        } else {
            std::slice::from_raw_parts_mut(ptr, count as usize)
        } {
            if let Some(a) = item.take() {
                let attrs: IMFAttributes = a.cast().map_err(|e| e.to_string())?;
                if let (Ok(id), Ok(name)) = (
                    string(
                        &attrs,
                        &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
                    ),
                    string(&attrs, &MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME),
                ) {
                    out.push((id, name));
                }
            }
        }
        CoTaskMemFree(Some(ptr.cast()));
        Ok(out)
    }
}
pub(crate) struct Source {
    source: IMFMediaSource,
    reader: IMFSourceReader,
    rx: mpsc::Receiver<Packet>,
    pub size: [u32; 2],
    stride: i32,
    last: Instant,
    clock_offset: Option<i128>,
    startup_retry: Option<(String, u32)>,
}
impl Drop for Source {
    fn drop(&mut self) {
        unsafe {
            let _ = self.reader.Flush(MF_SOURCE_READER_ALL_STREAMS.0 as u32);
            let _ = self.source.Shutdown();
        }
    }
}
impl Source {
    pub fn open(device: &str, height: u32) -> Result<Self, String> {
        Self::open_pipeline(device, height, true)
            .or_else(|_| Self::open_pipeline(device, height, false))
    }
    fn open_pipeline(device: &str, height: u32, hardware: bool) -> Result<Self, String> {
        let source = unsafe {
            let mut attrs = None;
            MFCreateAttributes(&mut attrs, 3).map_err(|e| e.to_string())?;
            let attrs = attrs.unwrap();
            attrs
                .SetGUID(
                    &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
                    &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
                )
                .map_err(|e| e.to_string())?;
            attrs
                .SetUINT32(&MFT_HW_TIMESTAMP_WITH_QPC_Attribute, 1)
                .map_err(|e| e.to_string())?;
            let device: Vec<_> = device.encode_utf16().chain(Some(0)).collect();
            attrs
                .SetString(
                    &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
                    PCWSTR(device.as_ptr()),
                )
                .map_err(|e| e.to_string())?;
            MFCreateDeviceSource(&attrs).map_err(|e|format!("Cannot open camera. Close other camera apps or allow camera access in Windows Settings: {e}"))?
        };
        let result = (|| -> Result<Self, String> {
            unsafe {
                let (tx, rx) = mpsc::sync_channel(1);
                let callback: IMFSourceReaderCallback = Callback { tx }.into();
                let mut attrs = None;
                MFCreateAttributes(&mut attrs, 5).map_err(|e| e.to_string())?;
                let attrs = attrs.unwrap();
                attrs
                    .SetUINT32(&MF_LOW_LATENCY, 1)
                    .map_err(|e| e.to_string())?;
                attrs
                    .SetUINT32(
                        &MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS,
                        u32::from(hardware),
                    )
                    .map_err(|e| e.to_string())?;
                attrs
                    .SetUnknown(&MF_SOURCE_READER_ASYNC_CALLBACK, &callback)
                    .map_err(|e| e.to_string())?;
                attrs
                    .SetUINT32(&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1)
                    .map_err(|e| e.to_string())?;
                let reader = MFCreateSourceReaderFromMediaSource(&source, &attrs)
                    .map_err(|e| e.to_string())?;
                let v = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
                let mut choices = vec![];
                for index in 0..256 {
                    let Ok(t) = reader.GetNativeMediaType(v, index) else {
                        break;
                    };
                    let Ok(n) = t.GetUINT64(&MF_MT_FRAME_SIZE) else {
                        continue;
                    };
                    let size = [(n >> 32) as u32, n as u32];
                    if size[0] < 64
                        || size[1] < 64
                        || size[0] > 1920
                        || size[1] > 1080
                        || size.iter().any(|v| !v.is_multiple_of(2))
                    {
                        continue;
                    }
                    let rate = t.GetUINT64(&MF_MT_FRAME_RATE).unwrap_or((30u64 << 32) | 1);
                    let fps = (rate >> 32) as f64 / (rate as u32).max(1) as f64;
                    let cost = size[1].abs_diff(height) as f64
                        + size[0].abs_diff(height * 16 / 9) as f64
                        + (fps - 30.).abs() * 100.
                        + if t.GetGUID(&MF_MT_SUBTYPE).ok() == Some(MFVideoFormat_MJPG) {
                            0.
                        } else {
                            1.
                        };
                    choices.push((cost, t, size));
                }
                choices.sort_by(|a, b| a.0.total_cmp(&b.0));
                let mut selected = None;
                let mut last_error = String::new();
                for (_, native, size) in choices {
                    let configured = (|| -> WinResult<_> {
                        reader.SetCurrentMediaType(v, None, &native)?;
                        let output = MFCreateMediaType()?;
                        output.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
                        output.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)?;
                        output.SetUINT64(
                            &MF_MT_FRAME_SIZE,
                            ((size[0] as u64) << 32) | size[1] as u64,
                        )?;
                        // Carry the device's actual rate, aspect and scan mode through
                        // conversion instead of asking the processor to invent defaults.
                        for key in [&MF_MT_FRAME_RATE, &MF_MT_PIXEL_ASPECT_RATIO] {
                            if let Ok(value) = native.GetUINT64(key) {
                                output.SetUINT64(key, value)?;
                            }
                        }
                        if let Ok(value) = native.GetUINT32(&MF_MT_INTERLACE_MODE) {
                            output.SetUINT32(&MF_MT_INTERLACE_MODE, value)?;
                        }
                        reader.SetCurrentMediaType(v, None, &output)?;
                        let output = reader.GetCurrentMediaType(v)?;
                        let stride = output
                            .GetUINT32(&MF_MT_DEFAULT_STRIDE)
                            .unwrap_or(size[0] * 4) as i32;
                        Ok((size, stride))
                    })();
                    match configured {
                        Ok(value) => {
                            selected = Some(value);
                            break;
                        }
                        Err(e) => last_error = format!("{} × {}: {e}", size[0], size[1]),
                    }
                }
                let (size, stride) = selected.ok_or_else(|| format!(
                    "Camera format setup failed ({last_error}). Choose another resolution or camera."
                ))?;
                reader
                    .ReadSample(v, 0, None, None, None, None)
                    .map_err(|e| format!("Camera could not start sending frames: {e}"))?;
                Ok(Self {
                    source: source.clone(),
                    reader,
                    rx,
                    size,
                    stride,
                    last: Instant::now(),
                    clock_offset: None,
                    startup_retry: hardware.then(|| (device.to_owned(), height)),
                })
            }
        })();
        if result.is_err() {
            unsafe {
                let _ = source.Shutdown();
            }
        }
        result
    }
    pub fn poll(&mut self) -> Result<Option<(u64, Vec<u8>)>, String> {
        let result = self.poll_frame();
        if result.is_err()
            && let Some((device, height)) = self.startup_retry.take()
        {
            unsafe {
                let _ = self.reader.Flush(MF_SOURCE_READER_ALL_STREAMS.0 as u32);
                let _ = self.source.Shutdown();
            }
            *self = Self::open_pipeline(&device, height, false)?;
            return Ok(None);
        }
        if matches!(result, Ok(Some(_))) {
            self.startup_retry = None;
        }
        result.map_err(|e| format!("Camera preview failed: {e}"))
    }
    fn poll_frame(&mut self) -> Result<Option<(u64, Vec<u8>)>, String> {
        let packet = match self.rx.try_recv() {
            Ok(v) => v?,
            Err(mpsc::TryRecvError::Empty) => {
                if self.last.elapsed() > Duration::from_secs(5) {
                    return Err(
                        "Camera stopped sending frames. Reconnect it or choose another camera."
                            .into(),
                    );
                }
                return Ok(None);
            }
            Err(_) => return Err("Camera stream closed.".into()),
        };
        unsafe {
            self.reader
                .ReadSample(
                    MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                    0,
                    None,
                    None,
                    None,
                    None,
                )
                .map_err(|e| e.to_string())?;
        }
        let Some((stamp, bytes)) = packet else {
            return Ok(None);
        };
        self.last = Instant::now();
        let stride = self.stride.unsigned_abs() as usize;
        let [w, h] = self.size;
        if stride < w as usize * 4 || bytes.len() < stride * h as usize {
            return Err(
                "Camera returned an incomplete frame. Choose a different resolution.".into(),
            );
        }
        let mut packed = if self.stride >= 0 && stride == w as usize * 4 {
            bytes
        } else {
            let mut packed = vec![0; w as usize * h as usize * 4];
            for y in 0..h as usize {
                let sy = if self.stride < 0 {
                    h as usize - 1 - y
                } else {
                    y
                };
                packed[y * w as usize * 4..(y + 1) * w as usize * 4]
                    .copy_from_slice(&bytes[sy * stride..sy * stride + w as usize * 4]);
            }
            packed
        };
        for p in packed.as_chunks_mut::<4>().0 {
            p[3] = 255;
        }
        let now = clock();
        let offset = *self.clock_offset.get_or_insert_with(|| {
            if stamp.abs_diff(now) < 50_000_000 {
                0
            } else {
                now as i128 - stamp as i128
            }
        });
        let at = (stamp as i128 + offset).max(0) as u64;
        Ok(Some((at, packed)))
    }
}
pub(crate) fn clock() -> u64 {
    unsafe { MFGetSystemTime().max(0) as u64 }
}
pub(crate) struct Recording {
    encoder: media::Encoder,
    path: tempfile::TempPath,
    epoch: u64,
    count: u64,
    last: Vec<u8>,
    pub size: [u32; 2],
}
impl Recording {
    pub fn new(folder: &Path, size: [u32; 2], epoch: u64, first: Vec<u8>) -> Result<Self, String> {
        let path = tempfile::Builder::new()
            .prefix("camera-")
            .suffix(".mp4")
            .tempfile_in(folder)
            .map_err(|e| e.to_string())?
            .into_temp_path();
        let encoder = media::Encoder::new(&path, size, 30, false).map_err(|e| e.to_string())?;
        Ok(Self {
            encoder,
            path,
            epoch,
            count: 0,
            last: first,
            size,
        })
    }
    pub fn push(&mut self, at: u64, pixels: Vec<u8>) -> Result<(), String> {
        if at < self.epoch {
            return Ok(());
        }
        let until = ((at - self.epoch) * 30 / 10_000_000).min(30 * 3600);
        while self.count < until {
            self.write()?;
        }
        self.last = pixels;
        Ok(())
    }
    fn write(&mut self) -> Result<(), String> {
        self.encoder
            .video(self.count * 10_000_000 / 30, &self.last)
            .map_err(|e| e.to_string())?;
        self.count += 1;
        Ok(())
    }
    pub fn finish(mut self, duration: u64) -> Result<super::voiceover_capture::CameraTake, String> {
        let until = (duration * 30).div_ceil(1000);
        while self.count < until {
            self.write()?;
        }
        self.encoder.finish().map_err(|e| e.to_string())?;
        drop(self.encoder);
        let path = self.path.keep().map_err(|e| e.to_string())?;
        Ok(super::voiceover_capture::CameraTake {
            path,
            size: self.size,
            duration: self.count * 1000 / 30,
        })
    }
}

/// Bounded bilinear BGRA preview; native capture pixels are never resized.
pub(super) fn preview_pixels(bytes: &[u8], size: [u32; 2]) -> image::RgbaImage {
    let scale = (640. / size[0] as f64).min(360. / size[1] as f64).min(1.);
    let w = (size[0] as f64 * scale).round().max(1.) as u32;
    let h = (size[1] as f64 * scale).round().max(1.) as u32;
    let positions = |source: u32, dest: u32| {
        (0..dest)
            .map(|i| {
                let f = ((i as f64 + 0.5) * source as f64 / dest as f64 - 0.5)
                    .clamp(0., (source - 1) as f64);
                let a = f as usize;
                (
                    a,
                    (a + 1).min(source as usize - 1),
                    ((f - a as f64) * 256.).round() as u32,
                )
            })
            .collect::<Vec<_>>()
    };
    let xs = positions(size[0], w);
    let ys = positions(size[1], h);
    let mut out = vec![255; w as usize * h as usize * 4];
    for (y, (y0, y1, fy)) in ys.into_iter().enumerate() {
        let a = &bytes[y0 * size[0] as usize * 4..(y0 + 1) * size[0] as usize * 4];
        let b = &bytes[y1 * size[0] as usize * 4..(y1 + 1) * size[0] as usize * 4];
        for (x, (x0, x1, fx)) in xs.iter().copied().enumerate() {
            let pixel = &mut out[(y * w as usize + x) * 4..][..4];
            for (ch, out) in pixel[..3].iter_mut().enumerate() {
                let ch = 2 - ch;
                let top = a[x0 * 4 + ch] as u32 * (256 - fx) + a[x1 * 4 + ch] as u32 * fx;
                let bottom = b[x0 * 4 + ch] as u32 * (256 - fx) + b[x1 * 4 + ch] as u32 * fx;
                *out = ((top * (256 - fy) + bottom * fy + 32768) >> 16) as u8;
            }
        }
    }
    image::RgbaImage::from_raw(w, h, out).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn camera_encoder_keeps_common_epoch_and_pads_only_to_audio_end() {
        let _runtime = media::Runtime::new().unwrap();
        let root = tempfile::tempdir().unwrap();
        let red: Vec<_> = (0..320 * 180).flat_map(|_| [0, 0, 240, 255]).collect();
        let blue: Vec<_> = (0..320 * 180).flat_map(|_| [240, 0, 0, 255]).collect();
        let mut r = Recording::new(root.path(), [320, 180], 10_000_000, red.clone()).unwrap();
        r.push(9_500_000, blue.clone()).unwrap();
        r.push(15_000_000, blue).unwrap();
        let take = r.finish(1000).unwrap();
        assert_eq!(take.duration, 1000);
        let mut video = media::Video::open(&take.path).unwrap();
        let first = video.frame(100).unwrap();
        let last = video.frame(900).unwrap();
        assert!(first.get_pixel(100, 100)[0] > 220);
        assert!(last.get_pixel(100, 100)[2] > 220);
        drop(video);
        std::fs::remove_file(&take.path).unwrap();
        let r = Recording::new(root.path(), [320, 180], 10_000_000, red).unwrap();
        let path = r.path.to_path_buf();
        drop(r);
        assert!(!path.exists());
    }
}
