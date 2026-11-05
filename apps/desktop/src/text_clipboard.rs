use crate::{egui, text_edit::TextDraft};
use capture_core::text::{TextStyle, clipboard, editing};

#[cfg(windows)]
fn read_format(name: &str) -> Result<Option<String>, String> {
    use clipboard_win::{Clipboard, raw};
    let _open = Clipboard::new_attempts(5).map_err(|e| e.to_string())?;
    let format = raw::register_format(name)
        .ok_or("Cannot register clipboard format")?
        .get();
    if !raw::is_format_avail(format) {
        return Ok(None);
    }
    let size = raw::size(format)
        .ok_or("Clipboard data is unavailable")?
        .get();
    if size > 1_048_576 {
        return Err("Formatted clipboard content is too large. Use Paste plain text or copy a smaller selection.".into());
    }
    let mut bytes = vec![0; size];
    let len = raw::get(format, &mut bytes).map_err(|e| e.to_string())?;
    bytes.truncate(len);
    while bytes.last() == Some(&0) {
        bytes.pop();
    }
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}
fn html_fragment(input: &str) -> &str {
    let offset = |key: &str| {
        input.lines().find_map(|line| {
            line.strip_prefix(key)
                .and_then(|s| s.trim().parse::<usize>().ok())
        })
    };
    if let (Some(a), Some(b)) = (offset("StartFragment:"), offset("EndFragment:"))
        && a <= b
        && b <= input.len()
        && input.is_char_boundary(a)
        && input.is_char_boundary(b)
    {
        return &input[a..b];
    }
    input
}
#[cfg(windows)]
fn read(base: &TextStyle) -> Result<Option<TextStyle>, String> {
    if let Some(value) = read_format("ClipCraft.RichText.1")? {
        let style: TextStyle =
            serde_json::from_str(&value).map_err(|e| format!("Cannot read formatted text: {e}"))?;
        style.validate().map_err(|e| e.to_string())?;
        return Ok(Some(style));
    }
    if let Some(value) = read_format("HTML Format")? {
        return clipboard::from_html(html_fragment(&value), base)
            .map(Some)
            .map_err(|e| e.to_string());
    }
    if let Some(value) = read_format("Rich Text Format")? {
        return clipboard::from_rtf(&value, base)
            .map(Some)
            .map_err(|e| e.to_string());
    }
    Ok(None)
}
#[cfg(not(windows))]
fn read(_base: &TextStyle) -> Result<Option<TextStyle>, String> {
    Ok(None)
}
fn write(style: &TextStyle) -> Result<(), String> {
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_html(clipboard::html(style), Some(style.content.clone())))
        .map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        use clipboard_win::{Clipboard, raw};
        let _open = Clipboard::new_attempts(5).map_err(|e| e.to_string())?;
        for (name, mut value) in [
            (
                "ClipCraft.RichText.1",
                serde_json::to_vec(style).map_err(|e| e.to_string())?,
            ),
            ("Rich Text Format", clipboard::rtf(style).into_bytes()),
        ] {
            value.push(0);
            let format = raw::register_format(name)
                .ok_or("Cannot register clipboard format")?
                .get();
            raw::set_without_clear(format, &value).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
/// Called only by a focused canvas text editor, before egui consumes clipboard
/// events. No clipboard access during rendering or when another field owns focus.
pub(super) fn events(ctx: &egui::Context, draft: &mut TextDraft) -> Result<(), String> {
    let id = egui::Id::unique(("text_content", draft.preview.id));
    if !ctx.memory(|m| m.has_focus(id)) {
        return Ok(());
    }
    let events = ctx.input_mut(|i| {
        let mut events = vec![];
        i.events.retain(|e| {
            if matches!(
                e,
                egui::Event::Copy | egui::Event::Cut | egui::Event::Paste(_)
            ) {
                events.push(e.clone());
                false
            } else {
                true
            }
        });
        events
    });
    for event in events {
        let Some(style) = draft.preview.text.as_mut() else {
            return Ok(());
        };
        let range = egui::TextEdit::load_state(ctx, id)
            .and_then(|s| s.cursor.char_range())
            .map(|r| {
                let r = r.as_sorted_char_range();
                r.start.0..r.end.0
            })
            .unwrap_or_else(|| draft.selection.clone());
        match event {
            egui::Event::Copy | egui::Event::Cut => {
                if range.is_empty() {
                    continue;
                }
                let fragment = editing::slice(style, range.clone());
                write(&fragment)?;
                if matches!(event, egui::Event::Cut) {
                    editing::replace(style, range.clone(), "").map_err(|e| e.to_string())?;
                    set_cursor(ctx, id, range.start);
                    draft.selection = range.start..range.start;
                }
            }
            egui::Event::Paste(text) => {
                let plain = ctx.input(|i| i.modifiers.shift);
                let formatted = if plain { None } else { read(style)? };
                let len = if let Some(fragment) = formatted {
                    editing::insert(style, range.clone(), &fragment).map_err(|e| e.to_string())?;
                    fragment.content.chars().count()
                } else {
                    editing::replace(style, range.clone(), &text).map_err(|e| e.to_string())?;
                    text.chars().count()
                };
                set_cursor(ctx, id, range.start + len);
                draft.selection = range.start + len..range.start + len;
            }
            _ => {}
        }
    }
    Ok(())
}
pub(super) fn set_cursor(ctx: &egui::Context, id: egui::Id, index: usize) {
    if let Some(mut state) = egui::TextEdit::load_state(ctx, id) {
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(index),
            )));
        state.store(ctx, id);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cf_html_byte_offsets_are_bounded() {
        assert_eq!(
            html_fragment("StartFragment:99999\nEndFragment:100000\n<p>text</p>"),
            "StartFragment:99999\nEndFragment:100000\n<p>text</p>"
        );
    }
}
