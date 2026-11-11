//! Local, review-assisted detection. Findings are suggestions, never a claim
//! that an image is free of sensitive information. No image or text is sent out.
use crate::{Annotation, Command, Document, Error, Kind, Result};
use std::ops::Range;
pub mod custom;
mod identifiers;
pub mod scan;
#[cfg(test)]
mod scan_stress;
pub mod stable;
pub mod tracking;

#[derive(Clone, Debug)]
pub struct Word {
    pub text: String,
    pub line: u32,
    pub bounds: [f32; 4], // x, y, width, height in source-image pixels
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Name,
    UserId,
    Email,
    Phone,
    Phrase,
    PaymentCard,
    UsSsn,
    IpAddress,
    /// A saved custom type, by its index in `Options::custom`.
    Custom(u8),
}
impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Self::Name => "Possible person name",
            Self::UserId => "User ID",
            Self::Email => "Email",
            Self::Phone => "Phone",
            Self::Phrase => "Custom phrase",
            Self::PaymentCard => "Payment card (checksum valid)",
            Self::UsSsn => "US Social Security number",
            Self::IpAddress => "IPv4 address",
            Self::Custom(_) => "Custom type",
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
    pub names: bool,
    pub user_ids: bool,
    pub emails: bool,
    pub phones: bool,
    pub phrases: Vec<String>,
    pub payment_cards: bool,
    pub us_ssns: bool,
    pub ip_addresses: bool,
    /// Custom types to look for (approved beta 5 tab 3); disabled ones are skipped.
    pub custom: Vec<custom::CustomType>,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            names: true,
            user_ids: true,
            emails: true,
            phones: true,
            phrases: Vec::new(),
            payment_cards: true,
            us_ssns: true,
            ip_addresses: false,
            custom: Vec::new(),
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
/// An address cut off before its top-level domain, as in a narrow window tab or
/// title bar (`name@exampled`). It needs a local part, so `@mentions` don't match.
fn partial_email(s: &str) -> bool {
    let Some((local, domain)) = s.split_once('@') else {
        return false;
    };
    local.len() >= 2
        && !local.starts_with('.')
        && !domain.contains('@')
        && domain.len() >= 3
        && domain
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        && domain
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
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
            if options.emails && (email(candidate) || partial_email(candidate)) {
                out.push((Category::Email, a..a + candidate.len()));
            }
        } else {
            i += 1;
        }
    }
    if options.names || options.user_ids {
        let words: Vec<_> = text
            .split_whitespace()
            .scan(0usize, |offset, w| {
                let start = text[*offset..].find(w)? + *offset;
                *offset = start + w.len();
                Some((start, w))
            })
            .collect();
        for (i, (at, w)) in words.iter().enumerate() {
            let key = w.trim_matches([':', '=', '"']).to_lowercase();
            let name_label = matches!(
                key.as_str(),
                "name" | "patient" | "employee" | "customer" | "author"
            );
            let id_label = matches!(
                key.as_str(),
                "username" | "userid" | "user_id" | "login" | "handle" | "account"
            );
            if (options.names && name_label || options.user_ids && id_label)
                && let Some((start, value)) = words.get(i + 1)
            {
                let mut end = start + value.len();
                if name_label
                    && let Some((next, value)) = words.get(i + 2)
                    && value.chars().next().is_some_and(char::is_uppercase)
                {
                    end = next + value.len();
                }
                out.push((
                    if name_label {
                        Category::Name
                    } else {
                        Category::UserId
                    },
                    *start..end,
                ));
            }
            if options.user_ids
                && w.starts_with('@')
                && w.len() > 2
                && w[1..]
                    .chars()
                    .all(|c| c.is_alphanumeric() || "_.-".contains(c))
            {
                out.push((Category::UserId, *at..at + w.len()));
            }
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
    for (i, t) in options.custom.iter().enumerate().filter(|(_, t)| t.enabled) {
        if let Ok(found) = t.find(text) {
            out.extend(found.into_iter().map(|r| (Category::Custom(i as u8), r)));
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
    custom::validate_all(&options.custom)?;
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
/// A cover of `kind` for each box, numbered after `last_id`.
pub fn covers(
    last_id: u64,
    boxes: impl IntoIterator<Item = [f32; 4]>,
    kind: Kind,
    width: u32,
) -> Vec<Annotation> {
    boxes
        .into_iter()
        .enumerate()
        .map(|(i, b)| Annotation {
            id: last_id + i as u64 + 1,
            kind,
            start: [b[0], b[1]],
            end: [b[0] + b[2], b[1] + b[3]],
            color: [0, 0, 0, 255],
            width,
            focus: None,
            highlighter_rect: false,
            hard_edge: false,
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
        })
        .collect()
}
/// Covers every finding, in one Undo step.
pub fn cover(
    document: &Document,
    findings: impl IntoIterator<Item = Finding>,
    kind: Kind,
    width: u32,
) -> Command {
    let last = document.annotations.iter().map(|a| a.id).max().unwrap_or(0);
    let boxes = findings.into_iter().flat_map(|f| f.bounds);
    Command::AddMany {
        annotations: covers(last, boxes, kind, width),
    }
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
    fn addresses_cut_off_in_a_window_tab_are_still_found() {
        let o = Options::default();
        let text = "so the reality is that jo@exampled";
        let m = matches(text, &o);
        assert_eq!(m.len(), 1);
        assert_eq!(&text[m[0].1.clone()], "jo@exampled");
        // Mentions and lone @ signs are not addresses (mentions are user IDs).
        let emails_only = Options {
            names: false,
            user_ids: false,
            ..Default::default()
        };
        assert!(matches("ping @team at 5 @ noon", &emails_only).is_empty());
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

#[cfg(test)]
mod custom_detect_tests {
    use super::*;
    #[test]
    fn custom_types_are_found_in_recognized_words_and_disabled_ones_are_not() {
        let word = |text: &str, x: f32| Word {
            text: text.into(),
            line: 0,
            bounds: [x, 10., 60., 14.],
        };
        let words = vec![
            word("Reviewed", 0.),
            word("by", 70.),
            word("W74828", 100.),
            word("and", 170.),
            word("X12345", 200.),
        ];
        let mut options = Options {
            custom: vec![custom::CustomType::new("Employee ID", "[WE]#####")],
            ..Default::default()
        };
        let found = detect(&words, &options, [400, 60]).unwrap();
        let custom: Vec<_> = found
            .iter()
            .filter(|f| f.category == Category::Custom(0))
            .collect();
        assert_eq!(custom.len(), 1);
        assert_eq!(custom[0].text, "W74828");
        assert_eq!(custom[0].bounds.len(), 1);
        assert!(
            custom[0].bounds[0][0] <= 100.
                && custom[0].bounds[0][0] + custom[0].bounds[0][2] >= 160.
        );
        options.custom[0].enabled = false;
        assert!(
            detect(&words, &options, [400, 60])
                .unwrap()
                .iter()
                .all(|f| f.category != Category::Custom(0))
        );
        options.custom[0].pattern = "[WE".into();
        assert!(
            detect(&words, &options, [400, 60]).is_err(),
            "a broken saved type is reported"
        );
    }
}
