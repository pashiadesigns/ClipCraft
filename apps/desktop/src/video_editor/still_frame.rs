use super::*;
pub(crate) struct Dialog {
    pub clip: u64,
    pub at: u64,
    pub original: Project,
    pub duration: f64,
    pub all_tracks: bool,
    pub error: String,
    pub job: Option<mpsc::Receiver<Result<PathBuf, String>>>,
}
impl Editor {
    pub(super) fn open_still_frame(&mut self, clip: u64) {
        let Some(p) = self.session.document.video.clone() else {
            return;
        };
        self.video.pause();
        self.video.still_frame = Some(Dialog {
            clip,
            at: p.cursor,
            original: p,
            duration: 3.,
            all_tracks: false,
            error: String::new(),
            job: None,
        });
    }
    pub(crate) fn show_still_frame(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.video.still_frame.take() else {
            return;
        };
        if let Some(result) = state.job.as_ref().and_then(|j| j.try_recv().ok()) {
            state.job = None;
            match result {
                Ok(path) => {
                    if !self
                        .session
                        .document
                        .video
                        .as_ref()
                        .is_some_and(|p| p.same_edit(&state.original))
                    {
                        state.error =
                            "The project changed. Close this window and choose the frame again."
                                .into();
                    } else {
                        match state.original.insert_still(
                            state.clip,
                            state.at,
                            (state.duration * 1000.).round() as u64,
                            path.to_string_lossy().into_owned(),
                            state.all_tracks,
                        ) {
                            Ok(p) => {
                                let id = p.segments.last().unwrap().id;
                                let mut document = self.session.document.clone();
                                document.video = Some(p.clone());
                                match document.validate() {
                                    Ok(()) => {
                                        self.commit_video(p);
                                        self.timeline_focus(
                                            capture_core::video::editing::Item::Media(id),
                                            false,
                                        );
                                        return;
                                    }
                                    Err(e) => state.error = e.to_string(),
                                }
                            }
                            Err(e) => state.error = e.to_string(),
                        }
                    }
                }
                Err(e) => state.error = e,
            }
        }
        let mut open = true;
        let mut generate = false;
        let mut cancel = false;
        egui::Window::new("Generate still frame").id(egui::Id::unique("generate-still-frame"))
            .default_width(420.).resizable(false).collapsible(false).open(&mut open).show(ctx,|ui|{
                ui.label("Hold the selected video's frame at the playhead.");
                ui.add_space(8.);
                ui.add_enabled_ui(state.job.is_none(),|ui|{
                    ui.horizontal(|ui|{ui.label("Hold duration");ui.add(egui::DragValue::new(&mut state.duration).range(0.1..=14400.).speed(0.1).suffix(" seconds"));});
                    crate::controls::toggle(ui,&mut state.all_tracks,"Shift all tracks to make room");
                    ui.small(if state.all_tracks {"Later content shifts; annotations spanning this frame extend with it."}else{"Later clips on this video and its linked tracks shift. Other tracks keep their timing."});
                });
                ui.add_space(12.);
                if state.job.is_some(){ui.spinner();ui.label("Generating frame…");ctx.request_repaint_after(Duration::from_millis(30));}
                if !state.error.is_empty(){ui.colored_label(egui::Color32::from_rgb(246,105,112),&state.error);}
                ui.horizontal(|ui|{
                    generate=ui.add_enabled(state.job.is_none(),crate::controls::dialog_button("Insert still frame",true)).clicked();
                    cancel=ui.add(crate::controls::dialog_button("Cancel",false)).clicked();
                });
                if ui.input(|i|i.key_pressed(egui::Key::Enter))&&state.job.is_none(){generate=true;}
            });
        if !open || cancel || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            return;
        }
        if generate {
            let folder = self.history.root().map(|r| r.join("stills"));
            if let Some(folder) = folder {
                let (tx, rx) = mpsc::channel();
                let p = state.original.clone();
                let id = state.clip;
                let at = state.at;
                let context = ctx.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(generate_frame(&p, id, at, &folder));
                    context.request_repaint();
                });
                state.error.clear();
                state.job = Some(rx);
            } else {
                state.error = "Choose a project storage folder in Settings, then try again.".into();
            }
        }
        self.video.still_frame = Some(state);
    }
}
pub(super) fn generate_frame(
    p: &Project,
    id: u64,
    at: u64,
    folder: &Path,
) -> Result<PathBuf, String> {
    #[cfg(windows)]
    {
        let c = p
            .segments
            .iter()
            .find(|c| c.id == id && c.kind() == capture_core::video::editor::MediaKind::Video)
            .ok_or("Select a video clip.")?;
        if at <= c.at || at >= c.until() {
            return Err("Move the playhead inside this video clip.".into());
        }
        let _runtime = media::Runtime::new().map_err(|e| e.to_string())?;
        let source = c.source.as_ref().map_or(&p.source, |s| &s.path);
        let pixels = media::Video::open(Path::new(source))?.frame(c.source_time(at))?;
        std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let path = folder.join(format!("Still-{stamp}-{id}.png"));
        pixels.save(&path).map_err(|e| e.to_string())?;
        let _ = capture_core::history::media_cleanup::mark_created(&path);
        Ok(path)
    }
    #[cfg(not(windows))]
    {
        let _ = (p, id, at, folder);
        Err("Video decoding requires Windows.".into())
    }
}
