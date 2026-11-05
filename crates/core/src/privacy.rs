//! Local, review-assisted detection. Findings are suggestions, never a claim
//! that an image is free of sensitive information. No image or text is sent out.
use crate::{Annotation, Command, Document, Error, Kind, Result};
use std::ops::Range;
mod identifiers;

#[derive(Clone, Debug)]
pub struct Word {
    pub text: String,
    pub line: u32,
    pub bounds: [f32; 4], // x, y, width, height in source-image pixels
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Email,
    Phone,
    Phrase,
    PaymentCard,
    UsSsn,
    IpAddress,
}
impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Self::Email => "Email",
            Self::Phone => "Phone",
            Self::Phrase => "Custom phrase",
            Self::PaymentCard => "Payment card (checksum valid)",
            Self::UsSsn => "US Social Security number",
            Self::IpAddress => "IPv4 address",
        }
    }
}
#[derive(Clone, Debug)]
pub struct Finding {
    pub category: Category,
    pub text: String,
    pub bounds: Vec<[f32; 4]>,
}
#[derive(Clone, Debug)]
pub struct Options {
    pub emails: bool,
    pub phones: bool,
    pub phrases: Vec<String>,
    pub payment_cards: bool,
    pub us_ssns: bool,
    pub ip_addresses: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            emails: true,
            phones: true,
            phrases: Vec::new(),
            payment_cards: true,
            us_ssns: true,
            ip_addresses: false,
        }
    }
}
fn email_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || "._%+-@".contains(c)
}
fn email(s: &str) -> bool {
    let Some((local, domain)) = s.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && !local.starts_with('.')
        && !local.ends_with('.')
        && !local.contains("..")
        && domain.split('.').count() >= 2
        && domain.split('.').all(|p| {
            !p.is_empty()
                && !p.starts_with('-')
                && !p.ends_with('-')
                && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
        && domain
            .rsplit('.')
            .next()
            .is_some_and(|p| p.len() >= 2 && p.chars().all(|c| c.is_ascii_alphabetic()))
}
fn matches(text: &str, options: &Options) -> Vec<(Category, Range<usize>)> {
    let chars: Vec<_> = text.char_indices().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if email_char(chars[i].1) {
            let start = i;
            while i < chars.len() && email_char(chars[i].1) {
                i += 1;
            }
            let a = chars[start].0;
            let b = chars.get(i).map_or(text.len(), |p| p.0);
            let candidate = text[a..b].trim_end_matches('.');
            if options.emails && email(candidate) {
                out.push((Category::Email, a..a + candidate.len()));
            }
        } else {
            i += 1;
        }
    }
    out.extend(identifiers::find(text, options));
    if options.phones {
        let mut i = 0;
        while i < chars.len() {
            if (chars[i].1.is_ascii_digit() || chars[i].1 == '+')
                && (i == 0 || !chars[i - 1].1.is_alphanumeric())
            {
                let start = i;
                while i < chars.len()
                    && (chars[i].1.is_ascii_digit() || "+()- .".contains(chars[i].1))
                {
                    if chars[i].1 == '.'
                        && !chars.get(i + 1).is_some_and(|(_, c)| c.is_ascii_digit())
                    {
                        break;
                    }
                    i += 1;
                }
                let a = chars[start].0;
                let b = chars.get(i).map_or(text.len(), |p| p.0);
                let s = text[a..b].trim_end_matches([' ', '.', '-']);
                let digits = s.chars().filter(char::is_ascii_digit).count();
                if (10..=15).contains(&digits)
                    && !s.contains("--")
                    && s.matches('+').count() <= 1
                    && !out.iter().any(|(_, r)| r.start < a + s.len() && a < r.end)
                {
                    out.push((Category::Phone, a..a + s.len()));
                }
            } else {
                i += 1;
            }
        }
    }
    // Case folding keeps a byte-to-source map, including expanding characters.
    let mut folded = String::new();
    let mut map = Vec::new();
    for (i, c) in text.char_indices() {
        let value: String = c.to_lowercase().collect();
        map.extend(std::iter::repeat_n((i, i + c.len_utf8()), value.len()));
        folded.push_str(&value);
    }
    for phrase in &options.phrases {
        let phrase = phrase.trim().to_lowercase();
        if phrase.is_empty() {
            continue;
        }
        for (i, _) in folded.match_indices(&phrase) {
            out.push((Category::Phrase, map[i].0..map[i + phrase.len() - 1].1));
        }
    }
    out.sort_by_key(|(_, r)| (r.start, r.end));
    out.dedup_by(|a, b| a.1 == b.1);
    out
}
pub fn detect(words: &[Word], options: &Options, dimensions: [u32; 2]) -> Result<Vec<Finding>> {
    if words.len() > 32768
        || options.phrases.len() > 100
        || options.phrases.iter().any(|p| p.len() > 512)
    {
        return Err(Error::Invalid(
            "Use at most 100 phrases of up to 512 bytes; scan a smaller page if needed.".into(),
        ));
    }
    let mut text = String::new();
    let mut ranges = Vec::new();
    let mut previous = None;
    for word in words {
        if word.bounds.iter().any(|v| !v.is_finite())
            || word.bounds[2] <= 0.
            || word.bounds[3] <= 0.
        {
            continue;
        }
        if !text.is_empty() {
            text.push(if previous == Some(word.line) {
                ' '
            } else {
                '\n'
            });
        }
        let start = text.len();
        text.push_str(&word.text);
        ranges.push((start..text.len(), word));
        previous = Some(word.line);
        if text.len() > 262144 {
            return Err(Error::Invalid(
                "Scan a smaller page: too much recognized text.".into(),
            ));
        }
    }
    let mut findings = Vec::new();
    for (category, range) in matches(&text, options) {
        let mut boxes: Vec<(u32, [f32; 4])> = Vec::new();
        for (r, w) in &ranges {
            if r.start >= range.end || r.end <= range.start {
                continue;
            }
            let b = [
                w.bounds[0],
                w.bounds[1],
                w.bounds[0] + w.bounds[2],
                w.bounds[1] + w.bounds[3],
            ];
            if let Some((_, last)) = boxes.last_mut().filter(|(line, _)| *line == w.line) {
                last[0] = last[0].min(b[0]);
                last[1] = last[1].min(b[1]);
                last[2] = last[2].max(b[2]);
                last[3] = last[3].max(b[3]);
            } else {
                boxes.push((w.line, b));
            }
        }
        let bounds = boxes
            .into_iter()
            .filter_map(|(_, b)| {
                let lo = [0, 1].map(|i| (b[i] - 2.).floor().clamp(0., dimensions[i] as f32));
                let hi = [0, 1].map(|i| (b[i + 2] + 2.).ceil().clamp(0., dimensions[i] as f32));
                (hi[0] > lo[0] && hi[1] > lo[1]).then_some([
                    lo[0],
                    lo[1],
                    hi[0] - lo[0],
                    hi[1] - lo[1],
                ])
            })
            .collect::<Vec<_>>();
        if !bounds.is_empty() {
            findings.push(Finding {
                category,
                text: text[range].to_string(),
                bounds,
            });
        }
        if findings.len() >= 2048 {
            return Err(Error::Invalid(
                "More than 2048 findings: scan a smaller page.".into(),
            ));
        }
    }
    Ok(findings)
}
pub fn redact(document: &Document, findings: impl IntoIterator<Item = Finding>) -> Command {
    let mut id = document.annotations.iter().map(|a| a.id).max().unwrap_or(0);
    let mut annotations = Vec::new();
    for finding in findings {
        for b in finding.bounds {
            id += 1;
            annotations.push(Annotation {
                id,
                kind: Kind::Redact,
                start: [b[0], b[1]],
                end: [b[0] + b[2], b[1] + b[3]],
                color: [0, 0, 0, 255],
                width: 1,
                focus: None,
                highlighter_rect: false,
                image: None,
                step_outline: None,
                line_style: None,
                shadow: None,
                rotation_degrees: 0.,
                mirror_x: false,
                group: None,
                corner_radius: 0.,
                fill: None,
                points: vec![],
                text: None,
                callout: None,
            });
        }
    }
    Command::AddMany { annotations }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rules_cover_email_phone_and_unicode_phrases_without_dates() {
        let o = Options {
            phrases: vec!["CLIENT secret".into(), "İD".into()],
            ..Default::default()
        };
        let text =
            "Contact a.person+tag@example.co.uk or +1 (312) 555-0182. 2026-11-02 CLIENT secret İD";
        let m = matches(text, &o);
        assert_eq!(m.len(), 4);
        assert_eq!(&text[m[0].1.clone()], "a.person+tag@example.co.uk");
        assert_eq!(&text[m[1].1.clone()], "+1 (312) 555-0182");
    }
    #[test]
    fn bounds_cover_whole_words_clip_and_merge_per_line() {
        let words = vec![
            Word {
                text: "Account".into(),
                line: 0,
                bounds: [1., 5., 30., 10.],
            },
            Word {
                text: "secret".into(),
                line: 0,
                bounds: [40., 5., 30., 10.],
            },
        ];
        let f = detect(
            &words,
            &Options {
                emails: false,
                phones: false,
                phrases: vec!["account secret".into()],
                ..Default::default()
            },
            [72, 20],
        )
        .unwrap();
        assert_eq!(f[0].bounds, vec![[0., 3., 72., 14.]]);
        assert!(
            detect(
                &words,
                &Options {
                    phrases: vec!["x".repeat(513)],
                    ..Default::default()
                },
                [72, 20]
            )
            .is_err()
        );
    }
}
