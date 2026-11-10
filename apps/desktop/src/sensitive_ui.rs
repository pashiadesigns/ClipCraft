//! Parts shared by the image and video Find Sensitive Info dialogs (approved
//! sensitive-info-proposal): what to look for, and what covers the findings.
use crate::tips::Tip as _;
use crate::{Kind, egui};
use capture_core::privacy::Options;

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
/// The categories and exact phrases to search for.
pub(crate) fn look_for(
    ui: &mut egui::Ui,
    options: &mut Options,
    phrases: &mut String,
) -> egui::Response {
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
    crate::controls::dialog_section(ui, "Exact names or phrases", None);
    ui.add(
        crate::unicode_input::TextEdit::multiline(phrases)
            .hint_text("One per line")
            .desired_rows(2)
            .desired_width(f32::INFINITY)
            .char_limit(10000),
    )
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
    ui.horizontal(|ui| {
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
