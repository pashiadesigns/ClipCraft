//! One background writer per editor, coalesced edits, and a durability barrier
//! before replacing or closing the current document. Draft text is recoverable
//! without adding Undo entries or committing the user's text transaction.
use super::*;
use capture_core::{Annotation, Document, RasterImage};
use std::{
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};

#[derive(Clone, PartialEq)]
struct State {
    video: Option<capture_core::video::editor::Project>,
    guide: Option<capture_core::guide_sections::Layout>,
    object_states: capture_core::object_state::ObjectStates,
    version: u32,
    size: [u32; 2],
    source_size: Option<[u32; 2]>,
    raster: Option<Arc<RasterImage>>,
    annotations: Vec<Annotation>,
}
impl State {
    // A new source image always resets Autosave. Comparing this small editable
    // state avoids hashing/serializing/cloning the original PNG every frame.
    fn capture(doc: &Document, draft: Option<&Annotation>) -> Self {
        let mut state = Self {
            video: doc.video.clone(),
            guide: doc.guide.clone(),
            object_states: doc.object_states.clone(),
            version: doc.version,
            size: [doc.width, doc.height],
            source_size: doc.source_size,
            raster: doc.raster.clone(),
            annotations: doc.annotations.clone(),
        };
        if let Some(draft) = draft
            && let Some(a) = state.annotations.iter_mut().find(|a| a.id == draft.id)
        {
            *a = draft.clone();
        }
        state
    }
    fn matches(&self, doc: &Document, draft: Option<&Annotation>) -> bool {
        (match (&self.video, &doc.video) {
            (Some(a), Some(b)) => a.same_edit(b),
            (None, None) => true,
            _ => false,
        }) && self.guide == doc.guide
            && self.object_states == doc.object_states
            && self.version == doc.version
            && self.size == [doc.width, doc.height]
            && self.source_size == doc.source_size
            && self.raster == doc.raster
            && self.annotations.len() == doc.annotations.len()
            && self
                .annotations
                .iter()
                .zip(&doc.annotations)
                .all(|(a, b)| a == draft.filter(|d| d.id == b.id).unwrap_or(b))
    }
}
struct Outcome {
    entry: Entry,
    warning: Option<String>,
}
struct Flight {
    receiver: mpsc::Receiver<Result<Outcome, String>>,
    state: State,
}

