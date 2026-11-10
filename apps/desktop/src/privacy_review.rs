//! Review-assisted local OCR. The modal owns its snapshot and stale results
//! cannot modify a different document. Applying checked findings is one undo.
use crate::{Editor, egui};
use capture_core::privacy::{self, Finding, Options};
use std::{
    hash::{Hash, Hasher},
    sync::mpsc::{self, Receiver},
};
static SCANNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub(crate) const VIDEO_LIMITATION: &str = "Automatic tracking needs review. Check the entire video, including uncertain intervals and newly appearing content, before sharing. Editable projects keep the original media.";
struct ScanGuard;
impl Drop for ScanGuard {
    fn drop(&mut self) {
        SCANNING.store(false, std::sync::atomic::Ordering::Release);
    }
}

struct Scan {
    findings: Vec<Finding>,
    pixels: image::RgbaImage,
    fingerprint: u64,
    scaled: bool,
}
#[derive(Default)]
pub(super) struct State {
    pub open: bool,
    options: Options,
    phrases: String,
    focus: bool,
    pending: Option<Receiver<Result<Scan, String>>>,
    result: Option<Scan>,
    checked: Vec<bool>,
    texture: Option<egui::TextureHandle>,
    error: String,
    cover: crate::sensitive_ui::Cover,
    /// What the review picture currently shows.
    shown: Option<(Vec<bool>, crate::sensitive_ui::Cover)>,
}
fn fingerprint(document: &capture_core::Document) -> Result<u64, String> {
    let bytes = serde_json::to_vec(document).map_err(|e| e.to_string())?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    Ok(h.finish())
}
impl Editor {
    pub(super) fn open_privacy_review(&mut self) {
        if self.is_video() {
            self.open_video_privacy_review();
            return;
        }
        if !self.commit_text() {
            return;
        }
        self.commit_property_preview();
        self.privacy_review.open = true;
        self.privacy_review.focus = true;
        self.privacy_review.result = None;
        self.privacy_review.checked.clear();
        self.privacy_review.texture = None;
        self.privacy_review.error.clear();
    }
    fn scan_sensitive(&mut self, ctx: &egui::Context) {
        if self.is_video() {
            self.privacy_review.error = "Find Sensitive Info scans still images only. Automatic video tracking is not available.".into();
            return;
        }
        if SCANNING.swap(true, std::sync::atomic::Ordering::AcqRel) {
            self.privacy_review.error =
                "The previous scan is finishing. Wait a moment, then scan again.".into();
            return;
        }
        let document = self.session.document.clone();
        let mut options = self.privacy_review.options.clone();
        options.phrases = self
            .privacy_review
            .phrases
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        self.privacy_review.pending = Some(rx);
        self.privacy_review.result = None;
        self.privacy_review.checked.clear();
        self.privacy_review.texture = None;
        self.privacy_review.error.clear();
        std::thread::spawn(move || {
            let _guard = ScanGuard;
            let result = (|| {
                let fingerprint = fingerprint(&document)?;
                let pixels = document.render().map_err(|e| e.to_string())?;
                let findings = scan_tiles(&pixels, &options)?;
                Ok(Scan {
                    findings,
                    pixels,
                    fingerprint,
                    scaled: false,
                })
            })();
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }
    pub(super) fn show_privacy_review(&mut self, ctx: &egui::Context) {
        if !self.privacy_review.open {
            return;
        }
        if let Some(result) =
            self.privacy_review
                .pending
                .as_ref()
                .and_then(|r| match r.try_recv() {
                    Ok(v) => Some(v),
                    Err(mpsc::TryRecvError::Disconnected) => {
                        Some(Err("Scan stopped. Try again.".into()))
                    }
                    _ => None,
                })
        {
            self.privacy_review.pending = None;
            match result {
                Ok(scan) => {
                    self.privacy_review.shown = None;
                    self.privacy_review.checked = vec![true; scan.findings.len()];
                    self.privacy_review.result = Some(scan);
                }
                Err(e) => self.privacy_review.error = e,
            }
        }
        let video = self.is_video();
        let state = &mut self.privacy_review;
        let busy = state.pending.is_some();
        let mut scan = false;
        let mut apply = false;
        let mut close = false;
        if let Some(result) = &state.result
            && state
                .shown
                .as_ref()
                .is_none_or(|(c, k)| *c != state.checked || *k != state.cover)
        {
            let boxes: Vec<[f32; 4]> = result
                .findings
                .iter()
                .zip(&state.checked)
                .filter(|(_, c)| **c)
                .flat_map(|(f, _)| f.bounds.iter().copied())
                .collect();
            let shown = crate::sensitive_ui::preview(&result.pixels, &boxes, state.cover);
            state.texture = Some(ctx.load_texture(
                "sensitive-review",
                egui::ColorImage::from_rgba_unmultiplied(
                    [shown.width() as usize, shown.height() as usize],
                    shown.as_raw(),
                ),
                egui::TextureOptions::LINEAR,
            ));
            state.shown = Some((state.checked.clone(), state.cover));
        }
        let modal =
            crate::controls::Modal::new(egui::Id::unique("sensitive-review")).show(ctx, |ui| {
                ui.set_width(820_f32.min((ctx.content_rect().width() - 48.).max(340.)));
                crate::controls::dialog_title(ui, "Find Sensitive Info");
                if video {
                    ui.label(VIDEO_LIMITATION);
                    close = ui
                        .add(crate::controls::dialog_button("Close", true))
                        .clicked();
                    return;
                }
                ui.add_enabled_ui(!busy, |ui| {
                    let field =
                        crate::sensitive_ui::look_for(ui, &mut state.options, &mut state.phrases);
                    crate::controls::focus_once(&field, &mut state.focus);
                });
                ui.add_space(4.);
                let row = egui::vec2(ui.available_width(), 28.);
                ui.allocate_ui_with_layout(
                    row,
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        let label = if state.result.is_some() {
                            "Scan again"
                        } else {
                            "Scan"
                        };
                        scan = ui
                            .add_enabled(
                                !busy,
                                crate::controls::dialog_button(label, state.result.is_none()),
                            )
                            .clicked();
                        if busy {
                            ui.spinner();
                            ui.label("Scanning visible text…");
                        }
                    },
                );
                if let Some(result) = &state.result {
                    crate::panel_ui::divider(ui);
                    let height = (ctx.content_rect().height() - 590.).clamp(110., 280.);
                    ui.columns(2, |columns| {
                        let ui = &mut columns[0];
                        if let Some(texture) = &state.texture {
                            let size = texture.size_vec2();
                            let scale = (ui.available_width() / size.x).min(height / size.y);
                            ui.image((texture.id(), size * scale));
                        }
                        let ui = &mut columns[1];
                        ui.horizontal(|ui| {
                            let n = result.findings.len();
                            ui.label(format!(
                                "{n} {}",
                                if n == 1 { "finding" } else { "findings" }
                            ));
                            if ui.small_button("Select all").clicked() {
                                state.checked.fill(true);
                            }
                            if ui.small_button("Clear").clicked() {
                                state.checked.fill(false);
                            }
                        });
                        egui::ScrollArea::vertical()
                            .id_salt("sensitive-findings")
                            .max_height(height - 30.)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                for (i, f) in result.findings.iter().enumerate() {
                                    let sample = f.text.chars().take(60).collect::<String>();
                                    ui.horizontal(|ui| {
                                        crate::controls::toggle(ui, &mut state.checked[i], "");
                                        ui.add(
                                            egui::Label::new(format!(
                                                "{} · {sample}",
                                                f.category.label()
                                            ))
                                            .truncate(),
                                        );
                                    });
                                }
                            });
                    });
                    if result.scaled {
                        ui.weak("This large image was reduced for OCR. Check small text manually.");
                    }
                    crate::sensitive_ui::cover(ui, &mut state.cover);
                }
                if !state.error.is_empty() {
                    ui.colored_label(egui::Color32::LIGHT_RED, &state.error);
                }
                let count = state.result.as_ref().map_or(0, |r| {
                    r.findings
                        .iter()
                        .zip(&state.checked)
                        .filter(|(_, c)| **c)
                        .count()
                });
                (apply, close) = crate::controls::dialog_actions(
                    ui,
                    &state.cover.action(count),
                    "Close",
                    !busy && count > 0,
                    false,
                );
            });
        if scan {
            self.scan_sensitive(ctx);
        }
        if apply {
            close = self.apply_sensitive_findings();
        }
        if close || crate::text_edit::cancel_on_escape(&modal) {
            // Options carry over; the cover goes back to Redact.
            let options = self.privacy_review.options.clone();
            let phrases = std::mem::take(&mut self.privacy_review.phrases);
            self.privacy_review = State {
                options,
                phrases,
                ..Default::default()
            };
        }
    }
    fn apply_sensitive_findings(&mut self) -> bool {
        if self.is_video() {
            self.privacy_review.error =
                "Still-image findings cannot be applied as tracked video redactions.".into();
            return false;
        }
        let Some(result) = self.privacy_review.result.as_ref() else {
            return false;
        };
        if fingerprint(&self.session.document).ok() != Some(result.fingerprint) {
            self.privacy_review.error =
                "The image changed. Scan again before applying redactions.".into();
            false
        } else {
            let findings = result
                .findings
                .iter()
                .zip(&self.privacy_review.checked)
                .filter(|(_, c)| **c)
                .map(|(f, _)| f.clone())
                .collect::<Vec<_>>();
            let before = self.session.document.annotations.len();
            if findings.is_empty() {
                return false;
            }
            let cover = self.privacy_review.cover;
            self.execute(privacy::cover(
                &self.session.document,
                findings,
                cover.kind,
                cover.width(),
            ));
            let applied = self.session.document.annotations.len() > before;
            if !applied {
                self.privacy_review.error = self.status.clone();
            }
            applied
        }
    }
}

