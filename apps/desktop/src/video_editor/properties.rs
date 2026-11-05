use super::*;
use capture_core::video::editor::Animation;
fn animation_stack(
    ui: &mut egui::Ui,
    a: &crate::Annotation,
    timing: &mut capture_core::video::editor::Timing,
    exit: bool,
) {
    let primary = if exit { timing.exit } else { timing.entrance };
    let fallback = if exit {
        timing.exit_ms
    } else {
        timing.entrance_ms
    };
    let durations = if exit {
        &mut timing.exit_durations
    } else {
        &mut timing.entrance_durations
    };
    let maximum = timing.end - timing.start;
    let stack = if exit {
        &mut timing.exit_stack
    } else {
        &mut timing.entrance_stack
    };
    ui.push_id(("animation-stack", exit), |ui| {
        let mut remove = None;
        for (i, effect) in stack.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(effect.label(exit));
                seconds_field(
                    ui,
                    "",
                    durations.entry(*effect).or_insert(fallback),
                    maximum,
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if super::ui::button(ui, "Close", "", 26., false)
                        .on_hover_text("Remove this animation")
                        .clicked()
                    {
                        remove = Some(i);
                    }
                });
            });
        }
        if let Some(i) = remove {
            durations.remove(&stack.remove(i));
        }
        ui.menu_button("+", |ui| {
            ui.label("Add compatible animation");
            for effect in Animation::ALL {
                if effect.supports(a)
                    && capture_core::video::editor::Timing::stack_compatible(primary, stack, effect)
                    && ui.button(effect.label(exit)).clicked()
                {
                    stack.push(effect);
                    ui.close();
                }
            }
        })
        .response
        .on_hover_text("Combine body, outline and text animations");
    });
}
fn animation_parts_preview(
    parts: &[crate::Annotation],
    viewport: capture_core::display::Viewport,
) -> capture_core::Result<image::RgbaImage> {
    let mut output = image::RgbaImage::new(viewport.size[0], viewport.size[1]);
    for a in parts {
        let pixels = capture_core::display::render_annotation_preview(a, viewport)?;
        image::imageops::overlay(&mut output, &pixels, 0, 0);
    }
    Ok(output)
}
pub(super) struct Preview {
    objects: Vec<crate::Annotation>,
    viewport: capture_core::display::Viewport,
    texture: egui::TextureHandle,
}
pub(super) fn seconds_field(ui: &mut egui::Ui, label: &str, value: &mut u64, maximum: u64) {
    seconds_bounded(ui, label, value, 0, maximum);
}
fn seconds_bounded(ui: &mut egui::Ui, label: &str, value: &mut u64, minimum: u64, maximum: u64) {
    ui.label(label);
    let mut s = *value as f64 / 1000.;
    if ui
        .add_sized(
            [ui.available_width(), 28.],
            egui::DragValue::new(&mut s)
                .range(minimum as f64 / 1000. ..=maximum as f64 / 1000.)
                .speed(0.01)
                .custom_formatter(|v, _| {
                    format!("{:02}:{:06.3}", (v / 60.).floor() as u64, v % 60.)
                })
                .custom_parser(|s| {
                    if let Some((m, sec)) = s.split_once(':') {
                        Some(m.trim().parse::<f64>().ok()? * 60. + sec.trim().parse::<f64>().ok()?)
                    } else {
                        s.trim().parse().ok()
                    }
                }),
        )
        .changed()
    {
        *value = (s * 1000.).round() as u64;
    }
}
impl Editor {
    fn video_animation_preview(
        &mut self,
        ui: &mut egui::Ui,
        a: &crate::Annotation,
        t: &capture_core::video::editor::Timing,
    ) {
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 114.), egui::Sense::hover());
        ui.painter()
            .rect_filled(rect, 5., egui::Color32::from_rgb(24, 29, 32));
        let (lo, hi) = a.bounds();
        let scale = ((rect.width() - 12.) as f64 / (hi[0] - lo[0]).max(1.) as f64)
            .min(76. / (hi[1] - lo[1]).max(1.) as f64)
            .min(1.);
        let viewport = capture_core::display::Viewport {
            size: [rect.width().ceil() as u32, 114],
            origin: [
                lo[0] as f64 - (rect.width() as f64 / scale - (hi[0] - lo[0]) as f64) / 2.,
                lo[1] as f64 - (114. / scale - (hi[1] - lo[1]) as f64) / 2.,
            ],
            scale,
        };
        let mut sample = t.clone();
        sample.start = 0;
        sample.end = sample.animation_span(false) + sample.animation_span(true) + 500;
        let t = &sample;
        let at = if let Some(clock) = self.video.animation_clock {
            let elapsed = clock.elapsed().as_millis() as u64;
            ui.ctx().request_repaint_after(Duration::from_millis(16));
            if elapsed >= t.end - t.start {
                self.video.animation_clock = None;
            }
            t.start + elapsed.min(t.end - t.start - 1)
        } else {
            t.start + t.animation_span(false)
        };
        let objects = capture_core::video::editor::animated_parts(a, t, at.min(t.end - 1));
        if !objects.is_empty() {
            if self
                .video
                .animation_preview
                .as_ref()
                .is_none_or(|p| p.objects != objects || p.viewport != viewport)
                && let Ok(pixels) = animation_parts_preview(&objects, viewport)
            {
                let texture = ui.ctx().load_texture(
                    "animation-preview",
                    egui::ColorImage::from_rgba_unmultiplied(
                        [pixels.width() as usize, pixels.height() as usize],
                        pixels.as_raw(),
                    ),
                    egui::TextureOptions::LINEAR,
                );
                self.video.animation_preview = Some(Preview {
                    objects,
                    viewport,
                    texture,
                });
            }
            if let Some(preview) = &self.video.animation_preview {
                ui.painter().image(
                    preview.texture.id(),
                    rect,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                    egui::Color32::WHITE,
                );
            }
        }
        ui.painter().text(
            rect.center_bottom() - egui::vec2(0., 11.),
            egui::Align2::CENTER_CENTER,
            "Preview only · playhead stays in place",
            egui::FontId::proportional(10.),
            crate::panel_ui::MUTED,
        );
    }
    pub(crate) fn video_property_tabs(&mut self, ui: &mut egui::Ui) {
        let has_audio = match self.video.selected {
            Selection::Recording(i) | Selection::Audio(i) => self
                .session
                .document
                .video
                .as_ref()
                .and_then(|p| p.segments.get(i))
                .is_some_and(|c| !c.audio.is_empty()),
            _ => false,
        };
        let tabs: Vec<_> = PropertyTab::ALL
            .into_iter()
            .filter(|t| *t != PropertyTab::Audio || has_audio)
            .collect();
        if !has_audio && self.video.tab == PropertyTab::Audio {
            self.video.tab = PropertyTab::Style;
        }
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(12, 0))
            .show(ui, |ui| {
                crate::panel_ui::rail(ui, |ui| {
                    let width = ((ui.available_width() - (tabs.len() - 1) as f32 * 4.)
                        / tabs.len() as f32)
                        .max(24.);
                    let labels_fit = tabs.iter().all(|tab| {
                        ui.painter()
                            .layout_no_wrap(
                                tab.label().into(),
                                egui::FontId::proportional(12.),
                                egui::Color32::WHITE,
                            )
                            .size()
                            .x
                            + 35.
                            <= width
                    });
                    #[cfg(test)]
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(egui::Id::unique("video-tabs-labels"), labels_fit)
                    });
                    ui.horizontal(|ui| {
                        for tab in tabs {
                            let label = tab.label();
                            let response = crate::panel_ui::tab(
                                ui,
                                label,
                                label,
                                self.video.tab == tab,
                                width,
                                labels_fit,
                            );
                            response.widget_info(|| {
                                egui::WidgetInfo::selected(
                                    egui::Role::Tab,
                                    true,
                                    self.video.tab == tab,
                                    label,
                                )
                            });
                            #[cfg(test)]
                            ui.ctx().data_mut(|d| {
                                d.insert_temp(egui::Id::unique(("video-tab", label)), response.rect)
                            });
                            if response.clicked() {
                                self.video.tab = tab;
                                if tab != PropertyTab::Audio {
                                    self.video.object_tab = tab;
                                }
                            }
                        }
                    });
                });
            });
    }
    pub(crate) fn video_properties_pane(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing = egui::vec2(8., 8.);
        ui.spacing_mut().interact_size.y = 28.;
        ui.visuals_mut().widgets.inactive.weak_bg_fill = egui::Color32::from_rgb(69, 78, 82);
        ui.visuals_mut().widgets.inactive.bg_stroke =
            egui::Stroke::new(1., egui::Color32::from_rgb(98, 112, 118));
        self.shell.property_tab = crate::shell::PropertyTab::Tool;
        if self.selected.is_some() {
            self.video.selected = Selection::Objects;
        }
        let selection = (self.video.selected, self.selected);
        if self.video.property_selection != Some(selection) {
            match self.video.selected {
                Selection::Audio(index) => {
                    self.video.audio_index = index;
                    self.video.tab = PropertyTab::Audio;
                }
                Selection::Objects if self.selected.is_some() => {
                    self.video.tab = self.video.object_tab;
                }
                _ => (),
            }
            self.video.property_selection = Some(selection);
        }

        let a = self.selected_annotation();
        let c = match self.video.selected {
            Selection::Recording(i) | Selection::Audio(i) => self
                .session
                .document
                .video
                .as_ref()
                .and_then(|p| p.segments.get(i)),
            _ => None,
        };
        let (glyph, title, subtitle, linked) = if let Some(id) = self.video.zoom_selected {
            (
                "Zoom in",
                format!("Zoom {id}"),
                "Focus lower layers".into(),
                false,
            )
        } else if let Some(c) = c {
            (
                if c.kind() == capture_core::video::editor::MediaKind::Audio {
                    "Narration"
                } else if c.frame.is_some() {
                    "Camera"
                } else {
                    "Recording"
                },
                c.label.clone(),
                if c.kind() == capture_core::video::editor::MediaKind::Audio {
                    "Audio clip"
                } else {
                    "Video frame"
                }
                .to_string(),
                c.link.is_some(),
            )
        } else if let Some(a) = &a {
            (
                if a.focus.is_some() {
                    "Focus"
                } else if a.text.is_some() {
                    "Text"
                } else {
                    "Style"
                },
                self.session
                    .document
                    .video
                    .as_ref()
                    .unwrap()
                    .objects
                    .get(&a.id)
                    .filter(|t| !t.label.is_empty())
                    .map(|t| t.label.clone())
                    .unwrap_or_else(|| {
                        a.text
                            .as_ref()
                            .map(|t| t.content.clone())
                            .unwrap_or_else(|| {
                                if a.focus.is_some() {
                                    "Focus".into()
                                } else {
                                    format!("{:?}", a.kind)
                                }
                            })
                    }),
                "Annotation track".into(),
                false,
            )
        } else {
            (
                "Recording",
                "Video project".into(),
                "Project settings".into(),
                false,
            )
        };
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(16, 10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(38., 38.), egui::Sense::hover());
                    ui.painter()
                        .rect_filled(r, 7., egui::Color32::from_rgb(38, 49, 55));
                    super::ui::icon(
                        ui.painter(),
                        r.shrink(9.),
                        glyph,
                        egui::Color32::from_rgb(101, 204, 232),
                    );
                    ui.add_space(3.);
                    let title_width = (ui.available_width() - 36.).max(40.);
                    ui.allocate_ui_with_layout(
                        egui::vec2(title_width, 38.),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_min_width(title_width);
                            ui.add(egui::Label::new(crate::panel_ui::title(title, 14.)).truncate());
                            ui.label(
                                egui::RichText::new(if linked {
                                    format!("{subtitle} · linked clip")
                                } else {
                                    subtitle
                                })
                                .size(11.)
                                .color(egui::Color32::from_rgb(181, 194, 199)),
                            );
                        },
                    );
                    if super::ui::button(
                        ui,
                        "Expand Properties pane",
                        "",
                        28.,
                        self.video.properties_priority,
                    )
                    .on_hover_text("Toggle Properties / Timeline layout priority")
                    .clicked()
                    {
                        self.video.properties_priority = !self.video.properties_priority;
                    }
                });
            });
        self.video_property_tabs(ui);
        let area = ui.available_rect_before_wrap();
        let preview = if self.video.tab == PropertyTab::Motion {
            a.and_then(|a| {
                self.session
                    .document
                    .video
                    .as_ref()
                    .unwrap()
                    .objects
                    .get(&a.id)
                    .cloned()
                    .map(|t| (a, t))
            })
        } else {
            None
        };
        super::property_design::begin(ui, (self.video.tab as u8, glyph));
        let scroll = egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .max_height(area.height().max(24.))
            .id_salt(("video-properties", self.video.tab as u8))
            .show(ui, |ui| {
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(
                        if ui.available_width() < 300. { 12 } else { 16 },
                        0,
                    ))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        if let Some((a, t)) = preview {
                            super::property_design::heading(ui, "Play", "Animation preview");
                            let _top = ui.next_widget_position();
                            self.video_animation_preview(ui, &a, &t);
                            if super::ui::button(
                                ui,
                                "Play",
                                "Preview animation",
                                ui.available_width(),
                                true,
                            )
                            .clicked()
                            {
                                self.video.animation_clock = Some(Instant::now());
                            }

                            super::property_design::end(ui);
                            #[cfg(test)]
                            ui.ctx().data_mut(|d| {
                                d.insert_temp(
                                    egui::Id::unique("video-motion-preview"),
                                    egui::Rect::from_min_max(_top, ui.min_rect().max),
                                )
                            });
                        }
                        self.video_properties(ui);
                    });
            });
        #[cfg(test)]
        ui.ctx().data_mut(|d| {
            d.insert_temp(
                egui::Id::unique("video-property-overflow"),
                scroll.content_size.y > scroll.inner_rect.height() + 1.,
            )
        });
        super::property_design::finish(ui, scroll.content_size.y, area.height().max(24.));
    }

    pub(crate) fn video_properties(&mut self, ui: &mut egui::Ui) {
        if let Some(id) = self.video.zoom_selected {
            self.zoom_properties(ui, id);
            return;
        }
        if self.video.voiceover.open {
            self.voiceover_properties(ui);
            return;
        }
        let Some(mut v) = self.session.document.video.clone() else {
            return;
        };
        let original = v.clone();
        let duration = 14_400_000;
        match self.video.tab {
            PropertyTab::Audio => {
                let index = match self.video.selected {
                    Selection::Recording(i) | Selection::Audio(i) => i,
                    _ => {
                        ui.label("Select an audio or video clip with sound.");
                        return;
                    }
                };
                let Some(c) = v.segments.get_mut(index) else {
                    ui.label("Select a media clip with audio.");
                    self.audio_track_inserts(ui);
                    return;
                };
                if c.audio.is_empty() {
                    ui.label("This clip has no audio. Import audio or record a voiceover.");
                    self.audio_track_inserts(ui);
                    return;
                }
                let duration = c.duration();
                self.video.audio_index = self.video.audio_index.min(c.audio.len() - 1);
                if c.audio.len() > 1 {
                    egui::ComboBox::from_id_salt("video-audio-source")
                        .width(ui.available_width() - 8.)
                        .selected_text(&c.audio[self.video.audio_index].label)
                        .show_ui(ui, |ui| {
                            for (i, a) in c.audio.iter().enumerate() {
                                ui.selectable_value(&mut self.video.audio_index, i, &a.label);
                            }
                        });
                }
                let a = &mut c.audio[self.video.audio_index];
                super::property_design::heading(ui, "Audio", "Level");
                let mut percent = a.volume * 100.;
                super::property_design::percent(ui, "Volume", &mut percent, 200.);
                a.volume = percent / 100.;
                crate::controls::toggle(ui, &mut a.muted, "Mute");
                super::property_design::end(ui);
                super::property_design::heading(ui, "De-esser", "Fades");
                ui.columns(2, |col| {
                    seconds_field(&mut col[0], "Fade in", &mut a.fade_in, duration);
                    seconds_field(&mut col[1], "Fade out", &mut a.fade_out, duration);
                });
                super::property_design::end(ui);
            }
            _ => match self.video.selected {
                Selection::Audio(_) => {
                    ui.label("Use the Audio tab to change sound settings.");
                }
                Selection::Recording(index) => {
                    let Some(c) = v.segments.get_mut(index) else {
                        self.video.selected = Selection::Objects;
                        return;
                    };
                    match self.video.tab {
                        PropertyTab::Style => {
                            if c.kind() != capture_core::video::editor::MediaKind::Audio {
                                self.video_frame_properties(ui, c);
                            } else {
                                ui.label("Use Audio for level, fades and vocal effects.");
                            }
                        }
                        PropertyTab::Timing => {
                            super::property_design::heading(ui, "Timing", "On the timeline");
                            let mut until = c.until();
                            ui.columns(2, |col| {
                                seconds_field(&mut col[0], "Start", &mut c.at, 14_000_000);
                                seconds_bounded(
                                    &mut col[1],
                                    "End",
                                    &mut until,
                                    c.at + 1,
                                    c.at + ((c
                                        .source
                                        .as_ref()
                                        .map_or(v.source_duration, |s| s.duration)
                                        - c.start)
                                        as f64
                                        / c.speed)
                                        .round() as u64,
                                );
                            });
                            if until != original.segments[index].until() {
                                c.end = (c.start
                                    + ((until.saturating_sub(c.at)) as f64 * c.speed).round()
                                        as u64)
                                    .clamp(
                                        c.start + 1,
                                        c.source.as_ref().map_or(v.source_duration, |s| s.duration),
                                    );
                            }
                            ui.horizontal(|ui| {
                                ui.label("Duration");
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(format!("{:.3} s", c.duration() as f64 / 1000.));
                                    },
                                );
                            });
                            ui.columns(2, |col| {
                                let w = col[0].available_width();
                                if super::ui::button(
                                    &mut col[0],
                                    "Start here",
                                    "Start here",
                                    w,
                                    false,
                                )
                                .on_hover_text("Start at playhead")
                                .clicked()
                                {
                                    c.at = v.cursor;
                                }
                                let w = col[1].available_width();
                                if super::ui::button(&mut col[1], "End here", "End here", w, false)
                                    .on_hover_text("End at playhead")
                                    .clicked()
                                    && v.cursor > c.at
                                {
                                    c.end = (c.start
                                        + ((v.cursor - c.at) as f64 * c.speed).round() as u64)
                                        .clamp(
                                            c.start + 1,
                                            c.source
                                                .as_ref()
                                                .map_or(v.source_duration, |s| s.duration),
                                        );
                                }
                            });
                            super::property_design::end(ui);
                            super::property_design::heading(ui, "Split", "Source range");
                            let max = c.source.as_ref().map_or(v.source_duration, |s| s.duration);
                            ui.columns(2, |col| {
                                let end = c.end;
                                seconds_bounded(&mut col[0], "In", &mut c.start, 0, end - 1);
                                let start = c.start;
                                seconds_bounded(&mut col[1], "Out", &mut c.end, start + 1, max);
                            });
                            let mut speed = c.speed;
                            ui.label("Playback speed");
                            egui::ComboBox::from_id_salt("video-playback-speed")
                                .width(ui.available_width())
                                .selected_text(format!("{speed:.2}×"))
                                .show_ui(ui, |ui| {
                                    for choice in [0.25, 0.5, 0.75, 1., 1.25, 1.5, 2., 3., 4.] {
                                        ui.selectable_value(
                                            &mut speed,
                                            choice,
                                            format!("{choice:.2}×"),
                                        );
                                    }
                                });
                            c.clamp_fades();
                            let len = c.duration();
                            for a in &mut c.audio {
                                a.fade_in = a.fade_in.min(len);
                                a.fade_out = a.fade_out.min(len);
                            }
                            if speed != c.speed
                                && let Err(e) = v.speed(index, speed)
                            {
                                self.status = e.to_string();
                            }
                            super::property_design::end(ui);
                            if original.segments[index].link.is_some() {
                                super::property_design::heading(ui, "Link", "Linked editing");
                                let mut linked = true;
                                if crate::controls::toggle(
                                    ui,
                                    &mut linked,
                                    "Keep linked clips together",
                                )
                                .changed()
                                {
                                    v.unlink(index);
                                }
                                ui.small("Move, trim and split linked clips together.");
                            }
                        }
                        PropertyTab::Motion => {
                            super::ui::section(ui, "Clip fades");
                            let duration = c.duration();
                            ui.columns(2, |col| {
                                seconds_field(&mut col[0], "Fade in", &mut c.fade_in, duration);
                                seconds_field(&mut col[1], "Fade out", &mut c.fade_out, duration);
                            });
                            c.clamp_fades();
                            ui.small("Drag the top-corner handles in the timeline. Overlapping clips on the same track crossfade automatically; adjust their overlap to change its length.");
                        }
                        _ => {}
                    }
                }
                Selection::Objects => {
                    if let Some(a) = self.selected_annotation()
                        && let Some(mut timing) = v.objects.get(&a.id).cloned()
                    {
                        let locked = self.session.document.object_state(a.id).locked;
                        if locked {
                            ui.small(
                                "This object is locked. Unlock it from its timeline menu to edit.",
                            );
                        }
                        ui.add_enabled_ui(!locked,|ui| { match self.video.tab {
                            PropertyTab::Style=>self.video_annotation_style(ui),
                            PropertyTab::Timing=>{
                                super::property_design::heading(ui,"Timing","On the timeline");
                                ui.columns(2,|col|{seconds_field(&mut col[0],"Start",&mut timing.start,duration-1);seconds_field(&mut col[1],"End",&mut timing.end,duration);});
                                ui.add_space(8.);
                                ui.columns(2,|col|{
                                    let w=col[0].available_width();if super::ui::button(&mut col[0],"Start here","Start here",w,false).clicked(){timing.start=v.cursor;}
                                    let w=col[1].available_width();if super::ui::button(&mut col[1],"End here","End here",w,false).clicked(){timing.end=(v.cursor+1).min(duration);}
                                });
                                timing.start=timing.start.min(duration-1);timing.end=timing.end.max(timing.start+1).min(duration);
                                super::property_design::end(ui);super::property_design::heading(ui,"Timing","Duration");ui.label(format!("{:.2} seconds on screen",(timing.end-timing.start) as f64/1000.));
                                ui.add_space(12.);ui.small("Drag the edges of this object’s timeline block to change its range. Other objects on the same track keep their timing.");
                            }
                            PropertyTab::Motion=>{
                                if matches!(a.kind,crate::Kind::Redact|crate::Kind::Blur|crate::Kind::Pixelate){ui.label("Privacy effects stay fully applied throughout their visible time. Revealing animations are disabled.");return;}
                                super::property_design::heading(ui,"Presets","Motion presets");
                                let current_motion=capture_core::reusable_presets::Motion::from(&timing);
                                let motion_name=capture_core::reusable_presets::Motion::builtins().into_iter().find(|(_,p)|*p==current_motion).map_or("Custom motion",|(name,_)|name);
                                egui::ComboBox::from_id_salt("bundled-motion-presets").width(ui.available_width()).selected_text(motion_name).show_ui(ui,|ui|{
                                    for (name,preset) in capture_core::reusable_presets::Motion::builtins(){if preset.entrance.supports(&a)&&preset.exit.supports(&a)&&ui.button(name).clicked(){if let Err(e)=preset.apply(&a,&mut timing){self.status=e.to_string();}self.video.animation_clock=Some(Instant::now());ui.close();}}
                                });
                                if let Some(capture_core::reusable_presets::Payload::Motion(preset))=self.presets.reusable.controls(ui,capture_core::reusable_presets::Payload::Motion((&timing).into())) && let Err(e)=preset.apply(&a,&mut timing){self.status=e.to_string();}
                                super::property_design::end(ui);
                                for exit in [false,true] {
                                    super::property_design::heading(ui,if exit {"End here"}else{"Start here"},if exit {"Exit"}else{"Entrance"});
                                    ui.columns(2,|col| {
                                        col[0].label("Animation");let anim=if exit {&mut timing.exit}else{&mut timing.entrance};let before=*anim;
                                        egui::ComboBox::from_id_salt(("video-animation",exit)).width(col[0].available_width()).selected_text(anim.label(exit)).show_ui(&mut col[0],|ui| {for choice in Animation::ALL.into_iter().filter(|x|x.supports(&a)){ui.selectable_value(anim,choice,choice.label(exit));}});
                                        if *anim!=before {self.video.animation_clock=Some(Instant::now());}
                                        if *anim!=before && timing.paired {if exit{timing.entrance=timing.exit}else{timing.exit=timing.entrance}}
                                        seconds_field(&mut col[1],"Duration",if exit{&mut timing.exit_ms}else{&mut timing.entrance_ms},timing.end-timing.start);
                                    });
                                    let stack_before=timing.clone();
                                    animation_stack(ui,&a,&mut timing,exit);
                                    timing.normalize_stacks(&a);
                                    if timing!=stack_before {self.video.animation_clock=Some(Instant::now());if timing.paired {if exit{timing.entrance=timing.exit;timing.entrance_stack=timing.exit_stack.clone();timing.entrance_durations=timing.exit_durations.clone();}else{timing.exit=timing.entrance;timing.exit_stack=timing.entrance_stack.clone();timing.exit_durations=timing.entrance_durations.clone();}}}
                                    if !exit {let before=timing.paired;crate::controls::toggle(ui,&mut timing.paired,"Pair with reverse exit");if timing.paired && !before {timing.exit=timing.entrance;timing.exit_ms=timing.entrance_ms;timing.exit_stack=timing.entrance_stack.clone();timing.exit_durations=timing.entrance_durations.clone();}}
                                    super::property_design::end(ui);
                                }
                                if timing.exit==Animation::Path || timing.exit_stack.contains(&Animation::Path) {
                                    super::property_design::heading(ui,"Motion","Retraction direction");
                                    ui.horizontal_wrapped(|ui| {ui.selectable_value(&mut timing.retract_from_start,true,"First → Last node");ui.selectable_value(&mut timing.retract_from_start,false,"Last → First node");});
                                    super::property_design::end(ui);
                                }
                                if (timing.entrance==Animation::Path || timing.exit==Animation::Path || timing.entrance_stack.contains(&Animation::Path) || timing.exit_stack.contains(&Animation::Path)) && (a.is_shape() || a.kind==crate::Kind::Callout || a.highlighter_rect) {
                                    super::property_design::heading(ui,"Motion","Path direction");ui.horizontal_wrapped(|ui| {ui.selectable_value(&mut timing.path_reverse,false,"Clockwise");ui.selectable_value(&mut timing.path_reverse,true,"Counterclockwise");});super::property_design::end(ui);
                                }
                                super::property_design::group(ui,"Motion","Motion curve",if timing.smooth{"Smooth"}else{"Linear"},|ui| {
                                    ui.horizontal(|ui| {ui.selectable_value(&mut timing.smooth,false,"Linear");ui.selectable_value(&mut timing.smooth,true,"Smooth");});
                                    if timing.entrance==Animation::Float || timing.exit==Animation::Float || timing.entrance_stack.contains(&Animation::Float) || timing.exit_stack.contains(&Animation::Float) {
                                        ui.label("Float direction");egui::ComboBox::from_id_salt("video-direction").width(ui.available_width()).selected_text(["From left","From right","From top","From bottom"][timing.direction as usize]).show_ui(ui,|ui|{for (i,label) in ["From left","From right","From top","From bottom"].iter().enumerate(){ui.selectable_value(&mut timing.direction,i as u8,*label);}});
                                    }
                                });
                            }
                            PropertyTab::Audio=>()
                        }});
                        timing.clamp_animations();
                        v.place(a.id, timing);
                    } else if self.video.tab == PropertyTab::Style {
                        if self.tool.is_some() || self.icon_library.active {
                            self.image_properties(ui);
                        } else {
                            super::ui::section(ui, "Project dimensions");
                            ui.columns(2, |col| {
                                let mut width = self.session.document.width;
                                let mut height = self.session.document.height;
                                col[0].label("Width");
                                col[1].label("Height");
                                let changed = col[0]
                                    .add(
                                        egui::DragValue::new(&mut width)
                                            .range(64..=1920)
                                            .speed(2.)
                                            .suffix(" px"),
                                    )
                                    .changed()
                                    | col[1]
                                        .add(
                                            egui::DragValue::new(&mut height)
                                                .range(64..=1080)
                                                .speed(2.)
                                                .suffix(" px"),
                                        )
                                        .changed();
                                if changed {
                                    self.execute(Command::VideoResize {
                                        width: width / 2 * 2,
                                        height: height / 2 * 2,
                                    });
                                }
                            });
                            super::ui::section(ui, "Frame rate");
                            ui.horizontal(|ui| {
                                for fps in [15, 30, 60] {
                                    let response =
                                        ui.selectable_value(&mut v.fps, fps, format!("{fps} fps"));
                                    #[cfg(test)]
                                    ui.ctx().data_mut(|d| {
                                        d.insert_temp(
                                            egui::Id::unique(("video-setup-fps", fps)),
                                            response.rect,
                                        )
                                    });
                                    #[cfg(not(test))]
                                    let _ = response;
                                }
                            });
                            super::ui::section(ui, "Framing");
                            self.video_fit_controls(ui, &mut v);
                            ui.add_space(12.);
                            ui.small("Place an object at the playhead. Select its timeline block to edit appearance, timing and motion.");
                        }
                    } else {
                        ui.label("Select an object or a recording clip in the timeline.");
                    }
                }
            },
        }
        if !v.same_edit(&original) {
            if let Some(id) = self.selected
                && let Some((a, b)) = v.objects.get(&id).zip(original.objects.get(&id))
                && capture_core::reusable_presets::Motion::from(a)
                    != capture_core::reusable_presets::Motion::from(b)
            {
                self.video.animation_clock = Some(Instant::now());
            }
            if let Selection::Recording(i) = self.video.selected
                && let Some(c) = v.segments.get(i).cloned()
            {
                let old = &original.segments[i];
                if (c.at, c.start, c.end) != (old.at, old.start, old.end) {
                    let mut linked = original.clone();
                    match linked.edit_linked(i, c, false) {
                        Ok(()) => v = linked,
                        Err(e) => {
                            self.status = e.to_string();
                            return;
                        }
                    }
                }
            }
            self.commit_video(v);
        }
        if self.video.tab == PropertyTab::Audio {
            self.audio_track_inserts(ui);
        }
    }
    fn video_fit_controls(&mut self, ui: &mut egui::Ui, v: &mut Project) {
        ui.label("Frame fit");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut v.fill, false, "Fit");
            ui.selectable_value(&mut v.fill, true, "Fill");
        });
        ui.add_space(6.);
        egui::CollapsingHeader::new("Source crop").show(ui, |ui| {
            let size = v.source_size;
            for (i, label) in ["Left", "Top", "Width", "Height"].iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(*label);
                    ui.add(
                        egui::DragValue::new(&mut v.crop[i])
                            .range(if i < 2 { 0 } else { 1 }..=size[i % 2]),
                    );
                });
            }
            v.crop[0] = v.crop[0].min(size[0] - 1);
            v.crop[1] = v.crop[1].min(size[1] - 1);
            v.crop[2] = v.crop[2].min(size[0] - v.crop[0]);
            v.crop[3] = v.crop[3].min(size[1] - v.crop[1]);
            if ui.button("Reset crop").clicked() {
                v.crop = [0, 0, size[0], size[1]];
            }
        });
    }
    pub(crate) fn video_insert_ribbon(&mut self, ui: &mut egui::Ui, _height: f32) {
        ui.set_min_height(_height + 14.);
        crate::ribbon::group(
            ui,
            "Annotations",
            8. * 72. + 7. * ui.spacing().item_spacing.x,
            _height,
            |ui| {
                for (index, tool) in crate::shell::TOOLS.iter().enumerate().take(8) {
                    if crate::shell::ribbon_tile(
                        ui,
                        tool.name,
                        tool.name,
                        self.current_tool().id == tool.id,
                        egui::Color32::from_rgb(106, 199, 223),
                    )
                    .on_hover_text(self.tool_tooltip(tool.id))
                    .clicked()
                    {
                        self.choose_tool(index);
                    }
                }
            },
        );
        crate::ribbon::divider(ui, _height);
        crate::ribbon::group(
            ui,
            "Focus",
            152. + ui.spacing().item_spacing.x,
            _height,
            |ui| {
                let index = crate::shell::TOOLS
                    .iter()
                    .position(|t| t.id == capture_core::toolbar::ToolId::Focus)
                    .unwrap();
                if crate::shell::ribbon_tile(
                    ui,
                    "Focus",
                    "Focus",
                    self.shell.focus.active,
                    crate::shell::ACCENT,
                )
                .on_hover_text(self.tool_tooltip(capture_core::toolbar::ToolId::Focus))
                .clicked()
                {
                    self.choose_tool(index);
                }
                if crate::shell::ribbon_tile(
                    ui,
                    "Zoom in",
                    "Zoom",
                    self.video.zoom_draw,
                    crate::shell::ACCENT,
                )
                .clicked()
                {
                    self.start_zoom_insert();
                }
            },
        );
        crate::ribbon::divider(ui, _height);
        crate::ribbon::group(ui, "Audio", 152., _height, |ui| {
            for (name, selected) in [
                ("Narration", self.video.voiceover.open),
                ("Audio effects", self.video.fx.open),
            ] {
                if crate::shell::ribbon_tile(
                    ui,
                    name,
                    name,
                    selected,
                    egui::Color32::from_rgb(106, 199, 223),
                )
                .clicked()
                {
                    if name == "Narration" {
                        self.open_voiceover();
                    } else {
                        self.open_effects_workstation();
                    }
                }
            }
        });
    }
    pub(crate) fn video_export_ribbon(&mut self, ui: &mut egui::Ui, height: f32) {
        ui.set_min_height(height + 14.);
        for (i, glyph, label) in [
            (0, "Capture Video", "Export MP4"),
            (1, "Export image", "Export frame"),
            (2, "Save As", "Save As Project"),
        ] {
            if crate::ribbon::command_tile(ui, glyph, label, label, true, false, 126., height)
                .clicked()
            {
                match i {
                    0 => self.export_video(),
                    1 => {
                        self.video.pause();
                        self.begin_export(false)
                    }
                    _ => {
                        self.save_video(true);
                    }
                }
            }
        }
    }
    pub(crate) fn video_advanced_ribbon(&mut self, ui: &mut egui::Ui, height: f32) {
        ui.set_min_height(height + 14.);
        if crate::ribbon::command_tile(
            ui,
            "Video setup",
            "Video setup",
            "Dimensions, crop and frame fit",
            true,
            self.video.settings,
            108.,
            height,
        )
        .clicked()
        {
            self.open_video_setup();
        }
        crate::ribbon::divider(ui, height);
        crate::ribbon::group(ui, "Frame rate", 210., height, |ui| {
            let mut v = self.session.document.video.clone().unwrap();
            for fps in [15, 30, 60] {
                if crate::ribbon::command_tile(
                    ui,
                    "Recording",
                    &format!("{fps} fps"),
                    "Output frames per second",
                    true,
                    v.fps == fps,
                    64.,
                    height - 4.,
                )
                .clicked()
                {
                    v.fps = fps;
                }
            }
            if v.fps != self.session.document.video.as_ref().unwrap().fps {
                self.commit_video(v);
            }
        });
        crate::ribbon::divider(ui, height);
        if crate::ribbon::command_tile(
            ui,
            "Revert",
            "Reset source crop",
            "Keep annotations and timing",
            true,
            false,
            126.,
            height,
        )
        .clicked()
        {
            let mut v = self.session.document.video.clone().unwrap();
            v.crop = [0, 0, v.source_size[0], v.source_size[1]];
            self.commit_video(v);
        }
        if crate::ribbon::command_tile(
            ui,
            "Copy",
            "Copy frame",
            "Copy current composited frame",
            true,
            false,
            110.,
            height,
        )
        .clicked()
        {
            self.copy_all();
        }
    }
}
