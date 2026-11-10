//! Jump to Object (approved mockup `artifacts/bg-proposal`): scrolls the timeline
//! to the clips of the objects selected on the canvas and flashes them yellow.
//! View-only: the playhead, the selection and the project never change.
use super::*;
use capture_core::privacy::tracking::{Stop, next_stop_after, review_stops};
use capture_core::video::{editing::Item, links::Layer};

/// How long a jumped-to clip glows; full strength for the first [`HOLD`] seconds.
pub(super) const FLASH: f32 = 1.4;
const HOLD: f32 = 0.35;

#[derive(Clone, Debug, Default)]
pub(crate) struct State {
    /// Clips to bring into view on the timeline's next frame.
    pending: Vec<u64>,
    /// Clips currently flashing and when the flash started.
    flash: Option<(Vec<u64>, Instant)>,
    /// The flagged moment being reviewed, marked on its timeline clip.
    review: Option<Stop>,
    /// A moment to open on the next frame, once this frame's edits are saved.
    go_to: Option<Stop>,
    /// The last redaction was just reviewed.
    finished: bool,
    reopen: bool,
}

/// What Tracked coverage shows for stepping through flagged moments, and what
/// the person chose.
#[derive(Default)]
pub(super) struct ReviewNav {
    /// Position of the current moment, counted from 1, and how many there are.
    pub position: Option<(usize, usize)>,
    pub stop: Option<Stop>,
    pub previous: Option<Stop>,
    pub next: Option<Stop>,
    pub finished: bool,
    pub chosen: Option<Stop>,
    pub reopen: bool,
}

impl Editor {
    /// The canvas objects Jump to Object applies to: the selected object plus any
    /// objects selected on the timeline, limited to those with a timeline clip.
    pub(crate) fn jump_targets(&self) -> Vec<u64> {
        let Some(v) = self.session.document.video.as_ref() else {
            return vec![];
        };
        let mut ids: Vec<u64> = self.selected.into_iter().collect();
        for item in &self.video.selection {
            if let Item::Annotation(id) = item
                && !ids.contains(id)
            {
                ids.push(*id);
            }
        }
        ids.retain(|id| v.objects.contains_key(id));
        ids
    }

    /// Opens one flagged moment: selects the redaction, moves the playhead to
    /// the start of the moment, and scrolls to and flashes its timeline clip.
    pub(super) fn show_review_stop(&mut self, stop: Stop) {
        self.select_only(Some(stop.id));
        self.video_seek(stop.start);
        if !matches!(self.video.tab, PropertyTab::Style | PropertyTab::Motion) {
            self.video.tab = PropertyTab::Style;
        }
        self.video.jump.review = Some(stop);
        self.video.jump.finished = false;
        self.jump_to_object();
    }
    /// Runs what Tracked coverage asked for on the previous frame.
    pub(super) fn run_review_request(&mut self) {
        if let Some(stop) = self.video.jump.go_to.take() {
            self.show_review_stop(stop);
        }
        if std::mem::take(&mut self.video.jump.reopen) {
            self.export_video();
        }
    }
    pub(super) fn review_nav(&self, id: u64, cursor: u64) -> ReviewNav {
        let stops = review_stops(&self.session.document);
        let current = self
            .video
            .jump
            .review
            .filter(|s| s.id == id && stops.contains(s))
            .or_else(|| {
                stops
                    .iter()
                    .find(|s| s.id == id && (s.start..s.end.max(s.start + 1)).contains(&cursor))
                    .copied()
            })
            .or_else(|| stops.iter().find(|s| s.id == id).copied());
        let index = current.and_then(|c| stops.iter().position(|s| *s == c));
        ReviewNav {
            position: index.map(|i| (i + 1, stops.len())),
            stop: current,
            previous: index.and_then(|i| i.checked_sub(1)).map(|i| stops[i]),
            next: index.and_then(|i| stops.get(i + 1)).copied(),
            finished: self.video.jump.finished && stops.is_empty(),
            chosen: None,
            reopen: false,
        }
    }
    /// Records a choice in Tracked coverage. Ticking the reviewed box moves on
    /// to the next redaction, or says that was the last one.
    pub(super) fn finish_review_nav(
        &mut self,
        ctx: &egui::Context,
        id: u64,
        nav: ReviewNav,
        newly_reviewed: bool,
    ) {
        let mut go_to = nav.chosen;
        self.video.jump.reopen |= nav.reopen;
        if newly_reviewed {
            let stops = review_stops(&self.session.document);
            let after = nav.stop.map_or(0, |s| s.start);
            go_to = next_stop_after(&stops, id, after);
            self.video.jump.finished = go_to.is_none();
            if go_to.is_none() {
                self.video.jump.review = None;
            }
        }
        self.video.jump.go_to = go_to;
        if go_to.is_some() || nav.reopen || newly_reviewed {
            ctx.request_repaint();
        }
    }
    /// Marks the flagged moment being reviewed on its timeline clip.
    pub(super) fn paint_review_mark(
        &self,
        ui: &egui::Ui,
        v: &Project,
        id: u64,
        rect: egui::Rect,
        scale: f32,
    ) {
        let Some(stop) = self.video.jump.review.filter(|s| s.id == id) else {
            return;
        };
        let Some(timing) = v.objects.get(&id) else {
            return;
        };
        if timing.motion_path.as_ref().is_none_or(|p| p.reviewed) {
            return;
        }
        let x = |ms: u64| rect.left() + ms.saturating_sub(timing.start) as f32 * scale;
        let (left, right) = (x(stop.start), x(stop.end).max(x(stop.start) + 3.));
        let mark = egui::Rect::from_x_y_ranges(left..=right, rect.y_range()).intersect(rect);
        let amber = |a| egui::Color32::from_rgba_unmultiplied(242, 201, 76, a);
        ui.painter().rect_filled(mark, 2., amber(90));
        ui.painter().rect_stroke(
            mark,
            2.,
            egui::Stroke::new(1.5, amber(255)),
            egui::StrokeKind::Inside,
        );
    }

