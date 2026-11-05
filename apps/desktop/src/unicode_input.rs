//! Shared grapheme-aware buffer. egui owns clipboard, composition and Undo.
use crate::egui;
use egui::text::{CCursor, CCursorRange, CharIndex};
type Layouter<'a> =
    &'a mut dyn FnMut(&egui::Ui, &dyn egui::TextBuffer, f32) -> std::sync::Arc<egui::Galley>;
pub(super) struct TextEdit<'a> {
    text: &'a mut String,
    multiline: bool,
    id: Option<egui::Id>,
    limit: Option<usize>,
    rows: Option<usize>,
    width: Option<f32>,
    size: Option<egui::Vec2>,
    align: Option<egui::Align2>,
    frame: Option<egui::Frame>,
    margin: Option<egui::Margin>,
    hint: Option<String>,
    font: Option<egui::FontSelection>,
    color: Option<egui::Color32>,
    layouter: Option<Layouter<'a>>,
}
impl<'a> TextEdit<'a> {
    pub fn singleline(text: &'a mut String) -> Self {
        Self {
            text,
            multiline: false,
            id: None,
            limit: None,
            rows: None,
            width: None,
            size: None,
            align: None,
            frame: None,
            margin: None,
            hint: None,
            font: None,
            color: None,
            layouter: None,
        }
    }
    pub fn multiline(text: &'a mut String) -> Self {
        Self {
            multiline: true,
            ..Self::singleline(text)
        }
    }
    pub fn id(mut self, n: egui::Id) -> Self {
        self.id = Some(n);
        self
    }
    pub fn char_limit(mut self, n: usize) -> Self {
        self.limit = Some(n);
        self
    }
    pub fn desired_rows(mut self, n: usize) -> Self {
        self.rows = Some(n);
        self
    }
    pub fn desired_width(mut self, n: f32) -> Self {
        self.width = Some(n);
        self
    }
    pub fn min_size(mut self, n: egui::Vec2) -> Self {
        self.size = Some(n);
        self
    }
    pub fn align(mut self, n: egui::Align2) -> Self {
        self.align = Some(n);
        self
    }
    pub fn frame(mut self, n: egui::Frame) -> Self {
        self.frame = Some(n);
        self
    }
    pub fn margin(mut self, n: impl Into<egui::Margin>) -> Self {
        self.margin = Some(n.into());
        self
    }
    pub fn hint_text(mut self, n: impl Into<String>) -> Self {
        self.hint = Some(n.into());
        self
    }
    pub fn font(mut self, n: impl Into<egui::FontSelection>) -> Self {
        self.font = Some(n.into());
        self
    }
    pub fn text_color(mut self, n: egui::Color32) -> Self {
        self.color = Some(n);
        self
    }
    pub fn layouter(mut self, n: Layouter<'a>) -> Self {
        self.layouter = Some(n);
        self
    }
    pub fn show(self, ui: &mut egui::Ui) -> egui::text_edit::TextEditOutput {
        let id = self.id.unwrap_or_else(|| ui.next_auto_id());
        let composing = ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Ime(_))))
            || ui.ctx().data(|d| {
                d.get_temp::<bool>(egui::Id::unique("dialog-ime"))
                    .unwrap_or(false)
            });
        if !composing {
            prepare(ui, id, self.text);
        }
        let enter = !self.multiline
            && !composing
            && ui.memory(|m| m.has_focus(id))
            && ui
                .input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers == egui::Modifiers::NONE);
        let mut buffer = Buffer {
            value: self.text,
            composing,
        };
        let mut edit = if self.multiline {
            egui::TextEdit::multiline(&mut buffer)
        } else {
            egui::TextEdit::singleline(&mut buffer)
        }
        .id(id);
        if let Some(n) = self.limit {
            edit = edit.char_limit(n);
        }
        if let Some(n) = self.rows {
            edit = edit.desired_rows(n);
        }
        if let Some(n) = self.width {
            edit = edit.desired_width(n);
        }
        if let Some(n) = self.size {
            edit = edit.min_size(n);
        }
        if let Some(n) = self.align {
            edit = edit.align(n);
        }
        if let Some(n) = self.frame {
            edit = edit.frame(n);
        }
        if let Some(n) = self.margin {
            edit = edit.margin(n);
        }
        if let Some(n) = self.hint {
            edit = edit.hint_text(n);
        }
        if let Some(n) = self.font {
            edit = edit.font(n);
        }
        if let Some(n) = self.color {
            edit = edit.text_color(n);
        }
        if let Some(n) = self.layouter {
            edit = edit.layouter(n);
        }
        let output = edit.show(ui);
        if enter {
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::unique("dialog-singleline-enter"), true));
        }
        output
    }
}
impl egui::Widget for TextEdit<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        self.show(ui).response.response
    }
}
struct Buffer<'a> {
    value: &'a mut String,
    composing: bool,
}
fn edges(text: &str, index: usize) -> (usize, usize) {
    let b = capture_core::text::unicode::boundaries(text);
    (
        b.iter().copied().rev().find(|x| *x <= index).unwrap_or(0),
        b.into_iter()
            .find(|x| *x >= index)
            .unwrap_or(text.chars().count()),
    )
}
impl egui::TextBuffer for Buffer<'_> {
    fn is_mutable(&self) -> bool {
        true
    }
    fn as_str(&self) -> &str {
        self.value
    }
    fn type_id(&self) -> std::any::TypeId {
        std::any::TypeId::of::<Buffer<'static>>()
    }
    fn insert_text(&mut self, text: &str, index: CharIndex) -> usize {
        let at = if self.composing {
            index
        } else {
            CharIndex(edges(self.value, index.0).1)
        };
        self.value.insert_text(text, at)
    }
    fn insert_text_at(&mut self, cursor: &mut CCursor, text: &str, limit: usize) {
        if self.composing {
            cursor.index += self.insert_text(text, cursor.index);
            return;
        }
        cursor.index = CharIndex(edges(self.value, cursor.index.0).1);
        let remaining = limit.saturating_sub(self.value.chars().count());
        let count = capture_core::text::unicode::boundaries(text)
            .into_iter()
            .take_while(|n| *n <= remaining)
            .last()
            .unwrap_or(0);
        let end = text.char_indices().nth(count).map_or(text.len(), |x| x.0);
        cursor.index += self.insert_text(&text[..end], cursor.index);
    }
    fn delete_char_range(&mut self, range: std::ops::Range<CharIndex>) {
        if range.is_empty() {
            return;
        }
        let range = if self.composing {
            range
        } else {
            CharIndex(edges(self.value, range.start.0).0)
                ..CharIndex(edges(self.value, range.end.0).1)
        };
        self.value.delete_char_range(range);
    }
    fn delete_selected_ccursor_range(&mut self, [a, b]: [CCursor; 2]) -> CCursor {
        let start = if self.composing {
            a.index.0
        } else {
            edges(self.value, a.index.0).0
        };
        self.delete_char_range(a.index..b.index);
        CCursor::new(start)
    }
}
fn prepare(ui: &egui::Ui, id: egui::Id, text: &str) {
    if !ui.memory(|m| m.has_focus(id)) {
        return;
    }
    let bounds = capture_core::text::unicode::boundaries(text);
    if bounds.len() == text.chars().count() + 1 {
        return;
    }
    let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id) else {
        return;
    };
    let Some(mut range) = state.cursor.char_range() else {
        return;
    };
    // Pointer selection can stop between an emoji's components. Copy/cut and
    // subsequent keyboard edits must operate on the same complete clusters.
    if !range.is_empty() {
        let sorted = range.sorted_cursors();
        let start = CCursor::new(edges(text, sorted[0].index.0).0);
        let end = CCursor::new(edges(text, sorted[1].index.0).1);
        range = if range.primary.index < range.secondary.index {
            CCursorRange::two(end, start)
        } else {
            CCursorRange::two(start, end)
        };
    }
    ui.input_mut(|input| {
        input.events.retain(|event| {
            if let egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } = event
                && !modifiers.ctrl
                && !modifiers.alt
                && !modifiers.command
                && matches!(key, egui::Key::ArrowLeft | egui::Key::ArrowRight)
            {
                let sorted = range.sorted_cursors();
                let to = if !range.is_empty() && !modifiers.shift {
                    if *key == egui::Key::ArrowLeft {
                        edges(text, sorted[0].index.0).0
                    } else {
                        edges(text, sorted[1].index.0).1
                    }
                } else if *key == egui::Key::ArrowLeft {
                    bounds
                        .iter()
                        .copied()
                        .rev()
                        .find(|n| *n < range.primary.index.0)
                        .unwrap_or(0)
                } else {
                    bounds
                        .iter()
                        .copied()
                        .find(|n| *n > range.primary.index.0)
                        .unwrap_or(text.chars().count())
                };
                range = if modifiers.shift {
                    CCursorRange::two(range.secondary, CCursor::new(to))
                } else {
                    CCursorRange::one(CCursor::new(to))
                };
                return false;
            }
            true
        })
    });
    state.cursor.set_char_range(Some(range));
    state.store(ui.ctx(), id);
}
#[cfg(test)]
mod tests {
    use super::*;
    use egui::TextBuffer;
    #[test]
    fn emoji_deletion_selection_limits_and_composition_preserve_clusters() {
        let mut text = "A👩‍💻👍🏽🇺🇸B".to_string();
        let mut b = Buffer {
            value: &mut text,
            composing: false,
        };
        let cursor = b.delete_previous_char(CCursor::new(8));
        assert_eq!(b.as_str(), "A👩‍💻👍🏽B");
        assert_eq!(cursor.index.0, 6);
        let cursor = b.delete_previous_char(cursor);
        assert_eq!(b.as_str(), "A👩‍💻B");
        assert_eq!(cursor.index.0, 4);
        let cursor = b.delete_next_char(CCursor::new(2));
        assert_eq!(b.as_str(), "AB");
        assert_eq!(cursor.index.0, 1);
        b.insert_text_at(&mut CCursor::new(1), "👍🏽", 3);
        assert_eq!(b.as_str(), "AB", "limits cannot insert half an emoji");
        b.insert_text_at(&mut CCursor::new(1), "👍🏽", 4);
        assert_eq!(b.as_str(), "A👍🏽B");
        b.composing = true;
        b.delete_char_range(CharIndex(2)..CharIndex(3));
        assert_eq!(b.as_str(), "A👍B", "IME owns preedit character ranges");
    }
}
