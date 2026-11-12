//! Parts shared by the image and video Find Sensitive Info dialogs (approved
//! sensitive-info-proposal): what to look for, and what covers the findings.
use crate::tips::Tip as _;
use crate::{Kind, egui};
use capture_core::privacy::Options;
use capture_core::privacy::custom::{self, CustomType};

/// What covers the ticked findings. Redact each time the dialog opens, so a
/// quick scan never adds a weaker cover by accident.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Cover {
    pub kind: Kind,
    blur: u32,
    pixelate: u32,
}
impl Default for Cover {
    fn default() -> Self {
        Self {
            kind: Kind::Redact,
            blur: 4,
            pixelate: 8,
        }
    }
}
impl Cover {
    pub(crate) fn width(&self) -> u32 {
        match self.kind {
            Kind::Blur => self.blur,
            Kind::Pixelate => self.pixelate,
            _ => 1,
        }
    }
    /// The noun for one cover of this kind.
    pub(crate) fn noun(&self) -> &'static str {
        match self.kind {
            Kind::Blur => "blur",
            Kind::Pixelate => "pixelation",
            _ => "redaction",
        }
    }
    /// The main button's words, such as "Add 3 blurs".
    pub(crate) fn action(&self, count: usize) -> String {
        format!(
            "Add {count} {}{}",
            self.noun(),
            if count == 1 { "" } else { "s" }
        )
    }
}
/// The categories, custom types and exact phrases to search for. The second
/// value is true when the custom types changed and should be saved.
pub(crate) fn look_for(
    ui: &mut egui::Ui,
    options: &mut Options,
    phrases: &mut String,
) -> (egui::Response, bool) {
    let field = look_for_builtin(ui, options, phrases, |ui, options| {
        custom_section(ui, &mut options.custom)
    });
    let changed = type_builder(ui.ctx(), &mut options.custom);
    (field.0, field.1 || changed)
}

fn look_for_builtin(
    ui: &mut egui::Ui,
    options: &mut Options,
    phrases: &mut String,
    custom: impl FnOnce(&mut egui::Ui, &mut Options) -> bool,
) -> (egui::Response, bool) {
    crate::controls::dialog_section(
        ui,
        "Look for",
        Some(
            "Runs on this PC. Detection can miss things, so review every finding and the rest of the picture before sharing.",
        ),
    );
    let columns = ((ui.available_width() / 190.).floor() as usize).clamp(1, 4);
    let entries = [
        (&mut options.names, "Names with labels"),
        (&mut options.user_ids, "User IDs and handles"),
        (&mut options.emails, "Email addresses"),
        (&mut options.phones, "Phone numbers"),
        (&mut options.payment_cards, "Payment cards"),
        (&mut options.us_ssns, "US SSNs"),
        (&mut options.ip_addresses, "IP addresses"),
    ];
    egui::Grid::new("sensitive-look-for")
        .num_columns(columns)
        .spacing([18., 2.])
        .show(ui, |ui| {
            for (i, (value, label)) in entries.into_iter().enumerate() {
                crate::controls::toggle(ui, value, label);
                if (i + 1) % columns == 0 {
                    ui.end_row();
                }
            }
        });
    let changed = custom(ui, options);
    crate::controls::dialog_section(
        ui,
        "Exact names or phrases",
        Some(
            "Type a name or phrase and press Enter. Commas and semicolons separate several at once. Matching ignores letter case.",
        ),
    );
    let field = phrase_chips(ui, phrases);
    (field, changed)
}

/// Adds typed text as chips: split at commas and semicolons, spaces inside a
/// phrase collapsed, duplicates (ignoring case) skipped. `phrases` keeps one
/// phrase per line, which every scan reads.
pub(crate) fn add_phrases(phrases: &mut String, typed: &str) {
    let mut list: Vec<String> = phrases.lines().map(str::to_owned).collect();
    for part in typed.split([',', ';', '\n']) {
        let phrase = part.split_whitespace().collect::<Vec<_>>().join(" ");
        if phrase.is_empty()
            || phrase.len() > 512
            || list.len() >= 100
            || list
                .iter()
                .any(|p| p.to_lowercase() == phrase.to_lowercase())
        {
            continue;
        }
        list.push(phrase);
    }
    *phrases = list.join("\n");
}

