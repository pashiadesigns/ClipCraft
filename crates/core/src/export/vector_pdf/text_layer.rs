//! Searchable text in PDF export (approved beta 6 tab 6): the words of PDF
//! pages, written invisibly (text render mode 3) over where they appear, so
//! they can be selected, searched and copied. A word that any redaction,
//! blur or pixelation touches is never written.
//!
//! The font is a Type 3 font whose glyphs are empty; a ToUnicode map gives
//! each code its character. Each font covers up to 254 characters, so a page
//! with many different characters uses several.
use super::*;
use std::collections::HashMap;

struct Fonts {
    /// Character to (font index, code).
    codes: HashMap<char, (usize, u8)>,
    /// Characters of each font, in code order (code = position + 1).
    fonts: Vec<Vec<char>>,
}
impl Fonts {
    fn code(&mut self, c: char) -> (usize, u8) {
        if let Some(found) = self.codes.get(&c) {
            return *found;
        }
        if self.fonts.last().is_none_or(|f| f.len() >= 254) {
            self.fonts.push(vec![]);
        }
        let font = self.fonts.len() - 1;
        self.fonts[font].push(c);
        let code = (font, self.fonts[font].len() as u8);
        self.codes.insert(c, code);
        code
    }
}

fn utf16(c: char) -> String {
    let mut units = [0u16; 2];
    c.encode_utf16(&mut units)
        .iter()
        .map(|u| format!("{u:04X}"))
        .collect()
}

pub(super) fn write(
    pdf: &mut Pdf,
    out: &mut String,
    document: &Document,
    covers: &[Rect],
) -> Result<()> {
    let (words, _) = crate::pdf_import::document_words(document);
    let mut fonts = Fonts {
        codes: HashMap::new(),
        fonts: vec![],
    };
    // (font, codes) runs per word, and where the word goes.
    let mut placed = vec![];
    for word in &words {
        let [x, y, w, h] = word.bounds.map(f64::from);
        let rect = Rect::new(x, y, x + w, y + h);
        if w <= 0. || h <= 0. || covers.iter().any(|c| c.intersect(rect).area() > 0.) {
            continue;
        }
        let chars: Vec<char> = word.text.chars().filter(|c| !c.is_control()).collect();
        if chars.is_empty() {
            continue;
        }
        let mut runs: Vec<(usize, Vec<u8>)> = vec![];
        // A real space after each word, so viewers see where words end; it
        // lies just past the word and covers nothing.
        for c in chars.iter().chain(std::iter::once(&' ')) {
            let (font, code) = fonts.code(*c);
            match runs.last_mut() {
                Some((f, codes)) if *f == font => codes.push(code),
                _ => runs.push((font, vec![code])),
            }
        }
        placed.push((rect, chars.len(), runs));
    }
    if placed.is_empty() {
        return Ok(());
    }
    // One empty glyph serves every code.
    let glyph = pdf.stream("", b"500 0 d0")?;
    let mut names = vec![];
    for chars in &fonts.fonts {
        let cmap = format!(
            "/CIDInit /ProcSet findresource begin 12 dict begin begincmap /CMapName /ClipDesignerText def /CMapType 2 def 1 begincodespacerange <00> <FF> endcodespacerange\n{} beginbfchar\n{}endbfchar endcmap CMapName currentdict /CMap defineresource pop end end",
            chars.len(),
            chars
                .iter()
                .enumerate()
                .map(|(i, c)| format!("<{:02X}> <{}>\n", i + 1, utf16(*c)))
                .collect::<String>()
        );
        let to_unicode = pdf.stream("", cmap.as_bytes())?;
        let id = pdf.add(
            format!(
                "<< /Type /Font /Subtype /Type3 /FontBBox [0 0 500 1000] /FontMatrix [0.001 0 0 0.001 0 0] /CharProcs << /g {glyph} 0 R >> /Encoding << /Type /Encoding /Differences [1 {}] >> /FirstChar 1 /LastChar {} /Widths [{}] /ToUnicode {to_unicode} 0 R /Resources << >> >>",
                "/g ".repeat(chars.len()).trim_end(),
                chars.len(),
                "500 ".repeat(chars.len()).trim_end()
            )
            .into_bytes(),
        );
        let name = format!("T{id}");
        pdf.fonts.push((name.clone(), id));
        names.push(name);
    }
    // The drawing is in document coordinates with y pointing down, so each
    // word's text matrix flips it back upright.
    out.push_str("q\n");
    for (rect, count, runs) in placed {
        let size = rect.height() * 0.8;
        let baseline = rect.y0 + rect.height() * 0.8;
        let natural = count as f64 * 0.5 * size;
        let scale = (rect.width() / natural.max(1e-6) * 100.).clamp(1., 10_000.);
        let _ = write!(
            out,
            "BT 3 Tr {scale:.3} Tz {size:.3} 0 0 {:.3} {:.3} {baseline:.3} Tm",
            -size, rect.x0
        );
        for (font, codes) in runs {
            let _ = write!(
                out,
                " /{} 1 Tf <{}> Tj",
                names[font],
                codes.iter().map(|c| format!("{c:02X}")).collect::<String>()
            );
        }
        out.push_str(" ET\n");
    }
    out.push_str("Q\n");
    Ok(())
}
