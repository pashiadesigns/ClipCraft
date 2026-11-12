//! Guided errors (approved: artifacts/error-guidance-proposal, "Approve as
//! shown", amended with an animated done check). Messages say what to do;
//! the message bar shows their kind (error, warning, done); a fix can ring
//! the control to change in amber with a short note, or offer a one-click
//! action. Internal checks are never shown as they are.
use crate::{Editor, egui};
use egui::{Color32, Id, Rect};

pub(crate) const AMBER: Color32 = Color32::from_rgb(242, 177, 52);
pub(crate) const RED: Color32 = Color32::from_rgb(255, 143, 135);
pub(crate) const GREEN: Color32 = Color32::from_rgb(79, 194, 138);

/// What a message in the message bar is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum Kind {
    #[default]
    Plain,
    Done,
    Warning,
    Error,
}

/// A one-click fix offered with a message.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Action {
    /// Unlock these objects.
    Unlock(Vec<u64>),
    /// Open Preferences at a section, ringing a control there.
    Preferences(crate::preferences::Section, Option<&'static str>),
    /// Find Sensitive Info: scan again.
    ScanAgain,
    /// Export video: go to the tracked redactions that need review.
    ReviewRedactions,
    /// Open Manage presets.
    ManagePresets,
    /// Undo: put deleted presets back.
    Restore(Vec<Restore>),
}

/// A deleted preset, kept so Undo can put it back.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Restore {
    ObjectStyle(capture_core::object_styles::ObjectStyle),
    Arrow(capture_core::styles::ArrowStyle),
    Entry(String, capture_core::reusable_presets::Payload),
    SensitiveType(capture_core::privacy::custom::CustomType),
    Watermark(capture_core::watermark::Watermark),
    Palette(String, Vec<[u8; 4]>),
    /// A hidden built-in shown again.
    Hidden(String),
}

/// The message in the bar and how it shows.
#[derive(Clone, Debug, Default)]
pub(crate) struct Notice {
    pub kind: Kind,
    /// The status text this notice belongs to; a different text is a new
    /// message.
    pub text: String,
    pub action: Option<(String, Action)>,
    /// The card above the bar (errors and warnings).
    pub card: bool,
    /// When the message appeared, for the flash and the done check.
    pub since: Option<f64>,
}

/// Messages that report a finished action; they get the green check.
const DONE: &[&str] = &[
    "Saved",
    "Exported",
    "Applied",
    "Added",
    "Copied",
    "Created",
    "Deleted",
    "Duplicated",
    "Inserted",
    "Pasted",
    "Rotated",
    "Resized",
    "Renamed",
    "Packed",
    "Unpacked",
    "Shared",
    "Imported",
    "Restored",
    "Removed",
    "Moved",
    "Flattened",
    "Cropped",
    "Cut Out complete",
    "Page Setup applied",
    "Opened a copy",
    "Video saved",
    "Reviewed every part",
    "Tracked",
    "Print job sent",
    "Objects grouped",
    "Template created",
    "Media added",
    "Block placed",
    "Icon placed",
    "Rotated the image",
    "Video frame copied",
];

pub(crate) fn kind_of(text: &str) -> Kind {
    if DONE.iter().any(|p| text.starts_with(p)) {
        Kind::Done
    } else {
        Kind::Plain
    }
}

/// Internal checks (validation the app protects itself with) never reach
/// the screen as they are; the action that hit one says so plainly.
pub(crate) fn friendly(message: &str) -> String {
    let m = message.trim();
    let low = m.to_lowercase();
    // Known checks that have a plain meaning.
    if low.starts_with("annotation coordinates are outside the image") {
        return "Some objects would end up off the canvas. Nothing was changed. Make the canvas larger, or turn on Extend canvas in Preferences â€º Editor.".into();
    }
    let internal = [
        "invalid ",
        "duplicate ",
        "non-finite",
        "expected ",
        "unknown ",
        "truncated ",
        "object identifier",
        "section identifier",
        "group identifiers",
        "effect identifier",
        "too many object identifiers",
        "no more object",
        "move identifier",
        "canvas changed during",
        "canvas raster",
        "embedded image dimensions",
        "video composite",
        "recording clock",
        "the encoder",
        "audio block",
        "audio packet",
        "decoded media",
        "playback queue",
        "focus surface",
        "pixel effects require",
        "manipulation requires",
        "text style is missing",
        "callout style is missing",
        "only callout",
        "only rectangles",
        "rectangle highlighting",
        "shape frames contain",
        "separate image opacity",
        "embedded images require",
        "step outline requires",
        "image object must",
        "this object type",
        "object rotation requires",
        "strokes need",
        "object dimensions exceed",
        "font size must be finite",
        "text stroke tolerance",
        "guide objects must belong",
        "groups need nonzero",
        "annotation does not exist",
        "guide section identifier",
        "preset identifier limit",
        "recorded actions contain invalid",
        "vector viewport is invalid",
    ];
    if internal.iter().any(|p| low.starts_with(p)) || low.contains("pasteboard") {
        return "ClipDesigner couldn't finish that, so nothing was changed. Undo and try again, or Save As a copy.".into();
    }
    m.to_owned()
}