/// Exact names or phrases as chips (approved beta 6 tab 1): Enter turns the
/// typed text into a blue chip with an x; Backspace in the empty field
/// removes the last chip; clicking a chip puts it back in the field to edit.
fn phrase_chips(ui: &mut egui::Ui, phrases: &mut String) -> egui::Response {
    let id = ui.scope_id().with("phrase-chips");
    let mut typed = ui
        .ctx()
        .data(|d| d.get_temp::<String>(id))
        .unwrap_or_default();
    let mut remove = None;
    let mut edit = None;
    let list: Vec<String> = phrases.lines().map(str::to_owned).collect();
    let frame = egui::Frame::new()
        .fill(egui::Color32::from_rgb(69, 78, 82))
        .stroke(egui::Stroke::new(1., egui::Color32::from_rgb(86, 99, 106)))
        .corner_radius(4)
        .inner_margin(5);
    let mut field_response = None;
    frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(5., 5.);
            for (i, phrase) in list.iter().enumerate() {
                let text = egui::RichText::new(phrase)
                    .size(12.5)
                    .color(egui::Color32::WHITE);
                // Measured unwrapped so a chip is only as wide as its text;
                // only a long phrase is cut to 260 px (BW1: chips filled whole
                // rows instead of sitting side by side).
                let text = egui::WidgetText::from(text);
                let mut galley = text.clone().into_galley(
                    ui,
                    Some(egui::TextWrapMode::Extend),
                    f32::INFINITY,
                    egui::TextStyle::Body,
                );
                if galley.size().x > 260. {
                    galley = text.into_galley(
                        ui,
                        Some(egui::TextWrapMode::Truncate),
                        260.,
                        egui::TextStyle::Body,
                    );
                }
                let size = egui::vec2(galley.size().x + 32., 22.);
                let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
                let p = ui.painter();
                p.rect_filled(rect, 11., crate::shell::ACCENT);
                p.galley(
                    egui::pos2(rect.left() + 10., rect.center().y - galley.size().y / 2.),
                    galley,
                    egui::Color32::WHITE,
                );
                let x = egui::Rect::from_center_size(
                    egui::pos2(rect.right() - 11., rect.center().y),
                    egui::vec2(16., 16.),
                );
                let x_response = ui
                    .interact(x, id.with(("remove", i)), egui::Sense::click())
                    .tip(format!("Remove {phrase}"));
                if x_response.hovered() {
                    p.circle_filled(x.center(), 8., egui::Color32::from_white_alpha(50));
                }
                p.text(
                    x.center(),
                    egui::Align2::CENTER_CENTER,
                    "✕",
                    egui::FontId::proportional(10.),
                    egui::Color32::from_rgb(223, 243, 250),
                );
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::Button, true, phrase.as_str())
                });
                if x_response.clicked() {
                    remove = Some(i);
                } else if response.clicked() {
                    edit = Some(i);
                }
            }
            // A fixed id: each new chip comes before the field, which used to
            // give it a new id and drop the cursor out of it (BW1).
            let field = ui.add(
                crate::unicode_input::TextEdit::singleline(&mut typed)
                    .id(id.with("field"))
                    .hint_text("Type a name or phrase, then Enter")
                    .frame(egui::Frame::NONE)
                    .desired_width(ui.available_width().clamp(160., 260.))
                    .char_limit(512),
            );
            let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if enter {
                // Enter adds the phrase; it never starts the scan.
                crate::controls::hold_dialog_enter(ui.ctx());
            }
            if enter || typed.contains([',', ';']) {
                let (done, rest) = match typed.rfind([',', ';']) {
                    Some(at) if !enter => (typed[..at].to_owned(), typed[at + 1..].to_owned()),
                    _ => (std::mem::take(&mut typed), String::new()),
                };
                add_phrases(phrases, &done);
                typed = rest;
                // The cursor stays in the box, ready for the next one.
                field.request_focus();
            } else if field.has_focus()
                && typed.is_empty()
                && !list.is_empty()
                && ui.input(|i| i.key_pressed(egui::Key::Backspace))
            {
                remove = Some(list.len() - 1);
            }
            field_response = Some(field);
        });
    });
    if let Some(i) = remove.or(edit) {
        let mut list = list;
        let taken = list.remove(i);
        *phrases = list.join("\n");
        if edit.is_some() {
            typed = taken;
            if let Some(f) = &field_response {
                f.request_focus();
            }
        }
    }
    let count = phrases.lines().count();
    ui.label(
        egui::RichText::new(match count {
            0 => "No phrases yet".to_owned(),
            1 => "1 phrase · matched in any letter case".to_owned(),
            n => format!("{n} phrases · matched in any letter case"),
        })
        .size(11.5)
        .color(egui::Color32::from_rgb(142, 154, 157)),
    );
    ui.ctx().data_mut(|d| d.insert_temp(id, typed));
    field_response.unwrap()
}

