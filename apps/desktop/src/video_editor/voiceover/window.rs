use super::*;
use egui::{Color32, Stroke};
const BLUE: Color32 = Color32::from_rgb(73, 197, 233);
const RED: Color32 = Color32::from_rgb(237, 107, 120);
impl Editor {
    pub(crate) fn show_voiceover_recorder(&mut self, ctx: &egui::Context) {
        if !self.video.voiceover.open || !self.is_video() {
            return;
        }
        let width = (ctx.content_rect().width() - 40.).clamp(520., 920.);
        let pos = self
            .video
            .voiceover
            .position
            .unwrap_or_else(|| ctx.content_rect().center() - egui::vec2(width * 0.5, 235.));
        let mut moved = None;
        let mut action = 0;
        let mut seek = None;
        let review = self.video.voiceover.take.is_some();
        let recording = self.video.voiceover.recording.is_some();
        let busy =
            recording || self.video.voiceover.deadline.is_some() || self.video.voiceover.waiting;
        let response=egui::Window::new("Narration studio").id(egui::Id::unique("voiceover-studio")).title_bar(false).collapsible(false).resizable(false).current_pos(pos).fixed_size(egui::vec2(width,if self.video.voiceover.with_camera {530.}else{480.})).frame(egui::Frame::new().fill(Color32::from_rgb(48,58,62)).stroke(Stroke::new(2.,crate::tabs::ACTIVE_BLUE)).corner_radius(10).inner_margin(16.)).show(ctx,|ui|{
            ui.spacing_mut().item_spacing=egui::vec2(10.,10.);
            ui.horizontal(|ui|{
                super::super::ui::icon(ui.painter(),egui::Rect::from_min_size(ui.cursor().min,egui::vec2(22.,22.)),"Voiceover",BLUE);ui.add_space(30.);
                ui.label(egui::RichText::new("Narration studio").size(19.).strong());
                ui.weak(if review {"/ Review your take"} else {"/ Record narration"});
                let (_,drag)=ui.allocate_exact_size(egui::vec2((ui.available_width()-30.).max(0.),26.),egui::Sense::drag());
                if drag.dragged(){moved=Some(pos+ui.input(|i|i.pointer.delta()));}
                if super::super::ui::button(ui,"Close","",28.,false).clicked(){action=1;}
            });
            ui.separator();
            ui.add_enabled_ui(!busy&&!review,|ui|{
                let before=self.video.voiceover.with_camera;
                ui.horizontal(|ui|{ui.selectable_value(&mut self.video.voiceover.with_camera,false,"Voice only");ui.selectable_value(&mut self.video.voiceover.with_camera,true,"Voice + Camera");});
                if before!=self.video.voiceover.with_camera {self.video.voiceover.capture=None;self.video.voiceover.ready=false;self.video.voiceover.camera_texture=None;self.video.voiceover.error.clear();self.video.voiceover.rearm_at=Some(Instant::now()+Duration::from_millis(150));}
            });
            ui.add_enabled_ui(!busy&&!review,|ui|{
                ui.horizontal(|ui|{
                    ui.vertical(|ui|{
                        ui.small("MICROPHONE");
                        let before=self.video.voiceover.device.clone();
                        egui::ComboBox::from_id_salt("studio-input").width((width*0.42).max(180.)).selected_text(self.video.voiceover.devices.iter().find(|(id,_)|id==&before).map(|(_,name)|name.as_str()).unwrap_or("Default input device")).show_ui(ui,|ui|{
                            for (id,name) in &self.video.voiceover.devices {ui.selectable_value(&mut self.video.voiceover.device,id.clone(),name);}
                        });
                        if before!=self.video.voiceover.device {self.video.voiceover.capture=None;self.video.voiceover.error.clear();self.video.voiceover.rearm_at=Some(Instant::now()+Duration::from_millis(150));}
                    });
                    ui.vertical(|ui|{ui.small("START AT");egui::ComboBox::from_id_salt("studio-start").width(118.).selected_text(if self.video.voiceover.from_start{"Project start"}else{"Playhead"}).show_ui(ui,|ui|{
                        ui.selectable_value(&mut self.video.voiceover.from_start,false,"Playhead");ui.selectable_value(&mut self.video.voiceover.from_start,true,"Project start");
                    });});
                    ui.vertical(|ui|{ui.small("COUNTDOWN");egui::ComboBox::from_id_salt("studio-countdown").width(90.).selected_text(if self.video.voiceover.countdown==0{"Off".into()}else{format!("{} seconds",self.video.voiceover.countdown)}).show_ui(ui,|ui|{
                        for n in [0,3,5] {ui.selectable_value(&mut self.video.voiceover.countdown,n,if n==0{"Off".into()}else{format!("{n} seconds")});}
                    });});
                });
            });
            if self.video.voiceover.with_camera {
                ui.add_enabled_ui(!busy&&!review,|ui|{ui.horizontal(|ui|{
                    ui.small("CAMERA");let before=(self.video.voiceover.camera_device.clone(),self.video.voiceover.camera_height);
                    egui::ComboBox::from_id_salt("narration-camera").width((width*0.48).max(180.)).selected_text(self.video.voiceover.cameras.iter().find(|c|c.0==self.video.voiceover.camera_device).map(|c|c.1.as_str()).unwrap_or("No camera found")).show_ui(ui,|ui|{for (id,name) in &self.video.voiceover.cameras{ui.selectable_value(&mut self.video.voiceover.camera_device,id.clone(),name);}});
                    egui::ComboBox::from_id_salt("narration-resolution").width(110.).selected_text(format!("{}p preferred",self.video.voiceover.camera_height)).show_ui(ui,|ui|{for h in [720,1080]{ui.selectable_value(&mut self.video.voiceover.camera_height,h,format!("{h}p"));}}).response.on_hover_text("Uses the nearest supported camera format. Recorded at 30 fps.");
                    if super::super::ui::button(ui,"Refresh","",26.,false).on_hover_text("Refresh camera list").clicked(){self.video.voiceover.error.clear();self.video.voiceover.cameras_job=Some(voiceover_capture::cameras());}
                    if before!=(self.video.voiceover.camera_device.clone(),self.video.voiceover.camera_height){self.video.voiceover.capture=None;self.video.voiceover.ready=false;self.video.voiceover.error.clear();self.video.voiceover.rearm_at=Some(Instant::now()+Duration::from_millis(150));}
                });});
            }
            ui.horizontal(|ui|{
                let label=if let Some(d)=self.video.voiceover.deadline {format!("Starting in {}…",d.saturating_duration_since(Instant::now()).as_secs()+1)} else if recording {"Recording · original sources".into()} else if review {"Take ready · click or drag the playhead to review".into()} else {"Ready when you are".into()};
                ui.colored_label(if recording{RED}else{BLUE},label);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                    let time=if recording {self.video.voiceover.wave.frames/48}else{self.video.voiceover.review_at};
                    ui.label(egui::RichText::new(timestamp(time)).monospace().size(20.));
                });
            });
            let duration=self.video.voiceover.take.as_ref().map(|t|t.duration).unwrap_or((self.video.voiceover.wave.frames/48).max(10_000));
            let current=if recording{self.video.voiceover.wave.frames/48}else{self.video.voiceover.review_at};
            if self.video.voiceover.with_camera {
                ui.columns(2,|columns|{
                    let (r,_)=columns[0].allocate_exact_size(egui::vec2(columns[0].available_width(),190.),egui::Sense::hover());
                    columns[0].painter().rect_filled(r,7.,Color32::from_rgb(20,27,30));
                    if let Some((pixels,texture))=&self.video.voiceover.camera_texture {
                        let size=egui::vec2(pixels.width() as f32,pixels.height() as f32);let scale=(r.width()/size.x).min(r.height()/size.y);let dest=egui::Rect::from_center_size(r.center(),size*scale);
                        columns[0].painter().image(texture.id(),dest,egui::Rect::from_min_max(egui::Pos2::ZERO,egui::pos2(1.,1.)),Color32::WHITE);
                    }else{columns[0].painter().text(r.center(),egui::Align2::CENTER_CENTER,"Camera preview",egui::FontId::proportional(14.),Color32::GRAY);}
                    if let Some(t)=waveform(&mut columns[1],&self.video.voiceover.wave,duration,current,review,recording){seek=Some(t);}
                });
            } else if let Some(t)=waveform(ui,&self.video.voiceover.wave,duration,current,review,recording){seek=Some(t);}
            ui.horizontal(|ui|{
                if review {
                    crate::controls::toggle(ui,&mut self.video.voiceover.review_with_project,"Follow project playhead");
                    if super::super::ui::button(ui,"Previous","",32.,false).on_hover_text("Start of take").clicked(){seek=Some(0);}
                    if super::super::ui::button(ui,if self.video.voiceover.review_clock.is_some(){"Pause"}else{"Play"},if self.video.voiceover.review_clock.is_some(){"Pause"}else{"Play take"},105.,false).clicked(){action=5;}
                    ui.weak(format!("{} total · Space to play / pause",timestamp(duration)));
                }else{
                    let level=self.video.voiceover.capture.as_ref().map(|c|f32::from_bits(c.level.load(Ordering::Relaxed))).unwrap_or(0.);
                    ui.small("INPUT");ui.add(egui::ProgressBar::new(level.sqrt().clamp(0.,1.)).desired_width(135.).fill(if level>0.97{RED}else{BLUE}));
                    let now=ui.input(|i|i.time);let key=egui::Id::unique("narration-input-status");
                    let mut activity=ui.ctx().data(|d|d.get_temp::<(f64,f64)>(key)).unwrap_or((-10.,-10.));
                    if level>0.97{activity.1=now;}
                    if level>0.001{activity.0=now;}ui.ctx().data_mut(|d|d.insert_temp(key,activity));
                    ui.small(if now-activity.1<0.7{"Too loud · lower input gain"}else if now-activity.0>1.5{"Speak to check your microphone"}else{"Microphone active"});
                }
            });
            if !review {
                ui.horizontal(|ui| { ui.label("Input channels"); ui.selectable_value(&mut self.video.voiceover.mono,false,"Stereo"); ui.selectable_value(&mut self.video.voiceover.mono,true,"Mono").on_hover_text("Combine input channels for both recording and headphone monitoring"); });
                if let Some(c)=&self.video.voiceover.capture {c.mono.store(self.video.voiceover.mono,Ordering::Relaxed);}
                crate::controls::toggle(ui,&mut self.video.voiceover.monitor,"Monitor through headphones");
                self.audio_output_controls(ui,true);
                if let Some(c)=&self.video.voiceover.capture {c.monitor.store(self.video.voiceover.monitor,Ordering::Relaxed);}
            }else{ui.weak(if self.video.voiceover.with_camera {"Keep creates linked Camera and Narration clips. Framing and sound stay independently editable."}else{"Keep adds editable narration. Your original audio is preserved."});}
            ui.separator();
            if !self.video.voiceover.error.is_empty(){ui.colored_label(RED,&self.video.voiceover.error);if ui.button("Reconnect input").clicked(){self.video.voiceover.error.clear();self.video.voiceover.rearm_at=Some(Instant::now()+Duration::from_millis(150));}}
            if self.video.voiceover.confirm_discard {
                ui.horizontal(|ui|{ui.colored_label(RED,"Discard this take and close?");ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                    if ui.add(crate::controls::dialog_button("Discard take",true)).clicked(){action=7;}
                    if ui.add(crate::controls::dialog_button("Keep reviewing",false)).clicked(){self.video.voiceover.confirm_discard=false;}
                });});
            }else{
                ui.horizontal(|ui|{
                    ui.small(if review{"REVIEW & KEEP"}else if recording{"RECORDING"}else{"RECORD A NEW TAKE"});
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                        if review {
                            if ui.add(crate::controls::dialog_button("Keep take",true)).clicked(){action=4;}
                            if ui.add(crate::controls::dialog_button("Keep & analyze",false)).clicked(){action=6;}
                            if ui.add(crate::controls::dialog_button("New take",false)).clicked(){action=3;}
                            if ui.add(crate::controls::dialog_button("Discard",false)).clicked(){action=1;}
                        }else if recording {
                            if ui.add_enabled(!self.video.voiceover.waiting,crate::controls::dialog_button("Stop & review",true)).clicked(){action=8;}
                            if ui.add(crate::controls::dialog_button("Cancel take",false)).clicked(){action=1;}
                        }else if self.video.voiceover.deadline.is_some(){if ui.add(crate::controls::dialog_button("Cancel countdown",false)).clicked(){self.video.voiceover.deadline=None;}}
                        else {
                            if ui.add_enabled(self.video.voiceover.ready&&!self.video.voiceover.waiting,crate::controls::dialog_button("Record narration",true).fill(Color32::from_rgb(190,57,74))).clicked(){action=9;}
                            if ui.add(crate::controls::dialog_button("Cancel",false)).clicked(){action=1;}
                        }
                    });
                });
            }
        });
        if let Some(r) = response {
            self.video.voiceover.position = Some(moved.unwrap_or(r.response.rect.min));
            #[cfg(test)]
            ctx.data_mut(|d| d.insert_temp(egui::Id::unique("voiceover-window"), r.response.rect));
        }
        if let Some(at) = seek {
            self.review_voiceover(at, self.video.voiceover.review_clock.is_some());
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            action = 1;
        }
        if action == 0
            && !ctx.egui_wants_keyboard_input()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
        {
            action = if self.video.voiceover.confirm_discard {
                7
            } else if review {
                4
            } else if recording && !self.video.voiceover.waiting {
                8
            } else if self.video.voiceover.ready && !busy {
                9
            } else {
                0
            };
        }
        match action {
            1 => {
                if review || recording || self.video.voiceover.waiting {
                    self.video.voiceover.confirm_discard = true;
                } else {
                    self.video.pause();
                    self.video.voiceover = State::default();
                }
            }
            3 => self.reset_voiceover_take(ctx),
            4 | 6 => {
                self.review_voiceover(self.video.voiceover.review_at, false);
                if self.keep_voiceover() && action == 6 {
                    self.open_effects_workstation();
                    self.video.fx.analysis_open = true;
                    self.begin_audio_analysis();
                }
            }
            5 => {
                let play = self.video.voiceover.review_clock.is_none();
                let at = if self
                    .video
                    .voiceover
                    .take
                    .as_ref()
                    .is_some_and(|t| self.video.voiceover.review_at >= t.duration)
                {
                    0
                } else {
                    self.video.voiceover.review_at
                };
                self.review_voiceover(at, play);
            }
            7 => {
                self.video.pause();
                self.video.voiceover = State::default();
            }
            8 => {
                self.video.voiceover.waiting = true;
                if let Some(c) = &self.video.voiceover.capture {
                    let _ = c.send.send(Control::Stop);
                }
                self.video.pause();
            }
            9 => {
                self.video.voiceover.at = if self.video.voiceover.from_start {
                    0
                } else {
                    self.video_time()
                };
                self.video.voiceover.wave = Default::default();
                self.video.voiceover.deadline = Some(
                    Instant::now() + Duration::from_secs(self.video.voiceover.countdown as u64),
                );
            }
            _ => (),
        }
    }
}
fn timestamp(ms: u64) -> String {
    format!(
        "{:02}:{:02}.{:01}",
        ms / 60000,
        ms / 1000 % 60,
        ms / 100 % 10
    )
}
fn waveform(
    ui: &mut egui::Ui,
    wave: &waveform::Snapshot,
    duration: u64,
    at: u64,
    review: bool,
    recording: bool,
) -> Option<u64> {
    let (r, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 190.),
        if review {
            egui::Sense::click_and_drag()
        } else {
            egui::Sense::hover()
        },
    );
    let p = ui.painter_at(r);
    p.rect_filled(r, 7., Color32::from_rgb(27, 36, 40));
    let plot = r.shrink2(egui::vec2(14., 24.));
    let cy = plot.center().y;
    for i in 0..=5 {
        let x = plot.left() + plot.width() * i as f32 / 5.;
        p.line_segment(
            [egui::pos2(x, plot.top()), egui::pos2(x, plot.bottom())],
            Stroke::new(1., Color32::from_gray(54)),
        );
        p.text(
            egui::pos2(x, r.bottom() - 6.),
            if i == 0 {
                egui::Align2::LEFT_BOTTOM
            } else if i == 5 {
                egui::Align2::RIGHT_BOTTOM
            } else {
                egui::Align2::CENTER_BOTTOM
            },
            timestamp(duration * i / 5),
            egui::FontId::monospace(10.),
            Color32::GRAY,
        );
    }
    p.line_segment(
        [plot.left_center(), plot.right_center()],
        Stroke::new(1., Color32::from_gray(76)),
    );
    for (i, v) in wave.peaks.iter().enumerate() {
        let x = plot.left()
            + (i as f32 + 0.5) * wave.bucket as f32 / 48. / duration.max(1) as f32 * plot.width();
        if x > plot.right() {
            break;
        }
        p.line_segment(
            [
                egui::pos2(x, cy - v[1] * plot.height() * 0.46),
                egui::pos2(x, cy - v[0] * plot.height() * 0.46),
            ],
            Stroke::new(
                (plot.width() / wave.peaks.len().max(1) as f32).clamp(1., 3.),
                if recording { RED } else { BLUE },
            ),
        );
    }
    if wave.peaks.is_empty() {
        p.text(
            plot.center(),
            egui::Align2::CENTER_CENTER,
            if recording {
                "Listening…"
            } else {
                "Your voice will appear here as you record"
            },
            egui::FontId::proportional(14.),
            Color32::from_gray(159),
        );
    }
    let x = plot.left() + at.min(duration) as f32 / duration.max(1) as f32 * plot.width();
    if review || recording {
        p.line_segment(
            [egui::pos2(x, plot.top() - 8.), egui::pos2(x, plot.bottom())],
            Stroke::new(1.5, Color32::WHITE),
        );
        p.circle_filled(egui::pos2(x, plot.top() - 8.), 4., Color32::WHITE);
    }
    #[cfg(test)]
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::unique("voiceover-waveform"), plot));
    if review && (response.clicked() || response.dragged()) {
        response
            .interact_pointer_pos()
            .map(|p| (((p.x - plot.left()) / plot.width()).clamp(0., 1.) * duration as f32) as u64)
    } else {
        None
    }
}
