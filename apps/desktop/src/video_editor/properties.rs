use super::*;
use crate::tips::Tip as _;
use capture_core::video::editor::Animation;
use egui::{Color32, Sense};
/// Column widths for entrance/exit rows: [chain arrow][name][duration][×].
const ARROW_W: f32 = 18.;
const DURATION_W: f32 = 76.;
const REMOVE_W: f32 = 26.;
/// One fixed-height row in the animation columns.
fn animation_row(ui: &mut egui::Ui, cells: impl FnOnce(&mut egui::Ui, f32)) {
    let width = ui.available_width();
    ui.allocate_ui_with_layout(
        egui::vec2(width, 30.),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_size(egui::vec2(width, 30.));
            ui.set_max_width(width);
            let gap = ui.spacing().item_spacing.x;
            let name_w = (width - ARROW_W - DURATION_W - REMOVE_W - gap * 3.).max(60.);
            cells(ui, name_w);
        },
    );
}
fn seconds_cell(ui: &mut egui::Ui, value: &mut u64, maximum: u64) -> bool {
    let mut s = *value as f64 / 1000.;
    let changed = ui
        .add_sized(
            [DURATION_W, 26.],
            egui::DragValue::new(&mut s)
                .range(0. ..=maximum as f64 / 1000.)
                .speed(0.01)
                .max_decimals(2)
                .suffix(" s"),
        )
        .changed();
    if changed {
        *value = (s * 1000.).round() as u64;
    }
    changed
}
/// Entrance or exit (approved mockup 3): the primary animation, chained effects
/// aligned in the same columns, and a last row with the chain arrow and
/// "Add animation". Every row is a fixed height, so the pane grows by one row.
fn animation_rows(
    ui: &mut egui::Ui,
    targets: &[crate::Annotation],
    timing: &mut capture_core::video::editor::Timing,
    exit: bool,
) {
    let maximum = timing.end - timing.start;
    ui.push_id(("animation-rows", exit), |ui| {
        ui.horizontal(|ui| {
            ui.add_space(ARROW_W + ui.spacing().item_spacing.x);
            ui.label(egui::RichText::new("Animation").size(11.).weak());
        });
        let (primary, primary_ms) = if exit {
            (&mut timing.exit, &mut timing.exit_ms)
        } else {
            (&mut timing.entrance, &mut timing.entrance_ms)
        };
        animation_row(ui, |ui, name_w| {
            ui.add_space(ARROW_W);
            egui::ComboBox::from_id_salt(("video-animation", exit))
                .width(name_w)
                .selected_text(primary.label(exit))
                .show_ui(ui, |ui| {
                    for choice in Animation::ALL
                        .into_iter()
                        .filter(|x| targets.iter().all(|a| x.supports(a)))
                    {
                        ui.selectable_value(primary, choice, choice.label(exit));
                    }
                });
            seconds_cell(ui, primary_ms, maximum);
        });
        let primary = *primary;
        let fallback = *primary_ms;
        let (stack, durations) = if exit {
            (&mut timing.exit_stack, &mut timing.exit_durations)
        } else {
            (&mut timing.entrance_stack, &mut timing.entrance_durations)
        };
        let mut remove = None;
        for (i, effect) in stack.iter().enumerate() {
            animation_row(ui, |ui, name_w| {
                let (r, _) = ui.allocate_exact_size(egui::vec2(ARROW_W, 22.), Sense::hover());
                super::ui::icon(ui.painter(), r.shrink(1.), "Chain", Color32::from_gray(150));
                ui.add_sized(
                    [name_w, 24.],
                    egui::Label::new(effect.label(exit)).truncate(),
                );
                seconds_cell(ui, durations.entry(*effect).or_insert(fallback), maximum);
                if super::ui::button(ui, "Close", "", REMOVE_W, false)
                    .tip(format!("Remove {}", effect.label(exit)))
                    .clicked()
                {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            durations.remove(&stack.remove(i));
        }
        let choices: Vec<_> = Animation::ALL
            .into_iter()
            .filter(|e| {
                primary != Animation::None
                    && targets.iter().all(|a| e.supports(a))
                    && capture_core::video::editor::Timing::stack_compatible(primary, stack, *e)
            })
            .collect();
        if !choices.is_empty() {
            animation_row(ui, |ui, _| {
                let (r, _) = ui.allocate_exact_size(egui::vec2(ARROW_W, 22.), Sense::hover());
                super::ui::icon(ui.painter(), r.shrink(1.), "Chain", Color32::from_gray(110));
                let add = super::ui::button(ui, "Plus", "Add animation", 132., false)
                    .tip("Chain another animation: combine body, outline and text effects");
                egui::Popup::menu(&add).show(|ui| {
                    for effect in &choices {
                        if ui.button(effect.label(exit)).clicked() {
                            stack.push(*effect);
                            ui.close();
                        }
                    }
                });
            });
        }
    });
}
/// Which way a float comes from, beside the animation it belongs to.
fn float_direction(
    ui: &mut egui::Ui,
    timing: &mut capture_core::video::editor::Timing,
    exit: bool,
) {
    const SIDES: [&str; 4] = ["From left", "From right", "From top", "From bottom"];
    ui.label(egui::RichText::new("Float direction").weak());
    egui::ComboBox::from_id_salt(("video-direction", exit))
        .width(ui.available_width())
        .selected_text(SIDES[timing.direction.min(3) as usize])
        .show_ui(ui, |ui| {
            for (i, label) in SIDES.iter().enumerate() {
                ui.selectable_value(&mut timing.direction, i as u8, *label);
            }
        });
}
#[derive(Clone, Copy)]
enum LineUp {
    Start,
    End,
}
/// Timing for several objects: Start and End move the group, keeping its spacing.
fn group_timing(
    ui: &mut egui::Ui,
    targets: &[crate::Annotation],
    v: &Project,
    timing: &mut capture_core::video::editor::Timing,
    duration: u64,
) -> Option<LineUp> {
    use crate::tips::Tip as _;
    let times: Vec<(u64, u64)> = targets
        .iter()
        .filter_map(|a| v.objects.get(&a.id))
        .map(|t| (t.start, t.end))
        .collect();
    let earliest = times.iter().map(|t| t.0).min().unwrap_or(0);
    let latest_start = times.iter().map(|t| t.0).max().unwrap_or(0);
    let latest = times.iter().map(|t| t.1).max().unwrap_or(1);
    let soonest_end = times.iter().map(|t| t.1).min().unwrap_or(1);
    let range =
        |a: u64, b: u64| format!("Mixed · {:.2}–{:.2} s", a as f64 / 1000., b as f64 / 1000.);
    super::property_design::heading(ui, "Timing", "On the timeline");
    let (mut start, mut end) = (earliest, latest);
    ui.columns(2, |col| {
        seconds_field(&mut col[0], "Start", &mut start, duration - 1);
        seconds_field(&mut col[1], "End", &mut end, duration);
    });
    timing.start = timing
        .start
        .saturating_add_signed(start as i64 - earliest as i64);
    timing.end = timing.end.saturating_add_signed(end as i64 - latest as i64);
    timing.start = timing.start.min(duration - 1);
    timing.end = timing.end.max(timing.start + 1).min(duration);
    if earliest != latest_start {
        ui.small(range(earliest, latest_start));
    }
    if soonest_end != latest {
        ui.small(range(soonest_end, latest));
    }
    let lengths: Vec<u64> = times.iter().map(|t| t.1 - t.0).collect();
    ui.small(if lengths.windows(2).all(|l| l[0] == l[1]) {
        format!("{:.2} seconds on screen", lengths[0] as f64 / 1000.)
    } else {
        "Duration: Mixed".to_string()
    });
    ui.small("Typing a Start moves the earliest object there. The others keep their spacing.");
    super::property_design::end(ui);
    super::property_design::heading(ui, "Timing", "Line up");
    let mut line = None;
    ui.columns(2, |col| {
        let w = col[0].available_width();
        if super::ui::button(&mut col[0], "Start here", "Start all here", w, false)
            .tip("Start every selected object at the playhead.")
            .clicked()
        {
            line = Some(LineUp::Start);
        }
        let w = col[1].available_width();
        if super::ui::button(&mut col[1], "End here", "End all here", w, false)
            .tip("End every selected object at the playhead.")
            .clicked()
        {
            line = Some(LineUp::End);
        }
    });
    line
}
fn line_up_objects(v: &mut Project, targets: &[crate::Annotation], line: LineUp) {
    for a in targets {
        let Some(old) = v.objects.get(&a.id).cloned() else {
            continue;
        };
        let mut after = old.clone();
        match line {
            LineUp::Start => after.start = v.cursor,
            LineUp::End => after.end = v.cursor + 1,
        }
        let mut next = old.clone();
        next.copy_change(&old, &after, a);
        v.place(a.id, next);
    }
}
fn animation_summary(primary: Animation, stack: &[Animation], exit: bool) -> String {
    let names: Vec<_> = std::iter::once(primary)
        .chain(stack.iter().copied())
        .filter(|a| *a != Animation::None)
        .map(|a| a.label(exit))
        .collect();
    if names.is_empty() {
        "None".into()
    } else {
        names.join(" + ")
    }
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
        let has_text = self.video_text_tab();
        let tabs: Vec<_> = PropertyTab::ALL
            .into_iter()
            .filter(|t| *t != PropertyTab::Audio || has_audio)
            .filter(|t| *t != PropertyTab::Text || has_text)
            .collect();
        if (!has_audio && self.video.tab == PropertyTab::Audio)
            || (!has_text && self.video.tab == PropertyTab::Text)
        {
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
                                self.video.text_return = None;
                                if tab != PropertyTab::Audio {
                                    self.video.object_tab = tab;
                                }
                            }
                        }
                    });
                });
            });
    }
    /// Objects that hold text, other than Text objects, get a Text tab.
    pub(crate) fn video_text_tab(&self) -> bool {
        self.video.selected == Selection::Objects
            && self.selected_annotation().is_some_and(|a| {
                a.text.is_some() && a.kind != crate::Kind::Text && a.focus.is_none()
            })
            && !self.mixed_selection()
    }
    /// Typing into an object opens its Text tab; when typing ends the pane
    /// returns to the tab it was on (bh-proposal).
    /// The Text tab opens once when typing starts, so the other tabs stay
    /// usable while typing (BH1); choosing a tab cancels the return.
    fn follow_inline_typing(&mut self) {
        let typing = self.text_draft.is_some() && self.video_text_tab();
        if typing && !self.video.typing {
            self.video.typing = true;
            if self.video.tab != PropertyTab::Text {
                self.video.text_return = Some(self.video.tab);
                self.video.tab = PropertyTab::Text;
            }
        } else if !typing && self.video.typing {
            self.video.typing = false;
            if let Some(tab) = self.video.text_return.take() {
                self.video.tab = tab;
            }
        }
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
        self.follow_inline_typing();

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
                if a.focus.and_then(|f| f.magnify).is_some() {
                    "Magnify"
                } else if a.focus.is_some() {
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
                                if a.focus.and_then(|f| f.magnify).is_some() {
                                    "Magnify".into()
                                } else if a.focus.is_some() {
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
        // Several objects are titled as a group, however they were selected.
        let count = self.selected_objects().len();
        let group = |matches: fn(&capture_core::video::editing::Item) -> bool| {
            self.video.selection.iter().filter(|i| matches(i)).count()
        };
        let clips = group(|i| matches!(i, capture_core::video::editing::Item::Media(_)));
        let zooms = group(|i| matches!(i, capture_core::video::editing::Item::Zoom(_)));
        let (title, subtitle) = if zooms > 1 && self.video.zoom_selected.is_some() {
            (format!("{zooms} zooms selected"), subtitle)
        } else if clips > 1 && c.is_some() {
            (format!("{clips} clips selected"), subtitle)
        } else if count > 1 && c.is_none() && self.video.zoom_selected.is_none() {
            (
                format!("{count} objects selected"),
                if self.mixed_selection() {
                    "Mixed".to_string()
                } else {
                    subtitle
                },
            )
        } else {
            (title, subtitle)
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
                    .tip("Toggle Properties / Timeline layout priority")
                    .clicked()
                    {
                        self.video.properties_priority = !self.video.properties_priority;
                    }
                });
            });
        if self.mixed_selection() {
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(16, 0))
                .show(ui, |ui| self.mixed_chips(ui));
        }
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
        self.run_review_request();
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
                                .tip("Start at playhead")
                                .clicked()
                                {
                                    c.at = v.cursor;
                                }
                                let w = col[1].available_width();
                                if super::ui::button(&mut col[1], "End here", "End here", w, false)
                                    .tip("End at playhead")
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
                            if c.kind() != capture_core::video::editor::MediaKind::Audio {
                                let values =
                                    capture_core::video::keyframes::Property::MEDIA.map(|p| {
                                        (p, capture_core::video::keyframes::media_value(c, p))
                                    });
                                let until = c.until();
                                self.keyframe_controls(
                                    ui,
                                    &mut c.keyframes,
                                    (c.at, until),
                                    &mut v.cursor,
                                    &values,
                                    false,
                                );
                            }
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
                        let several = self.selected_objects().len() > 1;
                        let targets = if several {
                            self.selected_objects()
                        } else {
                            vec![a.clone()]
                        };
                        let motions: Vec<_> = targets
                            .iter()
                            .filter_map(|t| v.objects.get(&t.id))
                            .map(capture_core::reusable_presets::Motion::from)
                            .collect();
                        let mixed_motion = motions.windows(2).any(|m| m[0] != m[1]);
                        let mut line_up = None;
                        let locked = self.session.document.object_state(a.id).locked;
                        if locked {
                            ui.small(
                                "This object is locked. Unlock it from its timeline menu to edit.",
                            );
                        }
                        ui.add_enabled_ui(!locked,|ui| {
                            if matches!(self.video.tab,PropertyTab::Style|PropertyTab::Motion){
                                let was_reviewed=timing.motion_path.as_ref().is_some_and(|p|p.reviewed);
                                let mut nav=self.review_nav(a.id,v.cursor);
                                super::path_properties::controls(ui,&mut timing,&mut v.cursor,&mut nav);
                                let newly=!was_reviewed&&timing.motion_path.as_ref().is_some_and(|p|p.reviewed);
                                self.finish_review_nav(ui.ctx(),a.id,nav,newly);
                            }
                            match self.video.tab {
                            PropertyTab::Style=>self.video_annotation_style(ui),
                            PropertyTab::Text=>self.image_text_groups(ui),
                            PropertyTab::Timing if several=>{line_up=group_timing(ui,&targets,&v,&mut timing,duration);}
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
                                // Motion controls sit directly under the animation preview;
                                // keyframes follow in their own collapsible section.
                                let base=self.session.document.annotations.iter().find(|v|v.id==a.id).unwrap();
                                let values=capture_core::video::keyframes::Property::ANNOTATION.map(|p|(p,capture_core::video::keyframes::annotation_value(base,p)));
                                let privacy=matches!(a.kind,crate::Kind::Redact|crate::Kind::Blur|crate::Kind::Pixelate);
                                if privacy{ui.label("Privacy effects stay fully applied throughout their visible time. Revealing animations are disabled.");self.keyframe_controls(ui,&mut timing.keyframes,(timing.start,timing.end),&mut v.cursor,&values,true);return;}
                                // Approved mockup 3: one Motion section holds the preset, Entrance,
                                // Exit and Motion curve, so one click hides all of them.
                                let summary=format!("{} · {} · {}",animation_summary(timing.entrance,&timing.entrance_stack,false),animation_summary(timing.exit,&timing.exit_stack,true),if timing.smooth{"Smooth"}else{"Linear"});
                                super::property_design::group(ui,"Motion","Motion",&summary,|ui| {
                                    ui.horizontal(|ui|{
                                        ui.add_sized([48.,22.],egui::Label::new(egui::RichText::new("Preset").weak()));
                                        let current_motion=capture_core::reusable_presets::Motion::from(&timing);
                                        let motion_name=if mixed_motion {"Mixed"} else {capture_core::reusable_presets::Motion::builtins().into_iter().find(|(_,p)|*p==current_motion).map_or("Custom motion",|(name,_)|name)};
                                        egui::ComboBox::from_id_salt("bundled-motion-presets").width(ui.available_width()).selected_text(motion_name).show_ui(ui,|ui|{
                                            for (name,preset) in capture_core::reusable_presets::Motion::builtins(){if targets.iter().all(|t|preset.entrance.supports(t)&&preset.exit.supports(t))&&ui.button(name).clicked(){if let Err(e)=preset.apply(&a,&mut timing){self.status=e.to_string();}self.video.animation_clock=Some(Instant::now());ui.close();}}
                                        });
                                    });
                                    if let Some(capture_core::reusable_presets::Payload::Motion(preset))=self.presets.reusable.controls(ui,capture_core::reusable_presets::Payload::Motion((&timing).into())) && let Err(e)=preset.apply(&a,&mut timing){self.status=e.to_string();}
                                    for exit in [false,true] {
                                        let side=if exit{animation_summary(timing.exit,&timing.exit_stack,true)}else{animation_summary(timing.entrance,&timing.entrance_stack,false)};
                                        super::property_design::subgroup(ui,if exit{"Exit"}else{"Entrance"},&side,|ui| {
                                            let before=timing.clone();
                                            animation_rows(ui,&targets,&mut timing,exit);
                                            if if exit {timing.exit==Animation::Float||timing.exit_stack.contains(&Animation::Float)} else {timing.entrance==Animation::Float||timing.entrance_stack.contains(&Animation::Float)} {float_direction(ui,&mut timing,exit);}
                                            timing.normalize_stacks(&a);
                                            if timing!=before {
                                                self.video.animation_clock=Some(Instant::now());
                                                // Paired: the other side mirrors this one.
                                                if timing.paired {if exit{timing.entrance=timing.exit;timing.entrance_ms=timing.exit_ms;timing.entrance_stack=timing.exit_stack.clone();timing.entrance_durations=timing.exit_durations.clone();}else{timing.exit=timing.entrance;timing.exit_ms=timing.entrance_ms;timing.exit_stack=timing.entrance_stack.clone();timing.exit_durations=timing.entrance_durations.clone();}}
                                            }
                                            if !exit {let before=timing.paired;crate::controls::toggle(ui,&mut timing.paired,"Pair with reverse exit");if timing.paired && !before {timing.exit=timing.entrance;timing.exit_ms=timing.entrance_ms;timing.exit_stack=timing.entrance_stack.clone();timing.exit_durations=timing.entrance_durations.clone();}}
                                            if exit && (timing.exit==Animation::Path || timing.exit_stack.contains(&Animation::Path)) {
                                                ui.label(egui::RichText::new("Retraction direction").weak());
                                                crate::controls::segments_wrapped(ui,|ui|{ui.selectable_value(&mut timing.retract_from_start,true,"First → Last node");ui.selectable_value(&mut timing.retract_from_start,false,"Last → First node");});
                                            }
                                        });
                                        ui.add_space(6.);
                                    }
                                    super::property_design::subgroup(ui,"Motion curve",if timing.smooth{"Smooth"}else{"Linear"},|ui| {
                                        crate::controls::segments(ui,|ui|{ui.selectable_value(&mut timing.smooth,false,"Linear");ui.selectable_value(&mut timing.smooth,true,"Smooth");});
                                        if (timing.entrance==Animation::Path || timing.exit==Animation::Path || timing.entrance_stack.contains(&Animation::Path) || timing.exit_stack.contains(&Animation::Path)) && (a.is_shape() || a.kind==crate::Kind::Callout || a.highlighter_rect) {
                                            ui.label(egui::RichText::new("Path direction").weak());
                                            crate::controls::segments_wrapped(ui,|ui|{ui.selectable_value(&mut timing.path_reverse,false,"Clockwise");ui.selectable_value(&mut timing.path_reverse,true,"Counterclockwise");});
                                        }
                                    });
                                });
                                self.keyframe_controls(ui,&mut timing.keyframes,(timing.start,timing.end),&mut v.cursor,&values,false);
                            }
                            PropertyTab::Audio=>()
                        }});
                        if self.video.tab == PropertyTab::Timing
                            && let Some(old) = original.objects.get(&a.id)
                            && timing.start != old.start
                            && let Some(path) = timing.motion_path.as_mut()
                        {
                            Arc::make_mut(path).offset_ms = old
                                .motion_path
                                .as_ref()
                                .map_or(0, |p| p.offset_ms)
                                .saturating_add_signed(timing.start as i64 - old.start as i64);
                        }
                        if self.video.tab == PropertyTab::Timing
                            && let Some(old) = original.objects.get(&a.id)
                        {
                            timing.keyframes.offset_ms = old.keyframes.offset_ms;
                            timing
                                .keyframes
                                .trim_start(timing.start as i64 - old.start as i64);
                        }
                        timing.clamp_animations();
                        if let Some(before) = original.objects.get(&a.id) {
                            self.spread_timing(&mut v, a.id, before, &timing);
                        }
                        v.place(a.id, timing);
                        if let Some(line) = line_up {
                            line_up_objects(&mut v, &targets, line);
                        }
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
                                                .range(64..=1920)
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
                            crate::controls::segments(ui, |ui| {
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
        if v.cursor != original.cursor {
            self.video_seek(v.cursor);
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
                            self.fail(e);
                            return;
                        }
                    }
                }
            }
            if let Selection::Recording(i) = self.video.selected {
                self.spread_clip_edit(&mut v, &original, i);
            }
            if let Some(preset) = self.video.clip_preset.take() {
                let ratio =
                    self.session.document.width as f32 / self.session.document.height as f32;
                self.spread_clip_preset(&mut v, preset, ratio);
            }
            self.commit_video(v);
        }
        if self.video.tab == PropertyTab::Audio {
            self.audio_track_inserts(ui);
        }
    }
    fn video_fit_controls(&mut self, ui: &mut egui::Ui, v: &mut Project) {
        ui.label("Frame fit");
        crate::controls::segments(ui, |ui| {
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
                    .tip(self.tool_tooltip(tool.id))
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
            228. + ui.spacing().item_spacing.x,
            _height,
            |ui| {
                let index = crate::shell::TOOLS
                    .iter()
                    .position(|t| t.id == capture_core::toolbar::ToolId::Focus)
                    .unwrap();
                // Magnify uses the focus machinery; only one of the two is ever highlighted.
                let magnify =
                    self.current_tool().id == capture_core::toolbar::ToolId::MagnifiedCallout;
                if crate::shell::ribbon_tile(
                    ui,
                    "Focus",
                    "Focus",
                    self.shell.focus.active && !magnify,
                    crate::shell::ACCENT,
                )
                .tip(self.tool_tooltip(capture_core::toolbar::ToolId::Focus))
                .clicked()
                {
                    self.choose_tool(index);
                }
                if crate::shell::ribbon_tile(
                    ui,
                    "Magnify",
                    "Magnify",
                    self.current_tool().id == capture_core::toolbar::ToolId::MagnifiedCallout,
                    crate::shell::ACCENT,
                )
                .tip("Magnified callout")
                .clicked()
                {
                    let index = crate::shell::TOOLS
                        .iter()
                        .position(|t| t.id == capture_core::toolbar::ToolId::MagnifiedCallout)
                        .unwrap();
                    self.choose_tool(index);
                }
                if crate::shell::ribbon_tile(
                    ui,
                    "Zoom in",
                    "Zoom",
                    self.video.zoom_draw,
                    crate::shell::ACCENT,
                )
                .tip_title("Zoom")
                .tip("Draw the area the video zooms in to at the playhead.")
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
                .tip_title(name)
                .tip(if name == "Narration" {
                    "Record your voice over the video."
                } else {
                    "Add and tune effects on clips and tracks."
                })
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
        self.group_preset_ribbon(ui, _height);
        crate::ribbon::divider(ui, _height);
        // Everything that adds media sits here (approved option A), last in
        // the tab (BZ8), as a compact 2 × 2 so the tab still fits a 1440 px
        // window.
        crate::ribbon::group(ui, "Media", 2. * 100. + 4., _height, |ui| {
            let items = [
                (
                    "Add Track",
                    "Video/audio",
                    "Add a video, sound or music file at the playhead.",
                ),
                (
                    "Image",
                    "Pictures",
                    "Add pictures as clips at the playhead.",
                ),
                (
                    "Open",
                    "PDF pages",
                    "Add pages from a PDF as picture clips at the playhead.",
                ),
                (
                    "Capture Video",
                    "Record more",
                    "Record another part and add it to this video.",
                ),
            ];
            let mut chosen = None;
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(4., 2.);
                for row in items.chunks(2) {
                    ui.horizontal(|ui| {
                        for (glyph, name, tip) in row {
                            if super::ui::button(ui, glyph, name, 100., false)
                                .tip_title(*name)
                                .tip(*tip)
                                .clicked()
                            {
                                chosen = Some(*name);
                            }
                        }
                    });
                }
            });
            if let Some(name) = chosen {
                let (at, track) = {
                    let v = self.session.document.video.as_ref().unwrap();
                    (v.cursor, *v.media_tracks.last().unwrap())
                };
                match name {
                    "Video/audio" => self.pick_timeline_media(at, track),
                    "Pictures" => self.pick_timeline_pictures(at, track),
                    "PDF pages" => self.pdf_pages_to_video(),
                    _ => self.open_video_capture(),
                }
            }
        });
    }
    pub(crate) fn video_export_ribbon(&mut self, ui: &mut egui::Ui, height: f32) {
        ui.set_min_height(height + 14.);
        for (i, glyph, label) in [
            (0, "Capture Video", "Export MP4"),
            (3, "GIF Studio", "GIF Studio"),
            (1, "Export image", "Export frame"),
            (4, "Copy", "Copy frame"),
            (2, "Save As", "Save As Project"),
        ] {
            if crate::ribbon::command_tile(ui, glyph, label, label, true, false, 126., height)
                .clicked()
            {
                match i {
                    0 => self.export_video(),
                    3 => self.open_gif_studio(),
                    1 => {
                        self.video.pause();
                        self.begin_export(false)
                    }
                    4 => self.copy_all(),
                    _ => {
                        self.save_video(true);
                    }
                }
            }
        }
    }
}