    /// Brings the selected objects' clips into view and flashes them.
    pub(crate) fn jump_to_object(&mut self) {
        let ids = self.jump_targets();
        if ids.is_empty() {
            return;
        }
        let v = self.session.document.video.as_ref().unwrap();
        // A clip on a hidden kind of track must become visible to be found.
        for id in &ids {
            let layer = Layer::Annotation(v.objects[id].track);
            let kind = track_kind(&self.session.document, v, layer);
            self.video.hidden_tracks.remove(&kind);
        }
        if self.video.collapsed {
            self.set_timeline_expanded(true);
        }
        self.video.jump.pending = ids;
    }

    /// Applies a pending jump to the timeline scroll area. `view` is the size of
    /// the track area and `top` the y of the first row inside the content.
    pub(super) fn jump_scroll(
        &mut self,
        v: &Project,
        scroll: egui::ScrollArea,
        scale: f32,
        view: egui::Vec2,
        key_rows: Option<(usize, usize)>,
    ) -> egui::ScrollArea {
        if self.video.jump.pending.is_empty() {
            return scroll;
        }
        let ids = std::mem::take(&mut self.video.jump.pending);
        let mut start = u64::MAX;
        let mut end = 0;
        let mut rows = (f32::MAX, 0f32);
        for id in &ids {
            let Some(t) = v.objects.get(id) else {
                continue;
            };
            start = start.min(t.start);
            end = end.max(t.end);
            let row = v.layer_index(Layer::Annotation(t.track));
            let y = keyframes::row_y(0., row, key_rows);
            rows = (rows.0.min(y), rows.1.max(y + 36.));
        }
        if start == u64::MAX {
            return scroll;
        }
        let offset = jump_offset(
            [start as f32 * scale, end as f32 * scale],
            [rows.0, rows.1],
            view,
        );
        self.video.jump.flash = Some((ids, Instant::now()));
        scroll.scroll_offset(offset)
    }

    /// Paints the fading yellow glow over a jumped-to clip.
    pub(super) fn paint_jump_flash(&self, ui: &egui::Ui, id: u64, rect: egui::Rect) {
        let Some((ids, at)) = &self.video.jump.flash else {
            return;
        };
        if !ids.contains(&id) {
            return;
        }
        let strength = flash_strength(at.elapsed().as_secs_f32());
        if strength <= 0. {
            return;
        }
        let yellow = |a: f32| egui::Color32::from_rgba_unmultiplied(242, 201, 76, (a * 255.) as u8);
        let painter = ui.painter();
        painter.rect_filled(rect, 4., yellow(strength * 0.85));
        painter.rect_stroke(
            rect.expand(2.),
            5.,
            egui::Stroke::new(3., yellow(strength)),
            egui::StrokeKind::Outside,
        );
        ui.ctx().request_repaint();
    }

    /// A clip, selection, recording or track is being dragged on the timeline.
    pub(super) fn timeline_dragging(&self) -> bool {
        self.video.drag.is_some()
            || self.video.multi_drag.is_some()
            || self.video.record_drag.is_some()
            || self.video.track_drag.is_some()
    }

    /// Ends a finished flash so it stops requesting repaints.
    pub(super) fn expire_jump_flash(&mut self) {
        if self
            .video
            .jump
            .flash
            .as_ref()
            .is_some_and(|(_, at)| at.elapsed().as_secs_f32() >= FLASH)
        {
            self.video.jump.flash = None;
        }
    }
}

/// Glow strength over time: full while held, then a linear fade to zero.
pub(super) fn flash_strength(elapsed: f32) -> f32 {
    if elapsed < HOLD {
        1.
    } else {
        (1. - (elapsed - HOLD) / (FLASH - HOLD)).clamp(0., 1.)
    }
}

/// Scroll offset that centers the clip span `x` and row span `y` in a view of
/// size `view`, never scrolling before the start of the content.
pub(super) fn jump_offset(x: [f32; 2], y: [f32; 2], view: egui::Vec2) -> egui::Vec2 {
    let center = |lo: f32, hi: f32, size: f32| {
        if hi - lo > size {
            lo - 24.
        } else {
            (lo + hi) * 0.5 - size * 0.5
        }
    };
    egui::vec2(
        center(x[0], x[1], view.x).max(0.),
        center(y[0], y[1], view.y).max(0.),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn jump_centers_the_clip_and_the_flash_fades_out() {
        let o = jump_offset([5000., 5200.], [400., 436.], egui::vec2(800., 200.));
        assert_eq!(o, egui::vec2(4700., 318.));
        // Content start never scrolls negative; long clips show their start.
        assert_eq!(
            jump_offset([10., 30.], [0., 36.], egui::vec2(800., 200.)),
            egui::Vec2::ZERO
        );
        assert_eq!(
            jump_offset([1000., 3000.], [0., 36.], egui::vec2(800., 200.)).x,
            976.
        );
        assert_eq!(flash_strength(0.), 1.);
        assert_eq!(flash_strength(0.3), 1.);
        assert!(flash_strength(0.9) > 0. && flash_strength(0.9) < 1.);
        assert_eq!(flash_strength(FLASH), 0.);
    }
}
