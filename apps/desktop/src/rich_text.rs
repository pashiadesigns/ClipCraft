use crate::{Annotation, Command, Editor, egui};
use capture_core::text::{
    TextStyle,
    editing::{self, Preset},
    rich::{self, Alignment, List, Paragraph},
};
use std::ops::Range;

mod dialog;
#[cfg(test)]
#[path = "rich_text_tests.rs"]
pub(crate) mod tests;

#[derive(Clone, Copy, Default, PartialEq)]
pub(super) enum Tab {
    Font,
    #[default]
    Paragraph,
    Styles,
    Link,
    Find,
    Spelling,
}
impl Tab {
    fn label(self) -> &'static str {
        match self {
            Self::Font => "Font",
            Self::Paragraph => "Paragraph",
            Self::Styles => "Styles",
            Self::Link => "Link",
            Self::Find => "Find / Replace",
            Self::Spelling => "Spelling",
        }
    }
}
pub(super) struct State {
    pub open: bool,
    pub tab: Tab,
    target: Option<Annotation>,
    working: TextStyle,
    range: Range<usize>,
    paragraph: Paragraph,
    link: String,
    format: rich::Format,
    synced_format: rich::Format,
    synced_paragraph: Paragraph,
    synced_link: String,
    font_query: String,
    session_before: (bool, String),
    preview: Option<(String, egui::TextureHandle)>,
    find: String,
    replacement: String,
    case: bool,
    whole: bool,
    focus: bool,
    error: String,
    language: String,
    spelling: Vec<super::spelling::Issue>,
    pending: Option<std::sync::mpsc::Receiver<Result<Vec<super::spelling::Issue>, String>>>,
    pub autocorrect: bool,
    pub corrections: String,
    pub caret_formatting: bool,
}
impl Default for State {
    fn default() -> Self {
        Self {
            open: false,
            tab: Tab::Paragraph,
            target: None,
            working: TextStyle::default(),
            range: 0..0,
            paragraph: Paragraph::default(),
            link: String::new(),
            format: rich::Format::from_style(&TextStyle::default()),
            synced_format: rich::Format::from_style(&TextStyle::default()),
            synced_paragraph: Paragraph::default(),
            synced_link: String::new(),
            font_query: String::new(),
            session_before: (false, String::new()),
            preview: None,
            find: String::new(),
            replacement: String::new(),
            case: false,
            whole: false,
            focus: true,
            error: String::new(),
            language: "en-US".into(),
            spelling: vec![],
            pending: None,
            autocorrect: false,
            corrections: "teh=the\nadn=and\nrecieve=receive".into(),
            caret_formatting: false,
        }
    }
}
impl State {
    fn range(&self) -> Range<usize> {
        let n = self.working.content.chars().count();
        self.range.start.min(n)..self.range.end.min(n)
    }
    fn refresh_fields(&mut self) {
        self.format = rich::format_at(&self.working, self.range().start);
        self.synced_format = self.format.clone();
        self.paragraph = self
            .working
            .rich
            .as_ref()
            .and_then(|r| {
                r.paragraphs
                    .iter()
                    .rev()
                    .find(|p| p.start <= self.range().start)
            })
            .cloned()
            .unwrap_or_else(|| Paragraph::for_style(&self.working, 0));
        self.synced_paragraph = self.paragraph.clone();
        self.link = self.format.link.clone().unwrap_or_default();
        self.synced_link = self.link.clone();
    }
    fn sync_fields(&mut self) {
        let range = self.range();
        if self.format != self.synced_format {
            rich::format_range(&mut self.working, range.clone(), |f| {
                f.changed_fields(&self.synced_format, &self.format)
            });
            if range.start == 0 && range.end == self.working.content.chars().count() {
                let mut base = rich::Format::from_style(&self.working);
                base.changed_fields(&self.synced_format, &self.format);
                base.apply(&mut self.working);
            }
            self.synced_format = self.format.clone();
        }
        if self.paragraph != self.synced_paragraph {
            rich::paragraph_range(&mut self.working, range.clone(), |p| {
                let start = p.start;
                *p = self.paragraph.clone();
                p.start = start;
            });
            self.synced_paragraph = self.paragraph.clone();
        }
        if self.link != self.synced_link && !range.is_empty() {
            let link = (!self.link.trim().is_empty()).then(|| self.link.trim().to_owned());
            rich::format_range(&mut self.working, range, |f| {
                f.link = link.clone();
                if link.is_some() {
                    f.underline = true;
                    f.color = Some([15, 143, 190, 255]);
                }
            });
            self.synced_link = self.link.clone();
            self.format = rich::format_at(&self.working, self.range().start);
            self.synced_format = self.format.clone();
        }
    }
}
impl Editor {
    pub(super) fn common_paragraph_controls(&mut self, ui: &mut egui::Ui) {
        let Some(mut a) = self.selected_annotation() else {
            return;
        };
        let Some(s) = &mut a.text else {
            return;
        };
        let range = self
            .text_draft
            .as_ref()
            .filter(|d| {
                d.selection_active && (!d.selection.is_empty() || self.rich_text.caret_formatting)
            })
            .map_or(0..s.content.chars().count(), |d| d.selection.clone());
        let current = s
            .rich
            .as_ref()
            .and_then(|r| r.paragraphs.iter().rev().find(|p| p.start <= range.start))
            .cloned()
            .unwrap_or_default();
        let mut list = current.list;
        let mut indent = current.indent;
        ui.horizontal(|ui| {
            ui.label("List");
            list_buttons(ui, &mut list);
        });
        ui.horizontal(|ui| {
            ui.label("Indent");
            if ui
                .small_button("−")
                .on_hover_text("Decrease indent")
                .clicked()
            {
                indent = (indent - 16.).max(0.);
            }
            if ui
                .small_button("+")
                .on_hover_text("Increase indent")
                .clicked()
            {
                indent = (indent + 16.).min(256.);
            }
        });
        if list != current.list || indent != current.indent {
            rich::paragraph_range(s, range, |p| {
                p.list = list;
                p.indent = indent;
            });
            if let Some(d) = &mut self.text_draft {
                d.preview = a;
                self.needs_render = true;
            } else {
                self.execute(Command::Update { annotation: a });
            }
        }
    }
    pub(super) fn rich_text_buttons(&mut self, ui: &mut egui::Ui) {
        let enabled = self.selected_annotation().is_some_and(|a| a.text.is_some());
        ui.add_enabled_ui(enabled, |ui| {
            if ui
                .button("Paragraph Settings")
                .on_hover_text("Spacing, indents, tabs and lists")
                .clicked()
            {
                self.open_rich_text(Tab::Paragraph);
            }
        });
    }
    pub(super) fn open_rich_text(&mut self, tab: Tab) {
        self.commit_property_preview();
        let Some(a) = self.selected_annotation().filter(|a| a.text.is_some()) else {
            return;
        };
        let s = a.text.as_ref().unwrap().clone();
        let range = self
            .text_draft
            .as_ref()
            .filter(|d| {
                d.selection_active && (!d.selection.is_empty() || self.rich_text.caret_formatting)
            })
            .map_or(0..s.content.chars().count(), |d| d.selection.clone());
        let start = rich::paragraph_starts(&s.content)
            .into_iter()
            .take_while(|i| *i <= range.start)
            .last()
            .unwrap_or(0);
        let paragraph = s
            .rich
            .as_ref()
            .and_then(|r| r.paragraphs.iter().find(|p| p.start == start))
            .cloned()
            .unwrap_or_else(|| Paragraph::for_style(&s, start));
        let state = &mut self.rich_text;
        state.open = true;
        state.tab = tab;
        state.range = range.clone();
        state.paragraph = paragraph.clone();
        state.synced_paragraph = paragraph;
        state.format = rich::format_at(&s, range.start);
        state.synced_format = state.format.clone();
        state.session_before = (state.autocorrect, state.corrections.clone());
        state.preview = None;
        state.link = rich::format_at(&s, range.start).link.unwrap_or_default();
        state.synced_link = state.link.clone();
        state.working = s;
        state.target = Some(a);
        state.error.clear();
        state.focus = true;
        state.spelling.clear();
        state.pending = None;
    }
    pub(super) fn show_rich_text(&mut self, ctx: &egui::Context) {
        if !self.rich_text.open {
            return;
        }
        let state = &mut self.rich_text;
        if let Some(result) = state.pending.as_ref().and_then(|rx| rx.try_recv().ok()) {
            state.pending = None;
            match result {
                Ok(v) => {
                    state.error = format!("{} spelling suggestions", v.len());
                    state.spelling = v;
                }
                Err(e) => state.error = e,
            }
        }
        state.sync_fields();
        let (apply, mut close) = dialog::show(ctx, state);
        state.sync_fields();
        if apply {
            let state = &mut self.rich_text;
            let style = state.working.clone();
            match style.validate() {
                Err(e) => state.error = e.to_string(),
                Ok(()) => {
                    let mut a = state.target.clone().unwrap();
                    a.text = Some(style);
                    match a
                        .fit_text_shape([self.session.document.width, self.session.document.height])
                    {
                        Err(e) => state.error = e.to_string(),
                        Ok(a) => {
                            if let Some(d) = &mut self.text_draft {
                                if self.rich_text.range.is_empty() {
                                    d.typing = Some(self.rich_text.format.clone());
                                }
                                d.preview = a;
                                self.needs_render = true;
                            } else {
                                self.execute(Command::Update { annotation: a });
                            }
                            close = true;
                        }
                    }
                }
            }
        }
        if close {
            if !apply {
                self.rich_text.autocorrect = self.rich_text.session_before.0;
                self.rich_text.corrections = self.rich_text.session_before.1.clone();
            }
            self.rich_text.open = false;
            self.rich_text.pending = None;
        }
    }
}
pub(super) fn autocorrect(style: &mut TextStyle, caret: usize, rules: &str) -> Option<usize> {
    let chars: Vec<_> = style.content.chars().collect();
    if caret == 0 || caret > chars.len() || !chars[caret - 1].is_whitespace() {
        return None;
    }
    let end = caret - 1;
    let start = chars[..end]
        .iter()
        .rposition(|c| !c.is_alphabetic())
        .map_or(0, |i| i + 1);
    let word = chars[start..end].iter().collect::<String>();
    for rule in rules.lines().take(64) {
        if let Some((from, to)) = rule.split_once('=')
            && from.trim() == word
            && !to.trim().is_empty()
            && editing::replace(style, start..end, to.trim()).is_ok()
        {
            return Some(start + to.trim().chars().count() + 1);
        }
    }
    None
}

/// The same conventional list icons are used in Properties and Text Formatting.
fn list_buttons(ui: &mut egui::Ui, value: &mut List) {
    for (list, name) in [
        (List::None, "No list"),
        (List::Bullet, "Bullet list"),
        (List::Numbered, "Numbered list"),
    ] {
        let r = ui
            .add(
                egui::Button::new("")
                    .min_size(egui::vec2(30., 26.))
                    .selected(*value == list),
            )
            .on_hover_text(name);
        crate::icons::draw_scaled(ui.painter(), r.rect, name, ui.visuals().text_color(), 0.65);
        r.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::Role::RadioButton,
                ui.is_enabled(),
                *value == list,
                name,
            )
        });
        if r.clicked() {
            *value = if *value == list { List::None } else { list };
        }
    }
}
