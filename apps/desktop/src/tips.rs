//! One tooltip per control (approved mockup `artifacts/bh-proposal`): the
//! control's name in bold, then what it does on the next line.
//!
//! Shared widgets register their name with [`Tip::tip_title`] and the screen
//! using them adds the explanation with [`Tip::tip`]. Both are collected during
//! the frame and drawn as a single box at the end of the pass, so a control
//! never shows two stacked boxes. A name without an explanation takes one from
//! [`explain`].
use crate::egui;
use std::sync::Arc;

#[derive(Clone)]
struct Pending {
    response: egui::Response,
    title: Option<String>,
    body: Vec<String>,
    disabled: Vec<String>,
}
#[derive(Clone, Default)]
struct Frame(Vec<Pending>);

fn key() -> egui::Id {
    egui::Id::unique("clipdesigner-tips")
}

/// Registers the end-of-pass tooltip drawing once per context.
pub(crate) fn install(ctx: &egui::Context) {
    let installed = egui::Id::unique("clipdesigner-tips-installed");
    if ctx.data(|d| d.get_temp::<bool>(installed)).is_some() {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(installed, true));
    ctx.on_end_pass("clipdesigner-tips", Arc::new(|ui| flush(ui.ctx())));
}

fn record(response: &egui::Response, f: impl FnOnce(&mut Pending)) {
    // Only hovered controls can show a tooltip; tests record everything so
    // they can check that every named control explains itself.
    if !cfg!(test) && !response.contains_pointer() {
        return;
    }
    response.ctx.data_mut(|d| {
        let frame = d.get_temp_mut_or_default::<Frame>(key());
        let i = match frame.0.iter().position(|p| p.response.id == response.id) {
            Some(i) => i,
            None => {
                frame.0.push(Pending {
                    response: response.clone(),
                    title: None,
                    body: vec![],
                    disabled: vec![],
                });
                frame.0.len() - 1
            }
        };
        f(&mut frame.0[i]);
    });
}

pub(crate) trait Tip: Sized {
    /// The explanation line(s) for this control.
    fn tip(self, body: impl Into<egui::WidgetText>) -> Self;
    /// Shown instead while the control is disabled: why it is.
    fn tip_disabled(self, body: impl Into<egui::WidgetText>) -> Self;
    /// The control's name, shown in bold.
    fn tip_title(self, title: impl Into<String>) -> Self;
}
impl Tip for egui::Response {
    fn tip(self, body: impl Into<egui::WidgetText>) -> Self {
        let text = body.into().text().to_owned();
        record(&self, |p| {
            if !p.body.contains(&text) {
                p.body.push(text)
            }
        });
        self
    }
    fn tip_disabled(self, body: impl Into<egui::WidgetText>) -> Self {
        let text = body.into().text().to_owned();
        record(&self, |p| {
            if !p.disabled.contains(&text) {
                p.disabled.push(text)
            }
        });
        self
    }
    fn tip_title(self, title: impl Into<String>) -> Self {
        let title = title.into();
        record(&self, |p| {
            p.title.get_or_insert(title);
        });
        self
    }
}