/// Custom types in Look for (approved beta 5 tab 3): a toggle per saved type,
/// + New type, and a menu to edit, delete, export or import types.
fn custom_section(ui: &mut egui::Ui, list: &mut Vec<CustomType>) -> bool {
    let mut changed = false;
    ui.add_space(6.);
    ui.horizontal(|ui| {
        crate::controls::dialog_section_label(
            ui,
            "Custom types",
            Some("Value shapes you define, such as an employee ID: a letter, then five digits."),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.menu_button("…", |ui| {
                if crate::controls::menu_action(ui, "Export types…").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("ClipDesigner sensitive types", &["cdtypes"])
                        .set_file_name("Sensitive types.cdtypes")
                        .save_file()
                        && let Ok(bytes) = serde_json::to_vec_pretty(list)
                    {
                        let _ = std::fs::write(&path, &bytes);
                    }
                    ui.close();
                }
                if crate::controls::menu_action(ui, "Import types…").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("ClipDesigner sensitive types", &["cdtypes"])
                        .pick_file()
                        && let Ok(bytes) = std::fs::read(&path)
                        && bytes.len() < 1_000_000
                        && let Ok(found) = serde_json::from_slice::<Vec<CustomType>>(&bytes)
                    {
                        let mut merged = list.clone();
                        for t in found {
                            if !merged
                                .iter()
                                .any(|o| o.name.trim().eq_ignore_ascii_case(t.name.trim()))
                            {
                                merged.push(t);
                            }
                        }
                        if custom::validate_all(&merged).is_ok() {
                            *list = merged;
                            changed = true;
                        }
                    }
                    ui.close();
                }
            });
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new("+ New type").color(crate::shell::ACCENT),
                    )
                    .frame(false),
                )
                .tip("Define a value shape to look for")
                .clicked()
            {
                open_builder(ui.ctx(), None, None);
            }
        });
    });
    let columns = ((ui.available_width() / 190.).floor() as usize).clamp(1, 4);
    let mut remove = None;
    egui::Grid::new("sensitive-custom-types")
        .num_columns(columns)
        .spacing([18., 2.])
        .show(ui, |ui| {
            for (i, t) in list.iter_mut().enumerate() {
                let response = crate::controls::toggle(ui, &mut t.enabled, &t.name);
                changed |= response.changed();
                response.context_menu(|ui| {
                    if crate::controls::menu_action(ui, "Edit").clicked() {
                        open_builder(ui.ctx(), Some(i), Some(t.clone()));
                        ui.close();
                    }
                    if crate::controls::menu_action(ui, "Delete").clicked() {
                        remove = Some(i);
                        ui.close();
                    }
                });
                if (i + 1) % columns == 0 {
                    ui.end_row();
                }
            }
        });
    if let Some(i) = remove {
        list.remove(i);
        changed = true;
    }
    if list.is_empty() {
        ui.label(
            egui::RichText::new("None yet. Right-click a type to edit or delete it.")
                .size(11.5)
                .color(egui::Color32::from_rgb(142, 154, 157)),
        );
    }
    changed
}

/// One block of the pattern builder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BlockKind {
    OneOf,
    Letters,
    Digits,
    LettersOrDigits,
    Exact,
    Separator,
}
#[derive(Clone, PartialEq, Eq)]
struct Block {
    kind: BlockKind,
    value: String,
    min: u32,
    max: u32,
}
impl BlockKind {
    const ALL: [(Self, &'static str); 6] = [
        (Self::OneOf, "One of"),
        (Self::Letters, "Letters"),
        (Self::Digits, "Digits"),
        (Self::LettersOrDigits, "Letters or digits"),
        (Self::Exact, "Exact text"),
        (Self::Separator, "Separator"),
    ];
    fn label(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(k, _)| *k == self)
            .map_or("", |(_, l)| l)
    }
}

/// Blocks as pattern text (the shorthand shown under the blocks).
fn to_pattern(blocks: &[Block]) -> String {
    let mut out = String::new();
    for b in blocks {
        let escape = |s: &str| {
            s.chars()
                .flat_map(|c| {
                    if matches!(c, ']' | '\\' | '-' | '[') {
                        vec!['\\', c]
                    } else {
                        vec![c]
                    }
                })
                .collect::<String>()
        };
        out += &match b.kind {
            BlockKind::OneOf => format!("[{}]", escape(&b.value)),
            BlockKind::Letters => "A".into(),
            BlockKind::Digits => "#".into(),
            BlockKind::LettersOrDigits => "*".into(),
            BlockKind::Exact => format!("\"{}\"", b.value.replace('"', "")),
            BlockKind::Separator => "[ \\-]".into(),
        };
        if b.kind != BlockKind::Exact {
            out += &match (b.min, b.max) {
                (1, 1) => String::new(),
                (0, 1) => "?".into(),
                (a, c) if a == c => format!("{{{a}}}"),
                (a, c) => format!("{{{a},{c}}}"),
            };
        }
    }
    out
}