/// [Editor::fail] for places that hold another part of the editor.
pub(crate) fn set_error(status: &mut String, notice: &mut Notice, message: impl ToString) {
    *status = friendly(&message.to_string());
    *notice = Notice {
        kind: Kind::Error,
        text: status.clone(),
        action: None,
        card: true,
        since: None,
    };
}

impl Editor {
    /// An error in the message bar: red, with the card above it. A message
    /// that names its fix gets that fix as a button on the card.
    pub(crate) fn fail(&mut self, message: impl ToString) {
        let text = friendly(&message.to_string());
        let fix = self.fix_for(&text);
        self.say(Kind::Error, text, fix);
    }
    /// The one-click fix a message names, if any.
    fn fix_for(&self, text: &str) -> Option<(String, Action)> {
        use crate::preferences::Section;
        let low = text.to_lowercase();
        if low.contains(" is locked") || low.contains(" are locked") {
            let mut ids: Vec<u64> = self.selected.into_iter().collect();
            ids.extend(self.extra_selected.iter().copied());
            if !ids.is_empty() {
                return Some(("Unlock".into(), Action::Unlock(ids)));
            }
        }
        if low.contains("manage presets") {
            return Some(("Manage presets".into(), Action::ManagePresets));
        }
        if low.contains("since the scan") {
            return Some(("Scan again".into(), Action::ScanAgain));
        }
        if low.contains("tracked redactions need") {
            return Some(("Review them".into(), Action::ReviewRedactions));
        }
        for (place, section) in [
            ("preferences › general", Section::General),
            ("preferences › library", Section::Library),
            ("preferences › editor", Section::Editor),
            ("preferences › team", Section::Team),
        ] {
            if low.contains(place) {
                return Some((
                    "Open Preferences".into(),
                    Action::Preferences(section, None),
                ));
            }
        }
        None
    }
    /// A warning in the message bar: amber, with the card above it.
    pub(crate) fn warn(&mut self, message: impl ToString, fix: Option<(&str, Action)>) {
        self.say(
            Kind::Warning,
            message.to_string(),
            fix.map(|(l, a)| (l.to_owned(), a)),
        );
    }
    /// A finished action with a fix-it link in the bar (Undo after a
    /// delete).
    pub(crate) fn done_with(&mut self, message: impl ToString, label: &str, action: Action) {
        self.say(
            Kind::Done,
            message.to_string(),
            Some((label.to_owned(), action)),
        );
    }
    fn say(&mut self, kind: Kind, text: String, action: Option<(String, Action)>) {
        self.status = text.clone();
        self.notice = Notice {
            kind,
            text,
            action,
            card: matches!(kind, Kind::Error | Kind::Warning),
            since: None,
        };
    }
    /// Keeps the notice in step with the status text: a message set the
    /// plain way is plain, or done when it reports a finished action.
    pub(crate) fn sync_notice(&mut self, now: f64) {
        if self.notice.text != self.status {
            self.notice = Notice {
                kind: kind_of(&self.status),
                text: self.status.clone(),
                ..Default::default()
            };
        }
        if self.notice.since.is_none() {
            self.notice.since = Some(now);
        }
        // A done message gives way to the save status after a few seconds.
        if self.notice.kind == Kind::Done && self.notice.since.is_some_and(|t| now - t > 6.) {
            self.status.clear();
            self.notice = Notice::default();
        }
    }
    /// Carries out a one-click fix.
    pub(crate) fn run_fix(&mut self, action: Action, ctx: &egui::Context) {
        match action {
            Action::Unlock(ids) => {
                self.execute(capture_core::Command::SetObjectsLocked { ids, locked: false });
                self.needs_render = true;
                self.status = "Unlocked. Try again.".into();
            }
            Action::Preferences(section, target) => {
                self.open_preferences(section);
                if let Some(t) = target {
                    request(ctx, t, String::new());
                }
            }
            Action::ScanAgain => {
                self.open_privacy_review();
            }
            Action::ReviewRedactions => {
                if !self.review_first_stop() {
                    self.status = "Nothing left to review.".into();
                }
            }
            Action::ManagePresets => {
                self.presets_ctx = Some(ctx.clone());
                self.open_manage_presets();
            }
            Action::Restore(items) => self.restore_presets(items),
        }
    }
}

