use super::*;
use capture_core::video::editor::{Audio, MediaKind, Segment, Source};
impl Editor {
    pub(super) fn pick_timeline_media(&mut self, at: u64, track: u64) {
        self.video.pause();
        self.suspend_global_capture();
        if let Some(paths) = rfd::FileDialog::new()
            .set_title("Add media to timeline")
            .add_filter(
                "Media",
                &[
                    "mp4", "mov", "m4v", "wmv", "avi", "png", "jpg", "jpeg", "webp", "wav", "mp3",
                    "m4a", "aac", "flac",
                ],
            )
            .pick_files()
        {
            self.import_timeline_media(paths, at, track);
        }
    }
    pub(super) fn import_timeline_media(&mut self, paths: Vec<PathBuf>, at: u64, track: u64) {
        if self.video.importing.is_some() {
            self.status = "Wait for the current media import to finish.".into();
            return;
        }
        self.video.pause();
        self.video.import_at = at;
        self.video.import_track = track;
        let (tx, rx) = mpsc::channel();
        self.video.importing = Some(rx);
        self.status = "Importing media…".into();
        std::thread::spawn(move || {
            let _ = tx.send(import_files(&paths));
        });
    }
    pub(super) fn poll_media_import(&mut self, ctx: &egui::Context) {
        if self.video.importing.is_some() {
            ctx.request_repaint_after(Duration::from_millis(60));
        }
        let result = self
            .video
            .importing
            .as_ref()
            .and_then(|r| r.try_recv().ok());
        if let Some(result) = result {
            self.video.importing = None;
            match result {
                Err(e) => self.status = e,
                Ok(clips) => {
                    self.append_media_clips(clips, self.video.import_at, self.video.import_track);
                }
            }
        }
    }
    pub(super) fn append_media_clips(&mut self, clips: Vec<Segment>, at: u64, track: u64) {
        let Some(mut p) = self.session.document.video.clone() else {
            return;
        };
        let first = (!clips.is_empty()).then_some(p.segments.len());
        p = plan_import(&p, &clips, at, track);
        p.cursor = at;
        self.commit_video(p);
        if let Some(index) = first
            && self
                .session
                .document
                .video
                .as_ref()
                .is_some_and(|p| index < p.segments.len())
        {
            self.select_only(None);
            self.video.selected = Selection::Recording(index);
            self.set_timeline_expanded(true);
            self.status="Media added. Drag a clip to move it; top handles set fades. Shift temporarily bypasses snapping.".into();
        }
    }
    pub(crate) fn append_recording(&mut self, doc: &Document, new_track: bool) {
        let Some(v) = &doc.video else { return };
        let Some(mut target) = self.session.document.video.clone() else {
            return;
        };
        let track = if new_track {
            target.add_media_track()
        } else {
            *target.media_tracks.last().unwrap()
        };
        let at = if new_track {
            target.cursor
        } else {
            target.duration()
        };
        let mut clips = v.segments.clone();
        for c in &mut clips {
            if c.source.is_none() {
                c.source = Some(Source {
                    path: v.source.clone(),
                    size: v.source_size,
                    duration: v.source_duration,
                    kind: MediaKind::Video,
                });
            }
        }
        let mut cursor = at;
        for mut c in clips {
            c.id = target.next_media_id();
            c.at = cursor;
            c.track = track;
            cursor = c.until();
            target.segments.push(c);
        }
        // Recording plus its track is one undoable edit.
        target.cursor = at;
        let index = target.segments.len() - 1;
        self.commit_video(target);
        self.select_only(None);
        self.video.selected = Selection::Recording(index);
        self.set_timeline_expanded(true);
    }
}
pub(super) fn import_files(paths: &[PathBuf]) -> Result<Vec<Segment>, String> {
    if paths.is_empty() || paths.len() > 64 {
        return Err("Import between 1 and 64 media files at a time.".into());
    }
    #[cfg(windows)]
    let _runtime = media::Runtime::new().map_err(|e| e.to_string())?;
    let mut clips = vec![];
    for path in paths {
        if !path.is_file()
            || !capture_core::video::editor::local_media_path(&path.to_string_lossy())
        {
            return Err("Choose local media files.".into());
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let kind = if ["png", "jpg", "jpeg", "webp"].contains(&ext.as_str()) {
            MediaKind::Image
        } else if ["wav", "mp3", "m4a", "aac", "flac"].contains(&ext.as_str()) {
            MediaKind::Audio
        } else {
            MediaKind::Video
        };
        let (size, duration, has_audio) = if kind == MediaKind::Image {
            let size = image::ImageReader::open(path)
                .map_err(|e| e.to_string())?
                .with_guessed_format()
                .map_err(|e| e.to_string())?
                .into_dimensions()
                .map_err(|e| e.to_string())?;
            if size.0 == 0
                || size.1 == 0
                || size.0 > 8192
                || size.1 > 8192
                || size.0 as u64 * size.1 as u64 > capture_core::MAX_PIXELS
            {
                return Err("Choose an image up to 25 megapixels and 8192 pixels per side.".into());
            }
            ([size.0, size.1], 5000, false)
        } else {
            #[cfg(windows)]
            {
                if kind == MediaKind::Audio {
                    let sound = media::Sound::open(path)?;
                    ([1, 1], sound.duration()?, true)
                } else {
                    let v = media::Video::open(path)?;
                    (v.size, v.duration, v.has_audio)
                }
            }
            #[cfg(not(windows))]
            {
                return Err("Media import requires Windows.".into());
            }
        };
        let source = Source {
            path: path.to_string_lossy().into_owned(),
            size,
            duration: if kind == MediaKind::Image {
                3_600_000
            } else {
                duration
            },
            kind,
        };
        let mut clip = Segment::new(1, 0, 1, Some(source), duration);
        if has_audio {
            clip.audio.push(Audio {
                path: path.to_string_lossy().into_owned(),
                label: if kind == MediaKind::Audio {
                    "Audio".into()
                } else {
                    "Linked audio".into()
                },
                volume: 1.,
                muted: false,
                fade_in: 0,
                fade_out: 0,
            });
        }
        clips.push(clip);
    }
    Ok(clips)
}

pub(super) fn plan_import(original: &Project, clips: &[Segment], at: u64, track: u64) -> Project {
    let mut p = original.clone();
    let mut cursor = at;
    for mut clip in clips.iter().cloned() {
        clip.id = p.next_media_id();
        clip.at = cursor;
        clip.track = track;
        let incompatible = p.media_kinds.get(&track).is_some_and(|kind| {
            matches!(
                kind,
                capture_core::video::editor::TrackKind::Audio
                    | capture_core::video::editor::TrackKind::Voiceover
            ) != (clip.kind() == MediaKind::Audio)
        });
        if incompatible
            || p.segments.iter().any(|c| {
                c.track == track
                    && c.at < clip.until()
                    && clip.at < c.until()
                    && (c.kind() == MediaKind::Audio) != (clip.kind() == MediaKind::Audio)
            })
        {
            clip.track = p.add_typed_track(if clip.kind() == MediaKind::Audio {
                capture_core::video::editor::TrackKind::Audio
            } else {
                capture_core::video::editor::TrackKind::Video
            });
        }
        cursor = clip.until();
        p.segments.push(clip);
    }
    p
}
