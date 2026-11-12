//! Video configuration and a pause-free recording timeline, independent of codecs.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
pub mod audio;
#[cfg(test)]
mod ba_review_tests;
pub mod editing;
pub mod editor;
#[cfg(test)]
mod editor_tests;
pub mod effects;
pub mod frame;
pub mod frame_cache;
pub mod instruction;
pub mod keyframes;
pub mod links;
pub mod motion_path;
pub mod project;
pub mod recording_file;
pub mod split;
pub mod templates;
pub mod wav;
pub mod webcam;
/// Recordings stop at this size, whichever quality they use.
pub const RECORDING_LIMIT_BYTES: u64 = 4_000_000_000;
/// How much data a recording or an export keeps per second.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quality {
    #[default]
    Standard,
    High,
    Maximum,
}
impl Quality {
    pub const ALL: [Self; 3] = [Self::Standard, Self::High, Self::Maximum];
    pub fn label(self) -> &'static str {
        match self {
            Self::Standard => "Standard",
            Self::High => "High",
            Self::Maximum => "Maximum",
        }
    }
    pub fn purpose(self) -> &'static str {
        match self {
            Self::Standard => "Good for tutorials and screen walkthroughs.",
            Self::High => "Good for video on screen and most games.",
            Self::Maximum => "Closest to the original, for fast games.",
        }
    }
    /// Bits per second at 15, 30 or 60 frames per second.
    pub fn bitrate(self, fps: u32) -> u32 {
        let mbps = match (self, fps) {
            (Self::Standard, _) => 8,
            (Self::High, ..=15) => 10,
            (Self::High, ..=30) => 16,
            (Self::High, _) => 25,
            (Self::Maximum, ..=15) => 20,
            (Self::Maximum, ..=30) => 32,
            (Self::Maximum, _) => 50,
        };
        mbps * 1_000_000
    }
    /// Estimated size in megabytes of a video this long.
    pub fn megabytes(self, fps: u32, milliseconds: u64) -> u64 {
        u64::from(self.bitrate(fps)) * milliseconds / 8_000 / 1_000_000
    }
    pub fn lower(self) -> Option<Self> {
        match self {
            Self::Standard => None,
            Self::High => Some(Self::Standard),
            Self::Maximum => Some(Self::High),
        }
    }
    /// Minutes until the size limit, when that comes before the one-hour limit.
    pub fn limit_minutes(self, fps: u32) -> Option<u64> {
        let minutes = RECORDING_LIMIT_BYTES * 8 / u64::from(self.bitrate(fps)) / 60;
        (minutes < 60).then_some(minutes)
    }
}
/// "60 MB", or "1.5 GB" from a thousand megabytes up.
pub fn size_text(megabytes: u64) -> String {
    if megabytes >= 1000 {
        format!("{:.1} GB", megabytes as f64 / 1000.)
    } else {
        format!("{megabytes} MB")
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub instruction: instruction::Options,
    pub fps: u32,
    pub quality: Quality,
    pub microphone: bool,
    pub system_audio: bool,
    pub cursor: bool,
    pub preview: bool,
    pub open_folder: bool,
    /// Open Find Sensitive Info when the recording arrives in the editor.
    pub find_sensitive: bool,
    pub pause_on_focus_loss: bool,
    pub countdown: bool,
    pub selection: crate::capture_options::Selection,
    /// Webcam picture-in-picture (AS13).
    pub webcam: webcam::Webcam,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            instruction: Default::default(),
            fps: 30,
            quality: Quality::Standard,
            microphone: false,
            system_audio: false,
            cursor: true,
            preview: true,
            open_folder: false,
            find_sensitive: false,
            pause_on_focus_loss: false,
            countdown: false,
            selection: crate::capture_options::Selection::Region,
            webcam: Default::default(),
        }
    }
}
impl Options {
    pub fn validate(&self) -> Result<()> {
        if ![15, 30, 60].contains(&self.fps) {
            return Err(Error::Invalid("Pick 15, 30 or 60 under Frame rate.".into()));
        }
        if !matches!(
            self.selection,
            crate::capture_options::Selection::Region
                | crate::capture_options::Selection::Window
                | crate::capture_options::Selection::Fullscreen
        ) {
            return Err(Error::Invalid(
                "Choose Region, Window or Fullscreen for video.".into(),
            ));
        }
        Ok(())
    }
    pub fn output_size(source: [u32; 2]) -> Result<[u32; 2]> {
        if source[0] < 64 || source[1] < 64 {
            return Err(Error::Invalid(
                "Drag a larger area: at least 64 × 64 pixels.".into(),
            ));
        }
        let scale = (1920.0 / source[0] as f64)
            .min(1080.0 / source[1] as f64)
            .min(1.);
        let size = [
            ((source[0] as f64 * scale) as u32 / 2) * 2,
            ((source[1] as f64 * scale) as u32 / 2) * 2,
        ];
        if size[0] < 64 || size[1] < 64 {
            return Err(Error::Invalid(
                "This area is too narrow. Drag a wider or taller area.".into(),
            ));
        }
        Ok(size)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Source {
    Display(String),
    Window(u64),
    Region { origin: [i32; 2], size: [u32; 2] },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub source: Source,
    pub size: [u32; 2],
    pub fps: u32,
    pub bitrate: u32,
    pub microphone: bool,
    pub system_audio: bool,
    pub cursor: bool,
}
impl Settings {
    pub fn validate(&self) -> Result<()> {
        let [w, h] = self.size;
        if w < 64
            || h < 64
            || w > 3840
            || h > 2160
            || w % 2 != 0
            || h % 2 != 0
            || ![15, 30, 60].contains(&self.fps)
            || !(500_000..=50_000_000).contains(&self.bitrate)
        {
            return Err(Error::Invalid("Video needs even dimensions from 64 × 64 through 3840 × 2160, 15/30/60 fps and a bitrate of 0.5–50 Mbps.".into()));
        }
        match &self.source {
            Source::Display(id) if id.is_empty() || id.len() > 256 || id.contains('\0') => {
                return Err(Error::Invalid("Choose a connected display.".into()));
            }
            Source::Window(0) => return Err(Error::Invalid("Choose an available window.".into())),
            Source::Region { size, .. } if size.contains(&0) => {
                return Err(Error::Invalid("Choose a non-empty capture region.".into()));
            }
            _ => (),
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Ready,
    Recording,
    Paused,
    Finalizing,
    Completed,
    Cancelled,
    Failed,
}
/// Monotonic timestamps use 100ns units, matching Media Foundation. Pauses are
/// removed from the output timeline. Input timestamp regression is rejected.
pub struct Session {
    settings: Settings,
    phase: Phase,
    origin: u64,
    last: u64,
    pause_start: u64,
    paused: u64,
    last_frame: Option<u64>,
    frames: u64,
    dropped: u64,
}
impl Session {
    pub fn new(settings: Settings) -> Result<Self> {
        settings.validate()?;
        Ok(Self {
            settings,
            phase: Phase::Ready,
            origin: 0,
            last: 0,
            pause_start: 0,
            paused: 0,
            last_frame: None,
            frames: 0,
            dropped: 0,
        })
    }
    pub fn phase(&self) -> Phase {
        self.phase
    }
    pub fn elapsed(&self, now: u64) -> u64 {
        let end = if self.phase == Phase::Paused {
            self.pause_start
        } else {
            now
        };
        end.saturating_sub(self.origin).saturating_sub(self.paused)
    }
    fn check(&mut self, now: u64) -> Result<()> {
        if now < self.last {
            return Err(Error::Invalid("Recording clock moved backwards.".into()));
        }
        self.last = now;
        Ok(())
    }
    pub fn start(&mut self, now: u64) -> Result<()> {
        if self.phase != Phase::Ready {
            return Err(Error::Invalid("Recording has already started.".into()));
        }
        self.check(now)?;
        self.origin = now;
        self.phase = Phase::Recording;
        Ok(())
    }
    pub fn pause(&mut self, now: u64) -> Result<()> {
        if self.phase != Phase::Recording {
            return Err(Error::Invalid("Only an active recording can pause.".into()));
        }
        self.check(now)?;
        self.pause_start = now;
        self.phase = Phase::Paused;
        Ok(())
    }
    pub fn resume(&mut self, now: u64) -> Result<()> {
        if self.phase != Phase::Paused {
            return Err(Error::Invalid("Only a paused recording can resume.".into()));
        }
        self.check(now)?;
        self.paused += now - self.pause_start;
        self.phase = Phase::Recording;
        Ok(())
    }
    /// Returns None to drop early frames without growing a queue. The encoder
    /// owns its bounded queue and may drop video frames, never audio samples.
    pub fn frame_time(&mut self, now: u64) -> Result<Option<u64>> {
        if self.phase != Phase::Recording {
            return Ok(None);
        }
        self.check(now)?;
        let pts = now - self.origin - self.paused;
        let interval = 10_000_000 / self.settings.fps as u64;
        if let Some(last) = self.last_frame {
            let gap = pts.saturating_sub(last) / interval;
            if gap == 0 {
                return Ok(None);
            }
            self.dropped += gap - 1;
        }
        self.last_frame = Some(pts);
        self.frames += 1;
        Ok(Some(pts))
    }
    /// What to try next time, when more than 2% of the frames were late.
    pub fn frame_advice(&self, quality: Quality) -> Option<String> {
        if self.dropped * 50 <= self.frames + self.dropped {
            return None;
        }
        match quality.lower() {
            Some(lower) => Some(format!(
                "Some frames were dropped at {} quality. Try {} next time.",
                quality.label(),
                lower.label()
            )),
            None => (self.settings.fps > 15).then(|| {
                format!(
                    "Some frames were dropped at {} fps. Try a lower frame rate next time.",
                    self.settings.fps
                )
            }),
        }
    }
    pub fn finish(&mut self) -> Result<()> {
        if !matches!(self.phase, Phase::Recording | Phase::Paused) || self.frames == 0 {
            return Err(Error::Invalid(
                "Record at least one frame before finishing.".into(),
            ));
        }
        self.phase = Phase::Finalizing;
        Ok(())
    }
    /// Only successful encoder finalization can make a recording publishable.
    pub fn finalized(&mut self, success: bool) -> Result<()> {
        if self.phase != Phase::Finalizing {
            return Err(Error::Invalid("The encoder is not finalizing.".into()));
        }
        self.phase = if success {
            Phase::Completed
        } else {
            Phase::Failed
        };
        Ok(())
    }
    pub fn cancel(&mut self) -> Result<()> {
        if matches!(
            self.phase,
            Phase::Completed | Phase::Failed | Phase::Cancelled
        ) {
            return Err(Error::Invalid("This recording has already ended.".into()));
        }
        self.phase = Phase::Cancelled;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn video_defaults_are_opt_in_and_size_is_bounded() {
        let options = Options::default();
        assert!(!options.microphone && !options.system_audio);
        assert_eq!(Options::output_size([3840, 2160]).unwrap(), [1920, 1080]);
        assert_eq!(Options::output_size([641, 481]).unwrap(), [640, 480]);
        assert!(Options::output_size([64, 32_000]).is_err());
        assert_eq!(
            serde_json::from_slice::<Options>(&serde_json::to_vec(&options).unwrap()).unwrap(),
            options
        );
        let mut bad = options;
        bad.selection = crate::capture_options::Selection::Steps;
        assert!(bad.validate().is_err());
    }
    fn settings() -> Settings {
        Settings {
            source: Source::Display("test".into()),
            size: [1920, 1080],
            fps: 30,
            bitrate: 8_000_000,
            microphone: false,
            system_audio: false,
            cursor: true,
        }
    }
    #[test]
    fn pauses_remove_time_and_terminal_states_do_not_record() {
        let mut s = Session::new(settings()).unwrap();
        s.start(100).unwrap();
        assert_eq!(s.frame_time(100).unwrap(), Some(0));
        assert_eq!(s.frame_time(101).unwrap(), None);
        s.pause(1_000_100).unwrap();
        assert_eq!(s.frame_time(2_000_100).unwrap(), None);
        s.resume(5_000_100).unwrap();
        assert_eq!(s.frame_time(5_000_100).unwrap(), Some(1_000_000));
        assert!(s.frame_time(20).is_err());
        s.finish().unwrap();
        assert_eq!(s.phase(), Phase::Finalizing);
        s.finalized(true).unwrap();
        assert_eq!(s.frame_time(6_000_100).unwrap(), None);
        assert!(s.start(0).is_err());
        assert!(s.cancel().is_err());
    }
    #[test]
    fn failed_finalize_never_reports_completed_and_configuration_roundtrips() {
        let p = settings();
        assert_eq!(
            serde_json::from_slice::<Settings>(&serde_json::to_vec(&p).unwrap()).unwrap(),
            p
        );
        let mut bad = p.clone();
        bad.size[0] = 1919;
        assert!(bad.validate().is_err());
        let mut s = Session::new(p).unwrap();
        assert!(s.finish().is_err());
        s.start(0).unwrap();
        s.frame_time(0).unwrap();
        s.finish().unwrap();
        s.finalized(false).unwrap();
        assert_eq!(s.phase(), Phase::Failed);
    }
}

#[cfg(test)]
mod workflow_repair_tests;
pub mod zoom;
