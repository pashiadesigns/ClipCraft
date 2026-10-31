use super::*;
use capture_core::text::typography::{Case, Script};
impl Editor {
    pub(super) fn typography_controls(
        &mut self,
        ui: &mut egui::Ui,
        objects: &[capture_core::Annotation],
        enabled: bool,
        compact: bool,
    ) {
        ui.add_enabled_ui(enabled,|ui|ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x=3.;
            for (script,icon,label) in [(Script::Superscript,"x²","Superscript"),(Script::Subscript,"x₂","Subscript")] {
                let selected=enabled && objects.iter().all(|a|a.text.as_ref().is_some_and(|s|s.typography().script==script));
                if text_button(ui,egui::RichText::new(icon),label,enabled,selected) { self.apply_quick_text(TextAction::Script(if selected {Script::Normal} else {script})); }
            }
            let response=ui.add_sized([33.,24.],egui::Button::new("Aa")).on_hover_text("Change case");
            response.widget_info(||egui::WidgetInfo::labeled(egui::Role::Button,enabled,"Change case"));
            #[cfg(test)] ui.ctx().data_mut(|d|d.insert_temp(egui::Id::unique("case-menu"),response.rect));
            egui::Popup::menu(&response).show(|ui| {
                for case in [Case::Upper,Case::Lower,Case::Title] {
                    let r=ui.button(case.label());
                    #[cfg(test)] ui.ctx().data_mut(|d|d.insert_temp(egui::Id::unique(("case-choice",case.label())),r.rect));
                    if r.clicked() {self.apply_quick_text(TextAction::Case(case));ui.close();}
                }
            });
            let response=ui.add_sized([36.,24.],egui::Button::new("AV")).on_hover_text("Character spacing");
            response.widget_info(||egui::WidgetInfo::labeled(egui::Role::Button,enabled,"Character spacing"));
            #[cfg(test)] ui.ctx().data_mut(|d|d.insert_temp(egui::Id::unique("spacing-menu"),response.rect));
            egui::Popup::menu(&response).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
                ui.set_width(244.);
                crate::property_layout::section_help(ui,"Character spacing","Extra space between characters, in image pixels. Applies to the selected labels or current text draft. Drag to preview; dismiss to retain one Undo.");
                let mut spacing=objects.first().and_then(|a|a.text.as_ref()).map_or(0.,|s|s.typography().spacing);
                let r=crate::property_layout::slider(ui,"Spacing",egui::Slider::new(&mut spacing,-4.0..=32.0).suffix(" px"));
                if r.changed() {self.preview_quick_text(TextAction::Spacing(spacing));}
                if ui.button("Reset spacing").clicked() {self.preview_quick_text(TextAction::Spacing(0.));ui.close();}
            });
            if compact { ui.label(egui::RichText::new("ab").background_color(egui::Color32::from_rgb(255,225,25)).color(egui::Color32::BLACK)); } else { ui.label("Highlight"); }
            let mut color=objects.first().and_then(|a|a.text.as_ref()).and_then(|s|s.typography().highlight).unwrap_or([255,230,50,0]);
            let r=ui.scope(|ui| {ui.spacing_mut().interact_size=egui::vec2(24.,24.);crate::color_widgets::rgba_live(ui,&mut color)}).inner.on_hover_text("Text highlight color · transparent removes highlighting");
            #[cfg(test)] ui.ctx().data_mut(|d|d.insert_temp(egui::Id::unique("text-highlight-color"),r.rect));
            if r.changed() {self.preview_quick_text(TextAction::Highlight((color[3]>0).then_some(color)));}
        }));
    }
}