/// Pattern text as blocks, when it uses only what the blocks can show.
fn from_pattern(text: &str) -> Option<Vec<Block>> {
    let chars: Vec<char> = text.chars().collect();
    let mut blocks: Vec<Block> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let (kind, value) = match chars[i] {
            '#' => (BlockKind::Digits, String::new()),
            'A' => (BlockKind::Letters, String::new()),
            '*' => (BlockKind::LettersOrDigits, String::new()),
            '[' => {
                let mut value = String::new();
                i += 1;
                while i < chars.len() && chars[i] != ']' {
                    if chars[i] == '\\' {
                        i += 1;
                    }
                    value.push(*chars.get(i)?);
                    i += 1;
                }
                chars.get(i)?;
                if value == " -" {
                    (BlockKind::Separator, String::new())
                } else {
                    (BlockKind::OneOf, value)
                }
            }
            '"' => {
                let end = chars[i + 1..].iter().position(|c| *c == '"')? + i + 1;
                let value: String = chars[i + 1..end].iter().collect();
                i = end;
                (BlockKind::Exact, value)
            }
            _ => return None,
        };
        i += 1;
        let (min, max) = match chars.get(i) {
            Some('?') => {
                i += 1;
                (0, 1)
            }
            Some('{') => {
                let end = chars[i..].iter().position(|c| *c == '}')? + i;
                let inner: String = chars[i + 1..end].iter().collect();
                i = end + 1;
                match inner.split_once(',') {
                    None => {
                        let n = inner.trim().parse().ok()?;
                        (n, n)
                    }
                    Some((a, b)) => (a.trim().parse().ok()?, b.trim().parse().ok()?),
                }
            }
            _ => (1, 1),
        };
        // "#####" is one Digits block of five, not five blocks.
        match blocks.last_mut() {
            Some(last)
                if last.kind == kind
                    && matches!(
                        kind,
                        BlockKind::Digits | BlockKind::Letters | BlockKind::LettersOrDigits
                    ) =>
            {
                last.min += min;
                last.max += max;
            }
            _ => blocks.push(Block {
                kind,
                value,
                min,
                max,
            }),
        }
    }
    Some(blocks)
}

#[derive(Clone)]
struct Builder {
    index: Option<usize>,
    draft: CustomType,
    blocks: Vec<Block>,
    as_text: bool,
    test: String,
    labels_on: bool,
    error: String,
    focus: bool,
}
fn builder_id() -> egui::Id {
    egui::Id::unique("clipdesigner-custom-type-builder")
}
/// Opens the custom type builder for a new type or for saved type `index`.
pub(crate) fn open_builder(ctx: &egui::Context, index: Option<usize>, ty: Option<CustomType>) {
    let draft = ty.unwrap_or_else(|| CustomType::new("", "[WE]#####"));
    let blocks = from_pattern(&draft.pattern);
    let builder = Builder {
        index,
        as_text: blocks.is_none(),
        blocks: blocks.unwrap_or_default(),
        labels_on: draft.near_label.is_some(),
        draft,
        test: "Reviewed by W74828 and E84839; escalated to X12345 on ticket OPS-2201. Badge AW748281 stays.".into(),
        error: String::new(),
        focus: true,
    };
    ctx.data_mut(|d| d.insert_temp(builder_id(), builder));
}

