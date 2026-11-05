use super::*;
use capture_core::video::effects::{
    Chain, Kind, Target,
    analysis::{Analyzer, Character, Report},
};
mod analysis_ui;
mod curve;
mod dynamics;
mod graph;
mod presets;
mod workstation;
pub(super) struct AnalysisJob {
    pub result: mpsc::Receiver<Result<Report, String>>,
    pub cancel: Arc<AtomicBool>,
    pub progress: Arc<std::sync::atomic::AtomicU32>,
}
impl Drop for AnalysisJob {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
pub(super) struct State {
    pub open: bool,
    pub clip: u64,
    pub track_scope: bool,
    pub selected: u64,
    pub band: usize,
    pub draft: Option<Chain>,
    pub position: Option<egui::Pos2>,
    pub dragging: Option<u64>,
    pub original: bool,
    pub audition_mix: f32,
    pub analysis_open: bool,
    pub auditioning: bool,
    pub character: Character,
    pub strength: f32,
    pub preserve: bool,
    pub selected_clip: bool,
    pub analysis: Option<AnalysisJob>,
    pub report: Option<Report>,
    pub proposal: Option<Chain>,
    pub error: String,
    pub owner: Option<(String, Option<PathBuf>)>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            open: false,
            clip: 1,
            track_scope: false,
            selected: 0,
            band: 2,
            draft: None,
            position: None,
            dragging: None,
            original: false,
            audition_mix: 1.,
            analysis_open: false,
            auditioning: false,
            character: Character::Natural,
            strength: 45.,
            preserve: true,
            selected_clip: false,
            analysis: None,
            report: None,
            proposal: None,
            error: String::new(),
            owner: None,
        }
    }
}
impl State {
    pub(super) fn target(&self, p: &Project) -> Target {
        if self.track_scope {
            Target::Track(
                p.segments
                    .iter()
                    .find(|c| c.id == self.clip)
                    .map_or(0, |c| c.track),
            )
        } else {
            Target::Clip(self.clip)
        }
    }
}
impl Editor {
    pub(super) fn focused_audio_clip(&self) -> Option<u64> {
        match self.video.selected {
            Selection::Recording(i) | Selection::Audio(i) => self
                .session
                .document
                .video
                .as_ref()?
                .segments
                .get(i)
                .filter(|c| !c.audio.is_empty())
                .map(|c| c.id),
            _ => None,
        }
    }
    fn follow_effects_clip(&mut self, id: u64) {
        if let Some(tx) = &self.video.audio_updates
            && let Some(p) = &self.session.document.video
        {
            let _ = tx.send(p.clone());
        }
        let fx = &mut self.video.fx;
        fx.clip = id;
        fx.draft = None;
        fx.analysis = None;
        fx.analysis_open = false;
        fx.report = None;
        fx.proposal = None;
        fx.auditioning = false;
        fx.original = false;
        fx.audition_mix = 1.;
        fx.selected = 0;
        fx.error.clear();
    }

