use super::*;
use capture_core::text::{TextStyle, recognition::Appearance};
use std::ops::Range;

#[derive(Clone, Debug)]
pub(super) struct Span {
    pub range: Range<usize>,
    pub appearance: Appearance,
}

impl Recognized {
    /// Retain formatting around an edit, including UTF-8 insertions and deletes.
    pub(super) fn edited_spans(&self, new: &str) -> Vec<Span> {
        let old = &self.text;
        let prefix = old
            .chars()
            .zip(new.chars())
            .take_while(|(a, b)| a == b)
            .map(|(a, _)| a.len_utf8())
            .sum::<usize>();
        let suffix = old[prefix..]
            .chars()
            .rev()
            .zip(new[prefix..].chars().rev())
            .take_while(|(a, b)| a == b)
            .map(|(a, _)| a.len_utf8())
            .sum::<usize>();
        let mut spans = Vec::new();
        for span in &self.spans {
            let end = span.range.end.min(prefix);
            if span.range.start < end {
                spans.push(Span {
                    range: span.range.start..end,
                    ..span.clone()
                });
            }
        }
        if new.len() - suffix > prefix
            && let Some(span) = self
                .spans
                .iter()
                .find(|s| s.range.start <= prefix && s.range.end >= prefix)
                .or_else(|| self.spans.first())
        {
            spans.push(Span {
                range: prefix..new.len() - suffix,
                ..span.clone()
            });
        }
        for span in &self.spans {
            let start = span.range.start.max(old.len() - suffix);
            if start < span.range.end {
                spans.push(Span {
                    range: new.len() - (old.len() - start)
                        ..new.len() - (old.len() - span.range.end),
                    ..span.clone()
                });
            }
        }
        spans
    }
    pub(super) fn job(&self, text: &str, plain: bool, width: f32) -> egui::text::LayoutJob {
        let mut job = egui::text::LayoutJob::default();
        job.wrap.max_width = width;
        let default = egui::TextFormat {
            font_id: egui::FontId::proportional(18.),
            color: egui::Color32::BLACK,
            ..Default::default()
        };
        if plain {
            job.append(text, 0., default);
            return job;
        }
        let mut offset = 0;
        for span in self.edited_spans(text) {
            if span.range.start > offset {
                job.append(&text[offset..span.range.start], 0., default.clone());
            }
            let style = &span.appearance.style;
            let c = style.color.unwrap_or([0, 0, 0, 255]);
            // Light source lettering remains readable on the white results page.
            let background = if u16::from(c[0]) + u16::from(c[1]) + u16::from(c[2]) > 560 {
                let b = span.appearance.background;
                egui::Color32::from_rgb(b[0], b[1], b[2])
            } else {
                egui::Color32::TRANSPARENT
            };
            job.append(
                &text[span.range.clone()],
                0.,
                egui::TextFormat {
                    font_id: egui::FontId::new(
                        style.size_points * 4. / 3.,
                        crate::text_layout::family(style),
                    ),
                    color: egui::Color32::from_rgb(c[0], c[1], c[2]),
                    background,
                    ..Default::default()
                },
            );
            offset = span.range.end;
        }
        if offset < text.len() {
            job.append(&text[offset..], 0., default);
        }
        job
    }
    pub(super) fn html(&self) -> String {
        let mut output = String::from(
            "<div style=\"white-space:pre-wrap;background-color:#ffffff;color:#000000\">",
        );
        let mut offset = 0;
        for span in &self.spans {
            escape(&self.text[offset..span.range.start], &mut output);
            let TextStyle {
                font,
                bold,
                italic,
                size_points,
                color,
                ..
            } = &span.appearance.style;
            let c = color.unwrap_or([0, 0, 0, 255]);
            let bg = span.appearance.background;
            output.push_str(&format!("<span style=\"font-family:'{}',{};font-size:{:.1}px;font-weight:{};font-style:{};color:#{:02x}{:02x}{:02x};{}\">",font.label(),if *font==capture_core::text::TextFont::HackRegular {"monospace"}else{"sans-serif"},size_points*4./3.,if *bold {700}else{400},if *italic {"italic"}else{"normal"},c[0],c[1],c[2],if u16::from(c[0])+u16::from(c[1])+u16::from(c[2])>560 {format!("background-color:#{:02x}{:02x}{:02x};",bg[0],bg[1],bg[2])}else{String::new()}));
            escape(&self.text[span.range.clone()], &mut output);
            output.push_str("</span>");
            offset = span.range.end;
        }
        escape(&self.text[offset..], &mut output);
        output.push_str("</div>");
        output
    }
}
fn escape(text: &str, out: &mut String) {
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("<br>"),
            _ => out.push(ch),
        }
    }
}