/// The New / Edit custom type dialog. Returns true when `list` changed.
pub(crate) fn type_builder(ctx: &egui::Context, list: &mut Vec<CustomType>) -> bool {
    let Some(mut b) = ctx.data(|d| d.get_temp::<Builder>(builder_id())) else {
        return false;
    };
    let mut save = false;
    let mut cancel = false;
    let mut delete = false;
    let response = crate::controls::Modal::new(egui::Id::unique("custom-type-builder")).show(ctx, |ui| {
        ui.set_width(560_f32.min(ctx.content_rect().width() - 48.));
        crate::controls::dialog_title(
            ui,
            if b.index.is_some() { "Edit custom type" } else { "New custom type" },
        );
        crate::controls::dialog_section(ui, "Name", None);
        let name = ui.add(
            crate::unicode_input::TextEdit::singleline(&mut b.draft.name)
                .hint_text("Employee ID")
                .char_limit(60)
                .desired_width(f32::INFINITY),
        );
        crate::controls::focus_once(&name, &mut b.focus);
        ui.add_space(6.);
        ui.horizontal(|ui| {
            crate::controls::dialog_section_label(ui, "Pattern",Some("# a digit, A a letter, * a letter or digit, [WE] one of, \"INV\" exact text, {5} or {2,4} a count, | between alternatives."));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = if b.as_text { "Edit as blocks" } else { "Edit as text" };
                if ui.small_button(label).clicked() {
                    if b.as_text {
                        match from_pattern(&b.draft.pattern) {
                            Some(blocks) => {
                                b.blocks = blocks;
                                b.as_text = false;
                            }
                            None => b.error = "This pattern uses text features the blocks can't show; keep editing it as text.".into(),
                        }
                    } else {
                        b.as_text = true;
                    }
                }
            });
        });
        if b.as_text {
            ui.add(
                egui::TextEdit::singleline(&mut b.draft.pattern)
                    .font(egui::TextStyle::Monospace)
                    .char_limit(400)
                    .desired_width(f32::INFINITY),
            );
        } else {
            let mut remove = None;
            ui.horizontal_wrapped(|ui| {
                for (i, block) in b.blocks.iter_mut().enumerate() {
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(58, 66, 70))
                        .stroke(egui::Stroke::new(1., egui::Color32::from_rgb(74, 84, 88)))
                        .corner_radius(8)
                        .inner_margin(egui::Margin::symmetric(8, 4))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(block.kind.label()).size(11.5).color(egui::Color32::from_rgb(142, 154, 157)));
                                if matches!(block.kind, BlockKind::OneOf | BlockKind::Exact) {
                                    ui.add(egui::TextEdit::singleline(&mut block.value).desired_width(56.).char_limit(40));
                                }
                                if block.kind != BlockKind::Exact {
                                    ui.label("×");
                                    ui.add(egui::DragValue::new(&mut block.min).range(0..=64));
                                    if block.max != block.min || ui.input(|i| i.modifiers.shift) {
                                        ui.label("–");
                                    }
                                    ui.add(egui::DragValue::new(&mut block.max).range(block.min.max(1)..=64));
                                }
                                if ui.small_button("✕").tip("Remove this block").clicked() {
                                    remove = Some(i);
                                }
                            });
                        });
                }
                ui.menu_button(egui::RichText::new("+ Add block").color(crate::shell::ACCENT), |ui| {
                    for (kind, label) in BlockKind::ALL {
                        if crate::controls::menu_action(ui, label).clicked() {
                            let (min, max) = match kind {
                                BlockKind::Separator => (0, 1),
                                BlockKind::Digits => (5, 5),
                                _ => (1, 1),
                            };
                            b.blocks.push(Block { kind, value: String::new(), min, max });
                            ui.close();
                        }
                    }
                });
            });
            if let Some(i) = remove {
                b.blocks.remove(i);
            }
            b.draft.pattern = to_pattern(&b.blocks);
            ui.label(
                egui::RichText::new(format!("Shorthand  {}", b.draft.pattern))
                    .monospace()
                    .size(11.5)
                    .color(egui::Color32::from_rgb(142, 154, 157)),
            );
        }
        ui.add_space(4.);
        ui.horizontal_wrapped(|ui| {
            crate::controls::toggle(ui, &mut b.draft.ignore_case, "Ignore case");
            crate::controls::toggle(ui, &mut b.draft.whole_words, "Whole words only");
            crate::controls::toggle(ui, &mut b.draft.separators, "Allow spaces or dashes")
                .tip("W-74828 and W 74828 also match.");
            crate::controls::toggle(ui, &mut b.draft.ocr_tolerant, "Tolerate OCR mix-ups")
                .tip("O and 0, I and 1, S and 5, B and 8 count as each other in screenshots and video.");
            crate::controls::toggle(ui, &mut b.labels_on, "Only near a label");
        });
        if b.labels_on {
            let mut labels = b.draft.near_label.clone().unwrap_or_default();
            ui.add(
                crate::unicode_input::TextEdit::singleline(&mut labels)
                    .hint_text("Employee ID, Badge")
                    .char_limit(200)
                    .desired_width(f32::INFINITY),
            );
            b.draft.near_label = Some(labels);
        } else {
            b.draft.near_label = None;
        }
        crate::controls::dialog_section(ui, "Test", Some("Paste sample text; matches highlight as you edit."));
        ui.add(
            crate::unicode_input::TextEdit::multiline(&mut b.test)
                .desired_rows(2)
                .char_limit(4000)
                .desired_width(f32::INFINITY),
        );
        let mut probe = b.draft.clone();
        if probe.near_label.as_deref().is_some_and(|l| l.trim().is_empty()) {
            probe.near_label = None;
        }
        match probe.find(&b.test) {
            Ok(found) => {
                let mut job = egui::text::LayoutJob::default();
                let mut at = 0;
                let plain = egui::TextFormat::simple(egui::FontId::proportional(13.), egui::Color32::from_rgb(42, 50, 56));
                let mark = egui::TextFormat {
                    background: egui::Color32::from_rgb(255, 224, 138),
                    ..plain.clone()
                };
                for r in &found {
                    job.append(&b.test[at..r.start], 0., plain.clone());
                    job.append(&b.test[r.clone()], 0., mark.clone());
                    at = r.end;
                }
                job.append(&b.test[at..], 0., plain);
                job.wrap.max_width = ui.available_width() - 20.;
                egui::Frame::new()
                    .fill(egui::Color32::WHITE)
                    .corner_radius(6)
                    .inner_margin(10)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(job);
                    });
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new(format!("{} {}", found.len(), if found.len() == 1 { "match" } else { "matches" }))
                            .color(egui::Color32::from_rgb(79, 194, 138)),
                    );
                    for e in probe.examples(3) {
                        ui.label(egui::RichText::new(e).monospace().size(12.));
                    }
                });
            }
            Err(e) => {
                ui.colored_label(egui::Color32::from_rgb(233, 166, 64), e.to_string());
            }
        }
        egui::CollapsingHeader::new("Start from a ready-made type")
            .id_salt("custom-ready-made")
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for t in custom::ready_made() {
                        if ui.button(&t.name).tip(&t.pattern).clicked() {
                            b.blocks = from_pattern(&t.pattern).unwrap_or_default();
                            b.as_text = from_pattern(&t.pattern).is_none();
                            b.labels_on = t.near_label.is_some();
                            b.draft = t;
                        }
                    }
                });
            });
        if !b.error.is_empty() {
            ui.colored_label(egui::Color32::LIGHT_RED, &b.error);
        }
        ui.add_space(8.);
        crate::panel_ui::divider(ui);
        ui.horizontal(|ui| {
            if b.index.is_some() && ui.add(crate::controls::dialog_button("Delete", false)).clicked() {
                delete = true;
            }
            ui.label(
                egui::RichText::new("Saved types appear under Custom types in image and video scans.")
                    .size(11.5)
                    .color(egui::Color32::from_rgb(142, 154, 157)),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(crate::controls::dialog_button("Save type", true)).clicked() {
                    save = true;
                }
                if ui.add(crate::controls::dialog_button("Cancel", false)).clicked() {
                    cancel = true;
                }
            });
        });
    });
    let mut changed = false;
    if save {
        let mut draft = b.draft.clone();
        draft.name = draft.name.trim().to_owned();
        if draft
            .near_label
            .as_deref()
            .is_some_and(|l| l.trim().is_empty())
        {
            draft.near_label = None;
        }
        let mut next = list.clone();
        match b.index {
            Some(i) if i < next.len() => next[i] = draft,
            _ => next.push(draft),
        }
        match custom::validate_all(&next) {
            Ok(()) => {
                *list = next;
                changed = true;
                cancel = true;
            }
            Err(e) => b.error = crate::guided::friendly(&e.to_string()),
        }
    }
    if delete && let Some(i) = b.index.filter(|i| *i < list.len()) {
        list.remove(i);
        changed = true;
        cancel = true;
    }
    if cancel || response.should_close() {
        ctx.data_mut(|d| d.remove::<Builder>(builder_id()));
    } else {
        ctx.data_mut(|d| d.insert_temp(builder_id(), b));
    }
    changed
}
/// Redact, Blur or Pixelate, and for the last two the preset strength tiles.
pub(crate) fn cover(ui: &mut egui::Ui, cover: &mut Cover) {
    crate::controls::dialog_section(
        ui,
        "Cover with",
        Some(
            "Redact replaces the pixels in the exported file. Blur and Pixelate are visual, and every cover stays editable.",
        ),
    );
    crate::controls::segments(ui, |ui| {
        for (kind, label, tip) in [
            (
                Kind::Redact,
                "Redact",
                "Cover each finding with a solid block.",
            ),
            (Kind::Blur, "Blur", "Soften each finding."),
            (
                Kind::Pixelate,
                "Pixelate",
                "Turn each finding into large blocks.",
            ),
        ] {
            if ui
                .selectable_label(cover.kind == kind, label)
                .tip(tip)
                .clicked()
            {
                cover.kind = kind;
            }
        }
    });
    if cover.kind == Kind::Redact {
        return;
    }
    crate::controls::dialog_section(ui, "Strength", None);
    ui.horizontal_wrapped(|ui| {
        for style in capture_core::object_styles::presets(cover.kind) {
            let label = format!("Strength {}", style.width);
            if crate::style_tile::tile(ui, &style, &label, style.width == cover.width()).clicked() {
                match cover.kind {
                    Kind::Blur => cover.blur = style.width,
                    _ => cover.pixelate = style.width,
                }
            }
        }
    });
    ui.label(
        egui::RichText::new(
            "Blurred or pixelated text can sometimes be recovered. Use Redact for passwords, account numbers and IDs.",
        )
        .size(11.5)
        .color(egui::Color32::from_rgb(142, 154, 157)),
    );
}
/// A review picture with the chosen cover on `boxes` (image pixels). Reduced
/// first, so choosing a cover stays instant on a large picture.
pub(crate) fn preview(
    base: &image::RgbaImage,
    boxes: &[[f32; 4]],
    cover: Cover,
) -> image::RgbaImage {
    let scale = (640. / base.width().max(1) as f32).min(1.);
    let small = if scale < 1. {
        image::imageops::resize(
            base,
            ((base.width() as f32 * scale) as u32).max(1),
            ((base.height() as f32 * scale) as u32).max(1),
            image::imageops::FilterType::Triangle,
        )
    } else {
        base.clone()
    };
    let Ok(mut document) = capture_core::Document::from_image(small.clone()) else {
        return small;
    };
    let width = ((cover.width() as f32 * scale).round() as u32).max(1);
    document.annotations = capture_core::privacy::covers(
        0,
        boxes.iter().map(|b| b.map(|v| v * scale)),
        cover.kind,
        width,
    );
    document.promote_features();
    document.render().unwrap_or(small)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn phrases_split_at_commas_collapse_spaces_and_skip_duplicates() {
        let mut phrases = String::new();
        add_phrases(&mut phrases, "Jane Doe, W74828; Acme   Corp");
        assert_eq!(phrases, "Jane Doe\nW74828\nAcme Corp");
        add_phrases(&mut phrases, "jane doe, ,  ;Project Nightjar");
        assert_eq!(phrases, "Jane Doe\nW74828\nAcme Corp\nProject Nightjar");
        let mut many = String::new();
        add_phrases(
            &mut many,
            &(0..150)
                .map(|i| format!("p{i}"))
                .collect::<Vec<_>>()
                .join(","),
        );
        assert_eq!(
            many.lines().count(),
            100,
            "the scan's limit of 100 phrases holds"
        );
    }
    #[test]
    fn typing_a_comma_list_makes_chips_and_backspace_removes_the_last() {
        let ctx = egui::Context::default();
        let mut phrases = String::new();
        let frame = |events: Vec<egui::Event>, phrases: &mut String| {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    events,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(500., 300.),
                    )),
                    ..Default::default()
                },
                |ui| {
                    let r = phrase_chips(ui, phrases);
                    r.request_focus();
                },
            );
            out.textures_delta.clear();
        };
        frame(vec![], &mut phrases);
        frame(
            vec![egui::Event::Text("Jane Doe, W74828;".into())],
            &mut phrases,
        );
        frame(vec![], &mut phrases);
        assert_eq!(phrases, "Jane Doe\nW74828");
        let key = |key, pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        frame(
            vec![
                key(egui::Key::Backspace, true),
                key(egui::Key::Backspace, false),
            ],
            &mut phrases,
        );
        frame(vec![], &mut phrases);
        assert_eq!(phrases, "Jane Doe");
    }
    /// BW1: Enter, commas and semicolons add chips and leave the cursor in
    /// the box; Enter never reaches the dialog's main button; chips sit side
    /// by side until a row is full.
    #[test]
    fn the_cursor_stays_in_the_phrase_box_and_chips_share_rows() {
        let ctx = egui::Context::default();
        let mut phrases = String::new();
        let frame = |events: Vec<egui::Event>, phrases: &mut String, focus: bool| {
            let mut field = None;
            let mut out = ctx.run_ui(
                egui::RawInput {
                    events,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(640., 400.),
                    )),
                    ..Default::default()
                },
                |ui| {
                    let r = phrase_chips(ui, phrases);
                    if focus {
                        r.request_focus();
                    }
                    field = Some((r.id, r.rect));
                },
            );
            out.textures_delta.clear();
            field.unwrap()
        };
        let key = |key, pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        frame(vec![], &mut phrases, true);
        let first_row = frame(vec![], &mut phrases, false).1.top();
        frame(
            vec![egui::Event::Text("Jane Doe".into())],
            &mut phrases,
            false,
        );
        ctx.data_mut(|d| d.insert_temp(egui::Id::unique("dialog-enter"), true));
        frame(
            vec![key(egui::Key::Enter, true), key(egui::Key::Enter, false)],
            &mut phrases,
            false,
        );
        assert_eq!(phrases, "Jane Doe");
        assert_eq!(
            ctx.data(|d| d.get_temp::<bool>(egui::Id::unique("dialog-enter"))),
            Some(false),
            "Enter is kept from the Scan button"
        );
        let (id, _) = frame(vec![], &mut phrases, false);
        assert!(
            ctx.memory(|m| m.has_focus(id)),
            "the cursor stays after Enter"
        );
        frame(
            vec![egui::Event::Text("W74828;".into())],
            &mut phrases,
            false,
        );
        frame(vec![egui::Event::Text("Acme,".into())], &mut phrases, false);
        let (id, rect) = frame(vec![], &mut phrases, false);
        assert_eq!(phrases, "Jane Doe\nW74828\nAcme");
        assert!(
            ctx.memory(|m| m.has_focus(id)),
            "the cursor stays after a comma"
        );
        assert!(
            (rect.top() - first_row).abs() < 6. && rect.left() > 150.,
            "three short chips and the box share the first row: {first_row} vs {rect:?}"
        );
    }
    #[test]
    fn blocks_and_shorthand_round_trip_and_the_builder_saves_a_type() {
        let blocks = from_pattern("[WE]#####").unwrap();
        assert_eq!(blocks.len(), 2, "one-of plus five digits");
        assert_eq!(
            (blocks[1].kind, blocks[1].min, blocks[1].max),
            (BlockKind::Digits, 5, 5)
        );
        let text = to_pattern(&blocks);
        assert_eq!(text, "[WE]#{5}");
        let t = CustomType::new("Employee ID", &text);
        let sample = "Reviewed by W74828 and E84839; escalated to X12345.";
        assert_eq!(t.find(sample).unwrap().len(), 2);
        assert!(
            from_pattern("\"INV\"-?#{4,8}|\"PO\"#{4}").is_none(),
            "alternatives stay as text"
        );
        // The builder saves a new type into the list it was given.
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        crate::text_layout::configure(&ctx);
        ctx.style_mut_of(egui::Theme::Dark, |style| style.animation_time = 0.);
        let mut list = Vec::new();
        open_builder(
            &ctx,
            None,
            Some(CustomType::new("Employee ID", "[WE]#####")),
        );
        let size = egui::vec2(1000., 900.);
        let textures = std::cell::RefCell::new(std::collections::HashMap::<
            egui::TextureId,
            Vec<egui::epaint::ImageDelta>,
        >::new());
        let mut options = Options::default();
        let mut phrases = String::new();
        let mut frame = |list: &mut Vec<CustomType>, events: Vec<egui::Event>| {
            let mut changed = false;
            let mut output = crate::property_layout::tests::PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| {
                    crate::controls::prepare_dialog_keys(ui.ctx());
                    options.custom = list.clone();
                    let (_, c) = look_for(ui, &mut options, &mut phrases);
                    if c {
                        *list = options.custom.clone();
                        changed = true;
                    }
                },
            ));
            for (id, deltas) in &output.0.textures_delta.set {
                textures
                    .borrow_mut()
                    .entry(*id)
                    .or_default()
                    .extend(deltas.clone());
            }
            output.0.textures_delta.set = textures
                .borrow()
                .iter()
                .map(|(id, deltas)| (*id, deltas.iter().cloned().collect()))
                .collect();
            (output, changed)
        };
        frame(&mut list, vec![]);
        frame(&mut list, vec![]);
        let (first, _) = frame(&mut list, vec![]);
        let (shown, _) = frame(&mut list, vec![]);
        if let Ok(root) = std::env::var("CLIPDESIGNER_PRESET_EVIDENCE") {
            crate::property_layout::tests::dump_meshes(
                &ctx,
                &root,
                "custom-type-builder",
                first,
                shown,
            );
        }
        // Save through the builder's state, as the Save type button does.
        let mut b = ctx.data(|d| d.get_temp::<Builder>(builder_id())).unwrap();
        b.draft.name = "Employee ID".into();
        list.push(b.draft.clone());
        assert!(custom::validate_all(&list).is_ok());
        assert_eq!(list[0].find(sample).unwrap().len(), 2);
    }
    #[test]
    fn the_button_names_the_chosen_cover_and_redact_is_the_default() {
        let mut c = Cover::default();
        assert_eq!(c.action(3), "Add 3 redactions");
        c.kind = Kind::Blur;
        assert_eq!((c.action(1).as_str(), c.width()), ("Add 1 blur", 4));
        c.kind = Kind::Pixelate;
        assert_eq!((c.action(2).as_str(), c.width()), ("Add 2 pixelations", 8));
    }
    #[test]
    fn the_preview_shows_the_chosen_cover_only_on_its_boxes() {
        let base = image::RgbaImage::from_fn(1280, 720, |x, y| {
            let v = if x % 8 < 4 && y % 8 < 4 { 240 } else { 20 };
            image::Rgba([v, v, v, 255])
        });
        let boxes = [[200., 100., 300., 60.]];
        let solid = preview(&base, &boxes, Cover::default());
        assert_eq!(solid.width(), 640);
        assert_eq!(solid.get_pixel(150, 60).0[..3], [0, 0, 0]);
        let clear = solid.get_pixel(400, 300);
        assert_eq!(
            clear,
            preview(&base, &boxes, Cover::default()).get_pixel(400, 300)
        );
        let blurred = preview(
            &base,
            &boxes,
            Cover {
                kind: Kind::Blur,
                ..Default::default()
            },
        );
        assert_ne!(blurred.get_pixel(150, 60).0[..3], [0, 0, 0]);
    }
}
