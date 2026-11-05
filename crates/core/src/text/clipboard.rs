//! Bounded interchange for text only. Ignores scripts, images, embedded objects,
//! external stylesheets and unknown destinations; never resolves external URLs.
use super::{
    TextStyle, editing,
    rich::{self, Format, RichText, Run},
};
use crate::{Error, Result};
fn escaped(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn decoded(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        if let Some(end) = rest.find(';').filter(|n| *n < 16) {
            let entity = &rest[1..end];
            let c = match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some(' '),
                _ => entity
                    .strip_prefix("#x")
                    .and_then(|n| u32::from_str_radix(n, 16).ok())
                    .or_else(|| entity.strip_prefix('#').and_then(|n| n.parse().ok()))
                    .and_then(char::from_u32),
            };
            if let Some(c) = c {
                out.push(c);
                rest = &rest[end + 1..];
                continue;
            }
        }
        out.push('&');
        rest = &rest[1..];
    }
    out.push_str(rest);
    out
}
fn color(s: &str) -> Option<[u8; 4]> {
    let s = s.trim().trim_start_matches('#');
    if s.len() == 6 && s.is_ascii() {
        Some([
            u8::from_str_radix(&s[..2], 16).ok()?,
            u8::from_str_radix(&s[2..4], 16).ok()?,
            u8::from_str_radix(&s[4..], 16).ok()?,
            255,
        ])
    } else {
        match s.to_ascii_lowercase().as_str() {
            "red" => Some([255, 0, 0, 255]),
            "blue" => Some([0, 0, 255, 255]),
            "black" => Some([0, 0, 0, 255]),
            "white" => Some([255; 4]),
            _ => None,
        }
    }
}
fn attributes(tag: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = tag.trim();
    while !rest.is_empty() {
        let end = rest
            .find(|c: char| c.is_whitespace() || c == '=')
            .unwrap_or(rest.len());
        let key = &rest[..end];
        rest = rest[end..].trim_start();
        if let Some(value) = rest.strip_prefix('=') {
            rest = value.trim_start();
            let (v, next) = if rest.starts_with(['\'', '"']) {
                let q = rest.as_bytes()[0] as char;
                let e = rest[1..].find(q).map_or(rest.len(), |i| i + 1);
                (&rest[1..e], &rest[(e + 1).min(rest.len())..])
            } else {
                let e = rest.find(char::is_whitespace).unwrap_or(rest.len());
                (&rest[..e], &rest[e..])
            };
            out.push((key.to_ascii_lowercase(), decoded(v)));
            rest = next.trim_start();
        } else if end == 0 {
            rest = &rest[1..];
        }
    }
    out
}
fn font(name: &str) -> Option<super::TextFont> {
    let name = name.trim().trim_matches(['\'', '"']);
    [super::TextFont::UbuntuLight, super::TextFont::HackRegular]
        .into_iter()
        .chain(super::system_fonts::choices())
        .find(|f| f.label().eq_ignore_ascii_case(name))
}
fn css(f: &mut Format, value: &str) {
    for item in value.split(';') {
        let Some((key, v)) = item.split_once(':') else {
            continue;
        };
        let v = v.trim();
        match key.trim().to_ascii_lowercase().as_str() {
            "font-weight" => f.bold = v == "bold" || v.parse::<u16>().is_ok_and(|n| n >= 600),
            "font-style" => f.italic = v == "italic",
            "text-decoration" => {
                f.underline = v.contains("underline");
                f.strike = v.contains("line-through");
            }
            "color" => {
                if let Some(c) = color(v) {
                    f.color = Some(c);
                }
            }
            "background-color" => f.typography.highlight = color(v),
            "font-family" => {
                if let Some(font) = v.split(',').find_map(font) {
                    f.font = font;
                }
            }
            "font-size" => {
                let px = v.ends_with("px");
                if let Ok(n) = v.trim_end_matches(['p', 't', 'x']).parse::<f32>() {
                    f.size = (n * if px { 0.75 } else { 1. }).clamp(6., 144.);
                }
            }
            _ => {}
        }
    }
}
fn paragraph_css(p: &mut rich::Paragraph, value: &str) {
    for item in value.split(';') {
        let Some((key, v)) = item.split_once(':') else {
            continue;
        };
        let v = v.trim();
        let px = || {
            v.trim_end_matches("px")
                .parse::<f32>()
                .ok()
                .filter(|v| v.is_finite())
        };
        match key.trim() {
            "text-align" => {
                p.alignment = match v {
                    "center" => rich::Alignment::Center,
                    "right" => rich::Alignment::Right,
                    "justify" => rich::Alignment::Justify,
                    _ => rich::Alignment::Left,
                }
            }
            "line-height" => {
                if let Some(n) = px() {
                    p.line_spacing = n.clamp(0.5, 4.);
                }
            }
            "margin-top" => {
                if let Some(n) = px() {
                    p.before = n.clamp(0., 512.);
                }
            }
            "margin-bottom" => {
                if let Some(n) = px() {
                    p.after = n.clamp(0., 512.);
                }
            }
            "margin-left" => {
                if let Some(n) = px() {
                    p.indent = n.clamp(0., 512.);
                }
            }
            "text-indent" => {
                if let Some(n) = px() {
                    p.first_indent = n.clamp(-512., 512.);
                }
            }
            _ => {}
        }
    }
}
fn push(s: &mut TextStyle, text: &str, f: &Format) -> Result<()> {
    let start = s.content.chars().count();
    let text: String = text
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .collect();
    if start + text.chars().count() > super::MAX_TEXT_CHARS {
        return Err(Error::Invalid(
            "Pasted text exceeds 4096 characters. Paste a smaller selection.".into(),
        ));
    }
    s.content.push_str(&text);
    let end = s.content.chars().count();
    if start == end {
        return Ok(());
    }
    let runs = &mut s.rich.get_or_insert_with(Default::default).runs;
    if let Some(r) = runs.last_mut().filter(|r| r.end == start && r.format == *f) {
        r.end = end;
    } else {
        runs.push(Run {
            start,
            end,
            format: f.clone(),
        });
    }
    Ok(())
}
pub fn html(s: &TextStyle) -> String {
    let mut out = String::from("<div>");
    let mut index = 0;
    let mut numbers = [0_u16; 9];
    for para in s.content.split('\n') {
        let p = s
            .rich
            .as_ref()
            .and_then(|r| r.paragraphs.iter().find(|p| p.start == index));
        let extra=p.map(|p|format!("text-align:{};margin-top:{}px;margin-bottom:{}px;margin-left:{}px;text-indent:{}px;line-height:{};",match p.alignment{rich::Alignment::Left=>"left",rich::Alignment::Center=>"center",rich::Alignment::Right=>"right",rich::Alignment::Justify=>"justify"},p.before,p.after,p.indent,p.first_indent,p.line_spacing)).unwrap_or_default();
        let list = p.filter(|p| p.list != rich::List::None);
        if list.is_none() {
            numbers.fill(0);
        }
        if let Some(p) = list {
            let n = &mut numbers[p.level as usize];
            *n = if p.continue_numbering && *n > 0 {
                n.saturating_add(1)
            } else {
                p.number
            };
            let number = *n;
            numbers[p.level as usize + 1..].fill(0);
            out.push_str(&format!(
                "<{} start=\"{}\">",
                if p.list == rich::List::Numbered {
                    "ol"
                } else {
                    "ul"
                },
                number
            ));
        }
        let tag = if list.is_some() { "li" } else { "p" };
        out.push_str(&format!("<{tag} style=\"white-space:pre-wrap;{extra}\">"));
        let chars: Vec<_> = para.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let f = rich::format_at(s, index + i);
            let mut j = i + 1;
            while j < chars.len() && rich::format_at(s, index + j) == f {
                j += 1;
            }
            let color = f.color.unwrap_or([35, 35, 35, 255]);
            let decoration = match (f.underline, f.strike) {
                (true, true) => "underline line-through",
                (true, false) => "underline",
                (false, true) => "line-through",
                _ => "none",
            };
            if let Some(link) = &f.link {
                out.push_str(&format!("<a href=\"{}\">", escaped(link)));
            }
            let highlight = f
                .typography
                .highlight
                .map(|c| format!("background-color:#{:02x}{:02x}{:02x};", c[0], c[1], c[2]))
                .unwrap_or_default();
            out.push_str(&format!("<span style=\"{highlight}font-family:{};font-size:{}pt;font-weight:{};font-style:{};color:#{:02x}{:02x}{:02x};text-decoration:{}\">{}</span>",escaped(f.font.label()),f.size,if f.bold{"bold"}else{"normal"},if f.italic{"italic"}else{"normal"},color[0],color[1],color[2],decoration,escaped(&chars[i..j].iter().collect::<String>())));
            if f.link.is_some() {
                out.push_str("</a>");
            }
            i = j;
        }
        out.push_str(&format!("</{tag}>"));
        if let Some(p) = list {
            out.push_str(if p.list == rich::List::Numbered {
                "</ol>"
            } else {
                "</ul>"
            });
        }
        index += chars.len() + 1;
    }
    out.push_str("</div>");
    out
}
pub fn from_html(input: &str, base: &TextStyle) -> Result<TextStyle> {
    if input.len() > 1_048_576 {
        return Err(Error::Invalid(
            "Formatted clipboard content is too large.".into(),
        ));
    }
    let input = if let Some(start) = input.find("<!--StartFragment-->") {
        let rest = &input[start + 20..];
        rest.split("<!--EndFragment-->").next().unwrap_or(rest)
    } else {
        input
    };
    let mut out = base.clone();
    out.content.clear();
    out.rich = Some(RichText::default());
    let mut format = Format::from_style(base);
    let mut stack: Vec<(String, Format)> = Vec::new();
    let mut rest = input;
    let mut skip: Option<String> = None;
    let mut lists = Vec::new();
    while !rest.is_empty() {
        if rest.starts_with('<') {
            let Some(end) = rest.find('>') else {
                break;
            };
            let tag = rest[1..end].trim();
            rest = &rest[end + 1..];
            let close = tag.starts_with('/');
            let tag = tag.trim_start_matches('/');
            let name = tag
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_end_matches('/')
                .to_ascii_lowercase();
            if let Some(skipped) = &skip {
                if close && &name == skipped {
                    skip = None;
                }
                continue;
            }
            if matches!(
                name.as_str(),
                "script" | "style" | "object" | "iframe" | "head"
            ) {
                if !close {
                    skip = Some(name);
                }
                continue;
            }
            if close {
                if name == "ul" || name == "ol" {
                    lists.pop();
                }
                if let Some(i) = stack.iter().rposition(|(n, _)| *n == name) {
                    format = stack[i].1.clone();
                    stack.truncate(i);
                }
                if matches!(name.as_str(), "p" | "div" | "li" | "h1" | "h2" | "h3")
                    && !out.content.is_empty()
                    && !out.content.ends_with('\n')
                {
                    push(&mut out, "\n", &format)?;
                }
                continue;
            }
            if name == "br" {
                push(&mut out, "\n", &format)?;
                continue;
            }
            if !name.is_ascii() {
                continue;
            }
            if name == "ul" || name == "ol" {
                if lists.len() < 8 {
                    let number = attributes(&tag[name.len()..])
                        .into_iter()
                        .find(|(k, _)| k == "start")
                        .and_then(|(_, v)| v.parse::<u16>().ok())
                        .unwrap_or(1)
                        .max(1);
                    lists.push((
                        if name == "ol" {
                            rich::List::Numbered
                        } else {
                            rich::List::Bullet
                        },
                        number,
                    ));
                }
                continue;
            }
            if [
                "b", "strong", "i", "em", "u", "s", "strike", "span", "a", "p", "div", "li", "h1",
                "h2", "h3", "font",
            ]
            .contains(&name.as_str())
            {
                if stack.len() >= 64 {
                    return Err(Error::Invalid(
                        "Clipboard formatting is nested too deeply.".into(),
                    ));
                }
                if ["p", "li", "h1", "h2", "h3"].contains(&name.as_str())
                    && !out.content.is_empty()
                    && !out.content.ends_with('\n')
                {
                    push(&mut out, "\n", &format)?;
                }
                stack.push((name.clone(), format.clone()));
                match name.as_str() {
                    "b" | "strong" => format.bold = true,
                    "i" | "em" => format.italic = true,
                    "u" => format.underline = true,
                    "s" | "strike" => format.strike = true,
                    "h1" => {
                        format.bold = true;
                        format.size = 36.;
                    }
                    "h2" => {
                        format.bold = true;
                        format.size = 28.;
                    }
                    _ => {}
                }
                let attributes = attributes(&tag[name.len()..]);
                if ["p", "li", "h1", "h2", "h3"].contains(&name.as_str()) {
                    let mut p = rich::Paragraph::for_style(base, out.content.chars().count());
                    if name == "li" {
                        let (list, number) =
                            lists.last().copied().unwrap_or((rich::List::Bullet, 1));
                        p.list = list;
                        p.number = number;
                        p.continue_numbering = false;
                        if let Some((_, n)) = lists.last_mut() {
                            *n = n.saturating_add(1);
                        }
                        p.level = lists.len().saturating_sub(1) as u8;
                    }
                    for (key, value) in &attributes {
                        if key == "style" {
                            paragraph_css(&mut p, value);
                        }
                    }
                    let paragraphs = &mut out.rich.as_mut().unwrap().paragraphs;
                    paragraphs.retain(|old| old.start != p.start);
                    paragraphs.push(p);
                }
                for (key, value) in attributes {
                    match key.as_str() {
                        "style" => css(&mut format, &value),
                        "href" if rich::safe_link(&value) => {
                            format.link = Some(value);
                            format.underline = true;
                        }
                        "color" => format.color = color(&value),
                        "face" => {
                            if let Some(f) = font(&value) {
                                format.font = f;
                            }
                        }
                        _ => {}
                    }
                }
            }
        } else {
            let end = rest.find('<').unwrap_or(rest.len());
            if skip.is_none() {
                push(&mut out, &decoded(&rest[..end]), &format)?;
            }
            rest = &rest[end..];
        }
    }
    if out.content.ends_with('\n') {
        let n = out.content.chars().count();
        editing::replace(&mut out, n - 1..n, "")?;
    }
    out.validate()?;
    Ok(out)
}
pub fn rtf(s: &TextStyle) -> String {
    let mut fonts = Vec::new();
    let mut colors = Vec::new();
    for i in 0..s.content.chars().count() {
        let f = rich::format_at(s, i);
        if !fonts.contains(&f.font) {
            fonts.push(f.font);
        }
        let c = f.color.unwrap_or([35, 35, 35, 255]);
        if !colors.contains(&c) {
            colors.push(c);
        }
    }
    let mut out = String::from("{\\rtf1\\ansi\\uc1{\\fonttbl");
    for (i, f) in fonts.iter().enumerate() {
        out.push_str(&format!(
            "{{\\f{i} {};}}",
            f.label().replace(['{', '}', '\\'], "")
        ));
    }
    out.push_str("}{\\colortbl;");
    for c in &colors {
        out.push_str(&format!("\\red{}\\green{}\\blue{};", c[0], c[1], c[2]));
    }
    out.push('}');
    for (i, c) in s.content.chars().enumerate() {
        let f = rich::format_at(s, i);
        let color = f.color.unwrap_or([35, 35, 35, 255]);
        out.push_str(&format!(
            "{{\\f{}\\fs{}\\b{}\\i{}\\ul{}\\strike{}\\cf{} ",
            fonts.iter().position(|v| *v == f.font).unwrap(),
            (f.size * 2.).round() as u32,
            f.bold as u8,
            f.italic as u8,
            f.underline as u8,
            f.strike as u8,
            colors.iter().position(|v| *v == color).unwrap() + 1
        ));
        match c {
            '\n' => out.push_str("\\par "),
            '\t' => out.push_str("\\tab "),
            '{' | '}' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            c if c.is_ascii() => out.push(c),
            _ => {
                for u in c.encode_utf16(&mut [0; 2]) {
                    out.push_str(&format!("\\u{}?", *u as i16));
                }
            }
        }
        out.push('}');
    }
    out.push('}');
    out
}
pub fn from_rtf(input: &str, base: &TextStyle) -> Result<TextStyle> {
    if input.len() > 1_048_576 || !input.starts_with("{\\rtf") {
        return Err(Error::Invalid(
            "Invalid or oversized rich-text clipboard content.".into(),
        ));
    }
    let mut out = base.clone();
    out.content.clear();
    out.rich = Some(RichText::default());
    let table = |name: &str| -> &str {
        let Some(a) = input.find(&format!("{{\\{name}")) else {
            return "";
        };
        let mut depth = 0;
        let mut escape = false;
        for (i, c) in input[a..].char_indices() {
            if escape {
                escape = false;
                continue;
            }
            if c == '\\' {
                escape = true;
            } else if c == '{' {
                depth += 1;
            } else if c == '}' {
                depth -= 1;
                if depth == 0 {
                    return &input[a..a + i + 1];
                }
            }
        }
        ""
    };
    let value = |s: &str, key: &str| -> Option<i32> {
        let a = s.find(&format!("\\{key}"))? + key.len() + 1;
        let tail = &s[a..];
        let n = tail
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>();
        n.parse().ok()
    };
    let colors: Vec<_> = table("colortbl")
        .split(';')
        .map(|s| {
            Some([
                value(s, "red")? as u8,
                value(s, "green")? as u8,
                value(s, "blue")? as u8,
                255,
            ])
        })
        .collect();
    let fonts: Vec<_> = table("fonttbl")
        .split(';')
        .filter_map(|s| {
            let id = value(s, "f")?;
            // The final control word ends at its delimiter; retain multiword family names.
            let tail = s.rsplit('\\').next()?;
            let name = tail.split_once(' ')?.1.trim().trim_matches(['{', '}']);
            Some((id, font(name)?))
        })
        .collect();
    let mut f = Format::from_style(base);
    let mut skip = false;
    let mut stack = Vec::new();
    let b = input.as_bytes();
    let mut i = 0;
    let mut uc = 1usize;
    let mut fallback = 0;
    let mut surrogate = None;
    while i < b.len() {
        match b[i] {
            b'{' => {
                if stack.len() >= 64 {
                    return Err(Error::Invalid(
                        "RTF formatting is nested too deeply.".into(),
                    ));
                }
                stack.push((f.clone(), skip, uc));
                i += 1;
            }
            b'}' => {
                if let Some((a, s, u)) = stack.pop() {
                    f = a;
                    skip = s;
                    uc = u;
                }
                i += 1;
            }
            b'\\' => {
                i += 1;
                if i >= b.len() {
                    break;
                }
                if b[i] == b'*' {
                    skip = true;
                    i += 1;
                    continue;
                }
                if b"\\{}".contains(&b[i]) {
                    if fallback > 0 {
                        fallback -= 1;
                    } else if !skip {
                        push(&mut out, &(b[i] as char).to_string(), &f)?;
                    }
                    i += 1;
                    continue;
                }
                if b[i] == b'\''
                    && i + 2 < b.len()
                    && b[i + 1].is_ascii_hexdigit()
                    && b[i + 2].is_ascii_hexdigit()
                {
                    if let Ok(v) = u8::from_str_radix(&input[i + 1..i + 3], 16) {
                        if fallback > 0 {
                            fallback -= 1;
                        } else if !skip {
                            push(&mut out, &char::from(v).to_string(), &f)?;
                        }
                    }
                    i += 3;
                    continue;
                }
                let start = i;
                while i < b.len() && b[i].is_ascii_alphabetic() {
                    i += 1;
                }
                let word = &input[start..i];
                let start = i;
                if i < b.len() && b[i] == b'-' {
                    i += 1;
                }
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
                let number = input[start..i].parse::<i32>().ok();
                if i < b.len() && b[i] == b' ' {
                    i += 1;
                }
                if word.is_empty() {
                    if i < b.len() {
                        i += input[i..].chars().next().unwrap().len_utf8();
                    }
                    continue;
                }
                if [
                    "fonttbl",
                    "colortbl",
                    "stylesheet",
                    "info",
                    "pict",
                    "object",
                    "fldinst",
                    "header",
                    "footer",
                ]
                .contains(&word)
                {
                    skip = true;
                    continue;
                }
                if word == "bin" {
                    let n = number.unwrap_or(0).max(0) as usize;
                    i = (i + n).min(b.len());
                    while !input.is_char_boundary(i) {
                        i += 1;
                    }
                    continue;
                }
                if skip {
                    continue;
                }
                match word {
                    "f" => {
                        if let Some((_, font)) = fonts.iter().find(|(i, _)| Some(*i) == number) {
                            f.font = *font;
                        }
                    }
                    "cf" => {
                        f.color = number
                            .and_then(|i| colors.get(i as usize))
                            .copied()
                            .flatten();
                    }
                    "highlight" => {
                        f.typography.highlight = number
                            .and_then(|i| colors.get(i as usize))
                            .copied()
                            .flatten();
                    }
                    "b" => f.bold = number != Some(0),
                    "i" => f.italic = number != Some(0),
                    "ul" => f.underline = number != Some(0),
                    "ulnone" => f.underline = false,
                    "strike" => f.strike = number != Some(0),
                    "fs" => f.size = (number.unwrap_or(36) as f32 / 2.).clamp(6., 144.),
                    "plain" => f = Format::from_style(base),
                    "uc" => uc = number.unwrap_or(1).clamp(0, 16) as usize,
                    "par" | "line" => push(&mut out, "\n", &f)?,
                    "tab" => push(&mut out, "\t", &f)?,
                    "u" => {
                        let u = number.unwrap_or(0) as u16;
                        if (0xd800..=0xdbff).contains(&u) {
                            surrogate = Some(u);
                        } else {
                            let value = if let Some(hi) = surrogate.take() {
                                0x10000
                                    + ((hi as u32 - 0xd800) << 10)
                                    + (u as u32).saturating_sub(0xdc00)
                            } else {
                                u as u32
                            };
                            if let Some(c) = char::from_u32(value) {
                                push(&mut out, &c.to_string(), &f)?;
                            }
                        }
                        fallback = uc;
                    }
                    _ => {}
                }
            }
            _ => {
                let c = input[i..].chars().next().unwrap();
                i += c.len_utf8();
                if fallback > 0 {
                    fallback -= 1;
                } else if !skip && c != '\r' && c != '\n' {
                    push(&mut out, &c.to_string(), &f)?;
                }
            }
        }
    }
    out.validate()?;
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn html_only_imports_text_and_safe_formatting() {
        let base = TextStyle::default();
        let s=from_html("<p>Hello <b>café</b><script>secret()</script><img src='https://example.com'><a href='javascript:x'> link</a></p>",&base).unwrap();
        assert_eq!(s.content, "Hello café link");
        assert!(rich::format_at(&s, 6).bold);
        assert!(rich::format_at(&s, 11).link.is_none());
    }
    #[test]
    fn rtf_unicode_and_formatting_roundtrip() {
        let mut s = TextStyle {
            content: "café\nBold".into(),
            ..Default::default()
        };
        rich::format_range(&mut s, 5..9, |f| f.bold = true);
        let copy = from_rtf(&rtf(&s), &TextStyle::default()).unwrap();
        assert_eq!(copy.content, s.content);
        assert!(rich::format_at(&copy, 5).bold);
    }
}