// ---------------------------------------------------------------- the ring

#[derive(Clone, Default)]
struct Ring {
    name: String,
    note: String,
    since: f64,
}
fn ring_id() -> Id {
    Id::unique("guided-ring")
}
fn spot_id(name: &str) -> Id {
    Id::unique(("guided-spot", name))
}

/// Rings the control registered under `name` in amber, with `note` beside
/// it, until [`clear`] (or another ring) replaces it.
pub(crate) fn request(ctx: &egui::Context, name: &str, note: String) {
    let now = ctx.input(|i| i.time);
    ctx.data_mut(|d| {
        d.insert_temp(
            ring_id(),
            Ring {
                name: name.to_owned(),
                note,
                since: now,
            },
        )
    });
    ctx.request_repaint();
}
/// Takes the ring off `name` (when its value has been fixed).
pub(crate) fn clear(ctx: &egui::Context, name: &str) {
    ctx.data_mut(|d| {
        if d.get_temp::<Ring>(ring_id())
            .is_some_and(|r| r.name == name)
        {
            d.remove::<Ring>(ring_id());
        }
    });
}
/// The control currently ringed, if any.
pub(crate) fn active(ctx: &egui::Context) -> Option<String> {
    ctx.data(|d| d.get_temp::<Ring>(ring_id()).map(|r| r.name))
}
/// Registers where a control is this frame, so a ring can find it.
pub(crate) fn mark(ui: &egui::Ui, name: &str, rect: Rect) {
    let pass = ui.ctx().cumulative_pass_nr();
    let clip = ui.clip_rect();
    ui.ctx()
        .data_mut(|d| d.insert_temp(spot_id(name), (rect, clip, pass)));
}
/// Draws the ring and its note over everything, once per frame after the
/// dialogs. Two pulses, then it holds.
pub(crate) fn paint(ctx: &egui::Context) {
    let Some(ring) = ctx.data(|d| d.get_temp::<Ring>(ring_id())) else {
        return;
    };
    let pass = ctx.cumulative_pass_nr();
    let Some((rect, clip, at)) = ctx.data(|d| d.get_temp::<(Rect, Rect, u64)>(spot_id(&ring.name)))
    else {
        return;
    };
    if at + 1 < pass {
        // Not on screen (another tab, or the dialog closed).
        return;
    }
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, ring_id()));
    let t = ctx.input(|i| i.time) - ring.since;
    let r = rect.expand(4.);
    painter.with_clip_rect(clip.expand(12.)).rect_filled(
        r,
        6.,
        Color32::from_rgba_unmultiplied(242, 177, 52, 30),
    );
    painter.rect_stroke(
        r,
        6.,
        egui::Stroke::new(2., AMBER),
        egui::StrokeKind::Outside,
    );
    let reduce = ctx.global_style().animation_time == 0.;
    if !reduce && t < 2.2 {
        let phase = (t % 1.1) / 1.1;
        let grow = (phase * 14.) as f32;
        let alpha = ((1. - phase) * 140.) as u8;
        painter.rect_stroke(
            r.expand(grow),
            6. + grow,
            egui::Stroke::new(2., Color32::from_rgba_unmultiplied(242, 177, 52, alpha)),
            egui::StrokeKind::Outside,
        );
        ctx.request_repaint();
    }
    if !ring.note.is_empty() {
        let galley = painter.layout(
            ring.note.clone(),
            egui::FontId::proportional(12.5),
            Color32::from_rgb(251, 227, 176),
            270.,
        );
        let size = galley.size() + egui::vec2(20., 16.);
        let mut at = egui::pos2(r.right() - size.x, r.bottom() + 10.);
        let screen = ctx.content_rect();
        if at.y + size.y > screen.bottom() {
            at.y = r.top() - 10. - size.y;
        }
        at.x = at.x.clamp(screen.left() + 4., screen.right() - size.x - 4.);
        let note = Rect::from_min_size(at, size);
        painter.rect_filled(note, 6., Color32::from_rgb(59, 50, 32));
        painter.rect_stroke(
            note,
            6.,
            egui::Stroke::new(1., AMBER),
            egui::StrokeKind::Inside,
        );
        painter.galley(note.min + egui::vec2(10., 8.), galley, Color32::WHITE);
    }
}