    pub(crate) fn open_effects_workstation(&mut self) {
        if self.video.voiceover.take.is_some()
            || self.video.voiceover.recording.is_some()
            || self.video.voiceover.deadline.is_some()
            || self.video.voiceover.waiting
        {
            self.video.voiceover.error =
                "Keep or discard this take before opening the Effects Workstation.".into();
            return;
        }
        let Some(p) = &self.session.document.video else {
            return;
        };
        self.video.fx.clip = match self.video.selected {
            Selection::Recording(i) | Selection::Audio(i) => p.segments.get(i).map(|s| s.id),
            _ => None,
        }
        .unwrap_or_else(|| {
            p.segments
                .iter()
                .find(|c| !c.audio.is_empty())
                .map(|c| c.id)
                .unwrap_or(0)
        });
        self.video.fx.open = true;
        self.video.fx.owner = Some((p.source.clone(), self.path.clone()));
        self.video.fx.error.clear();
        self.video.fx.selected = p
            .effect_chain(self.video.fx.target(p))
            .and_then(|c| c.effects.first())
            .map(|f| f.id)
            .unwrap_or(0);
        self.video.fx.draft = None;
        self.video.voiceover = voiceover::State::default();
    }
    pub(super) fn start_audio_preview(&mut self, p: Project, at: u64) {
        if let Some(stop) = self.video.audio_stop.take() {
            stop.store(true, Ordering::Relaxed);
        }
        let job = workers::audio(
            p,
            at,
            self.quick_controls.audio_outputs().selected().to_owned(),
        );
        self.video.audio_stop = Some(job.stop);
        self.video.audio_error = Some(job.errors);
        self.video.audio_updates = Some(job.updates);
        self.video.audio_spectrum = Some(job.spectrum);
        self.video.audio_meters = Some(job.meters);
    }
    fn preview_chain(&mut self, chain: &Chain) {
        let Some(mut p) = self.session.document.video.clone() else {
            return;
        };
        let mut chain = if self.video.fx.analysis_open && self.video.fx.auditioning {
            self.video
                .fx
                .proposal
                .clone()
                .unwrap_or_else(|| chain.clone())
        } else {
            chain.clone()
        };
        chain.mix *= self.video.fx.audition_mix;
        if self.video.fx.original {
            chain.bypass = true;
        }
        p.set_effect_chain(self.video.fx.target(&p), chain);
        if let Some(tx) = &self.video.audio_updates {
            let _ = tx.send(p);
        }
    }
    fn commit_chain(&mut self, chain: Chain) {
        let Some(mut p) = self.session.document.video.clone() else {
            return;
        };
        p.set_effect_chain(self.video.fx.target(&p), chain.clone());
        self.execute_audio_edit(Command::Video { project: p });
        self.video.fx.draft = None;
        self.preview_chain(&chain);
    }
    pub(super) fn execute_audio_edit(&mut self, command: Command) {
        // The general command path pauses playback; retain this worker and its clock
        // while still using the normal session Undo, dirty-state and autosave handling.
        let before = self.session.document.video.clone();
        let spectrum = self.video.audio_spectrum.take();
        let run = (
            self.video.playing,
            self.video.clock,
            self.video.replay_end,
            self.video.audio_stop.take(),
            self.video.audio_updates.take(),
            self.video.audio_meters.take(),
        );
        self.execute(command);
        if !before
            .as_ref()
            .zip(self.session.document.video.as_ref())
            .is_some_and(|(a, b)| a.same_visual(b))
        {
            if let Some(stop) = run.3 {
                stop.store(true, Ordering::Relaxed);
            }
            return;
        }
        self.video.audio_spectrum = spectrum;
        self.video.playing = run.0;
        self.video.clock = run.1;
        self.video.replay_end = run.2;
        self.video.audio_stop = run.3;
        self.video.audio_updates = run.4;
        self.video.audio_meters = run.5;
    }
    pub(crate) fn audio_track_inserts(&mut self, ui: &mut egui::Ui) {
        let Some(p) = &self.session.document.video else {
            return;
        };
        let Some(track) = (match self.video.selected {
            Selection::Recording(i) | Selection::Audio(i) => p
                .segments
                .get(i)
                .filter(|c| !c.audio.is_empty())
                .map(|c| c.id),
            _ => None,
        }) else {
            return;
        };
        let source_info = match self.video.selected {
            Selection::Recording(i) | Selection::Audio(i) => p.segments.get(i).map(|c| {
                (
                    c.audio
                        .iter()
                        .map(|a| {
                            (
                                a.label.clone(),
                                Path::new(&a.path)
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .into_owned(),
                            )
                        })
                        .collect::<Vec<_>>(),
                    c.link.is_some(),
                )
            }),
            _ => None,
        };
        let scope = self.video.fx.track_scope;
        ui.horizontal(|ui| {
            ui.label("Effects on");
            ui.selectable_value(&mut self.video.fx.track_scope, false, "Clip");
            ui.selectable_value(&mut self.video.fx.track_scope, true, "Track");
        });
        if scope != self.video.fx.track_scope {
            self.follow_effects_clip(track);
        }
        let p = self.session.document.video.as_ref().unwrap();
        let target = if self.video.fx.track_scope {
            Target::Track(p.segments.iter().find(|c| c.id == track).unwrap().track)
        } else {
            Target::Clip(track)
        };
        let mut chain = p.effect_chain(target).cloned().unwrap_or_default();
        let mut mono = p.segments.iter().find(|c| c.id == track).unwrap().mono;
        let mut routing = false;
        ui.horizontal(|ui| {
            ui.label("Channels");
            routing |= ui.selectable_value(&mut mono, false, "Stereo").changed();
            routing |= ui
                .selectable_value(&mut mono, true, "Mono")
                .on_hover_text("Merge left and right input into both speakers")
                .changed();
        });
        if routing {
            let mut next = p.clone();
            next.segments
                .iter_mut()
                .find(|c| c.id == track)
                .unwrap()
                .mono = mono;
            self.execute_audio_edit(Command::Video { project: next });
        }
        let mut changed = false;
        let mut open = None;
        super::property_design::heading(ui, "Audio effects", "Vocal effects");
        for (i, effect) in chain.effects.iter_mut().enumerate() {
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(37, 45, 48))
                .corner_radius(4)
                .inner_margin(egui::Margin {
                    left: 8,
                    right: 17,
                    top: 8,
                    bottom: 8,
                })
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if effect_row(
                            ui,
                            effect,
                            i,
                            (ui.available_width() - ui.spacing().item_spacing.x - 26.).max(40.),
                        )
                        .clicked()
                        {
                            open = Some(effect.id);
                        }
                        if super::ui::button(ui, "Effect power", "", 26., effect.enabled)
                            .on_hover_text("Enable / bypass effect")
                            .clicked()
                        {
                            effect.enabled = !effect.enabled;
                            changed = true;
                        }
                    });
                });
        }
        if super::ui::button(
            ui,
            "Audio effects",
            "Open Effects Workstation",
            ui.available_width(),
            true,
        )
        .clicked()
        {
            open = Some(chain.effects.first().map(|e| e.id).unwrap_or(0));
        }
        super::property_design::end(ui);
        let summary = source_info
            .as_ref()
            .map(|(sources, linked)| {
                format!(
                    "{} source{} · {}",
                    sources.len(),
                    if sources.len() == 1 { "" } else { "s" },
                    if *linked { "Linked" } else { "Independent" }
                )
            })
            .unwrap_or_default();
        super::property_design::group(ui, "Link", "Source & link", &summary, |ui| {
            if let Some((sources, linked)) = &source_info {
                for (label, filename) in sources {
                    ui.label(egui::RichText::new(label).strong());
                    ui.add(egui::Label::new(filename).truncate())
                        .on_hover_text(filename);
                }
                ui.small(if *linked {
                    "Timing linked · change linking in Timing"
                } else {
                    "Independent clip · original audio preserved"
                });
                ui.add_space(8.);
            }
            if super::ui::button(
                ui,
                "Microphone",
                "Record narration",
                ui.available_width(),
                false,
            )
            .clicked()
            {
                self.open_voiceover();
            }
        });
        if changed {
            self.video.fx.clip = track;
            self.commit_chain(chain);
        }
        if let Some(id) = open {
            self.open_effects_workstation();
            self.video.fx.clip = track;
            self.video.fx.selected = id;
        }
    }
    pub(super) fn poll_audio_analysis(&mut self, ctx: &egui::Context) {
        let owner = self
            .session
            .document
            .video
            .as_ref()
            .map(|p| (p.source.clone(), self.path.clone()));
        if self.video.fx.open && owner != self.video.fx.owner {
            self.video.fx = State::default();
            return;
        }
        if let Some(result) = self
            .video
            .fx
            .analysis
            .as_ref()
            .and_then(|job| job.result.try_recv().ok())
        {
            self.video.fx.analysis = None;
            match result {
                Ok(report) => {
                    self.video.fx.report = Some(report);
                    self.update_audio_suggestion();
                }
                Err(e) => self.video.fx.error = e,
            }
        }
        if self.video.fx.analysis.is_some() {
            ctx.request_repaint_after(Duration::from_millis(80));
        }
    }
    fn update_audio_suggestion(&mut self) {
        let fx = &mut self.video.fx;
        let existing = self
            .session
            .document
            .video
            .as_ref()
            .and_then(|p| p.effect_chain(fx.target(p)))
            .cloned()
            .unwrap_or_default();
        if let Some(r) = &fx.report {
            match r.suggest(fx.character, fx.strength, &existing, fx.preserve) {
                Ok(chain) => {
                    fx.proposal = Some(chain);
                    fx.auditioning = true;
                }
                Err(e) => {
                    fx.proposal = None;
                    fx.error = e.to_string();
                }
            }
        }
    }
    pub(crate) fn begin_audio_analysis(&mut self) {
        let Some(mut p) = self.session.document.video.clone() else {
            return;
        };
        let target = self.video.fx.target(&p);
        let clip = if self.video.fx.selected_clip {
            match self.video.selected {
                Selection::Recording(i) | Selection::Audio(i) => p.segments.get(i).map(|c| c.id),
                _ => None,
            }
        } else {
            None
        };
        if self.video.fx.selected_clip && clip.is_none() {
            self.video.fx.error =
                "Select a voiceover clip on the timeline, or analyze the entire track.".into();
            return;
        }
        p.segments.retain(|c| {
            (match target {
                Target::Clip(id) => c.id == id,
                Target::Track(id) => c.track == id,
            }) && clip.is_none_or(|id| c.id == id)
                && c.audio.iter().any(|a| !a.muted)
        });
        p.clip_effects.clear();
        p.track_effects.clear();
        p.media_levels.clear();
        // Analyze the dry source even when its timeline track is currently muted.
        for settings in &mut p.track_settings {
            settings.muted = false;
        }
        for c in &mut p.segments {
            for a in &mut c.audio {
                a.volume = 1.;
                a.fade_in = 0;
                a.fade_out = 0;
            }
            c.fade_in = 0;
            c.fade_out = 0;
        }
        self.video.fx.error.clear();
        self.video.fx.report = None;
        self.video.fx.proposal = None;
        self.video.fx.analysis = Some(analyze(p));
    }
}
pub(super) fn track_label(p: &Project, id: u64) -> String {
    p.segments
        .iter()
        .find(|c| c.id == id)
        .map(|c| c.label.clone())
        .unwrap_or_else(|| "Select audio clip".into())
}

