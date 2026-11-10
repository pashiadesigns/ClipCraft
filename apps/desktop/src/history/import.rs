use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
#[derive(Default)]
pub(super) struct Import {
    receiver: Option<mpsc::Receiver<capture_core::history::import::Outcome>>,
    stop: Arc<AtomicBool>,
    total: usize,
    done: usize,
    added: usize,
    errors: Vec<String>,
    last_success: Option<PathBuf>,
}
impl Drop for Import {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
impl Editor {
    pub(super) fn library_import_button(&mut self, ui: &mut egui::Ui) {
        if ui
            .add_enabled(
                self.history.store.is_some() && self.history.import.receiver.is_none(),
                egui::Button::new("Import images…"),
            )
            .clicked()
        {
            self.suspend_global_capture();
            if let Some(paths) = rfd::FileDialog::new()
                .set_title("Import images into Library")
                .set_directory(self.dialog_directory())
                .add_filter(
                    "Images",
                    &["png", "jpg", "jpeg", "bmp", "gif", "webp", "svg", "eps"],
                )
                .pick_files()
            {
                self.begin_library_import(paths, ui.ctx());
            }
        }
    }
    fn begin_library_import(&mut self, paths: Vec<PathBuf>, ctx: &egui::Context) {
        if paths.is_empty() || self.history.import.receiver.is_some() {
            return;
        }
        if paths.len() > 1000 {
            self.status = "Choose at most 1000 images per import.".into();
            return;
        }
        let Some(root) = self.history.root() else {
            return;
        };
        let (tx, rx) = mpsc::channel();
        self.history.query.clear();
        self.history.library.start_import();
        let stop = Arc::new(AtomicBool::new(false));
        self.history.import = Import {
            receiver: Some(rx),
            stop: stop.clone(),
            total: paths.len(),
            done: 0,
            added: 0,
            errors: vec![],
            last_success: None,
        };
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            capture_core::history::import::run(&root, &paths, &stop, |v| {
                let _ = tx.send(v);
                ctx.request_repaint();
            });
            ctx.request_repaint();
        });
    }
    pub(crate) fn poll_library_import(&mut self) {
        let mut finished = false;
        let mut changed = false;
        if let Some(rx) = &self.history.import.receiver {
            loop {
                match rx.try_recv() {
                    Ok(outcome) => {
                        self.history.import.done += 1;
                        match outcome.result {
                            Ok(entry) => {
                                changed = true;
                                self.history.import.added += 1;
                                self.history.import.last_success = Some(outcome.path);
                                self.history.library.selected.insert(entry.id);
                            }
                            Err(e) => self.history.import.errors.push(format!(
                                "{}: {e}",
                                outcome
                                    .path
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                            )),
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        finished = true;
                        break;
                    }
                }
            }
        }
        if changed || finished {
            self.history.refresh();
        }
        if finished {
            self.history.import.receiver = None;
            if let Some(path) = self.history.import.last_success.take() {
                self.remember_dialog_folder(capture_core::quick_preferences::Folder::Open, &path);
            }
            self.status = format!(
                "Imported {} of {} images. {} failed{}.",
                self.history.import.added,
                self.history.import.total,
                self.history.import.errors.len(),
                if self.history.import.done < self.history.import.total {
                    "; import cancelled"
                } else {
                    ""
                }
            );
        }
    }
    pub(super) fn library_import_progress(&mut self, ui: &mut egui::Ui) {
        let job = &mut self.history.import;
        if job.receiver.is_some() {
            ui.horizontal(|ui| {
                ui.add(
                    egui::ProgressBar::new(job.done as f32 / job.total.max(1) as f32)
                        .text(format!("Importing {} / {}", job.done, job.total)),
                );
                if ui.button("Cancel import").clicked() {
                    job.stop.store(true, Ordering::Relaxed);
                }
            });
        }
        if !job.errors.is_empty() {
            ui.collapsing(format!("{} import errors", job.errors.len()), |ui| {
                egui::ScrollArea::vertical()
                    .max_height(140.)
                    .show(ui, |ui| {
                        for error in &job.errors {
                            ui.label(error);
                        }
                    });
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn background_import_retains_current_canvas_and_selects_successes() {
        let root = tempfile::tempdir().unwrap();
        let src = tempfile::tempdir().unwrap();
        let file = src.path().join("Example.png");
        image::RgbaImage::from_pixel(30, 20, image::Rgba([10, 80, 160, 255]))
            .save(&file)
            .unwrap();
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        e.history = HistoryPanel::isolated(root.path());
        e.history.query = "old filter".into();
        let before = serde_json::to_vec(&e.session.document).unwrap();
        e.begin_library_import(vec![file, src.path().join("missing.png")], &ctx);
        let started = std::time::Instant::now();
        while e.history.import.receiver.is_some() {
            e.poll_library_import();
            let _output = crate::property_layout::tests::PaintedOutput(
                ctx.run_ui(egui::RawInput::default(), |ui| e.show_library(ui)),
            );
            assert!(started.elapsed().as_secs() < 10);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(e.history.entries().len(), 1);
        assert!(e.history.query.is_empty());
        assert_eq!(e.history.library.selected.len(), 1);
        assert_eq!(e.history.import.errors.len(), 1);
        assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
        assert!(!e.session.can_undo());
    }
}