#[derive(Default)]
pub(crate) struct Autosave {
    removed: bool,
    observed: Option<State>,
    durable: Option<State>,
    expected: Option<Entry>,
    flight: Option<Flight>,
    changed_at: Option<Instant>,
    dirty_since: Option<Instant>,
    label: String,
    origin: Option<Origin>,
    error: Option<String>,
    pub(crate) action: Option<FileAction>,
}
impl Autosave {
    pub(crate) fn reset(
        &mut self,
        doc: &Document,
        entry: Option<Entry>,
        label: &str,
        origin: Origin,
    ) {
        // Document switches only happen after the writer finishes, or after an
        // explicit failure-dialog choice. Never detach a live writer.
        debug_assert!(self.flight.is_none());
        *self = Self {
            observed: Some(State::capture(doc, None)),
            durable: entry.as_ref().map(|_| State::capture(doc, None)),
            expected: entry,
            label: label.into(),
            origin: Some(origin),
            ..Default::default()
        };
    }
    fn is_durable(&self, doc: &Document, draft: Option<&Annotation>) -> bool {
        self.durable.as_ref().is_some_and(|s| s.matches(doc, draft))
    }
    fn observe(&mut self, doc: &Document, draft: Option<&Annotation>, now: Instant) {
        if !self
            .observed
            .as_ref()
            .is_some_and(|s| s.matches(doc, draft))
        {
            self.observed = Some(State::capture(doc, draft));
            self.changed_at = Some(now);
            self.error = None;
        }
        if !self.is_durable(doc, draft) {
            self.dirty_since.get_or_insert(now);
            self.changed_at.get_or_insert(now);
        } else {
            self.dirty_since = None;
            self.changed_at = None;
        }
    }
    fn due(&self, now: Instant, force: bool) -> bool {
        self.flight.is_none()
            && self.error.is_none()
            && self.dirty_since.is_some()
            && (force
                || self
                    .changed_at
                    .is_some_and(|t| now.duration_since(t) >= Duration::from_millis(750))
                || self
                    .dirty_since
                    .is_some_and(|t| now.duration_since(t) >= Duration::from_secs(2)))
    }
    fn start(
        &mut self,
        root: PathBuf,
        mut doc: Document,
        draft: Option<Annotation>,
        ctx: &egui::Context,
    ) {
        let state = State::capture(&doc, draft.as_ref());
        doc.annotations.clone_from(&state.annotations);
        let expected = self.expected.clone();
        let label = if self.label.is_empty() {
            "New canvas".into()
        } else {
            self.label.clone()
        };
        let origin = self.origin.unwrap_or(Origin::Canvas);
        let ctx = ctx.clone();
        let (sender, receiver) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("clipcraft-autosave".into())
            .spawn(move || {
                let result = (|| {
                    let mut store = History::open(&root)?;
                    let entry = match expected {
                        Some(entry) => store.update(&entry, &doc)?,
                        None => store.create(&doc, &label, origin)?,
                    };
                    Ok::<_, capture_core::Error>(Outcome {
                        entry,
                        warning: store.cleanup_warning().map(str::to_owned),
                    })
                })()
                .map_err(|e| e.to_string());
                let _ = sender.send(result);
                ctx.request_repaint();
            });
        match thread {
            Ok(_) => self.flight = Some(Flight { receiver, state }),
            Err(e) => self.error = Some(format!("Cannot start Library save: {e}")),
        }
    }
    fn receive(&mut self) -> Option<Result<Outcome, String>> {
        let flight = self.flight.as_ref()?;
        let result = match flight.receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("Library save stopped unexpectedly. Your open image is intact.".into())
            }
        };
        let flight = self.flight.take().unwrap();
        match &result {
            Ok(outcome) => {
                self.removed = false;
                self.durable = Some(flight.state);
                self.expected = Some(outcome.entry.clone());
                self.error = None;
                // A newer edit can now start its own short debounce period.
                self.dirty_since = None;
            }
            Err(error) => self.error = Some(error.clone()),
        }
        Some(result)
    }
}

