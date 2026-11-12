use super::*;
use egui::{Color32, Stroke};
const DARK: Color32 = Color32::from_rgb(37, 45, 48);
const PANEL: Color32 = Color32::from_rgb(48, 58, 62);
const BLUE: Color32 = Color32::from_rgb(73, 197, 233);
impl Editor {
    pub(crate) fn show_effects_workstation(&mut self, ctx: &egui::Context) {
        if !self.video.fx.open || !self.is_video() {
            return;
        }
        let p = self.session.document.video.clone().unwrap();
        if let Some(id) = self.focused_audio_clip()
            && id != self.video.fx.clip
        {
            self.follow_effects_clip(id);
        }
        if !p
            .segments
            .iter()
            .any(|c| c.id == self.video.fx.clip && !c.audio.is_empty())
        {
            self.video.fx.open = false;
            return;
        }
        let original = p
            .effect_chain(self.video.fx.target(&p))
            .cloned()
            .unwrap_or_default();
        let mut chain = self
            .video
            .fx
            .draft
            .clone()
            .unwrap_or_else(|| original.clone());
        let mut moved = None;
        let mut close = false;
        let mut play = false;
        let mut commit = false;
        let mut analyze = false;
        let width = (ctx.content_rect().width() - 36.).clamp(520., 940.);
        let pos = self
            .video
            .fx
            .position
            .unwrap_or_else(|| ctx.content_rect().min + egui::vec2(26., 160.));
        let window_height = (ctx.content_rect().height() - 60.).clamp(500., 740.);
        let response=egui::Window::new("Effects Workstation").id(egui::Id::unique("audio-effects-workstation")).title_bar(false).collapsible(false).resizable(false).current_pos(pos).fixed_size(egui::vec2(width,window_height)).frame(egui::Frame::new().fill(PANEL).stroke(Stroke::new(2.,crate::tabs::ACTIVE_BLUE)).corner_radius(10).inner_margin(12.)).show(ctx,|ui|{
   ui.spacing_mut().item_spacing=egui::vec2(8.,8.); ui.set_min_height(window_height-20.);
   let heading=ui.horizontal(|ui|{super::super::ui::icon(ui.painter(),egui::Rect::from_min_size(ui.cursor().min,egui::vec2(18.,18.)),"Audio effects",BLUE);ui.add_space(25.);ui.label(egui::RichText::new("Effects Workstation").size(15.).strong());ui.weak(format!("/ {}",track_label(&p,self.video.fx.clip)));let remaining=(ui.available_width()-34.).max(0.);let (_,drag)=ui.allocate_exact_size(egui::vec2(remaining,24.),egui::Sense::drag());
if drag.dragged(){moved=Some(pos+ui.input(|i|i.pointer.delta()));}
if super::super::ui::button(ui,"Close","",28.,false).clicked(){close=true;}});
   #[cfg(test)]ui.ctx().data_mut(|d|d.insert_temp(egui::Id::unique("effects-heading"),heading.response.rect));
   #[cfg(not(test))]let _=heading;
   ui.separator();ui.horizontal_wrapped(|ui|{
    let scope=self.video.fx.track_scope;
    ui.label("Apply to");crate::controls::segments(ui,|ui|{ui.selectable_value(&mut self.video.fx.track_scope,false,"Clip");ui.selectable_value(&mut self.video.fx.track_scope,true,"Track");});
    if scope!=self.video.fx.track_scope {self.follow_effects_clip(self.video.fx.clip);chain=p.effect_chain(self.video.fx.target(&p)).cloned().unwrap_or_default();}
    let old=self.video.fx.clip;egui::ComboBox::from_id_salt("effects-track").width(170.).selected_text(track_label(&p,old)).show_ui(ui,|ui|{for clip in p.segments.iter().filter(|c|!c.audio.is_empty()){ui.selectable_value(&mut self.video.fx.clip,clip.id,&clip.label);}});
if old!=self.video.fx.clip{self.select_only(None);self.video.selected=Selection::Recording(p.segments.iter().position(|c|c.id==self.video.fx.clip).unwrap());if let Some(tx)=&self.video.audio_updates{let _=tx.send(p.clone());}self.video.fx.original=false;self.video.fx.draft=None;self.video.fx.report=None;self.video.fx.proposal=None;self.video.fx.analysis=None;self.video.fx.analysis_open=false;self.video.fx.auditioning=false;chain=p.effect_chain(self.video.fx.target(&p)).cloned().unwrap_or_default();self.video.fx.selected=chain.effects.first().map(|e|e.id).unwrap_or(0);}
    egui::ComboBox::from_id_salt("effects-chain-preset").width(160.).selected_text("Chain presets").show_ui(ui,|ui|{for(label,value)in [("Clear narration",Chain::narration()),("Natural voice",Chain{bypass:false,mix:1.,effects:[Kind::Equalizer,Kind::Compressor,Kind::Limiter].into_iter().enumerate().map(|(i,k)|capture_core::video::effects::Effect::new(i as u64+1,k)).collect()}),("Empty chain",Chain::default())]{if ui.button(label).clicked(){chain=value;commit=true;self.video.fx.selected=chain.effects.first().map(|e|e.id).unwrap_or(0);ui.close();}}});
    if super::super::ui::button(ui,"Analyze Voice","Analyze Voice",126.,true).clicked(){analyze=true;}
    if super::super::ui::button(ui,"Effect power","Bypass chain",130.,chain.bypass).clicked(){chain.bypass= !chain.bypass;commit=true;}
   });
   self.audio_output_controls(ui,false);
   if let Some(capture_core::reusable_presets::Payload::AudioChain(saved))=self.presets.reusable.controls(ui,capture_core::reusable_presets::Payload::AudioChain(chain.clone())) {chain=saved;commit=true;self.video.fx.selected=chain.effects.first().map(|e|e.id).unwrap_or(0);}
   if !self.video.fx.analysis_open { ui.horizontal(|ui|{ui.small("PROCESSING ORDER · Left to right");ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{ui.small("Drag effects to reorder");});});
   self.effect_chain_ui(ui,&mut chain,&mut commit); }
   ui.separator();let analysis_open=self.video.fx.analysis_open;
   let body_height=if analysis_open {window_height-296.}else{window_height-404.};let body=ui.allocate_ui_with_layout(egui::vec2(ui.available_width(),body_height),egui::Layout::top_down(egui::Align::Min),|ui|{
    ui.set_min_height(body_height);
    egui::ScrollArea::vertical().id_salt("effect-controls").auto_shrink([false,false]).max_height(body_height).show(ui,|ui| {
    if analysis_open{self.audio_analysis_ui(ui,&mut chain,&mut commit);}else{
     let selected=chain.effects.iter().position(|e|e.id==self.video.fx.selected).unwrap_or(0);
if let Some(effect)=chain.effects.get_mut(selected){self.video.fx.selected=effect.id;let left=(ui.available_width()-145.).max(300.);ui.horizontal_top(|ui|{
      ui.allocate_ui_with_layout(egui::vec2(left,320.),egui::Layout::top_down(egui::Align::Min),|ui|{ui.horizontal(|ui|{ui.label(egui::RichText::new(effect.kind.label()).strong().size(17.));ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{egui::ComboBox::from_id_salt("effect-preset").selected_text("Presets").width(130.).show_ui(ui,|ui|{if ui.button("Default").clicked(){*effect=capture_core::video::effects::Effect::new(effect.id,effect.kind);commit=true;ui.close();}
for (name,preset) in super::presets::choices(effect.kind,effect.id) {if ui.button(name).clicked(){*effect=preset;commit=true;ui.close();}}
if ui.button("Bypass this effect").clicked(){effect.enabled=false;commit=true;ui.close();}});});});
       if let Some(capture_core::reusable_presets::Payload::AudioEffect(mut saved))=self.presets.reusable.controls(ui,capture_core::reusable_presets::Payload::AudioEffect(effect.clone())) {saved.id=effect.id;*effect=saved;commit=true;}
       let target=self.video.fx.target(&p);let _ = graph::draw(ui,effect,&mut self.video.fx.band,self.video.meters.get(&target).and_then(|m|m.get(selected)).copied());
       if effect.kind==Kind::Equalizer{self.eq_controls(ui,effect,&mut commit);}else{ui.small(match effect.kind{Kind::Compressor=>"Control peaks while retaining natural speech dynamics.",Kind::Clarity=>"Balance warmth, intelligibility and air.",Kind::DeEsser=>"Reduce sharp consonants when they exceed the threshold.",Kind::Leveler=>"Smooth speech levels; RMS target is not final export loudness.",Kind::Limiter=>"Sample-peak protection at the end of the chain.",_=>"Remove low-frequency rumble before shaping the voice."});let params=effect.kind.parameters();ui.columns(params.len(),|cols|{for(i,p)in params.iter().enumerate(){parameter(&mut cols[i],p.label,&mut effect.values[i],p.min,p.max,p.step,p.unit);cols[i].response().tip(super::presets::help(effect.kind,i));}});}
      });ui.separator();ui.allocate_ui_with_layout(egui::vec2(118.,315.),egui::Layout::top_down(egui::Align::Min),|ui|{graph::meters(ui,self.video.meters.get(&self.video.fx.target(&p)).and_then(|m|m.get(selected)).copied());});
     });
    }else{ui.heading("Build your vocal chain");ui.label("Add an effect with +, choose a chain preset, or analyze your voice.");}}
   });});
   #[cfg(test)]ui.ctx().data_mut(|d|d.insert_temp(egui::Id::unique("effects-body"),body.response.rect));
   #[cfg(not(test))]let _=body;
   ui.separator();if !analysis_open{self.effect_actions(ui,&mut chain,&mut commit);}else{ui.allocate_space(egui::vec2(ui.available_width(),28.));}
   if !self.video.fx.error.is_empty(){ui.colored_label(Color32::from_rgb(241,148,148),&self.video.fx.error);}
   ui.add_space((ui.min_rect().bottom()-ui.cursor().min.y-46.).max(0.));ui.separator();ui.horizontal_wrapped(|ui|{
    if super::super::ui::button(ui,if self.video.playing{"Pause"}else{"Play"},"",30.,false).clicked(){play=true;}
    ui.label("Original"); ui.spacing_mut().slider_width=180.;
    if ui.add(egui::Slider::new(&mut self.video.fx.audition_mix,0. ..=1.).show_value(false)).tip("Blend the original sound with the processed sound. 50% is an equal mix.").changed(){self.video.fx.original=false;self.video.fx.auditioning=analysis_open&&self.video.fx.proposal.is_some();self.preview_chain(&chain);}
    ui.label(format!("Processed · {:.0}%",self.video.fx.audition_mix*100.));
    if analysis_open {self.audio_analysis_actions(ui,&mut chain,&mut commit);}else{
    ui.small(if self.video.fx.track_scope {"Track effects · after clip effects"}else{"Clip effects · before track effects"});
    if ui.add_enabled(self.session.can_undo(),crate::controls::dialog_button("Undo",false)).clicked(){self.video.fx.draft=None;self.execute_audio_edit(Command::Undo);chain=self.session.document.video.as_ref().and_then(|p|p.effect_chain(self.video.fx.target(p))).cloned().unwrap_or_default();self.preview_chain(&chain);}
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{if ui.add(crate::controls::dialog_button("Done",true)).clicked(){close=true;}});
    }

   });
  });
        if let Some(r) = response {
            self.video.fx.position = Some(moved.unwrap_or(r.response.rect.min));
            #[cfg(test)]
            ctx.data_mut(|d| d.insert_temp(egui::Id::unique("effects-window"), r.response.rect));
        }
        if analyze {
            self.video.fx.auditioning = false;
            self.preview_chain(&chain);
            self.video.fx.analysis_open = true;
            self.video.fx.report = None;
            self.video.fx.proposal = None;
            self.video.fx.error.clear();
        }
        let stored = self
            .session
            .document
            .video
            .as_ref()
            .and_then(|p| p.effect_chain(self.video.fx.target(p)))
            .cloned()
            .unwrap_or_default();
        if chain != stored {
            self.preview_chain(&chain);
            if commit || !ctx.input(|i| i.pointer.primary_down()) {
                self.commit_chain(chain);
            } else {
                self.video.fx.draft = Some(chain);
            }
        }
        if play {
            self.video_play();
            let chain = self
                .session
                .document
                .video
                .as_ref()
                .and_then(|p| p.effect_chain(self.video.fx.target(p)))
                .cloned()
                .unwrap_or_default();
            self.preview_chain(&chain);
        }
        if close || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            self.video.fx.open = false;
            self.video.fx.analysis_open = false;
            self.video.fx.auditioning = false;
            self.video.fx.analysis = None;
            self.video.fx.original = false;
            let c = self
                .session
                .document
                .video
                .as_ref()
                .and_then(|p| p.effect_chain(self.video.fx.target(p)))
                .cloned()
                .unwrap_or_default();
            self.preview_chain(&c);
        }
    }

    fn eq_controls(
        &mut self,
        ui: &mut egui::Ui,
        effect: &mut capture_core::video::effects::Effect,
        commit: &mut bool,
    ) {
        use capture_core::video::effects::{Band, BandKind, MAX_EQ_BANDS};
        self.video.fx.band = self.video.fx.band.min(effect.bands.len().saturating_sub(1));
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("eq-band")
                .width(78.)
                .selected_text(if effect.bands.is_empty() {
                    "No bands".into()
                } else {
                    format!("Band {}", self.video.fx.band + 1)
                })
                .show_ui(ui, |ui| {
                    for i in 0..effect.bands.len() {
                        ui.selectable_value(&mut self.video.fx.band, i, format!("Band {}", i + 1));
                    }
                });
            if let Some(b) = effect.bands.get_mut(self.video.fx.band) {
                egui::ComboBox::from_id_salt("eq-band-type")
                    .width(148.)
                    .selected_text(b.kind.label())
                    .show_ui(ui, |ui| {
                        for kind in BandKind::ALL {
                            ui.selectable_value(&mut b.kind, kind, kind.label());
                        }
                    });
            }
            let add = ui.add_enabled(
                effect.bands.len() < MAX_EQ_BANDS,
                egui::Button::new("+ Band"),
            );
            egui::Popup::menu(&add).show(|ui| {
                for kind in BandKind::ALL {
                    if ui.button(kind.label()).clicked() {
                        effect.bands.push(Band::new(
                            kind,
                            match kind {
                                BandKind::Bell => 1000.,
                                BandKind::HighPass => 80.,
                                BandKind::LowPass => 12000.,
                            },
                        ));
                        self.video.fx.band = effect.bands.len() - 1;
                        *commit = true;
                        ui.close();
                    }
                }
            });
            if super::super::ui::button(ui, "Delete effect", "", 26., false)
                .tip("Remove selected EQ band")
                .clicked()
                && !effect.bands.is_empty()
            {
                effect.bands.remove(self.video.fx.band);
                self.video.fx.band = self.video.fx.band.min(effect.bands.len().saturating_sub(1));
                *commit = true;
            }
        });
        if let Some(b) = effect.bands.get_mut(self.video.fx.band) {
            ui.columns(4, |cols| {
                parameter(
                    &mut cols[0],
                    if b.kind == BandKind::Bell {
                        "Frequency"
                    } else {
                        "Cutoff"
                    },
                    &mut b.frequency,
                    20.,
                    20000.,
                    1.,
                    "Hz",
                );
                if b.kind == BandKind::Bell {
                    parameter(&mut cols[1], "Gain", &mut b.gain, -12., 12., 0.1, "dB");
                } else {
                    cols[1].label("Slope");
                    egui::ComboBox::from_id_salt("eq-slope")
                        .width(90.)
                        .selected_text(format!("{} dB/oct", b.slope))
                        .show_ui(&mut cols[1], |ui| {
                            for slope in [12, 24] {
                                ui.selectable_value(&mut b.slope, slope, format!("{slope} dB/oct"));
                            }
                        });
                }
                parameter(
                    &mut cols[2],
                    if b.kind == BandKind::Bell {
                        "Width · Q"
                    } else {
                        "Resonance · Q"
                    },
                    &mut b.q,
                    0.3,
                    5.,
                    0.1,
                    "",
                );
                parameter(
                    &mut cols[3],
                    "Output",
                    &mut effect.values[0],
                    -12.,
                    12.,
                    0.1,
                    "dB",
                );
            });
        }
    }
    fn effect_chain_ui(&mut self, ui: &mut egui::Ui, chain: &mut Chain, commit: &mut bool) {
        let mut target = None;
        let mut toggle = None;
        ui.horizontal_top(|ui| {
            let width = (ui.available_width() - 40.).max(200.);
            ui.allocate_ui_with_layout(
                egui::vec2(width, 54.),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_min_height(54.);
                    let rail = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(width, 54.));
                    ui.painter().rect_filled(rail, 5., crate::tabs::BACKGROUND);
                    egui::ScrollArea::horizontal()
                        .id_salt("effect-chain-scroll")
                        .max_height(66.)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                for (i, f) in chain.effects.iter().enumerate() {
                                    let (rect, r) = ui.allocate_exact_size(
                                        egui::vec2(((width - 40.) / 6.).max(119.), 54.),
                                        egui::Sense::click_and_drag(),
                                    );
                                    let selected = f.id == self.video.fx.selected;
                                    if i > 0 {
                                        let x = rect.left() - ui.spacing().item_spacing.x * 0.5;
                                        ui.painter().line_segment(
                                            [
                                                egui::pos2(x, rect.top() + 8.),
                                                egui::pos2(x, rect.bottom() - 8.),
                                            ],
                                            Stroke::new(1., Color32::from_rgb(143, 154, 159)),
                                        );
                                    }
                                    let color = if f.enabled {
                                        Color32::WHITE
                                    } else {
                                        Color32::GRAY
                                    };
                                    ui.painter().rect_filled(
                                        rect,
                                        5.,
                                        if selected {
                                            BLUE
                                        } else if r.hovered() {
                                            Color32::from_rgb(54, 64, 69)
                                        } else {
                                            Color32::TRANSPARENT
                                        },
                                    );
                                    if selected {
                                        ui.painter().rect_stroke(
                                            rect,
                                            5.,
                                            Stroke::new(1.5, BLUE),
                                            egui::StrokeKind::Inside,
                                        );
                                    }
                                    ui.painter().text(
                                        rect.min + egui::vec2(7., 4.),
                                        egui::Align2::LEFT_TOP,
                                        format!("{:02}", i + 1),
                                        egui::FontId::proportional(10.),
                                        Color32::GRAY,
                                    );
                                    super::super::ui::icon(
                                        ui.painter(),
                                        egui::Rect::from_min_size(
                                            rect.min + egui::vec2(7., 29.),
                                            egui::vec2(16., 16.),
                                        ),
                                        f.kind.label(),
                                        color,
                                    );
                                    ui.painter().text(
                                        rect.min + egui::vec2(29., 36.),
                                        egui::Align2::LEFT_CENTER,
                                        f.kind.short(),
                                        crate::panel_ui::font(12.),
                                        color,
                                    );
                                    let power = egui::Rect::from_min_size(
                                        rect.right_top() + egui::vec2(-26., 2.),
                                        egui::vec2(24., 24.),
                                    );
                                    super::super::ui::icon(
                                        ui.painter(),
                                        power.shrink(4.),
                                        "Effect power",
                                        if f.enabled {
                                            if selected { Color32::WHITE } else { BLUE }
                                        } else {
                                            Color32::GRAY
                                        },
                                    );
                                    if ui
                                        .interact(
                                            power,
                                            egui::Id::unique(("fx-power", f.id)),
                                            egui::Sense::click(),
                                        )
                                        .tip("Enable / bypass effect")
                                        .clicked()
                                    {
                                        toggle = Some(i);
                                    }
                                    if r.clicked() {
                                        self.video.fx.selected = f.id;
                                        self.video.fx.analysis_open = false;
                                    }
                                    if r.drag_started() {
                                        self.video.fx.dragging = Some(f.id);
                                    }
                                    if self.video.fx.dragging.is_some()
                                        && rect.contains(ui.input(|i| {
                                            i.pointer.interact_pos().unwrap_or_default()
                                        }))
                                    {
                                        ui.painter().line_segment(
                                            [rect.left_top(), rect.left_bottom()],
                                            Stroke::new(3., BLUE),
                                        );
                                        if ui.input(|i| i.pointer.any_released()) {
                                            target = Some(i);
                                        }
                                    }
                                    if self.video.fx.dragging == Some(f.id) {
                                        ui.painter().rect_stroke(
                                            rect.translate(egui::vec2(0., -2.)),
                                            4.,
                                            Stroke::new(1., BLUE),
                                            egui::StrokeKind::Inside,
                                        );
                                    }
                                    #[cfg(test)]
                                    ui.ctx().data_mut(|d| {
                                        d.insert_temp(egui::Id::unique(("fx-slot", f.id)), rect)
                                    });
                                }
                            });
                        });
                },
            );
            let (rect, add) = ui.allocate_exact_size(egui::vec2(32., 54.), egui::Sense::click());
            ui.painter().rect_filled(rect, 5., DARK);
            ui.painter().rect_stroke(
                rect,
                5.,
                Stroke::new(1., Color32::from_gray(92)),
                egui::StrokeKind::Inside,
            );
            super::super::ui::icon(
                ui.painter(),
                egui::Rect::from_center_size(rect.center(), egui::vec2(18., 18.)),
                "Plus",
                BLUE,
            );
            egui::Popup::menu(&add).show(|ui| {
                ui.label("ClipDesigner audio effects");
                for kind in Kind::ALL {
                    if super::super::ui::button(ui, kind.label(), kind.label(), 190., false)
                        .clicked()
                    {
                        match chain.add(kind) {
                            Ok(id) => {
                                self.video.fx.selected = id;
                                self.video.fx.analysis_open = false;
                                *commit = true;
                            }
                            Err(e) => self.video.fx.error = crate::guided::friendly(&e.to_string()),
                        }
                        ui.close();
                    }
                }
            });
        });
        if let Some(i) = toggle {
            chain.effects[i].enabled = !chain.effects[i].enabled;
            *commit = true;
        }
        if let Some(i) = target
            && let Some(from) = chain
                .effects
                .iter()
                .position(|f| Some(f.id) == self.video.fx.dragging)
        {
            let effect = chain.effects.remove(from);
            chain.effects.insert(i, effect);
            *commit = true;
        }
        if ui.input(|i| i.pointer.any_released()) {
            self.video.fx.dragging = None;
        }
    }
    fn effect_actions(&mut self, ui: &mut egui::Ui, chain: &mut Chain, commit: &mut bool) {
        let Some(i) = chain
            .effects
            .iter()
            .position(|f| f.id == self.video.fx.selected)
        else {
            return;
        };
        ui.horizontal_wrapped(|ui| {
            if super::super::ui::button(
                ui,
                "Effect power",
                if chain.effects[i].enabled {
                    "Effect on"
                } else {
                    "Bypassed"
                },
                110.,
                chain.effects[i].enabled,
            )
            .clicked()
            {
                chain.effects[i].enabled = !chain.effects[i].enabled;
                *commit = true;
            }
            if super::super::ui::button(ui, "Reset effect", "Reset", 85., false).clicked() {
                let f = &mut chain.effects[i];
                *f = capture_core::video::effects::Effect::new(f.id, f.kind);
                *commit = true;
            }
            if super::super::ui::button(ui, "Delete effect", "Remove Effect", 128., false).clicked()
            {
                chain.effects.remove(i);
                self.video.fx.selected = chain.effects.first().map(|f| f.id).unwrap_or(0);
                *commit = true;
            }
        });
    }
}

