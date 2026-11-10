//! The recording behind a project, for saving it without a render.
use super::editor::{MediaKind, Project};

impl Project {
    /// The one screen recording this project shows, if it uses exactly one.
    pub fn recording_path(&self) -> Option<&str> {
        let mut paths = self
            .segments
            .iter()
            .filter(|c| c.kind() == MediaKind::Video && c.label != "Webcam")
            .map(|c| {
                c.source
                    .as_ref()
                    .map_or(self.source.as_str(), |s| s.path.as_str())
            });
        let first = paths.next()?;
        paths.all(|p| p == first).then_some(first)
    }
    /// The pointer is recorded as an object, so it is not in the recording file.
    pub fn has_recorded_cursor(&self) -> bool {
        self.objects.values().any(|t| t.label == "Recorded cursor")
    }
    /// True when rendering would only reproduce the recording: one whole clip
    /// at normal speed, filling the canvas, with nothing added or adjusted.
    pub fn is_unedited(&self, size: [u32; 2], annotations: usize) -> bool {
        let [clip] = self.segments.as_slice() else {
            return false;
        };
        let (duration, picture) = clip
            .source
            .as_ref()
            .map_or((self.source_duration, self.source_size), |s| {
                (s.duration, s.size)
            });
        annotations == 0
            && self.objects.is_empty()
            && self.zooms.is_empty()
            && clip.kind() == MediaKind::Video
            && clip.at == 0
            && clip.start == 0
            && clip.end == duration
            && clip.speed == 1.
            && clip.fade_in == 0
            && clip.fade_out == 0
            && !clip.hidden
            && !clip.mono
            && clip.keyframes.is_empty()
            && clip.rect == [0., 0., 1., 1.]
            && clip
                .frame
                .as_ref()
                .is_none_or(|f| f.unmodified_pixels() && f.line_style.is_none())
            && clip
                .audio
                .iter()
                .all(|a| a.volume == 1. && !a.muted && a.fade_in == 0 && a.fade_out == 0)
            && picture == size
            && (clip.source.is_some() || self.crop == [0, 0, size[0], size[1]])
            && self.clip_effects.is_empty()
            && self.track_effects.is_empty()
            && self.media_levels.values().all(|v| *v == 1.)
            && self.track_settings.iter().all(|s| !s.hidden && !s.muted)
    }
}

#[cfg(test)]
mod tests {
    use super::super::editor::{Project, Timing};
    use super::super::{Options, Quality, Session, Settings, Source, size_text};

    fn plain() -> Project {
        Project::new("recording.mp4".into(), [640, 360], 5000, 30)
    }

    #[test]
    fn only_an_untouched_recording_counts_as_unedited() {
        assert!(plain().is_unedited([640, 360], 0));
        let edits: [fn(&mut Project); 12] = [
            |p| p.segments[0].start = 10,
            |p| p.segments[0].end = 4000,
            |p| p.segments[0].speed = 2.,
            |p| p.segments[0].at = 5,
            |p| p.segments[0].fade_in = 100,
            |p| p.segments[0].hidden = true,
            |p| p.segments[0].mono = true,
            |p| p.segments[0].rect = [0., 0., 0.5, 0.5],
            |p| p.crop = [10, 0, 630, 360],
            |p| {
                p.objects.insert(9, Timing::new(1, 0, 100));
            },
            |p| {
                let copy = p.segments[0].clone();
                p.segments.push(copy);
            },
            |p| {
                p.media_levels.insert(1, 0.5);
            },
        ];
        for (index, edit) in edits.iter().enumerate() {
            let mut p = plain();
            edit(&mut p);
            assert!(!p.is_unedited([640, 360], 0), "edit {index}");
        }
        assert!(
            !plain().is_unedited([640, 360], 1),
            "an annotation is an edit"
        );
        assert!(
            !plain().is_unedited([1280, 720], 0),
            "a different canvas renders"
        );
    }

    #[test]
    fn a_project_has_a_recording_only_when_it_shows_one_file() {
        let mut p = plain();
        assert_eq!(p.recording_path(), Some("recording.mp4"));
        let mut webcam = p.segments[0].clone();
        webcam.label = "Webcam".into();
        webcam.source = Some(super::super::editor::Source {
            path: "camera.mp4".into(),
            size: [320, 240],
            duration: 5000,
            kind: super::super::editor::MediaKind::Video,
        });
        p.segments.push(webcam.clone());
        assert_eq!(
            p.recording_path(),
            Some("recording.mp4"),
            "the webcam is separate"
        );
        webcam.label = "Second".into();
        p.segments.push(webcam);
        assert_eq!(p.recording_path(), None);
    }

    #[test]
    fn quality_rates_sizes_and_limits_follow_the_approved_table() {
        for (quality, rates) in [
            (Quality::Standard, [8, 8, 8]),
            (Quality::High, [10, 16, 25]),
            (Quality::Maximum, [20, 32, 50]),
        ] {
            for (fps, mbps) in [15, 30, 60].into_iter().zip(rates) {
                assert_eq!(quality.bitrate(fps), mbps * 1_000_000);
            }
        }
        assert_eq!(Quality::Standard.megabytes(60, 60_000), 60);
        assert_eq!(Quality::High.megabytes(60, 60_000), 187);
        assert_eq!(Quality::Maximum.megabytes(60, 60_000), 375);
        assert_eq!(size_text(375), "375 MB");
        assert_eq!(size_text(1500), "1.5 GB");
        assert_eq!(Quality::Standard.limit_minutes(60), None);
        assert_eq!(Quality::Maximum.limit_minutes(60), Some(10));
        assert_eq!(Options::default().quality, Quality::Standard);
    }

    #[test]
    fn late_frames_suggest_a_lower_setting_only_when_they_add_up() {
        let settings = |fps| Settings {
            source: Source::Display("main".into()),
            size: [640, 360],
            fps,
            bitrate: 8_000_000,
            microphone: false,
            system_audio: false,
            cursor: false,
        };
        let interval = 10_000_000 / 30;
        let run = |gaps: &[(u64, u64)], fps| {
            let mut session = Session::new(settings(fps)).unwrap();
            session.start(0).unwrap();
            let mut now = 0;
            for (count, step) in gaps {
                for _ in 0..*count {
                    session.frame_time(now).unwrap();
                    now += step;
                }
            }
            session
        };
        let smooth = run(&[(300, interval)], 30);
        assert_eq!(smooth.frame_advice(Quality::High), None);
        let slight = run(&[(300, interval), (2, interval * 3), (1, interval)], 30);
        assert_eq!(
            slight.frame_advice(Quality::High),
            None,
            "a brief hitch is fine"
        );
        let late = run(&[(100, interval), (20, interval * 3), (1, interval)], 30);
        assert!(
            late.frame_advice(Quality::Maximum)
                .unwrap()
                .contains("Try High")
        );
        assert!(
            late.frame_advice(Quality::Standard)
                .unwrap()
                .contains("lower frame rate")
        );
    }
}