// ------------------------------------------------------- shared drawing

/// The round kind icon: "!" for errors and warnings, an animated check for
/// done (pops in, draws its tick, glows once; still when motion is off).
pub(crate) fn icon(ui: &egui::Ui, center: egui::Pos2, kind: Kind, since: f64) {
    let p = ui.painter();
    let now = ui.input(|i| i.time);
    let t = (now - since) as f32;
    let still = ui.ctx().global_style().animation_time == 0.;
    match kind {
        Kind::Plain => {
            p.circle_filled(center, 3., GREEN);
        }
        Kind::Error | Kind::Warning => {
            let color = if kind == Kind::Error { RED } else { AMBER };
            p.circle_filled(center, 8., color);
            p.text(
                center,
                egui::Align2::CENTER_CENTER,
                "!",
                egui::FontId::proportional(11.),
                Color32::from_rgb(40, 20, 12),
            );
        }
        Kind::Done => {
            let pop = if still {
                1.
            } else {
                // Overshoots a little, then settles.
                let x = (t / 0.5).clamp(0., 1.);
                1. + 0.25 * (std::f32::consts::PI * x).sin() * (1. - x) - (1. - x).powi(3)
            };
            let radius = 8. * pop.max(0.);
            if !still && (0.35..1.75).contains(&t) {
                let g = (t - 0.35) / 1.4;
                p.circle_stroke(
                    center,
                    radius + g * 12.,
                    egui::Stroke::new(2., GREEN.gamma_multiply(1. - g)),
                );
            }
            p.circle_filled(center, radius, GREEN);
            let draw = if still {
                1.
            } else {
                ((t - 0.25) / 0.35).clamp(0., 1.)
            };
            let a = center + egui::vec2(-3.6, 0.2);
            let b = center + egui::vec2(-1., 2.8);
            let c = center + egui::vec2(3.8, -2.6);
            let stroke = egui::Stroke::new(1.9, Color32::from_rgb(13, 42, 28));
            let first = (draw * 2.).min(1.);
            p.line_segment([a, a + (b - a) * first], stroke);
            if draw > 0.5 {
                let second = (draw - 0.5) * 2.;
                p.line_segment([b, b + (c - b) * second], stroke);
            }
            if !still && t < 1.8 {
                ui.ctx().request_repaint();
            }
        }
    }
}

