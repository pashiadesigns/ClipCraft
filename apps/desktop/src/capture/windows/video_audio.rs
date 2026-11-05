//! Explicit opt-in WASAPI inputs. Shared conversion yields 48 kHz stereo PCM;
//! devices are stopped/reset during pause and released on every exit path.
use windows::{
    Win32::{Media::Audio::*, System::Com::*},
    core::Result,
};
pub(crate) struct Input {
    client: IAudioClient,
    capture: IAudioCaptureClient,
    seen: bool,
    delivered: u64,
}
pub(crate) struct Packet {
    pub qpc: u64,
    pub position: u64,
    pub samples: Vec<i16>,
}
impl Input {
    pub fn open(system: bool) -> Result<Self> {
        Self::open_device(system, None)
    }
    pub fn open_device(system: bool, id: Option<&str>) -> Result<Self> {
        unsafe {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let device = if let Some(id) = id.filter(|s| !s.is_empty()) {
                let wide: Vec<u16> = id.encode_utf16().chain(Some(0)).collect();
                enumerator.GetDevice(windows::core::PCWSTR(wide.as_ptr()))?
            } else {
                enumerator
                    .GetDefaultAudioEndpoint(if system { eRender } else { eCapture }, eConsole)?
            };
            let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
            let format = WAVEFORMATEX {
                wFormatTag: 1,
                nChannels: 2,
                nSamplesPerSec: 48_000,
                nAvgBytesPerSec: 192_000,
                nBlockAlign: 4,
                wBitsPerSample: 16,
                cbSize: 0,
            };
            client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
                    | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY
                    | if system {
                        AUDCLNT_STREAMFLAGS_LOOPBACK
                    } else {
                        0
                    },
                4_000_000,
                0,
                &format,
                None,
            )?;
            let capture = client.GetService()?;
            Ok(Self {
                client,
                capture,
                seen: false,
                delivered: 0,
            })
        }
    }
    pub fn start(&self) -> Result<()> {
        unsafe { self.client.Start() }
    }
    pub fn pause(&mut self) -> Result<()> {
        unsafe {
            self.client.Stop()?;
            self.client.Reset()?;
            self.seen = false;
            self.delivered = 0;
            Ok(())
        }
    }
    pub fn packets(&mut self) -> Result<Vec<Packet>> {
        unsafe {
            let mut out = Vec::new();
            // A stalled encoder cannot make packet collection an unbounded loop.
            while self.capture.GetNextPacketSize()? > 0 {
                if out.len() >= 128 {
                    return Err(windows::core::Error::new(
                        windows::Win32::Foundation::E_FAIL,
                        "Audio queue overflow. Use a lower video frame rate.",
                    ));
                }
                let mut data = std::ptr::null_mut();
                let mut frames = 0;
                let mut flags = 0;
                let mut qpc = 0;
                let mut position = 0;
                self.capture.GetBuffer(
                    &mut data,
                    &mut frames,
                    &mut flags,
                    Some(&mut position),
                    Some(&mut qpc),
                )?;
                let result = (|| {
                    if frames > 96_000 {
                        return Err(windows::core::Error::from_hresult(
                            windows::Win32::Foundation::E_INVALIDARG,
                        ));
                    }
                    if flags & AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR.0 as u32 != 0 {
                        return Err(windows::core::Error::new(
                            windows::Win32::Foundation::E_FAIL,
                            "The audio device returned an unreliable timestamp.",
                        ));
                    }
                    if self.seen && flags & AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY.0 as u32 != 0 {
                        return Err(windows::core::Error::new(
                            windows::Win32::Foundation::E_FAIL,
                            "Audio was interrupted. Try a lower frame rate or reconnect the device.",
                        ));
                    }
                    let samples = if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 {
                        vec![0; frames as usize * 2]
                    } else if data.is_null() {
                        return Err(windows::core::Error::from_hresult(
                            windows::Win32::Foundation::E_POINTER,
                        ));
                    } else {
                        std::slice::from_raw_parts(data.cast::<i16>(), frames as usize * 2).to_vec()
                    };
                    Ok(Packet {
                        qpc,
                        position: self.delivered,
                        samples,
                    })
                })();
                self.capture.ReleaseBuffer(frames)?;
                out.push(result?);
                self.seen = true;
                self.delivered += frames as u64;
            }
            Ok(out)
        }
    }
}
pub(crate) fn devices() -> Result<Vec<(String, String)>> {
    endpoint_devices(false)
}
pub(crate) fn endpoint_devices(output: bool) -> Result<Vec<(String, String)>> {
    unsafe {
        let e: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let devices =
            e.EnumAudioEndpoints(if output { eRender } else { eCapture }, DEVICE_STATE_ACTIVE)?;
        let mut result = vec![(
            String::new(),
            if output {
                "Windows default output"
            } else {
                "Default input device"
            }
            .into(),
        )];
        for i in 0..devices.GetCount()?.min(128) {
            let device = devices.Item(i)?;
            let ptr = device.GetId()?;
            let id = ptr.to_string();
            CoTaskMemFree(Some(ptr.0.cast()));
            let id = id?;
            let key = windows::Win32::Foundation::PROPERTYKEY {
                fmtid: windows::core::GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0),
                pid: 14,
            };
            let label = device
                .OpenPropertyStore(STGM_READ)
                .and_then(|p| p.GetValue(&key))
                .map(|p| p.to_string())
                .unwrap_or_else(|_| {
                    format!("{} {}", if output { "Output" } else { "Microphone" }, i + 1)
                });
            result.push((id, label));
        }
        Ok(result)
    }
}
impl Drop for Input {
    fn drop(&mut self) {
        unsafe {
            let _ = self.client.Stop();
        }
    }
}
