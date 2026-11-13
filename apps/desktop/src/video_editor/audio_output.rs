use super::*;
type Devices = Arc<std::sync::Mutex<Option<Result<Vec<(String, String)>, String>>>>;
fn devices(ctx: &egui::Context) -> Devices {
    let key = egui::Id::unique("audio-output-devices");
    if let Some(value) = ctx.data(|d| d.get_temp::<Devices>(key)) {
        return value;
    }
    let value: Devices = Default::default();
    ctx.data_mut(|d| d.insert_temp(key, value.clone()));
    let out = value.clone();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        #[cfg(windows)]
        let result = (|| {
            let _runtime = media::Runtime::new().map_err(|e| e.to_string())?;
            crate::capture::windows::video_audio::endpoint_devices(true).map_err(|e| e.to_string())
        })();
        #[cfg(not(windows))]
        let result = Err("Audio outputs require Windows.".into());
        *out.lock().unwrap_or_else(|e| e.into_inner()) = Some(result);
        ctx.request_repaint();
    });
    value
}
impl Editor {
    pub(super) fn audio_output_controls(&mut self, ui: &mut egui::Ui, monitor_only: bool) {
        let before = self.quick_controls.audio_outputs();
        let mut current = before.clone();
        ui.horizontal_wrapped(|ui| {
            if !monitor_only {
                if super::ui::button(ui, "Audio", "Speakers", 100., !current.use_headphones)
                    .clicked()
                {
                    current.use_headphones = false;
                }
                if super::ui::button(ui, "Headphones", "Headphones", 116., current.use_headphones)
                    .clicked()
                {
                    current.use_headphones = true;
                }
            }
            ui.menu_button(
                if monitor_only {
                    "Headphones output…"
                } else {
                    "Output devices…"
                },
                |ui| {
                    ui.set_min_width(260.);
                    let shared = devices(ui.ctx());
                    let loaded = shared.lock().unwrap_or_else(|e| e.into_inner()).clone();
                    match loaded {
                        Some(Ok(choices)) => {
                            // A single popup owns the list; nesting ComboBox popups inside
                            // a menu closes the parent before its item can be selected.
                            let key = ui.scope_id().with("output-role");
                            let mut headphones = monitor_only
                                || ui.ctx().data(|d| {
                                    d.get_temp::<bool>(key).unwrap_or(current.use_headphones)
                                });
                            if !monitor_only {
                                crate::controls::segments(ui, |ui| {
                                    ui.selectable_value(&mut headphones, false, "Speakers");
                                    ui.selectable_value(&mut headphones, true, "Headphones");
                                });
                                ui.ctx().data_mut(|d| d.insert_temp(key, headphones));
                            }
                            let id = if headphones {
                                &mut current.headphones
                            } else {
                                &mut current.speakers
                            };
                            ui.separator();
                            egui::ScrollArea::vertical()
                                .max_height(280.)
                                .show(ui, |ui| {
                                    for (key, name) in &choices {
                                        ui.selectable_value(id, key.clone(), name);
                                    }
                                });
                        }
                        Some(Err(error)) => {
                            ui.colored_label(egui::Color32::LIGHT_RED, error);
                        }
                        None => {
                            ui.spinner();
                            ui.label("Finding audio outputs…");
                        }
                    }
                    if ui.button("Refresh outputs").clicked() {
                        ui.ctx().data_mut(|d| {
                            d.remove::<Devices>(egui::Id::unique("audio-output-devices"))
                        });
                    }
                },
            );
        });
        if current != before {
            self.quick_controls.remember_audio_outputs(current.clone());
            if let Some(capture) = &self.video.voiceover.capture {
                let _ = capture.output_device.send(current.headphones.clone());
            }
            if self.video.playing
                && current.selected() != before.selected()
                && let Some(project) = self.session.document.video.clone()
            {
                let at = project.cursor;
                self.start_audio_preview(project, at);
            }
        }
    }
}
