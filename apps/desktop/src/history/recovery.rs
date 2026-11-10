use super::*;
use capture_core::history::recovery::Health;
use std::sync::{Arc, Mutex};
type ScanResult = Arc<Mutex<Option<Result<Vec<Health>, String>>>>;
#[derive(Clone, Default)]
struct Scan {
    result: ScanResult,
    filter: Option<String>,
    message: String,
}

impl Editor {
    pub(crate) fn library_storage_settings(&mut self, ui: &mut egui::Ui) {
        crate::panel_ui::heading(ui, "Library", "Library storage & recovery");
        ui.label(
            egui::RichText::new(
                "Capacity applies to future saves. Existing items are never deleted automatically.",
            )
            .size(12.)
            .color(crate::panel_ui::MUTED),
        );
        let Some(store) = self.history.store.as_mut() else {
            ui.label("Library is unavailable.");
            return;
        };
        let key = egui::Id::unique("library-capacity-draft");
        let mut limits = ui
            .ctx()
            .data(|d| d.get_temp::<capture_core::history::Limits>(key))
            .unwrap_or_else(|| store.limits().unwrap_or_default());
        let used = store.entries().iter().map(|e| e.bytes).sum::<u64>();
        ui.label(format!(
            "{} items · {:.1} MB",
            store.entries().len(),
            used as f64 / 1_000_000.
        ));
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 110.),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_min_width(ui.available_width());
                crate::property_layout::slider(
                    ui,
                    "Item capacity",
                    egui::Slider::new(&mut limits.entries, 1..=1000),
                );
                let mut mb = limits.bytes / 1_000_000;
                if crate::property_layout::slider(
                    ui,
                    "Storage (MB)",
                    egui::Slider::new(&mut mb, 1..=16000),
                )
                .changed()
                {
                    limits.bytes = mb * 1_000_000;
                }
            },
        );
        ui.ctx().data_mut(|d| d.insert_temp(key, limits));
        let mut scan = false;
        ui.horizontal(|ui| {
            scan = ui.button("Check Library / Recover…").clicked();
        });
        if let Some(root) = self.history.root() {
            ui.label("Folder");
            ui.add(egui::Label::new(root.display().to_string()).wrap());
        }
        if !self.history.error.is_empty() {
            ui.label(&self.history.error);
        }
        if scan {
            self.start_library_scan(ui.ctx(), None);
        }
    }
    pub(crate) fn save_library_capacity(&mut self, ctx: &egui::Context) {
        let key = egui::Id::unique("library-capacity-draft");
        if let (Some(limits), Some(store)) = (
            ctx.data_mut(|d| d.remove_temp::<capture_core::history::Limits>(key)),
            self.history.store.as_mut(),
        ) && store.limits().ok() != Some(limits)
        {
            self.history.error = store
                .set_limits(limits)
                .map(|_| "Library capacity saved.".into())
                .unwrap_or_else(|e| e.to_string());
        }
    }
    pub(crate) fn discard_library_capacity(&mut self, ctx: &egui::Context) {
        let key = egui::Id::unique("library-capacity-draft");
        ctx.data_mut(|d| d.remove::<capture_core::history::Limits>(key));
    }
    pub(crate) fn start_library_scan(&mut self, ctx: &egui::Context, filter: Option<String>) {
        let Some(root) = self.history.root() else {
            return;
        };
        let scan = Scan {
            filter,
            ..Default::default()
        };
        let result = scan.result.clone();
        let ctx2 = ctx.clone();
        std::thread::spawn(move || {
            let report = History::open(&root)
                .map(|s| s.check_integrity())
                .map_err(|e| e.to_string());
            *result.lock().unwrap() = Some(report);
            ctx2.request_repaint();
        });
        ctx.data_mut(|d| d.insert_temp(egui::Id::unique("library-recovery"), scan));
    }
    pub(crate) fn show_library_recovery(&mut self, ctx: &egui::Context) {
        let key = egui::Id::unique("library-recovery");
        let Some(mut scan) = ctx.data_mut(|d| d.remove_temp::<Scan>(key)) else {
            return;
        };
        let result = scan.result.lock().unwrap().clone();
        let mut close = false;
        let mut restore = None;
        let modal=crate::controls::Modal::new(key).show(ctx,|ui|{
            ui.set_width(480.);crate::property_layout::section_help(ui,"Library check & previous versions","Check validates current projects and thumbnails without modifying them. Recover copy validates the saved revision, creates a separate Library item, and leaves the original untouched. Older builds did not retain previous revisions.");
            match &result {
                None=>{ui.spinner();ui.label("Checking local Library…");ctx.request_repaint_after(std::time::Duration::from_millis(100));},
                Some(Err(e))=>{ui.colored_label(egui::Color32::LIGHT_RED,e);},
                Some(Ok(rows))=>{
                    ui.label(format!("{} items checked · {} need attention",rows.len(),rows.iter().filter(|r|!r.problems.is_empty()).count()));
                    egui::ScrollArea::vertical().auto_shrink([false,true]).max_height((ctx.content_rect().height()-200.).clamp(100.,360.)).show(ui,|ui|{
                        for row in rows.iter().filter(|r|scan.filter.as_ref().is_none_or(|id|*id==r.entry.id)){
                            ui.push_id(&row.entry.id,|ui|{ui.separator();ui.strong(&row.entry.label);if row.problems.is_empty(){ui.label("Current project and thumbnail checked.");}else{for p in &row.problems{ui.colored_label(egui::Color32::LIGHT_RED,p);}}
                                if row.revisions.is_empty(){ui.small("No earlier versions retained.");}
                                for (i,r) in row.revisions.iter().enumerate(){ui.horizontal(|ui|{let mut dated=r.clone();dated.created_ms=r.updated_ms.unwrap_or(r.created_ms);ui.label(format!("Previous {} · {}",i+1,dated.created_utc()));if ui.button("Recover copy").clicked(){restore=Some(r.clone());}});}
                            });
                        }
                    });
                }
            }
            if !scan.message.is_empty(){ui.label(&scan.message);}
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{close=ui.add(crate::controls::dialog_button("Close",true)).clicked();});
        });
        if close || crate::text_edit::cancel_on_escape(&modal) {
            return;
        }
        if let Some(revision) = restore {
            scan.message = match self
                .history
                .store
                .as_mut()
                .ok_or("Library is unavailable".to_owned())
                .and_then(|s| s.restore_revision(&revision).map_err(|e| e.to_string()))
            {
                Ok(e) => format!(
                    "Recovered as “{}”. Open it from Library; the original was kept.",
                    e.label
                ),
                Err(e) => e,
            };
            self.history.refresh();
        }
        ctx.data_mut(|d| d.insert_temp(key, scan));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::property_layout::tests::{PaintedOutput, dump_meshes};
    #[test]
    fn recovery_dialog_restores_copy_and_general_embeds_capacity() {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        ctx.style_mut_of(egui::Theme::Dark, |s| s.animation_time = 0.);
        let mut e = crate::canvas::tests::editor(&ctx);
        let root = tempfile::tempdir().unwrap();
        let mut store = History::open(root.path()).unwrap();
        let doc = capture_core::Document::canvas(50, 40, [50, 150, 200, 255]).unwrap();
        let entry = store
            .create(&doc, "Synthetic guide", Origin::Canvas)
            .unwrap();
        let current = store
            .update(
                &entry,
                &capture_core::Document::canvas(50, 40, [200, 30, 50, 255]).unwrap(),
            )
            .unwrap();
        e.history = HistoryPanel::isolated(root.path());
        let scan = Scan {
            result: Arc::new(Mutex::new(Some(Ok(store.check_integrity())))),
            ..Default::default()
        };
        let key = egui::Id::unique("library-recovery");
        ctx.data_mut(|d| d.insert_temp(key, scan));
        let frame = |e: &mut Editor, events| {
            PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900., 580.),
                    )),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| e.show_library_recovery(ui.ctx()),
            ))
        };
        let mut first = frame(&mut e, vec![]);
        let out = frame(&mut e, vec![]);
        let button = crate::color_widgets::tests::text_rect(&out.shapes, "Recover copy").unwrap();
        assert!(button.bottom() < 530.);
        first.0.textures_delta.append(out.0.textures_delta.clone());
        if let Ok(dir) = std::env::var("CLIPCRAFT_HEADLESS_RECOVERY_DIR") {
            dump_meshes(&ctx, &dir, "recovery-dialog", first, out);
        }
        for pressed in [true, false] {
            let pos = button.center();
            frame(
                &mut e,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        store.refresh().unwrap();
        assert_eq!(store.entries().len(), 2);
        assert_eq!(
            store
                .load(&current.id)
                .unwrap()
                .render()
                .unwrap()
                .get_pixel(0, 0)
                .0,
            [200, 30, 50, 255]
        );
        let recovered = store.entries().iter().find(|e| e.id != current.id).unwrap();
        assert_eq!(
            store.load(&recovered.id).unwrap().render().unwrap(),
            doc.render().unwrap()
        );
        ctx.data_mut(|d| d.remove::<Scan>(key));
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let frame = |e: &mut Editor| {
            PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900., 580.),
                    )),
                    focused: true,
                    ..Default::default()
                },
                |ui| e.preferences_library(ui, &mut true),
            ))
        };
        let first = frame(&mut e);
        let out = frame(&mut e);
        assert!(
            crate::color_widgets::tests::text_rect(&out.shapes, "Library storage & recovery")
                .is_some()
        );
        if let Ok(dir) = std::env::var("CLIPCRAFT_HEADLESS_RECOVERY_DIR") {
            dump_meshes(&ctx, &dir, "general-storage", first, out);
        }
    }
}