fn analyze(project: Project) -> AnalysisJob {
    let (tx, result) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let stop = cancel.clone();
    let progress = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let meter = progress.clone();
    std::thread::spawn(move || {
        #[cfg(windows)]
        let run = || -> Result<Report, String> {
            let _runtime = media::Runtime::new().map_err(|e| e.to_string())?;
            let mut mixer = media::Mixer::new(&project)?;
            let start = project.segments.iter().map(|c| c.at).min().ok_or(
                "This track has no audible clips. Choose a voiceover track or unmute its audio.",
            )?;
            let end = project
                .segments
                .iter()
                .map(|c| c.until())
                .max()
                .unwrap_or(start)
                .min(start + 120_000);
            let mut at = start * 48;
            let end = end * 48;
            let mut analyzer = Analyzer::default();
            while at < end {
                if stop.load(Ordering::Relaxed) {
                    return Err("Analysis canceled.".into());
                }
                let n = (end - at).min(2400) as usize;
                analyzer.push(&mixer.samples(&project, at, n)?);
                at += n as u64;
                meter.store(
                    ((at - start * 48) * 100 / (end - start * 48).max(1)) as u32,
                    Ordering::Relaxed,
                );
            }
            Ok(analyzer.report())
        };
        #[cfg(windows)]
        let result = run();
        #[cfg(not(windows))]
        let result = {
            let _ = (project, stop, meter);
            Err("Audio analysis requires Windows.".into())
        };
        let _ = tx.send(result);
    });
    AnalysisJob {
        result,
        cancel,
        progress,
    }
}