/// What a tooltip shows: a bold title and its explanation lines.
pub(crate) fn content(title: Option<&str>, body: &[String]) -> (Option<String>, Vec<String>) {
    let mut body: Vec<String> = body.to_vec();
    let mut title = title.map(str::to_owned);
    // "Move (V)" on the first line and what it does on the next.
    if title.is_none()
        && let Some((name, rest)) = body.first().and_then(|b| b.split_once('\n'))
        && is_name(name)
    {
        let (name, rest) = (name.to_owned(), rest.to_owned());
        title = Some(name);
        body[0] = rest;
    }
    // A lone short line is the control's name; a lone sentence is its
    // explanation. Either way the box gets both a name and an explanation.
    if title.is_none() && !body.is_empty() && is_name(&body[0]) {
        title = Some(body.remove(0));
    }
    // "Undo · Ctrl+Z" names the control and then says something about it.
    if title.is_none()
        && let Some((name, rest)) = body.first().and_then(|b| b.split_once(" · "))
        && is_name(name)
    {
        let (name, rest) = (name.to_owned(), rest.to_owned());
        title = Some(name);
        body[0] = rest;
    } else if let Some(t) = &title
        && let Some(rest) = body
            .first()
            .and_then(|b| b.strip_prefix(&format!("{t} · ")))
    {
        let rest = rest.to_owned();
        body[0] = rest;
    }
    if title.is_some() && title.as_deref() == body.first().map(String::as_str) {
        body.remove(0);
    }
    if body.is_empty()
        && let Some(t) = &title
        && let Some(e) = explain(t)
    {
        body.push(e.to_owned());
    }
    (title, body)
}
fn is_name(s: &str) -> bool {
    s.len() <= 40 && !s.contains(['.', '\n', ':', '·']) && s.split_whitespace().count() <= 5
}

/// Preferences > Show tooltips.
pub(crate) fn set_enabled(ctx: &egui::Context, enabled: bool) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("clipdesigner-tips-enabled"), enabled));
}
/// The widest a tooltip grows before its text wraps.
pub(crate) const MAX_WIDTH: f32 = 250.;
fn flush(ctx: &egui::Context) {
    let pending = ctx
        .data_mut(|d| d.remove_temp::<Frame>(key()))
        .map_or_else(Vec::new, |Frame(pending)| pending);
    let enabled = ctx
        .data(|d| d.get_temp::<bool>(egui::Id::unique("clipdesigner-tips-enabled")))
        .unwrap_or(true);
    let pending = if enabled { pending } else { Vec::new() };
    #[cfg(test)]
    tests::observe(&pending);
    let mut hovered = None;
    for p in pending {
        let enabled = p.response.enabled();
        let lines = if enabled { &p.body } else { &p.disabled };
        if !enabled && lines.is_empty() {
            continue;
        }
        let (title, body) = content(p.title.as_deref(), lines);
        if title.is_none() && body.is_empty() {
            continue;
        }
        // Controls with a Help article end with an F1 hint (bi-proposal 4).
        let article = title.as_deref().and_then(crate::help::for_control);
        if article.is_some() && p.response.contains_pointer() {
            hovered = article;
        }
        let tooltip = if enabled {
            egui::containers::Tooltip::for_enabled(&p.response)
        } else {
            egui::containers::Tooltip::for_disabled(&p.response)
        };
        tooltip.show(|ui| {
            // As wide as the longest line, up to half the old 500 px width
            // (approved `artifacts/br-proposal/tooltip-width.html`); longer wraps.
            let font = egui::TextStyle::Body.resolve(ui.style());
            let longest = title
                .iter()
                .chain(body.iter())
                .map(|line| {
                    ui.painter()
                        .layout_no_wrap(line.clone(), font.clone(), egui::Color32::WHITE)
                        .size()
                        .x
                })
                .fold(0f32, f32::max);
            let width = longest.clamp(40., MAX_WIDTH);
            ui.set_max_width(width);
            ui.set_min_width(width.min(longest));
            if let Some(t) = &title {
                ui.label(egui::RichText::new(t).strong().color(egui::Color32::WHITE));
            }
            for line in &body {
                ui.label(egui::RichText::new(line).color(egui::Color32::from_rgb(183, 193, 195)));
            }
            if article.is_some() {
                ui.separator();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new("F1 Help")
                            .small()
                            .color(egui::Color32::from_rgb(142, 154, 157)),
                    );
                });
            }
        });
    }
    // F1 opens the topic of the control under the pointer, or Help's first page.
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F1)) {
        crate::help::request(ctx, hovered, "");
        ctx.request_repaint();
    }
}