/// The dialog with sample findings, for the dialog audit.
#[cfg(test)]
pub(crate) fn seed_results(e: &mut Editor) {
    let pixels = e.session.document.render().unwrap();
    let words = vec![
        privacy::Word {
            text: "alex@example.com".into(),
            line: 0,
            bounds: [430., 156., 180., 24.],
        },
        privacy::Word {
            text: "555-010-0143".into(),
            line: 1,
            bounds: [430., 196., 150., 24.],
        },
    ];
    let size = [e.session.document.width, e.session.document.height];
    let findings = privacy::detect(&words, &Options::default(), size).unwrap();
    e.privacy_review.checked = vec![true; findings.len()];
    e.privacy_review.cover.kind = crate::Kind::Blur;
    e.privacy_review.result = Some(Scan {
        findings,
        pixels,
        fingerprint: 0,
        scaled: false,
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn video_projects_cannot_scan_or_apply_still_image_findings() {
        let ctx = egui::Context::default();
        let mut e = crate::rich_text::tests::editor(&ctx);
        e.session.document.video = Some(capture_core::video::editor::Project::new(
            std::env::temp_dir()
                .join("privacy-fixture.mp4")
                .to_string_lossy()
                .into(),
            [640, 400],
            1000,
            30,
        ));
        let before = serde_json::to_value(&e.session.document).unwrap();
        e.open_privacy_review();
        e.scan_sensitive(&ctx);
        assert!(e.privacy_review.pending.is_none());
        assert!(e.privacy_review.error.contains("still images only"));
        assert!(!e.apply_sensitive_findings());
        assert_eq!(serde_json::to_value(&e.session.document).unwrap(), before);
    }
    #[cfg(windows)]
    #[test]
    #[ignore = "Opt-in Windows OCR with synthetic image; no screen or clipboard access"]
    fn local_ocr_finds_email_geometry() {
        let ctx = egui::Context::default();
        let e = crate::rich_text::tests::editor(&ctx);
        let pixels = e.session.document.render().unwrap();
        let result =
            crate::ocr::recognize_bounded(&pixels, std::time::Duration::from_secs(30)).unwrap();
        let findings = privacy::detect(
            &result.words,
            &Options::default(),
            [pixels.width(), pixels.height()],
        )
        .unwrap();
        assert!(
            findings
                .iter()
                .any(|f| f.category == privacy::Category::Email
                    && f.text.contains("alex@example.com")),
            "{} {:?}",
            result.text,
            findings
        );
    }
    #[test]
    fn review_refuses_stale_snapshot_and_applies_checked_findings_as_one_undo() {
        let ctx = egui::Context::default();
        let mut e = crate::rich_text::tests::editor(&ctx);
        e.open_privacy_review();
        let original = e.session.document.clone();
        let words = vec![privacy::Word {
            text: "alex@example.com".into(),
            line: 0,
            bounds: [120., 58., 220., 35.],
        }];
        let findings = privacy::detect(&words, &Options::default(), [640, 400]).unwrap();
        let pixels = original.render().unwrap();
        let texture = ctx.load_texture(
            "review-test",
            egui::ColorImage::from_rgba_unmultiplied([640, 400], pixels.as_raw()),
            egui::TextureOptions::LINEAR,
        );
        e.privacy_review.result = Some(Scan {
            findings,
            pixels,
            fingerprint: fingerprint(&original).unwrap(),
            scaled: false,
        });
        e.privacy_review.checked = vec![true];
        e.privacy_review.texture = Some(texture);
        crate::rich_text::tests::screenshot(&ctx, &mut e, "Privacy");
        e.session.document.annotations[0].start[0] += 1.;
        assert!(!e.apply_sensitive_findings());
        assert!(e.privacy_review.error.contains("Scan again"));
        e.session.document = original.clone();
        assert!(e.apply_sensitive_findings());
        assert_eq!(e.session.document.annotations.len(), 2);
        e.execute(crate::Command::Undo);
        assert_eq!(
            serde_json::to_value(&e.session.document).unwrap(),
            serde_json::to_value(&original).unwrap()
        );
    }
}

/// Overlapping full-resolution tiles preserve small text on long scroll captures.
/// A failed tile fails the scan; never present an incomplete scan as complete.
fn scan_tiles(pixels: &image::RgbaImage, options: &Options) -> Result<Vec<Finding>, String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
    scan_with(pixels, options, |tile| {
        if std::time::Instant::now() >= deadline {
            return Err(
                "Scan took too long. Split the image into smaller pages and scan again.".into(),
            );
        }
        #[cfg(windows)]
        {
            crate::ocr::recognize_bounded(
                tile,
                deadline.saturating_duration_since(std::time::Instant::now()),
            )
        }
        #[cfg(not(windows))]
        {
            crate::ocr::recognize(tile)
        }
    })
}
fn scan_with(
    pixels: &image::RgbaImage,
    options: &Options,
    mut recognize: impl FnMut(&image::RgbaImage) -> Result<crate::ocr::Recognized, String>,
) -> Result<Vec<Finding>, String> {
    let mut findings: Vec<Finding> = Vec::new();
    let mut count = 0;
    for y in (0..pixels.height()).step_by(1792) {
        for x in (0..pixels.width()).step_by(1792) {
            count += 1;
            if count > 64 {
                return Err("Split this image into smaller pages to scan all text safely.".into());
            }
            let tile = image::imageops::crop_imm(
                pixels,
                x,
                y,
                2048.min(pixels.width() - x),
                2048.min(pixels.height() - y),
            )
            .to_image();
            let recognized = recognize(&tile)?;
            if recognized.scaled {
                return Err(
                    "This device's OCR image limit is too small. Scan a smaller image.".into(),
                );
            }
            if recognized.words.is_empty() && !recognized.text.trim().is_empty() {
                return Err("Rotate the text upright, then scan again. This scan could not locate its text safely.".into());
            }
            let local = privacy::detect(&recognized.words, options, [tile.width(), tile.height()])
                .map_err(|e| e.to_string())?;
            for mut f in local {
                for b in &mut f.bounds {
                    b[0] += x as f32;
                    b[1] += y as f32;
                }
                if !findings.iter().any(|old| {
                    old.category == f.category
                        && old.text == f.text
                        && old.bounds.iter().any(|a| {
                            f.bounds
                                .iter()
                                .any(|b| (a[0] - b[0]).abs() < 6. && (a[1] - b[1]).abs() < 6.)
                        })
                }) {
                    findings.push(f);
                }
                if findings.len() > 2048 {
                    return Err("More than 2048 findings: scan a smaller image.".into());
                }
            }
            if x + 2048 >= pixels.width() {
                break;
            }
        }
        if y + 2048 >= pixels.height() {
            break;
        }
    }
    Ok(findings)
}

#[cfg(test)]
mod tile_tests {
    use super::*;
    #[test]
    fn full_resolution_tiles_map_to_canvas_and_fail_closed() {
        let pixels = image::RgbaImage::new(100, 3000);
        let mut calls = 0;
        let found = scan_with(&pixels, &Options::default(), |tile| {
            assert!(tile.height() <= 2048);
            calls += 1;
            Ok(crate::ocr::synthetic_recognized(vec![privacy::Word {
                text: "alex@example.com".into(),
                line: 0,
                bounds: [10., 20., 60., 20.],
            }]))
        })
        .unwrap();
        assert_eq!(calls, 2);
        assert_eq!(found.len(), 2);
        assert_eq!(found[1].bounds[0][1], 1810.);
        let mut calls = 0;
        assert!(
            scan_with(&pixels, &Options::default(), |_| {
                calls += 1;
                if calls == 2 {
                    Err("Synthetic OCR failure".into())
                } else {
                    Ok(crate::ocr::synthetic_recognized(vec![]))
                }
            })
            .is_err()
        );
    }
}
