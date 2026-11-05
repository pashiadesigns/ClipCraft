use super::*;
use egui::{Color32, Stroke};
const BLUE: Color32 = Color32::from_rgb(73, 197, 233);
const MUTED: Color32 = Color32::from_rgb(177, 193, 200);
fn card(ui: &mut egui::Ui, height: f32, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(Color32::from_rgb(35, 45, 50))
        .stroke(Stroke::new(1., Color32::from_rgb(70, 87, 94)))
        .corner_radius(7)
        .inner_margin(12.)
        .show(ui, |ui| {
            ui.set_min_height(height);
            ui.set_width((ui.available_width()).max(30.));
            body(ui);
        });
}
impl Editor {
    pub(super) fn audio_analysis_ui(
        &mut self,
        ui: &mut egui::Ui,
        _chain: &mut Chain,
        _commit: &mut bool,
    ) {
        let report = self.video.fx.report.clone();
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(if report.is_some() {
                    "Your voice, made clearer"
                } else {
                    "Find a starting point for your voice"
                })
                .size(20.)
                .strong(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.small(if report.is_some() {
                    "2 / 2 · REVIEW"
                } else {
                    "1 / 2 · ANALYZE"
                });
            });
        });
        if let Some(job) = &self.video.fx.analysis {
            ui.add_space(35.);
            card(ui, 160., |ui| {
                ui.heading("Listening to the recording");
                ui.label("Measuring level, changes between phrases and tonal balance.");
                ui.add_space(20.);
                ui.add(
                    egui::ProgressBar::new(job.progress.load(Ordering::Relaxed) as f32 / 100.)
                        .show_percentage(),
                );
                ui.small("Runs locally · original audio stays untouched");
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(crate::controls::dialog_button("Cancel analysis", false))
                    .clicked()
                {
                    self.video.fx.analysis = None;
                }
            });
            return;
        }
        if let Some(warning) = report.as_ref().and_then(|r| r.warnings.first()) {
            ui.colored_label(Color32::from_rgb(238, 187, 113), warning);
        } else {
            ui.colored_label(MUTED,if report.is_some(){"Review the measured voice profile, then listen before applying the suggested chain."}else{"Choose a source and a voice character. We’ll suggest editable settings for your vocal chain."});
        }
        ui.add_space(4.);
        if let Some(report) = report {
            ui.columns(3, |cols| {
                card(&mut cols[0], 85., |ui| {
                    ui.small("RECORDING LEVEL");
                    ui.label(
                        egui::RichText::new(if report.clipped_fraction > 0. {
                            "Peaks are clipping"
                        } else if report.rms_db < -32. {
                            "A quiet recording"
                        } else {
                            "Room to work"
                        })
                        .strong()
                        .size(16.),
                    );
                    ui.add(
                        egui::ProgressBar::new(((report.rms_db + 60.) / 60.).clamp(0., 1.) as f32)
                            .fill(BLUE),
                    );
                    ui.small(format!(
                        "{:.1} dBFS RMS · peak {:.1} dBFS",
                        report.rms_db, report.peak_db
                    ));
                });
                card(&mut cols[1], 85., |ui| {
                    ui.small("PHRASE DYNAMICS");
                    ui.label(
                        egui::RichText::new(if report.dynamic_db > 16. {
                            "Wide level changes"
                        } else {
                            "Fairly even phrases"
                        })
                        .strong()
                        .size(16.),
                    );
                    ui.add(
                        egui::ProgressBar::new((report.dynamic_db / 36.).clamp(0., 1.) as f32)
                            .fill(Color32::from_rgb(231, 184, 115)),
                    );
                    ui.small(format!(
                        "{:.1} dB range · {:.1} s analyzed",
                        report.dynamic_db, report.seconds
                    ));
                });
                card(&mut cols[2], 85., |ui| {
                    ui.small("TONAL BALANCE");
                    let (r, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), 48.),
                        egui::Sense::hover(),
                    );
                    let max = report.bands.iter().copied().fold(0f64, f64::max).max(1e-10);
                    for (i, v) in report.bands.iter().enumerate() {
                        let w = r.width() / 6.;
                        let h = (v / max).sqrt() as f32 * 43.;
                        ui.painter().rect_filled(
                            egui::Rect::from_min_max(
                                egui::pos2(r.left() + i as f32 * w + 3., r.bottom() - h),
                                egui::pos2(r.left() + (i + 1) as f32 * w - 3., r.bottom()),
                            ),
                            2.,
                            BLUE,
                        );
                    }
                    ui.small("Low / warmth / voice / presence / S / air");
                });
            });
            ui.add_space(2.);
            self.analysis_character(ui, true);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Suggested chain").strong());
                ui.weak("Processing order →");
            });
            let proposal = self.video.fx.proposal.clone();
            egui::ScrollArea::vertical()
                .id_salt("suggested-chain-cards")
                .max_height(184.)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if let Some(proposal) = proposal {
                        let columns = if ui.available_width() > 740. { 4 } else { 3 };
                        for row in proposal.effects.chunks(columns) {
                            ui.columns(columns, |cols| {
                                for (col, effect) in cols.iter_mut().zip(row) {
                                    card(col, 58., |ui| {
                                        ui.horizontal(|ui| {
                                            super::super::ui::icon(
                                                ui.painter(),
                                                egui::Rect::from_min_size(
                                                    ui.cursor().min,
                                                    egui::vec2(16., 16.),
                                                ),
                                                effect.kind.label(),
                                                BLUE,
                                            );
                                            ui.add_space(22.);
                                            ui.label(
                                                egui::RichText::new(effect.kind.short()).strong(),
                                            );
                                            if !effect.enabled {
                                                ui.weak("off");
                                            }
                                        });
                                        ui.colored_label(MUTED, workstation::reason(effect.kind));
                                        ui.small(workstation::suggestion(effect));
                                    });
                                }
                            });
                            ui.add_space(3.);
                        }
                    } else {
                        ui.colored_label(
                            Color32::from_rgb(238, 187, 113),
                            "A clearer recording is needed before suggesting effects.",
                        );
                    }
                    for warning in &report.warnings {
                        ui.colored_label(Color32::from_rgb(238, 187, 113), warning);
                    }
                });
            ui.small("Suggestions are a starting point. Check by ear; the final mix may need different levels.");
        } else {
            ui.columns(2, |cols| {
                card(&mut cols[0], 233., |ui| {
                    ui.label(
                        egui::RichText::new("1  Choose your recording")
                            .strong()
                            .size(16.),
                    );
                    ui.add_space(12.);
                    if self.video.fx.track_scope {
                        egui::ComboBox::from_id_salt("analysis-source")
                            .width(ui.available_width() - 8.)
                            .selected_text(if self.video.fx.selected_clip {
                                "Selected clip"
                            } else {
                                "Entire voiceover track"
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.video.fx.selected_clip,
                                    false,
                                    "Entire voiceover track",
                                );
                                ui.selectable_value(
                                    &mut self.video.fx.selected_clip,
                                    true,
                                    "Selected clip",
                                );
                            });
                    } else {
                        ui.label("Selected clip · independent effects");
                    }
                    ui.add_space(15.);
                    ui.label("What we measure");
                    for (icon, title, detail) in [
                        ("Audio", "Level", "Quiet voice, loud peaks and clipping"),
                        (
                            "Compressor",
                            "Consistency",
                            "Changes between softer and louder phrases",
                        ),
                        (
                            "Parametric EQ",
                            "Tone",
                            "Low rumble, voice presence and sharp consonants",
                        ),
                    ] {
                        ui.horizontal(|ui| {
                            super::super::ui::icon(
                                ui.painter(),
                                egui::Rect::from_min_size(ui.cursor().min, egui::vec2(18., 18.)),
                                icon,
                                BLUE,
                            );
                            ui.add_space(26.);
                            ui.vertical(|ui| {
                                ui.label(title);
                                ui.small(detail);
                            });
                        });
                    }
                });
                card(&mut cols[1], 233., |ui| {
                    ui.label(
                        egui::RichText::new("2  Choose the sound")
                            .strong()
                            .size(16.),
                    );
                    ui.add_space(12.);
                    self.analysis_character(ui, false);
                    ui.add_space(10.);
                    crate::controls::toggle(
                        ui,
                        &mut self.video.fx.preserve,
                        "Preserve bypassed effects",
                    );
                    ui.small("Effects you turned off stay off in the suggestion.");
                });
            });
            ui.add_space(12.);
            card(ui, 35., |ui| {
                ui.label("Listen → compare → apply");
                ui.small("Analyze up to two minutes locally. Preview the proposed chain and change any setting afterward.");
            });
            ui.add_space(12.);
        }
    }
    pub(super) fn audio_analysis_actions(
        &mut self,
        ui: &mut egui::Ui,
        chain: &mut Chain,
        commit: &mut bool,
    ) {
        if self.video.fx.analysis.is_some() {
            return;
        }
        if self.video.fx.report.is_some() {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(
                        self.video.fx.proposal.is_some(),
                        crate::controls::dialog_button(
                            if self.video.fx.track_scope {
                                "Apply to track"
                            } else {
                                "Apply to clip"
                            },
                            true,
                        ),
                    )
                    .clicked()
                {
                    *chain = self.video.fx.proposal.take().unwrap();
                    chain.mix *= self.video.fx.audition_mix;
                    self.video.fx.audition_mix = 1.;
                    self.video.fx.analysis_open = false;
                    self.video.fx.auditioning = false;
                    self.video.fx.original = false;
                    self.video.fx.selected = chain.effects.first().map_or(0, |f| f.id);
                    *commit = true;
                }
                if ui
                    .add(crate::controls::dialog_button("Cancel", false))
                    .clicked()
                {
                    self.video.fx.analysis_open = false;
                    self.video.fx.auditioning = false;
                    self.video.fx.original = false;
                    self.video.fx.proposal = None;
                    self.preview_chain(chain);
                }
            });
        } else {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(crate::controls::dialog_button("Analyze voice", true))
                    .clicked()
                {
                    self.begin_audio_analysis();
                }
                if ui
                    .add(crate::controls::dialog_button("Back to effects", false))
                    .clicked()
                {
                    self.video.fx.analysis_open = false;
                    self.video.fx.auditioning = false;
                }
            });
        }
    }
    fn analysis_character(&mut self, ui: &mut egui::Ui, compact: bool) {
        let before = (self.video.fx.character, self.video.fx.strength);
        let select = |ui: &mut egui::Ui, this: &mut Editor| {
            egui::ComboBox::from_id_salt(if compact {
                "analysis-review-character"
            } else {
                "analysis-character"
            })
            .width(if compact {
                180.
            } else {
                ui.available_width() - 8.
            })
            .selected_text(this.video.fx.character.label())
            .show_ui(ui, |ui| {
                for c in Character::ALL {
                    ui.selectable_value(&mut this.video.fx.character, c, c.label());
                }
            });
        };
        if compact {
            ui.horizontal(|ui| {
                select(ui, self);
                ui.add(egui::Slider::new(&mut self.video.fx.strength, 10. ..=85.).text("Strength"));
                ui.weak("Gentle → more noticeable");
            });
        } else {
            select(ui, self);
            ui.add_space(8.);
            ui.colored_label(
                MUTED,
                match self.video.fx.character {
                    Character::Natural => "Balanced speech with your natural tone intact.",
                    Character::Warm => "A fuller, softer voice with less sharpness.",
                    Character::Bright => "More definition to help words feel close and clear.",
                },
            );
            ui.add_space(12.);
            ui.label("How much enhancement?");
            ui.spacing_mut().slider_width = (ui.available_width() - 100.).max(80.);
            ui.add(egui::Slider::new(&mut self.video.fx.strength, 10. ..=85.).suffix("%"));
            ui.small("Gentle                                           More noticeable");
        }
        if before != (self.video.fx.character, self.video.fx.strength)
            && self.video.fx.report.is_some()
        {
            self.update_audio_suggestion();
            if self.video.fx.auditioning {
                self.preview_chain(&Chain::default());
            }
        }
    }
}