/// Explanations for controls whose screens give only a name.
pub(crate) fn explain(title: &str) -> Option<&'static str> {
    if title.starts_with("Text justification") {
        return Some("Place the text in this part of the shape.");
    }
    if title.ends_with("ClipDesigner") {
        return Some("The open project. Drag here to move the window.");
    }
    // Tabs that show a count, such as "Icons (2)".
    if let Some((base, _)) = title.split_once(" (")
        && let Some(e) = explain_exact(base)
    {
        return Some(e);
    }
    explain_exact(title)
}
fn explain_exact(title: &str) -> Option<&'static str> {
    Some(match title {
        "Close window" => "Close ClipDesigner; it keeps running in the system tray.",
        "Maximize window" => "Fill the screen with the ClipDesigner window, or restore its size.",
        "Minimize window" => "Minimize ClipDesigner to the taskbar.",
        "File" => "Open, save, export and print projects.",
        "Library" => "Your captures and saved projects.",
        "Insert" => "Add shapes, text, steps, icons and other objects.",
        "Formatting" => "Text and object formatting for the selection.",
        "Advanced" => "Image adjustments, privacy and other tools.",
        "Export" => "Save the result as an image, PDF, GIF or video.",
        "Icons" => "Icons and stamps you can place.",
        "Magnified callout" => "Enlarge part of the picture in a callout.",
        "Timeline snapping" => "Snap clips and the playhead to edges and keys.",
        "Zoom preview out" => "Show the video preview smaller.",
        "Zoom preview in" => "Show the video preview larger.",
        "Fit preview" => "Fit the whole video preview in view.",
        "Drag to reorder track" => "Drag up or down to change which track is on top.",
        "Hide track" => "Hide this track in the preview and export.",
        "Add Track" => "Add an empty track to the timeline.",
        "Text stroke color" => "Color of the outline drawn around the letters.",
        "Reset rotation to 0°" => "Turn the object back to upright.",
        "Start here" => "Start the selected item at the playhead.",
        "End here" => "End the selected item at the playhead.",
        "Preview animation" => "Play the entrance or exit once on the canvas.",
        "Previous key" => "Move the playhead to the previous keyframe.",
        "Next key" => "Move the playhead to the next keyframe.",
        "More key actions" => "Copy, paste, duplicate or delete the key at the playhead.",
        "No list" => "Remove bullets or numbers from these paragraphs.",
        "Bullet list" => "Start each paragraph with a bullet.",
        "Numbered list" => "Number each paragraph.",
        "Decrease indent" => "Move these paragraphs to the left.",
        "Increase indent" => "Move these paragraphs to the right.",
        "Spacing, indents, tabs and lists" => {
            "Open paragraph spacing, indent, tab and list settings."
        }
        "Padding" => "Space between the text and the shape's edge.",
        "Show or hide recent clips" => "Show or hide the Recent Clips strip. Shortcut: Ctrl+R",
        "Zoom out" => "Show the canvas smaller.",
        "Zoom in" => "Show the canvas larger.",
        "Fit to Screen" => "Fit the whole canvas in view.",
        "Actual size · 100%" => "Show the canvas at its real pixel size.",
        "Resize Image" => "Change the image's size in pixels or percent.",
        "Layers" => "Every object in this image: order, visibility and locks.",
        "Width" => "Width in pixels.",
        "Start size" => "Size of the line's starting end.",
        "End size" => "Size of the line's finishing end.",
        "Text style" => "Apply a saved heading or body text style.",
        "Increase font size" => "Make the text one step larger.",
        "Decrease font size" => "Make the text one step smaller.",
        "Clear text formatting" => "Return the text to its plain style.",
        "Superscript" => "Raise the characters above the line.",
        "Subscript" => "Lower the characters below the line.",
        "Change case" => "Switch between UPPERCASE, lowercase and Title Case.",
        "Character spacing" => "Space between letters.",
        "Sides" => "Number of sides of the polygon.",
        "New intensity" => "Strength of blur or pixelation for the next one you draw.",
        "Find Sensitive Info…" => {
            "Search for emails, phone numbers and other private details to redact."
        }
        "Style" => "Presets, appearance and placement of the selected object.",
        "Text" => "Font, paragraph and text fitting for the selected object's text.",
        "Timing" => "When the selected item starts and ends, and how it enters and exits.",
        "Motion" => "Keyframes and motion paths for the selected item.",
        "Audio" => "Volume, fades and voice effects for the selected clip.",
        "Image" => "The picture inside this frame: replace, crop or reposition it.",
        "Layout" => "Position, size, rotation and alignment.",
        "Objects" => "Every object in this image: order, visibility and locks.",
        "Properties" => "Settings for the selected object or tool.",
        "Fill" => "Color inside the shape.",
        "Outline" => "Color of the shape's outline.",
        "Stroke" => "Color of the outline drawn around the letters.",
        "Highlight" => "Background color behind the letters.",
        "Color" => "Color of the line or stroke.",
        "Thickness" => "Width of the outline or line, in pixels.",
        "Opacity" => "How see-through the outline or line is.",
        "Fill opacity" => "How see-through the fill is.",
        "Corner radius" => "How rounded the corners are.",
        "Outline style" => "Solid, dashed, dotted or wavy outline.",
        "Text stroke size" => "Width of the outline drawn around the letters.",
        "Marker size" => "Diameter of the step marker.",
        "Shadow" => "Drop shadow behind the object.",
        "Rotation" => "Turn the object around its center, in degrees.",
        "Split" => "Cut the selected clip in two at the playhead.",
        "Trim" => "Remove everything before or after the playhead.",
        "Import…" => "Add audio, an image or a video at the playhead.",
        "Keyframes" => "Animate properties over time with keys.",
        "Hide tracks" => "Hide kinds of tracks to make the timeline easier to read.",
        "Timeline" => "Show or hide the timeline.",
        "Jump to Object" => "Scroll the timeline to the selected object's clip. Shortcut: F",
        "Previous frame" => "Step back one frame.",
        "Next frame" => "Step forward one frame.",
        "Zoom timeline in" => "Show more detail on the timeline.",
        "Zoom timeline out" => "Show more of the timeline at once.",
        "Microphone" => "Record your default Windows microphone.",
        "System audio" => "Record the sound playing on this PC.",
        "Cursor" => "Record the mouse pointer as an editable cursor.",
        "Webcam" => "Record your webcam as its own picture-in-picture track.",
        "Open editor" => "Open the recording in ClipDesigner when it finishes.",
        "Open folder" => "Show the recording in File Explorer when it finishes.",
        "Region" => "Drag to choose part of the screen.",
        "Window" => "Click a window to capture it.",
        "Fullscreen" => "Capture a whole display.",
        "Undo" => "Undo the last change. Shortcut: Ctrl+Z",
        "Redo" => "Redo the change you undid. Shortcut: Ctrl+Y",
        "Save" => "Save this project. Shortcut: Ctrl+S",
        "Copy" => "Copy to the clipboard. Shortcut: Ctrl+C",
        "Paste" => "Paste from the clipboard. Shortcut: Ctrl+V",
        "Delete object" => "Remove the selected object. Shortcut: Delete",
        "Delete selected objects" => "Remove the selected objects. Shortcut: Delete",
        "Capture Clip" => "Capture an image of part of the screen.",
        "Capture Video" => "Record a video of part of the screen.",
        "Hide UI" => "Hide the panels to see more of the canvas. Shortcut: Tab",
        "Show UI" => "Show the panels again. Shortcut: Tab",
        "Minimize" => "Minimize ClipDesigner to the taskbar.",
        "Maximize" | "Restore" => "Resize the ClipDesigner window.",
        "Close" => "Close ClipDesigner; it keeps running in the system tray.",
        "Align left" => "Line text up along the left edge.",
        "Align center" => "Center each line of text.",
        "Align right" => "Line text up along the right edge.",
        "Bold" => "Make text bold. Shortcut: Ctrl+B",
        "Italic" => "Make text italic. Shortcut: Ctrl+I",
        "Underline" => "Underline text. Shortcut: Ctrl+U",
        "Strikethrough" => "Draw a line through text.",
        _ => return None,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::cell::RefCell;
    thread_local! {
        /// Names seen without an explanation in this test thread's frames.
        pub(crate) static UNEXPLAINED: RefCell<Vec<String>> = const { RefCell::new(vec![]) };
        /// Every control name seen in this test thread's frames.
        pub(crate) static NAMED: RefCell<Vec<String>> = const { RefCell::new(vec![]) };
        /// Tooltips that explain a control without naming it.
        pub(crate) static UNNAMED: RefCell<Vec<String>> = const { RefCell::new(vec![]) };
    }
    pub(super) fn observe(pending: &[Pending]) {
        for p in pending {
            let (title, body) = content(p.title.as_deref(), &p.body);
            if title.is_none()
                && let Some(b) = body.first()
            {
                UNNAMED.with(|seen| {
                    let mut seen = seen.borrow_mut();
                    if !seen.contains(b) {
                        seen.push(b.clone());
                    }
                });
            }
            if let Some(t) = &title {
                NAMED.with(|seen| {
                    let mut seen = seen.borrow_mut();
                    if !seen.contains(t) {
                        seen.push(t.clone());
                    }
                });
            }
            if let Some(t) = title
                && body.is_empty()
            {
                UNEXPLAINED.with(|seen| {
                    let mut seen = seen.borrow_mut();
                    if !seen.contains(&t) {
                        seen.push(t);
                    }
                });
            }
        }
    }
    #[test]
    fn f1_opens_the_topic_of_the_hovered_control() {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut rect = egui::Rect::NOTHING;
        let mut frame = |events: Vec<egui::Event>| {
            crate::property_layout::tests::PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400., 300.),
                    )),
                    events,
                    focused: true,
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        rect = ui.button("Text").tip_title("Text").tip("Add text.").rect;
                    });
                },
            ));
            rect
        };
        let f1 = || egui::Event::Key {
            key: egui::Key::F1,
            physical_key: Some(egui::Key::F1),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let button = frame(vec![]);
        frame(vec![egui::Event::PointerMoved(button.center())]);
        frame(vec![egui::Event::PointerMoved(button.center()), f1()]);
        let text = crate::help::for_control("Text");
        assert!(text.is_some());
        assert_eq!(crate::help::take_request(&ctx), Some((text, String::new())));
        frame(vec![egui::Event::PointerMoved(egui::pos2(380., 280.))]);
        frame(vec![
            egui::Event::PointerMoved(egui::pos2(380., 280.)),
            f1(),
        ]);
        assert_eq!(crate::help::take_request(&ctx), Some((None, String::new())));
    }
    #[test]
    fn a_name_and_an_explanation_make_one_box() {
        assert_eq!(
            content(Some("Thickness"), &["0 shows no outline".into()]),
            (Some("Thickness".into()), vec!["0 shows no outline".into()])
        );
        assert_eq!(content(None, &[]), (None, vec![]));
        // A lone name gets its explanation from the shared table.
        let (t, b) = content(None, &["Split".into()]);
        assert_eq!(t.as_deref(), Some("Split"));
        assert_eq!(b.len(), 1);
        // The same text as name and explanation is shown once.
        assert_eq!(
            content(Some("Zoom"), &["Zoom".into()]),
            (Some("Zoom".into()), vec![])
        );
    }
    #[test]
    fn two_tooltip_calls_on_one_control_collect_into_a_single_box() {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400., 300.),
                )),
                ..Default::default()
            },
            |ui| {
                ui.button("Split")
                    .tip_title("Split")
                    .tip("Cut the selected clip in two at the playhead.")
                    .tip("Cut the selected clip in two at the playhead.");
                let frame = ui.ctx().data(|d| d.get_temp::<Frame>(key())).unwrap();
                assert_eq!(frame.0.len(), 1, "one pending tooltip for the control");
                assert_eq!(frame.0[0].title.as_deref(), Some("Split"));
                assert_eq!(frame.0[0].body.len(), 1, "repeated explanations collapse");
            },
        );
        out.textures_delta.clear();
    }
}
