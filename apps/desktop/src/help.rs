//! ClipCraft Help (HELP-001, approved bi-proposal 4): articles shipped with
//! the app, a local search that tolerates typos and everyday words, and F1
//! from any control with an article. Tooltips and Help share control names,
//! so a tooltip can say when F1 opens more.
mod art;
mod pictures;
#[cfg(test)]
mod tests;
mod window;
pub(crate) use window::{State, draw_glyph};

const SOURCE: &str = include_str!("../assets/help/articles.txt");

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Block {
    Text(String),
    /// A numbered step: its instruction, an optional icon and picture, and the
    /// short title shown in the step list.
    Step(String, Option<String>, Option<String>, String),
    /// An illustration drawn by ClipCraft itself, with a caption.
    Art(String, String),
    /// Two illustrations side by side, with a caption.
    Pair(String, String, String),
    /// A short animation note (shown as a loop label).
    Loop(String),
    Tip(String),
}
#[derive(Clone, Debug)]
pub(crate) struct Article {
    pub id: String,
    pub title: String,
    pub section: String,
    /// Ribbon path such as "Insert › Obfuscate".
    pub location: String,
    /// Control names (as in tooltips) that open this article with F1.
    pub controls: Vec<String>,
    pub keywords: Vec<String>,
    pub hero: Option<String>,
    pub blocks: Vec<Block>,
}
impl Article {
    pub(crate) fn has_steps(&self) -> bool {
        self.blocks.iter().any(|b| matches!(b, Block::Step(..)))
    }
    fn text(&self) -> String {
        self.blocks
            .iter()
            .map(|b| match b {
                Block::Text(t) | Block::Step(t, _, _, _) | Block::Loop(t) | Block::Tip(t) => {
                    t.as_str()
                }
                Block::Art(_, c) | Block::Pair(_, _, c) => c.as_str(),
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}
/// Whether a picture key can be shown.
#[cfg(test)]
pub(crate) fn has_picture(key: &str) -> bool {
    pictures::find(key).is_some() || art::draw(key).is_some()
}
pub(crate) fn articles() -> &'static [Article] {
    static ARTICLES: std::sync::OnceLock<Vec<Article>> = std::sync::OnceLock::new();
    ARTICLES.get_or_init(|| parse(SOURCE))
}
fn parse(source: &str) -> Vec<Article> {
    let mut out = vec![];
    for chunk in source.split("\n# ").filter(|c| !c.trim().is_empty()) {
        let chunk = chunk.trim_start_matches("# ");
        let mut lines = chunk.lines();
        let title = lines.next().unwrap_or("").trim().to_owned();
        let mut a = Article {
            id: String::new(),
            title,
            section: String::new(),
            location: String::new(),
            controls: vec![],
            keywords: vec![],
            hero: None,
            blocks: vec![],
        };
        let list = |v: &str| {
            v.split(',')
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        };
        let mut body = false;
        let mut paragraph = String::new();
        let flush = |p: &mut String, blocks: &mut Vec<Block>| {
            if !p.trim().is_empty() {
                blocks.push(Block::Text(p.trim().to_owned()));
            }
            p.clear();
        };
        for line in lines {
            let line = line.trim_end();
            if !body {
                if line.trim() == "---" {
                    body = true;
                    continue;
                }
                let (k, v) = line.split_once(':').unwrap_or((line, ""));
                let v = v.trim();
                match k.trim() {
                    "id" => a.id = v.into(),
                    "section" => a.section = v.into(),
                    "where" => a.location = v.replace('>', "›"),
                    "controls" => a.controls = list(v),
                    "keywords" => a.keywords = list(v),
                    "hero" => a.hero = Some(v.into()),
                    _ => {}
                }
                continue;
            }
            let t = line.trim();
            if t.is_empty() {
                flush(&mut paragraph, &mut a.blocks);
            } else if let Some(rest) = t.strip_prefix("step:") {
                flush(&mut paragraph, &mut a.blocks);
                let mut fields = rest.split('|').map(str::trim);
                let text = fields.next().unwrap_or("");
                let (mut icon, mut pic, mut title) = (None, None, String::new());
                for field in fields {
                    if let Some(v) = field.strip_prefix("title:") {
                        title = v.trim().to_owned();
                    } else if let Some(v) = field.strip_prefix("icon:") {
                        icon = Some(v.trim().to_owned());
                    } else if let Some(v) = field.strip_prefix("pic:") {
                        pic = Some(v.trim().to_owned());
                    }
                }
                a.blocks.push(Block::Step(text.into(), icon, pic, title));
            } else if let Some(rest) = t.strip_prefix("art:") {
                flush(&mut paragraph, &mut a.blocks);
                let (key, caption) = rest.split_once('|').unwrap_or((rest, ""));
                a.blocks
                    .push(Block::Art(key.trim().into(), caption.trim().into()));
            } else if let Some(rest) = t.strip_prefix("pair:") {
                flush(&mut paragraph, &mut a.blocks);
                let (keys, caption) = rest.split_once('|').unwrap_or((rest, ""));
                let (l, r) = keys.split_once(',').unwrap_or((keys, keys));
                a.blocks.push(Block::Pair(
                    l.trim().into(),
                    r.trim().into(),
                    caption.trim().into(),
                ));
            } else if let Some(rest) = t.strip_prefix("loop:") {
                flush(&mut paragraph, &mut a.blocks);
                a.blocks.push(Block::Loop(rest.trim().into()));
            } else if let Some(rest) = t.strip_prefix("tip:") {
                flush(&mut paragraph, &mut a.blocks);
                a.blocks.push(Block::Tip(rest.trim().into()));
            } else {
                if !paragraph.is_empty() {
                    paragraph.push(' ');
                }
                paragraph.push_str(t);
            }
        }
        flush(&mut paragraph, &mut a.blocks);
        if !a.id.is_empty() {
            out.push(a);
        }
    }
    out
}

/// Everyday words people search with, and the ClipCraft words they mean.
const SYNONYMS: &[(&str, &[&str])] = &[
    ("hide", &["obfuscate", "redact", "blur", "pixelate"]),
    ("censor", &["redact", "blur", "pixelate"]),
    ("blackout", &["redact"]),
    ("private", &["redact", "privacy", "sensitive"]),
    ("number", &["steps", "numbered"]),
    ("numbers", &["steps", "numbered"]),
    ("counter", &["steps"]),
    ("animation", &["gif", "keyframes"]),
    ("animated", &["gif"]),
    ("hotkey", &["shortcut", "shortcuts"]),
    ("keys", &["shortcut", "shortcuts"]),
    ("screenshot", &["capture"]),
    ("snip", &["capture"]),
    ("record", &["recording", "video"]),
    ("film", &["video", "recording"]),
    ("camera", &["webcam"]),
    ("zoom", &["zoom", "magnify"]),
    ("enlarge", &["magnify", "resize"]),
    ("spotlight", &["focus"]),
    ("highlight", &["penlighter", "highlighter", "focus"]),
    ("marker", &["penlighter", "steps"]),
    ("trim", &["crop", "cut"]),
    ("cut", &["cut", "crop"]),
    ("delete", &["eraser", "remove"]),
    ("erase", &["eraser"]),
    ("arrow", &["lines", "arrow"]),
    ("line", &["lines"]),
    ("box", &["shape", "rectangle"]),
    ("circle", &["shape", "ellipse"]),
    ("bubble", &["callout"]),
    ("speech", &["callout"]),
    ("label", &["text", "callout"]),
    ("write", &["text"]),
    ("type", &["text"]),
    ("logo", &["watermark", "svg", "icons"]),
    ("stamp", &["icons", "watermark"]),
    ("tilt", &["perspective"]),
    ("3d", &["perspective"]),
    ("color", &["colors", "palette"]),
    ("colour", &["colors", "palette"]),
    ("save", &["save", "export", "project"]),
    ("pdf", &["export", "print"]),
    ("png", &["export"]),
    ("mp4", &["export", "video"]),
    ("sound", &["audio", "microphone"]),
    ("voice", &["narration", "microphone"]),
    ("mic", &["microphone"]),
    ("ocr", &["grab", "text", "recognition"]),
    ("copy", &["copy", "grab"]),
    ("name", &["rename"]),
    ("vector", &["svg"]),
    ("template", &["templates"]),
    ("find", &["jump", "search"]),
];
fn words(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}
fn edits(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut prev = row[0];
        row[0] = i;
        for j in 1..=b.len() {
            let cur = row[j];
            row[j] = (row[j] + 1)
                .min(row[j - 1] + 1)
                .min(prev + usize::from(a[i - 1] != b[j - 1]));
            prev = cur;
        }
    }
    row[b.len()]
}
/// How well one query word matches one article word.
fn closeness(query: &str, word: &str) -> f32 {
    if query == word {
        3.
    } else if query.len() >= 3 && word.starts_with(query) {
        2.
    } else if query.len() >= 4 && edits(query, word) <= 1 + usize::from(query.len() >= 8) {
        1.5
    } else {
        0.
    }
}
pub(crate) struct Hit {
    pub index: usize,
    pub score: f32,
    /// Article words that matched, for highlighting.
    pub matched: Vec<String>,
}
/// Ranked results: title matches first, then headings and control names,
/// then keywords and text. Every query word must match something.
pub(crate) fn search(query: &str) -> Vec<Hit> {
    let terms = words(query);
    let list = articles();
    if terms.is_empty() {
        return (0..list.len())
            .map(|index| Hit {
                index,
                score: 0.,
                matched: vec![],
            })
            .collect();
    }
    let mut hits = vec![];
    'articles: for (index, a) in list.iter().enumerate() {
        let fields = [
            (words(&a.title), 4.),
            (words(&a.controls.join(" ")), 3.),
            (words(&a.keywords.join(" ")), 2.5),
            (words(&a.location), 2.),
            (words(&a.text()), 1.),
        ];
        let mut score = 0.;
        let mut matched = vec![];
        for term in &terms {
            let candidates: Vec<&str> = std::iter::once(term.as_str())
                .chain(
                    SYNONYMS
                        .iter()
                        .filter(|(k, _)| k == term)
                        .flat_map(|(_, v)| v.iter().copied()),
                )
                .collect();
            let mut best = (0f32, String::new());
            for (field, weight) in &fields {
                for word in field {
                    for (n, c) in candidates.iter().enumerate() {
                        // A synonym counts a little less than the word itself.
                        let s = closeness(c, word) * weight * if n == 0 { 1. } else { 0.8 };
                        if s > best.0 {
                            best = (s, word.clone());
                        }
                    }
                }
            }
            if best.0 == 0. {
                continue 'articles;
            }
            score += best.0;
            matched.push(best.1);
        }
        hits.push(Hit {
            index,
            score,
            matched,
        });
    }
    hits.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.index.cmp(&b.index)));
    hits
}
/// The article F1 opens for a control, by its tooltip name.
pub(crate) fn for_control(title: &str) -> Option<usize> {
    let title = title.trim();
    let base = title.split_once(" (").map_or(title, |(b, _)| b);
    // "Text justification: Top left" and its eight neighbors share one topic.
    let base = base.split_once(": ").map_or(base, |(b, _)| b);
    // "Flatten page · Ctrl+Shift+T", "Trim image — planned".
    let base = base.split_once(" · ").map_or(base, |(b, _)| b);
    let base = base.split_once(" — ").map_or(base, |(b, _)| b);
    // The one control name with commas; the list in articles.txt splits on them.
    let base = if base == "Spacing, indents, tabs and lists" {
        "Spacing indents tabs and lists"
    } else {
        base
    };
    articles().iter().position(|a| {
        a.controls
            .iter()
            .any(|c| c.eq_ignore_ascii_case(base) || c.eq_ignore_ascii_case(title))
    })
}
fn request_id() -> crate::egui::Id {
    crate::egui::Id::unique("help-request")
}
/// Opens Help at an article (F1) or with a search.
pub(crate) fn request(ctx: &crate::egui::Context, article: Option<usize>, query: &str) {
    ctx.data_mut(|d| d.insert_temp(request_id(), (article, query.to_owned())));
}
pub(crate) fn take_request(ctx: &crate::egui::Context) -> Option<(Option<usize>, String)> {
    ctx.data_mut(|d| d.remove_temp::<(Option<usize>, String)>(request_id()))
}
