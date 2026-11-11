//! Custom sensitive types (approved `artifacts/beta5-proposal`, tab 3): a value
//! *shape* such as "W or E, then 5 digits" instead of an exact phrase.
//!
//! Pattern text, shared by the block editor's shorthand and "Edit as text":
//! `#` a digit, `A` a letter, `*` a letter or digit, `.` any visible character,
//! `[WE]` or `[A-Z0-9]` one of a set, `"INV"` exact text, `\d` `\w` and `\x`
//! (literal x), counts `{5}` `{2,4}` `{2,}` `?` `+` `*`, and `|` between whole
//! alternatives. Matching runs every start position at once over a bounded set
//! of states, so a scan's time grows linearly with the text for any pattern.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::ops::Range;

/// Most saved types, alternatives per type, steps per alternative, and the
/// longest value a type can match.
const MAX_TYPES: usize = 32;
const MAX_ALTERNATIVES: usize = 8;
const MAX_STEPS: usize = 64;
const MAX_LENGTH: u32 = 256;
const OPEN_COUNT: u32 = 32;

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomType {
    pub name: String,
    pub pattern: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default = "yes")]
    pub ignore_case: bool,
    #[serde(default = "yes")]
    pub whole_words: bool,
    /// Spaces or dashes may appear between the characters of a value.
    #[serde(default)]
    pub separators: bool,
    /// O/0, I/l/1, S/5 and B/8 count as each other, for text read by OCR.
    #[serde(default = "yes")]
    pub ocr_tolerant: bool,
    /// Comma-separated labels, one of which must appear just before a value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub near_label: Option<String>,
}

impl CustomType {
    pub fn new(name: &str, pattern: &str) -> Self {
        Self {
            name: name.into(),
            pattern: pattern.into(),
            enabled: true,
            ignore_case: true,
            whole_words: true,
            separators: false,
            ocr_tolerant: true,
            near_label: None,
        }
    }
    pub fn validate(&self) -> Result<()> {
        let name = self.name.trim();
        if name.is_empty() || name.chars().count() > 60 || name.chars().any(char::is_control) {
            return Err(Error::Invalid(
                "Name the type with 1 to 60 characters.".into(),
            ));
        }
        if self
            .near_label
            .as_ref()
            .is_some_and(|l| l.len() > 200 || l.chars().any(char::is_control))
        {
            return Err(Error::Invalid("Keep labels under 200 characters.".into()));
        }
        Pattern::parse(&self.pattern).map(|_| ())
    }
    /// Byte ranges of every value of this type in `text`, leftmost first and
    /// never overlapping.
    pub fn find(&self, text: &str) -> Result<Vec<Range<usize>>> {
        let pattern = Pattern::parse(&self.pattern)?;
        let chars: Vec<(usize, char)> = text.char_indices().collect();
        let labels: Vec<String> = self
            .near_label
            .iter()
            .flat_map(|l| l.split(','))
            .map(|l| l.trim().to_lowercase())
            .filter(|l| !l.is_empty())
            .collect();
        let mut best: Vec<Option<usize>> = vec![None; chars.len()];
        for steps in &pattern.alternatives {
            run(steps, &chars, self, &labels, text, &mut best);
        }
        let mut out = Vec::new();
        let mut next = 0;
        for (start, end) in best.iter().enumerate() {
            if let Some(end) = end
                && start >= next
            {
                let a = chars[start].0;
                let b = chars.get(*end).map_or(text.len(), |c| c.0);
                out.push(a..b);
                next = *end;
            }
        }
        Ok(out)
    }
    /// Up to `n` values this type would match, for the builder's examples.
    pub fn examples(&self, n: usize) -> Vec<String> {
        let Ok(pattern) = Pattern::parse(&self.pattern) else {
            return Vec::new();
        };
        let mut out: Vec<String> = Vec::new();
        for seed in 0..n * 4 {
            let steps = &pattern.alternatives[seed % pattern.alternatives.len()];
            let mut s = String::new();
            for (k, step) in steps.iter().enumerate() {
                let count = if step.max > step.min && seed % 2 == 1 {
                    step.min + 1
                } else {
                    step.min
                };
                for j in 0..count {
                    let pick = seed * 7 + k * 3 + j as usize * 5;
                    s.push(step.set.sample(pick));
                }
            }
            if self.ignore_case && seed % 3 == 2 {
                s = s.to_lowercase();
            }
            if !s.is_empty() && !out.contains(&s) {
                out.push(s);
            }
            if out.len() == n {
                break;
            }
        }
        out
    }
}