fn parameter(
    ui: &mut egui::Ui,
    label: &str,
    v: &mut f32,
    min: f32,
    max: f32,
    step: f32,
    unit: &str,
) {
    let before = *v;
    ui.small(label);
    ui.add(
        egui::DragValue::new(v)
            .range(min..=max)
            .speed(step)
            .suffix(format!(" {unit}")),
    );
    ui.spacing_mut().slider_width = ui.available_width();
    ui.add(
        egui::Slider::new(v, min..=max)
            .step_by(step as f64)
            .show_value(false),
    );
    if (*v - before).abs() < 0.00001 {
        *v = before;
    }
}
pub(super) fn suggestion(f: &capture_core::video::effects::Effect) -> String {
    let v = f.values;
    match f.kind {
        Kind::Equalizer => {
            let mut bands: Vec<_> = f
                .bands
                .iter()
                .filter(|b| b.kind == capture_core::video::effects::BandKind::Bell)
                .collect();
            bands.sort_by(|a, b| b.gain.abs().total_cmp(&a.gain.abs()));
            bands
                .into_iter()
                .take(2)
                .map(|b| format!("{:+.1} dB / {:.0} Hz", b.gain, b.frequency))
                .collect::<Vec<_>>()
                .join(" · ")
        }
        Kind::Compressor => format!("{:.1} dB · {:.1}:1 ratio", v[0], v[1]),
        Kind::Clarity => format!("Presence {:.0}% · Air {:.0}%", v[1], v[2]),
        Kind::DeEsser => format!("{:.1} kHz · Up to {:.1} dB", v[0] / 1000., v[2]),
        Kind::Leveler => format!("{:.0} dBFS RMS · ±{:.1} dB", v[0], v[1]),
        Kind::Limiter => format!("{:.1} dBFS ceiling", v[0]),
    }
}
pub(super) fn reason(k: Kind) -> &'static str {
    match k {
        Kind::Equalizer => "Shape measured mud, presence and harshness",
        Kind::Compressor => "Control louder phrases",
        Kind::Clarity => "Keep speech distinct",
        Kind::DeEsser => "Soften sharp consonants",
        Kind::Leveler => "Even out spoken levels",
        Kind::Limiter => "Leave output headroom",
    }
}