impl Editor {
    pub(super) fn library_entry_busy(&self) -> bool {
        self.autosave.flight.is_some() || !self.library_saved()
    }
    pub(super) fn update_history_metadata(&mut self, entry: Entry) {
        self.autosave.label = entry.label.clone();
        self.autosave.expected = Some(entry);
    }
    pub(super) fn detach_deleted_history(&mut self) {
        self.project_sessions
            .forget_active(self.history.active.as_deref());
        self.video.leave_project();
        self.cancel_gesture();
        self.session = capture_core::Session::new(
            capture_core::Document::blank(1920, 1080).expect("valid blank canvas"),
        )
        .expect("valid session");
        self.path = None;
        self.select_only(None);
        self.reset_document_tools();
        self.view = Default::default();
        self.needs_render = true;
        self.history.active = None;
        self.autosave.expected = None;
        self.autosave.durable = Some(State::capture(&self.session.document, None));
        self.autosave.observed = self.autosave.durable.clone();
        self.autosave.dirty_since = None;
        self.autosave.changed_at = None;
        self.autosave.error = None;
        self.autosave.removed = true;
    }
    pub(crate) fn library_saved(&self) -> bool {
        !self.autosave.removed
            && self.autosave.is_durable(
                &self.session.document,
                self.text_draft.as_ref().map(|d| &d.preview),
            )
    }
    pub(crate) fn autosave_tick(&mut self, ctx: &egui::Context, force: bool) {
        if let Some(result) = self.autosave.receive() {
            match result {
                Ok(outcome) => {
                    self.history.active = Some(outcome.entry.id);
                    self.history.refresh();
                    if let Some(warning) = outcome.warning {
                        self.status = warning;
                    } else if self.status.starts_with("Library autosave failed:") {
                        self.status =
                            "Library save recovered. Your editable changes are saved.".into();
                    }
                }
                Err(error) => self.status = format!("Library autosave failed: {error}"),
            }
        }
        // The untouched startup blank is disposable. Imported images, new
        // canvases and opened projects are explicitly assigned an origin.
        if self.autosave.origin.is_none() && !self.is_modified() {
            return;
        }
        let now = Instant::now();
        let draft = self.text_draft.as_ref().map(|d| &d.preview);
        self.autosave.observe(&self.session.document, draft, now);
        if self.autosave.due(now, force) {
            if let Some(store) = &self.history.store {
                self.autosave.start(
                    store.root().to_owned(),
                    self.session.document.clone(),
                    draft.cloned(),
                    ctx,
                );
            } else {
                self.autosave.error =
                    Some(format!("Library is unavailable: {}", self.history.error));
            }
        }
        if self.autosave.dirty_since.is_some() && self.autosave.error.is_none() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
    pub(crate) fn protect_with_autosave(&mut self, action: FileAction, ctx: &egui::Context) {
        self.autosave_tick(ctx, true);
        // Even a manually saved file must finish an in-flight Library write
        // before switching sources (or the completion could attach to it).
        if self.autosave.flight.is_some() {
            self.autosave.action = Some(action);
        } else if self.library_saved()
            || !self.session.is_modified()
            || (self.autosave.removed && self.autosave.is_durable(&self.session.document, None))
        {
            self.perform_action(action, ctx);
        } else if self.autosave.error.is_some() {
            self.pending = Some(action);
        } else {
            self.autosave.action = Some(action);
        }
    }
    pub(crate) fn finish_autosave_action(&mut self, ctx: &egui::Context) {
        if let Some(action) = self.autosave.action.take() {
            self.protect_with_autosave(action, ctx);
        }
    }
    pub(crate) fn autosave_summary(&self) -> (&'static str, egui::Color32) {
        if self.autosave.removed
            && self.autosave.is_durable(
                &self.session.document,
                self.text_draft.as_ref().map(|d| &d.preview),
            )
        {
            return ("Removed from Library", egui::Color32::from_gray(160));
        }
        if self.autosave.error.is_some() {
            ("Not saved to Library", egui::Color32::LIGHT_RED)
        } else if self.autosave.flight.is_some()
            || (self.autosave.origin.is_some() && !self.library_saved())
        {
            ("Saving to Library…", egui::Color32::from_rgb(230, 183, 74))
        } else if self.library_saved() {
            ("Saved to Library", egui::Color32::from_rgb(110, 195, 143))
        } else {
            ("Library save status", egui::Color32::from_gray(160))
        }
    }
    pub(crate) fn autosave_status(&mut self, ui: &mut egui::Ui) {
        if self.autosave_summary().0 == "Removed from Library" {
            ui.small("Removed from Library");
            return;
        }
        if let Some(error) = &self.autosave.error {
            ui.colored_label(egui::Color32::LIGHT_RED, "Not saved to Library")
                .on_hover_text(error);
            if ui.small_button("Retry save").clicked() {
                self.history.refresh();
                self.autosave.error = None;
                self.autosave_tick(ui.ctx(), true);
            }
        } else if self.autosave.flight.is_some()
            || (self.autosave.origin.is_some() && !self.library_saved())
        {
            ui.small("Saving to Library…");
        } else if self.library_saved() {
            ui.small("Saved to Library").on_hover_text("Editable objects and the original image are saved locally. Save As exports an additional project copy.");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Command;

    fn editor(ctx: &egui::Context, root: &std::path::Path) -> Editor {
        let mut e = crate::canvas::tests::editor(ctx);
        e.history.store = Some(History::open(root).unwrap());
        let fixture: Document = serde_json::from_str(include_str!(
            "../../../../crates/core/tests/fixtures/translation.capture.json"
        ))
        .unwrap();
        let mut doc = Document::canvas(640, 400, [230, 240, 250, 255]).unwrap();
        doc.annotations = fixture.annotations;
        e.session = Session::unsaved(doc).unwrap();
        e.record_history("Test capture", Origin::Capture);
        e
    }
    fn settle(e: &mut Editor, ctx: &egui::Context) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            e.autosave_tick(ctx, true);
            if e.autosave.flight.is_none() {
                break;
            }
            assert!(Instant::now() < deadline, "background save timed out");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    fn bytes(doc: &Document) -> Vec<u8> {
        serde_json::to_vec(doc).unwrap()
    }

    #[test]
    fn layer_visibility_and_locks_autosave_even_without_schema_or_geometry_changes() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut e = editor(&ctx, dir.path());
        settle(&mut e, &ctx);
        let history_id = e.history.active.clone().unwrap();
        let id = e.session.document.annotations[0].id;
        let original_annotations = e.session.document.annotations.clone();
        e.execute(Command::SetObjectsVisible {
            ids: vec![id],
            visible: false,
        });
        settle(&mut e, &ctx);
        let hidden = History::open(dir.path())
            .unwrap()
            .load(&history_id)
            .unwrap();
        assert!(hidden.object_state(id).hidden);
        e.execute(Command::SetObjectsLocked {
            ids: vec![id],
            locked: true,
        });
        assert_eq!(e.session.document.version, hidden.version);
        assert_eq!(e.session.document.annotations, original_annotations);
        settle(&mut e, &ctx);
        let locked = History::open(dir.path())
            .unwrap()
            .load(&history_id)
            .unwrap();
        assert!(locked.object_state(id).hidden && locked.object_state(id).locked);
        assert_eq!(bytes(&locked), bytes(&e.session.document));
        e.execute(Command::Undo);
        settle(&mut e, &ctx);
        assert_eq!(
            bytes(
                &History::open(dir.path())
                    .unwrap()
                    .load(&history_id)
                    .unwrap()
            ),
            bytes(&hidden)
        );
        assert_eq!(e.history.entries().len(), 1);
    }

    #[test]
    fn debounce_coalesces_changes_but_bounds_continuous_typing_and_skips_idle_writes() {
        let doc = Document::blank(64, 40).unwrap();
        let now = Instant::now();
        let mut a = Autosave::default();
        a.observe(&doc, None, now);
        assert!(!a.due(now + Duration::from_millis(749), false));
        assert!(a.due(now + Duration::from_millis(750), false));
        assert!(a.due(now, true));
        let mut edited = doc.clone();
        for i in 1..=3 {
            edited.width += 1;
            a.observe(&edited, None, now + Duration::from_millis(600 * i));
            assert!(!a.due(now + Duration::from_millis(600 * i), false));
        }
        assert!(a.due(now + Duration::from_secs(2), false));
        a.durable = Some(State::capture(&edited, None));
        a.observe(&edited, None, now + Duration::from_secs(3));
        assert!(!a.due(now + Duration::from_secs(60), true));
    }

    #[test]
    fn drafts_recover_without_committing_undo_and_cancellation_restores_durable_original() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut e = editor(&ctx, dir.path());
        let original = bytes(&e.session.document);
        settle(&mut e, &ctx);
        let id = e.history.active.clone().unwrap();
        e.select_only(Some(1));
        e.begin_text_edit();
        e.text_draft
            .as_mut()
            .unwrap()
            .preview
            .text
            .as_mut()
            .unwrap()
            .content = "Uncommitted draft survives".into();
        settle(&mut e, &ctx);
        assert!(!e.session.can_undo());
        assert_eq!(bytes(&e.session.document), original);
        let recovered = History::open(dir.path()).unwrap().load(&id).unwrap();
        assert_eq!(
            recovered.annotations[0].text.as_ref().unwrap().content,
            "Uncommitted draft survives"
        );
        assert_eq!(recovered.source_png, e.session.document.source_png);
        assert_eq!(
            &recovered.annotations[1..],
            &e.session.document.annotations[1..]
        );
        e.text_draft = None; // Escape cancels the typing transaction.
        settle(&mut e, &ctx);
        assert_eq!(
            bytes(&History::open(dir.path()).unwrap().load(&id).unwrap()),
            original
        );
        e.begin_text_edit();
        e.text_draft
            .as_mut()
            .unwrap()
            .preview
            .text
            .as_mut()
            .unwrap()
            .content = "Committed text".into();
        settle(&mut e, &ctx);
        assert!(e.commit_text());
        assert!(e.library_saved());
        e.execute(Command::Undo);
        settle(&mut e, &ctx);
        assert_eq!(
            bytes(&History::open(dir.path()).unwrap().load(&id).unwrap()),
            original
        );
        assert_eq!(e.history.entries().len(), 1);
    }

    #[test]
    fn edits_during_a_write_are_not_marked_saved_and_close_flushes_latest_once() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut e = editor(&ctx, dir.path());
        e.autosave_tick(&ctx, true);
        assert!(e.autosave.flight.is_some());
        let mut changed = e.session.document.annotations[0].clone();
        changed.color = [4, 80, 210, 255];
        e.execute(Command::Update {
            annotation: changed,
        });
        // Receive only the original write: it must not acknowledge this edit.
        let deadline = Instant::now() + Duration::from_secs(10);
        while e.autosave.receive().is_none() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!e.library_saved());
        e.request_action(FileAction::Close, &ctx);
        assert!(!e.allow_close);
        assert!(e.autosave.action.is_some());
        assert!(e.pending.is_none());
        settle(&mut e, &ctx);
        e.finish_autosave_action(&ctx);
        assert!(e.allow_close);
        assert!(e.pending.is_none());
        let h = History::open(dir.path()).unwrap();
        assert_eq!(h.entries().len(), 1);
        assert_eq!(
            bytes(&h.load(&h.entries()[0].id).unwrap()),
            bytes(&e.session.document)
        );
        assert!(e.session.can_undo());
    }

    #[test]
    fn disk_failure_keeps_document_open_and_explicit_retry_safely_recovers() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut e = editor(&ctx, dir.path());
        settle(&mut e, &ctx);
        let id = e.history.active.clone().unwrap();
        let original = bytes(&e.history.store.as_ref().unwrap().load(&id).unwrap());
        e.execute(Command::FlattenAll);
        let pending = bytes(&e.session.document);
        // Lock the store from an independent handle, simulating another writer.
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(dir.path().join("history.lock"))
            .unwrap();
        lock.lock().unwrap();
        e.request_action(FileAction::Close, &ctx);
        settle(&mut e, &ctx);
        e.finish_autosave_action(&ctx);
        assert!(!e.allow_close);
        assert!(e.pending.is_some());
        assert!(e.autosave.error.is_some());
        assert_eq!(bytes(&e.session.document), pending);
        assert_eq!(
            bytes(&History::open(dir.path()).unwrap().load(&id).unwrap()),
            original
        );
        e.pending = None; // Cancel leaves the edited document available.
        drop(lock);
        e.autosave.error = None; // Retry save.
        settle(&mut e, &ctx);
        assert!(e.library_saved());
        assert!(!e.status.starts_with("Library autosave failed:"));
        assert_eq!(
            bytes(&History::open(dir.path()).unwrap().load(&id).unwrap()),
            pending
        );
        e.request_action(FileAction::Close, &ctx);
        assert!(e.allow_close);
    }

    #[test]
    fn switching_documents_preserves_each_identity_and_undo_updates_the_active_item() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut e = editor(&ctx, dir.path());
        settle(&mut e, &ctx);
        let first = e.history.active.clone().unwrap();
        let original = bytes(&e.session.document);
        e.execute(Command::FlattenAll);
        let edited = bytes(&e.session.document);
        e.request_action(
            FileAction::FromClipboard(Session::unsaved(Document::blank(80, 60).unwrap()).unwrap()),
            &ctx,
        );
        assert!(e.autosave.action.is_some());
        settle(&mut e, &ctx);
        e.finish_autosave_action(&ctx);
        settle(&mut e, &ctx);
        let second = e.history.active.clone().unwrap();
        assert_ne!(first, second);
        assert_eq!(e.history.entries().len(), 2);
        e.open_history(&first, &ctx);
        assert_eq!(bytes(&e.session.document), edited);
        assert!(e.library_saved());
        assert!(e.pending.is_none());
        assert_eq!(e.history.active.as_deref(), Some(first.as_str()));
        let mut restored: Document = serde_json::from_slice(&original).unwrap();
        restored.annotations[0].width += 1;
        e.session = Session::unsaved(restored).unwrap();
        // Same source/item identity, but a new editable revision.
        settle(&mut e, &ctx);
        let h = History::open(dir.path()).unwrap();
        assert_eq!(h.entries().len(), 2);
        assert_eq!(bytes(&h.load(&first).unwrap()), bytes(&e.session.document));
        assert_eq!(h.load(&second).unwrap().width, 80);
    }
}