/// Every saved type valid, and not too many of them.
pub fn validate_all(types: &[CustomType]) -> Result<()> {
    if types.len() > MAX_TYPES {
        return Err(Error::Invalid(format!(
            "Save at most {MAX_TYPES} custom types."
        )));
    }
    for (i, t) in types.iter().enumerate() {
        t.validate()?;
        if types[..i]
            .iter()
            .any(|o| o.name.trim().eq_ignore_ascii_case(t.name.trim()))
        {
            return Err(Error::Invalid(format!(
                "Two custom types are called {}.",
                t.name.trim()
            )));
        }
    }
    Ok(())
}

/// Ready-made types to add with one click (approved tab 3).
pub fn ready_made() -> Vec<CustomType> {
    let t = |name: &str, pattern: &str, f: &dyn Fn(&mut CustomType)| {
        let mut t = CustomType::new(name, pattern);
        f(&mut t);
        t
    };
    vec![
        t("Employee or badge ID", "[A-Z]#####", &|_| {}),
        t("Ticket or case key", "[A-Z]{2,6}-#{1,6}", &|t| {
            t.ignore_case = false
        }),
        t(
            "Invoice or PO number",
            "\"INV\"-?#{4,8}|\"PO\"-?#{4,8}",
            &|_| {},
        ),
        t("IBAN", "[A-Z]{2}##[A-Z0-9]{11,30}", &|t| {
            t.separators = true
        }),
        t("US EIN", "##-#######", &|_| {}),
        t(
            "UK National Insurance number",
            "[A-Z]{2}######[A-D]",
            &|t| t.separators = true,
        ),
        t("Canadian SIN", "#########", &|t| {
            t.separators = true;
            t.near_label = Some("SIN, social insurance".into());
        }),
        t(
            "Date of birth (labeled)",
            "##[/.-]##[/.-]####|####-##-##",
            &|t| {
                t.near_label = Some("DOB, date of birth, born, birth date".into());
            },
        ),
        t(
            "MAC address",
            "[0-9A-F]{2}[:-][0-9A-F]{2}[:-][0-9A-F]{2}[:-][0-9A-F]{2}[:-][0-9A-F]{2}[:-][0-9A-F]{2}",
            &|t| t.ocr_tolerant = false,
        ),
        t(
            "API keys and tokens",
            "\"AKIA\"[A-Z0-9]{16}|\"ghp_\"[A-Za-z0-9]{36}|\"sk-\"[A-Za-z0-9_-]{20,64}|\"xox\"[bpar]-[A-Za-z0-9-]{10,64}",
            &|t| {
                t.ignore_case = false;
                t.ocr_tolerant = false;
            },
        ),
        t(
            "Internal hostname",
            "[a-z0-9-]{1,63}\".internal\"|[a-z0-9-]{1,63}\".corp\"|[a-z0-9-]{1,63}\".local\"",
            &|t| t.ocr_tolerant = false,
        ),
        t("Account number (labeled)", "#{6,12}", &|t| {
            t.separators = true;
            t.near_label = Some("account, acct, account no".into());
        }),
    ]
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Set {
    Digit,
    Letter,
    Alnum,
    Word,
    Visible,
    Ranges(Vec<(char, char)>),
}

impl Set {
    fn plain(&self, c: char, ignore_case: bool) -> bool {
        match self {
            Set::Digit => c.is_ascii_digit(),
            Set::Letter => c.is_alphabetic(),
            Set::Alnum => c.is_alphanumeric(),
            Set::Word => c.is_alphanumeric() || c == '_',
            Set::Visible => !c.is_whitespace(),
            Set::Ranges(r) => {
                let hit = |c: char| r.iter().any(|(a, b)| (*a..=*b).contains(&c));
                hit(c) || ignore_case && (c.to_lowercase().any(hit) || c.to_uppercase().any(hit))
            }
        }
    }
    fn matches(&self, c: char, t: &CustomType) -> bool {
        self.plain(c, t.ignore_case)
            || t.ocr_tolerant && twins(c).iter().any(|d| self.plain(*d, t.ignore_case))
    }
    fn sample(&self, pick: usize) -> char {
        const DIGITS: &[u8] = b"7482951036";
        const LETTERS: &[u8] = b"WEKRTPMHDN";
        match self {
            Set::Digit => DIGITS[pick % 10] as char,
            Set::Letter | Set::Word | Set::Visible => LETTERS[pick % 10] as char,
            Set::Alnum => {
                if pick % 3 == 0 {
                    LETTERS[pick % 10] as char
                } else {
                    DIGITS[pick % 10] as char
                }
            }
            Set::Ranges(r) => {
                let all: Vec<char> = r.iter().flat_map(|(a, b)| (*a..=*b).take(64)).collect();
                all[pick % all.len()]
            }
        }
    }
}

/// Characters OCR commonly reads as each other.
fn twins(c: char) -> &'static [char] {
    match c {
        '0' => &['O', 'o'],
        'O' | 'o' => &['0'],
        '1' => &['I', 'l'],
        'I' | 'l' => &['1'],
        '5' => &['S', 's'],
        'S' | 's' => &['5'],
        '8' => &['B'],
        'B' => &['8'],
        _ => &[],
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Step {
    set: Set,
    min: u32,
    max: u32,
}

struct Pattern {
    alternatives: Vec<Vec<Step>>,
}

impl Pattern {
    fn parse(text: &str) -> Result<Self> {
        let bad = |why: &str| Error::Invalid(format!("Pattern: {why}"));
        if text.trim().is_empty() {
            return Err(bad("add at least one block."));
        }
        if text.chars().count() > 400 {
            return Err(bad("keep it under 400 characters."));
        }
        let mut alternatives = Vec::new();
        let mut current: Vec<Step> = Vec::new();
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            i += 1;
            let mut literal_run: Vec<char> = Vec::new();
            let set = match c {
                '|' => {
                    alternatives.push(std::mem::take(&mut current));
                    continue;
                }
                '#' => Set::Digit,
                'A' => Set::Letter,
                '*' => Set::Alnum,
                '.' => Set::Visible,
                '\\' => {
                    let Some(&n) = chars.get(i) else {
                        return Err(bad("a \\ needs a character after it."));
                    };
                    i += 1;
                    match n {
                        'd' => Set::Digit,
                        'w' => Set::Word,
                        n => Set::Ranges(vec![(n, n)]),
                    }
                }
                '[' => {
                    let mut ranges = Vec::new();
                    let mut closed = false;
                    while i < chars.len() {
                        let mut a = chars[i];
                        i += 1;
                        if a == ']' && !ranges.is_empty() {
                            closed = true;
                            break;
                        }
                        if a == '\\' {
                            a = *chars.get(i).ok_or_else(|| bad("unfinished [ ] set."))?;
                            i += 1;
                        }
                        if chars.get(i) == Some(&'-') && chars.get(i + 1).is_some_and(|b| *b != ']')
                        {
                            let b = chars[i + 1];
                            i += 2;
                            if b < a {
                                return Err(bad("a range in [ ] runs backwards."));
                            }
                            ranges.push((a, b));
                        } else {
                            ranges.push((a, a));
                        }
                    }
                    if !closed {
                        return Err(bad("close the [ ] set."));
                    }
                    Set::Ranges(ranges)
                }
                '"' => {
                    let mut closed = false;
                    while i < chars.len() {
                        let a = chars[i];
                        i += 1;
                        if a == '"' {
                            closed = true;
                            break;
                        }
                        literal_run.push(a);
                    }
                    if !closed || literal_run.is_empty() {
                        return Err(bad("close the quoted text."));
                    }
                    let last = literal_run.pop().unwrap();
                    for a in literal_run.drain(..) {
                        current.push(Step {
                            set: Set::Ranges(vec![(a, a)]),
                            min: 1,
                            max: 1,
                        });
                    }
                    Set::Ranges(vec![(last, last)])
                }
                '{' | '}' | '?' | '+' | ']' => {
                    return Err(bad("a count needs a block before it."));
                }
                c if c.is_whitespace() => {
                    return Err(bad("use \" \" for a space."));
                }
                c => Set::Ranges(vec![(c, c)]),
            };
            let (min, max) = match chars.get(i) {
                Some('?') => {
                    i += 1;
                    (0, 1)
                }
                Some('+') => {
                    i += 1;
                    (1, OPEN_COUNT)
                }
                Some('{') => {
                    let end = chars[i..]
                        .iter()
                        .position(|c| *c == '}')
                        .ok_or_else(|| bad("close the { } count."))?
                        + i;
                    let inner: String = chars[i + 1..end].iter().collect();
                    i = end + 1;
                    let number = |s: &str| {
                        s.trim()
                            .parse::<u32>()
                            .map_err(|_| bad("counts are whole numbers."))
                    };
                    match inner.split_once(',') {
                        None => {
                            let n = number(&inner)?;
                            (n, n)
                        }
                        Some((a, "")) => {
                            let a = number(a)?;
                            (a, a + OPEN_COUNT)
                        }
                        Some((a, b)) => (number(a)?, number(b)?),
                    }
                }
                _ => (1, 1),
            };
            if min > max || max > 64 {
                return Err(bad("counts go from 0 to 64, smallest first."));
            }
            if max > 0 {
                match current.last_mut() {
                    Some(last) if last.set == set => {
                        last.min += min;
                        last.max += max;
                    }
                    _ => current.push(Step { set, min, max }),
                }
            }
        }
        alternatives.push(current);
        if alternatives.len() > MAX_ALTERNATIVES {
            return Err(bad("use at most 8 alternatives."));
        }
        for steps in &alternatives {
            if steps.iter().all(|s| s.min == 0) {
                return Err(bad("every alternative must match at least one character."));
            }
            if steps.len() > MAX_STEPS {
                return Err(bad("use at most 64 blocks."));
            }
            if steps.iter().map(|s| s.max).sum::<u32>() > MAX_LENGTH {
                return Err(bad("a value can be at most 256 characters long."));
            }
        }
        Ok(Self { alternatives })
    }
}

/// One alternative over the whole text. A thread is a (step, count) state with
/// the earliest start that reaches it; `best[start]` keeps the longest end.
fn run(
    steps: &[Step],
    chars: &[(usize, char)],
    t: &CustomType,
    labels: &[String],
    text: &str,
    best: &mut [Option<usize>],
) {
    let base: Vec<usize> = steps
        .iter()
        .scan(0, |at, s| {
            let here = *at;
            *at += s.max as usize + 1;
            Some(here)
        })
        .collect();
    let total = base
        .last()
        .map_or(0, |b| b + steps.last().unwrap().max as usize + 1);
    // Whether every step after `i` may be skipped, so (i, enough) accepts.
    let tail_optional: Vec<bool> = (0..steps.len())
        .map(|i| steps[i + 1..].iter().all(|s| s.min == 0))
        .collect();
    let mut current: Vec<Option<usize>> = vec![None; total];
    let mut next: Vec<Option<usize>> = vec![None; total];
    let separator = |c: char| t.separators && (c == ' ' || c == '-');
    let boundary = |k: usize| !chars.get(k).is_some_and(|(_, c)| c.is_alphanumeric());
    let labelled = |k: usize| {
        if labels.is_empty() {
            return true;
        }
        // The label sits right before the value, with only spaces and
        // `: # . = -` between them ("Acct: 12345678", "SIN 046 454 286").
        let at = chars[k].0;
        let from = chars[k.saturating_sub(48)].0;
        let before = text[from..at].to_lowercase();
        let before = before.trim_end_matches(|c: char| c.is_whitespace() || ":#.=-".contains(c));
        labels.iter().any(|l| {
            before.ends_with(l.as_str())
                && !before[..before.len() - l.len()]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_alphanumeric)
        })
    };
    let keep = |slot: &mut Option<usize>, start: usize| {
        if slot.is_none_or(|s| start < s) {
            *slot = Some(start);
        }
    };
    for (k, &(_, c)) in chars.iter().enumerate() {
        // A value can start here: (0, 0) with this start.
        if !separator(c) && (!t.whole_words || k == 0 || boundary(k - 1)) && labelled(k) {
            keep(&mut current[base[0]], k);
        }
        // Skippable steps: (i, c >= min) also stands at (i + 1, 0).
        for i in 0..steps.len().saturating_sub(1) {
            for count in steps[i].min..=steps[i].max {
                if let Some(start) = current[base[i] + count as usize] {
                    keep(&mut current[base[i + 1]], start);
                }
            }
        }
        next.iter_mut().for_each(|s| *s = None);
        for (i, step) in steps.iter().enumerate() {
            for count in 0..=step.max {
                let Some(start) = current[base[i] + count as usize] else {
                    continue;
                };
                if separator(c) && start < k && (i > 0 || count > 0) {
                    keep(&mut next[base[i] + count as usize], start);
                }
                if count < step.max && step.set.matches(c, t) {
                    keep(&mut next[base[i] + count as usize + 1], start);
                }
            }
        }
        // Record values ending after this character.
        if !separator(c) && (!t.whole_words || boundary(k + 1)) {
            for (i, step) in steps.iter().enumerate() {
                if !tail_optional[i] {
                    continue;
                }
                for count in step.min.max(1)..=step.max {
                    if let Some(start) = next[base[i] + count as usize]
                        && best[start].is_none_or(|end| k + 1 > end)
                    {
                        best[start] = Some(k + 1);
                    }
                }
            }
        }
        std::mem::swap(&mut current, &mut next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(t: &CustomType, text: &str) -> Vec<String> {
        t.find(text)
            .unwrap()
            .into_iter()
            .map(|r| text[r].to_string())
            .collect()
    }

    #[test]
    fn employee_ids_match_the_mockup_test_box() {
        let t = CustomType::new("Employee ID", "[WE]#####");
        let text = "Reviewed by W74828 and E84839; escalated to X12345 on ticket OPS-2201. Badge AW748281 stays.";
        assert_eq!(found(&t, text), ["W74828", "E84839"]);
        // The block editor's own shorthand and regular-expression style agree.
        for pattern in ["[WE]#####", "[WE]\\d{5}", "[WE]#{5}", "[EW]\\d\\d\\d\\d\\d"] {
            let t = CustomType::new("Employee ID", pattern);
            assert_eq!(found(&t, text), ["W74828", "E84839"], "{pattern}");
        }
    }

    #[test]
    fn options_change_what_counts() {
        let mut t = CustomType::new("Employee ID", "[WE]#####");
        assert_eq!(found(&t, "w74828"), ["w74828"], "ignore case");
        t.ignore_case = false;
        assert!(found(&t, "w74828").is_empty());
        t.ignore_case = true;
        assert_eq!(
            found(&t, "W7482B and E8O839"),
            ["W7482B", "E8O839"],
            "OCR mix-ups"
        );
        t.ocr_tolerant = false;
        assert!(found(&t, "W7482B").is_empty());
        assert!(found(&t, "W-74828").is_empty());
        t.separators = true;
        assert_eq!(
            found(&t, "ID W-74828 and E 84 839."),
            ["W-74828", "E 84 839"]
        );
        t.whole_words = false;
        assert_eq!(found(&t, "AW748281"), ["W74828"]);
        let mut labelled = CustomType::new("Account", "#{6,12}");
        labelled.near_label = Some("account, acct".into());
        assert_eq!(
            found(&labelled, "Acct: 12345678 and order 99887766"),
            ["12345678"]
        );
    }

    #[test]
    fn alternatives_quotes_and_examples() {
        let t = CustomType::new("Invoice", "\"INV\"-?#{4,8}|\"PO\"-?#{4,8}");
        assert_eq!(
            found(&t, "INV-000123, PO 4455 and PO4455"),
            ["INV-000123", "PO4455"]
        );
        let e = CustomType::new("Employee ID", "[WE]#####").examples(3);
        assert_eq!(e.len(), 3);
        let t = CustomType::new("Employee ID", "[WE]#####");
        for value in e {
            assert_eq!(found(&t, &value), [value.clone()], "examples match");
        }
        for r in ready_made() {
            r.validate().unwrap_or_else(|e| panic!("{}: {e}", r.name));
            for value in r.examples(2) {
                let mut probe = r.clone();
                probe.near_label = None;
                assert!(
                    !found(&probe, &value).is_empty(),
                    "{} example {value}",
                    r.name
                );
            }
        }
        let key = ready_made()
            .into_iter()
            .find(|t| t.name == "API keys and tokens")
            .unwrap();
        assert_eq!(
            found(&key, "key AKIAIOSFODNN7EXAMPLE end"),
            ["AKIAIOSFODNN7EXAMPLE"]
        );
    }

    #[test]
    fn bad_patterns_explain_themselves_and_scans_stay_linear() {
        for bad in [
            "", "#?", "[WE", "#{3", "#{9,2}", "#{99}", "?#", "\"open", "a b",
        ] {
            assert!(CustomType::new("x", bad).validate().is_err(), "{bad:?}");
        }
        // A pathological pattern over a long page still finishes promptly.
        let mut t = CustomType::new("Digits", "#{1,64}#{1,64}#{1,64}");
        let text = "1".repeat(200_000);
        assert!(
            t.find(&text).unwrap().is_empty(),
            "no word boundary, no value"
        );
        t.whole_words = false;
        let start = std::time::Instant::now();
        assert!(!t.find(&text).unwrap().is_empty());
        assert!(
            start.elapsed().as_secs() < 20,
            "scan took {:?}",
            start.elapsed()
        );
    }
}