/// A dialog's message line in the guided style: the kind icon, the message
/// (leading with what to do) and an optional "Show me" that repeats the
/// jump to the control. Returns true when Show me was clicked.
pub(crate) fn dialog_message(ui: &mut egui::Ui, kind: Kind, text: &str, show_me: bool) -> bool {
    if text.is_empty() {
        return false;
    }
    let key = ui.scope_id().with(("guided-msg", text));
    let now = ui.input(|i| i.time);
    let since = ui
        .ctx()
        .data_mut(|d| *d.get_temp_mut_or_insert_with(key, || now));
    let mut clicked = false;
    ui.horizontal_top(|ui| {
        let (r, _) = ui.allocate_exact_size(egui::vec2(18., 18.), egui::Sense::hover());
        icon(ui, r.center(), kind, since);
        ui.vertical(|ui| {
            ui.add(
                egui::Label::new(egui::RichText::new(text).strong().color(match kind {
                    Kind::Error => Color32::from_rgb(255, 214, 209),
                    Kind::Warning => Color32::from_rgb(251, 227, 176),
                    Kind::Done => Color32::from_rgb(200, 240, 219),
                    Kind::Plain => ui.visuals().text_color(),
                }))
                .wrap(),
            );
            if show_me {
                clicked = ui
                    .link(egui::RichText::new("Show me").color(Color32::from_rgb(127, 208, 239)))
                    .clicked();
            }
        });
    });
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn internal_checks_are_never_shown_as_they_are() {
        for raw in [
            "Invalid pasteboard object ownership",
            "Guide objects must belong to one existing section",
            "Duplicate annotation id",
            "Invalid image perspective",
            "Strokes need 2-8192 points; polygons need 3-64; other objects cannot carry paths",
        ] {
            let shown = friendly(raw);
            assert!(
                shown.starts_with("ClipDesigner couldn't finish that"),
                "{raw} -> {shown}"
            );
        }
        assert!(friendly("Annotation coordinates are outside the image").contains("Extend canvas"));
        // Messages written for people pass through unchanged.
        let ok = "Choose at least one page.";
        assert_eq!(friendly(ok), ok);
    }
    /// Approved test: no engine message with internal words reaches the
    /// screen unguided. Every Error::Invalid literal in the engine that uses
    /// internal words (annotation, identifier, payload, pasteboard, invalid)
    /// is replaced by `friendly`.
    #[test]
    fn no_internal_check_reaches_the_screen_as_it_is() {
        let core = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/core/src");
        let jargon = [
            "annotation",
            "identifier",
            "payload",
            "pasteboard",
            "invalid ",
        ];
        let mut leaks = vec![];
        let mut stack = vec![core];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                if !name.ends_with(".rs") || name.contains("test") {
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap();
                for m in invalid_literals(&text) {
                    let m = m.as_str();
                    let low = m.to_lowercase();
                    if jargon.iter().any(|j| low.contains(j)) && friendly(m) == m {
                        leaks.push(format!("{name}: {m}"));
                    }
                }
            }
        }
        assert!(
            leaks.is_empty(),
            "internal wording would reach the screen:\n{}",
            leaks.join("\n")
        );
    }
    /// The string literals passed straight to Error::Invalid.
    fn invalid_literals(text: &str) -> Vec<String> {
        let mut out = vec![];
        for (at, _) in text.match_indices("Error::Invalid(") {
            let rest = text[at + "Error::Invalid(".len()..].trim_start();
            let Some(body) = rest.strip_prefix('"') else {
                continue;
            };
            let mut value = String::new();
            let mut escaped = false;
            for ch in body.chars() {
                if escaped {
                    value.push(ch);
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == '"' {
                    break;
                } else {
                    value.push(ch);
                }
            }
            out.push(value);
        }
        out
    }
    #[test]
    fn errors_that_name_their_fix_get_its_button() {
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        e.selected = Some(7);
        e.fail("This object is locked, so it can't turn or flip. Unlock it, then try again.");
        assert_eq!(e.notice.kind, Kind::Error);
        assert!(e.notice.card);
        assert_eq!(
            e.notice.action,
            Some(("Unlock".into(), Action::Unlock(vec![7])))
        );
        e.fail("You have 24 arrow styles. Delete one in Manage presets first.");
        assert_eq!(
            e.notice.action.as_ref().map(|a| a.0.as_str()),
            Some("Manage presets")
        );
        e.fail("Choose a Project folder in Preferences › General first.");
        assert!(matches!(
            e.notice.action,
            Some((_, Action::Preferences(..)))
        ));
        e.fail("Invalid pasteboard object ownership");
        assert!(e.status.starts_with("ClipDesigner couldn't finish that"));
        assert_eq!(e.notice.action, None);
        // A plain status line afterwards is a new, plain message.
        e.status = "Drag to draw the selected shape.".into();
        e.sync_notice(0.);
        assert_eq!(e.notice.kind, Kind::Plain);
    }
    #[test]
    fn finished_actions_are_done_and_others_plain() {
        assert_eq!(kind_of("Saved C:\\x.iclip"), Kind::Done);
        assert_eq!(
            kind_of("Page Setup applied. Undo restores the previous layout."),
            Kind::Done
        );
        assert_eq!(kind_of("Drag to draw the selected shape."), Kind::Plain);
    }
}