#[cfg(test)]
mod tests;

fn effect_row(
    ui: &mut egui::Ui,
    effect: &capture_core::video::effects::Effect,
    index: usize,
    width: f32,
) -> egui::Response {
    let (r, response) = ui.allocate_exact_size(egui::vec2(width, 46.), egui::Sense::click());
    let p = ui.painter();
    if response.hovered() {
        p.rect_filled(r, 4., egui::Color32::from_rgb(54, 64, 69));
    }
    super::ui::icon(
        p,
        egui::Rect::from_center_size(r.left_center() + egui::vec2(14., 0.), egui::vec2(17., 17.)),
        effect.kind.label(),
        crate::panel_ui::MUTED,
    );
    let p = p.with_clip_rect(r.shrink2(egui::vec2(2., 0.)));
    p.text(
        r.min + egui::vec2(30., 5.),
        egui::Align2::LEFT_TOP,
        format!("{}  {}", index + 1, effect.kind.short()),
        crate::panel_ui::font(12.),
        egui::Color32::WHITE,
    );
    let summary = if !effect.enabled {
        "Bypassed".into()
    } else if effect.kind == Kind::Equalizer {
        format!("{} bands · {:.1} dB", effect.bands.len(), effect.values[0])
    } else {
        let param = effect.kind.parameters();
        format!(
            "{} {:.1} {}",
            param[0].label, effect.values[0], param[0].unit
        )
    };
    p.text(
        r.min + egui::vec2(30., 25.),
        egui::Align2::LEFT_TOP,
        summary,
        egui::FontId::proportional(11.),
        crate::panel_ui::MUTED,
    );
    response.on_hover_text(format!("Open {} settings", effect.kind.label()))
}
