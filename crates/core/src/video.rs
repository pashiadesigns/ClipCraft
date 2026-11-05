//! Video configuration and a pause-free recording timeline, independent of codecs.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
pub mod audio;
pub mod editing;
pub mod editor;
#[cfg(test)]
mod editor_tests;
pub mod effects;
pub mod frame;
pub mod frame_cache;
pub mod links;
pub mod project;
pub mod wav;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub fps: u32,
    pub microphone: bool,
    pub system_audio: bool,
    pub cursor: bool,
    pub preview: bool,
    pub open_folder: bool,
    pub pause_on_focus_loss: bool,
    pub countdown: bool,
    pub selection: crate::capture_options::Selection,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            fps: 30,
            microphone: false,
            system_audio: false,
            cursor: true,
            preview: true,
            open_folder: false,
            pause_on_focus_loss: false,
            countdown: false,
            selection: crate::capture_options::Selection::Region,
        }
    }
}
impl Options {
    pub fn validate(&self) -> Result<()> {
        if ![15, 30, 60].contains(&self.fps) {
            return Err(Error::Invalid(
                "Choose 15, 30 or 60 frames per second.".into(),
            ));
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
                "Select a video area at least 64 × 64 pixels.".into(),
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
                "This selection is too narrow for video. Choose a wider or taller area.".into(),
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
            || !(500_000..=40_000_000).contains(&self.bitrate)
        {
            return Err(Error::Invalid("Video needs even dimensions from 64 × 64 through 3840 × 2160, 15/30/60 fps and a bitrate of 0.5–40 Mbps.".into()));
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
        if self
            .last_frame
            .is_some_and(|t| pts.saturating_sub(t) < 10_000_000 / self.settings.fps as u64)
        {
            return Ok(None);
        }
        self.last_frame = Some(pts);
        self.frames += 1;
        Ok(Some(pts))
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
