use super::*;
use crate::tips::Tip as _;

impl State {
    pub(super) fn show(&mut self, ctx: &egui::Context) {
        self.show_with_copy(ctx, &mut |html, text| {
            arboard::Clipboard::new()
                .and_then(|mut c| c.set_html(html, Some(text)))
                .map_err(|e| e.to_string())
        });
    }
    pub(super) fn show_with_copy(
        &mut self,
        ctx: &egui::Context,
        copy: &mut impl FnMut(&str, &str) -> Result<(), String>,
    ) {
        if self.copy_pending {
            self.copy_pending = false;
            self.copy(ctx, copy);
        }
        if !self.open {
            return;
        }
        let mut open = true;
        let mut close = false;
        let mut copy_clicked = false;
        let busy = self.busy();
        egui::Window::new("Grab Text Results")
            .id(egui::Id::unique("grab-text-result"))
            .open(&mut open).collapsible(false).resizable(true)
            .default_size([590.,370.]).min_size([340.,220.])
            .default_pos(egui::pos2(60.,60.))
            .show(ctx,|ui| {
                let area=ui.available_size();
                let page_height=(area.y-58.-if self.copy_status.is_some() {20.}else{0.}).max(130.);
                egui::Frame::new().fill(egui::Color32::WHITE)
                    .stroke(egui::Stroke::new(1.,egui::Color32::from_gray(135))).inner_margin(6.)
                    .show(ui,|ui| {
                        ui.set_min_size(egui::vec2((area.x-14.).max(310.),page_height));
                        ui.set_max_height(page_height);
                        if busy {ui.horizontal(|ui| {ui.spinner();ui.colored_label(egui::Color32::DARK_GRAY,"Recognizing text on this PC…");});}
                        match self.result.as_mut() {
                            Some(Ok(result))=> {
                                let snapshot=result.clone(); let plain=self.plain;
                                let mut layouter=|ui:&egui::Ui,buffer:&dyn egui::TextBuffer,width:f32|ui.fonts_mut(|fonts|fonts.layout_job(snapshot.job(buffer.as_str(),plain,width)));
                                ui.visuals_mut().override_text_color=Some(egui::Color32::BLACK);
                                ui.visuals_mut().selection.bg_fill=egui::Color32::from_rgb(160,205,245);
                                ui.visuals_mut().selection.stroke=egui::Stroke::new(1.,egui::Color32::from_rgb(10,50,100));
                                egui::ScrollArea::vertical().auto_shrink([false,false]).max_height(page_height).show(ui,|ui| {
                                    let response=ui.add(crate::unicode_input::TextEdit::multiline(&mut result.text).id(egui::Id::unique("ocr-editor")).frame(egui::Frame::NONE)
                                        .desired_width(f32::INFINITY).desired_rows(1).hint_text("No text was found in this image.").layouter(&mut layouter));
                                    crate::controls::focus_once(&response,&mut self.focus_pending);
                                    if response.changed() {result.spans=snapshot.edited_spans(&result.text);self.copy_status=None;}
                                });
                            }
                            Some(Err(error))=>{ui.colored_label(egui::Color32::from_rgb(160,25,25),error.as_str());}
                            None=>{}
                        }
                    });
                ui.add_space(8.);
                ui.horizontal(|ui| {
                    let metadata=match &self.result {Some(Ok(r))=>format!("Language: {}. Formatting estimates color, size and the nearest bundled font per line.{}",r.language,if r.scaled {" Large image reduced for recognition; review small text carefully."} else {""}),_=>"Show and copy text without formatting.".into()};
                    ui.label("Plain text").tip(&metadata);
                    let (rect,mut response)=ui.allocate_exact_size(egui::vec2(36.,20.),egui::Sense::click());
                    if response.clicked() {self.plain = !self.plain;self.copy_status=None;response.mark_changed();}
                    response.widget_info(||egui::WidgetInfo::selected(egui::Role::CheckBox,ui.is_enabled(),self.plain,"Plain text"));
                    ui.painter().rect_filled(rect,10,if self.plain {crate::shell::ACCENT}else{egui::Color32::from_gray(105)});
                    ui.painter().circle_filled(egui::pos2(if self.plain {rect.right()-10.}else{rect.left()+10.},rect.center().y),8.,egui::Color32::WHITE);
                    if response.hovered() || response.has_focus() {ui.painter().rect_stroke(rect,10,egui::Stroke::new(1.,egui::Color32::WHITE),egui::StrokeKind::Outside);}
                    response.tip(metadata);
                    #[cfg(test)] ctx.data_mut(|d|d.insert_temp(egui::Id::unique("ocr-plain"),rect));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui| {
                        let response=ui.add_sized([94.,crate::controls::DIALOG_ACTION_HEIGHT],egui::Button::new("Close"));close=response.clicked();
                        #[cfg(test)] ctx.data_mut(|d|d.insert_temp(egui::Id::unique("ocr-close"),response.rect));
                        let enabled=self.result.as_ref().is_some_and(|r|r.as_ref().is_ok_and(|r|!r.text.trim().is_empty()));
                        let response=ui.add_enabled_ui(enabled,|ui|ui.add_sized([100.,26.],egui::Button::new("Copy All").fill(crate::shell::ACCENT))).inner;
                        copy_clicked=response.clicked();
                        #[cfg(test)] ctx.data_mut(|d|d.insert_temp(egui::Id::unique("ocr-copy"),response.rect));
                    });
                });
                if let Some(status)=&self.copy_status {ui.small(status);}
            });
        if copy_clicked {
            self.copy(ctx, copy);
        }
        if close || !open {
            self.open = false;
        }
    }
    fn copy(
        &mut self,
        ctx: &egui::Context,
        copy: &mut impl FnMut(&str, &str) -> Result<(), String>,
    ) {
        let Some(Ok(result)) = &self.result else {
            return;
        };
        if result.text.trim().is_empty() {
            return;
        }
        if self.plain || result.spans.is_empty() {
            ctx.copy_text(result.text.clone());
            self.copy_status = Some("Copied plain text.".into());
        } else {
            self.copy_status = Some(match copy(&result.html(), &result.text) {
                Ok(()) => "Copied formatted text.".into(),
                Err(error) => format!("Could not copy text: {error}"),
            });
        }
    }
}
