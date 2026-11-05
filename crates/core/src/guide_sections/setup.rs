//! Persisted print geometry. Distances use points (1/72 inch), independent of pixels.
use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Alignment {
    #[default]
    Left,
    Center,
    Right,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RunningText {
    pub enabled: bool,
    pub text: String,
    pub alignment: Alignment,
    pub distance: f64,
}
impl Default for RunningText {
    fn default() -> Self {
        Self {
            enabled: false,
            text: String::new(),
            alignment: Alignment::Left,
            distance: 18.,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NumberStyle {
    Decimal,
    Roman,
    Letters,
    #[default]
    PageTotal,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PageSetup {
    pub size: [u32; 2],
    pub cropped: bool,
    pub content_shift: [f32; 2],
    pub ppi: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub physical_points: Option<[f64; 2]>,
    pub centimeters: bool,
    /// Top, bottom, left, right in points.
    pub margins: [f64; 4],
    pub linked_margins: bool,
    pub header: RunningText,
    pub footer: RunningText,
    pub numbers: bool,
    pub number_style: NumberStyle,
    pub number_in_header: bool,
    pub number_alignment: Alignment,
    pub number_start: u32,
    pub number_first: bool,
    pub bleed: f64,
    pub crop_marks: bool,
    pub mark_offset: f64,
    pub mark_length: f64,
    pub mark_weight: f64,
}
impl Default for PageSetup {
    fn default() -> Self {
        Self {
            size: [816, 1056],
            cropped: false,
            content_shift: [0.; 2],
            ppi: 96.,
            physical_points: None,
            centimeters: false,
            margins: [36.; 4],
            linked_margins: true,
            header: RunningText::default(),
            footer: RunningText::default(),
            numbers: false,
            number_style: NumberStyle::PageTotal,
            number_in_header: false,
            number_alignment: Alignment::Right,
            number_start: 1,
            number_first: true,
            bleed: 0.,
            crop_marks: false,
            mark_offset: 9.,
            mark_length: 18.,
            mark_weight: 0.25,
        }
    }
}
impl PageSetup {
    pub fn from_pixels(size: [u32; 2]) -> Self {
        Self {
            size,
            margins: [0.; 4],
            ..Self::default()
        }
    }
    pub fn trim_points(&self) -> [f64; 2] {
        self.physical_points
            .unwrap_or_else(|| self.size.map(|n| n as f64 * 72. / self.ppi))
    }
    pub fn surround(&self) -> f64 {
        if self.crop_marks {
            self.bleed
                .max(self.mark_offset + self.mark_length + self.mark_weight)
        } else {
            self.bleed
        }
    }
    pub fn output_points(&self) -> [f64; 2] {
        self.trim_points().map(|n| n + 2. * self.surround())
    }
    pub fn pixel_margins(&self) -> [f32; 4] {
        self.margins.map(|n| (n * self.ppi / 72.) as f32)
    }
    pub fn validate(&self) -> Result<()> {
        if !self
            .content_shift
            .iter()
            .all(|n| n.is_finite() && n.abs() <= 1_000_000.)
        {
            return Err(Error::Invalid("Invalid cropped page offset".into()));
        }
        let trim = self.trim_points();
        let error = if !self.ppi.is_finite() || !(24.0..=2400.0).contains(&self.ppi) {
            Some("Paper: set Resolution to 24–2400 PPI.")
        } else if self.size.iter().any(|n| !(128..=32768).contains(n)) {
            Some("Paper: set Width and Height to 128–32768 pixels each.")
        } else if trim.iter().any(|n| !n.is_finite() || *n <= 0.)
            || (0..2).any(|i| (trim[i] * self.ppi / 72. - self.size[i] as f64).abs() > 0.51)
        {
            Some(
                "Paper: re-enter Width and Height so physical dimensions match the pixel size and Resolution.",
            )
        } else if self.margins.iter().any(|n| !n.is_finite() || *n < 0.) {
            Some("Layout: set each margin to zero or a positive distance.")
        } else if self.margins[0] + self.margins[1] + 36. > trim[1] {
            Some(
                "Layout: reduce Top/Bottom margins or increase Paper Height to leave at least 0.5 in (1.27 cm) of content.",
            )
        } else if self.margins[2] + self.margins[3] + 36. > trim[0] {
            Some(
                "Layout: reduce Left/Right margins or increase Paper Width to leave at least 0.5 in (1.27 cm) of content.",
            )
        } else if [
            self.bleed,
            self.mark_offset,
            self.mark_length,
            self.mark_weight,
        ]
        .iter()
        .any(|n| !n.is_finite() || *n < 0. || *n > 144.)
        {
            Some(
                "Print Marks: set Bleed, Offset, Length and Weight between 0 and 144 pt (2 in / 5.08 cm).",
            )
        } else if self.crop_marks && self.mark_offset < self.bleed {
            Some("Print Marks: increase Offset to at least the Bleed distance, or reduce Bleed.")
        } else if self.crop_marks && (self.mark_length <= 0. || self.mark_weight <= 0.) {
            Some("Print Marks: set Length and Weight above zero, or turn off Crop marks.")
        } else if self.number_start == 0 || self.number_start > 100000 {
            Some("Headers & Footers: set the starting page number to 1–100000.")
        } else if self.output_points().iter().any(|n| *n > 14400.) {
            Some(
                "Paper: reduce physical Width/Height or Print Marks spacing so output fits within 200 in (508 cm).",
            )
        } else {
            None
        };
        if let Some(error) = error {
            return Err(Error::Invalid(error.into()));
        }
        for (name, text) in [("Header", &self.header), ("Footer", &self.footer)] {
            if text.text.chars().count() > 500 {
                return Err(Error::Invalid(format!(
                    "Headers & Footers: shorten {name} text to 500 characters or fewer."
                )));
            }
            if !text.distance.is_finite()
                || text.distance < 0.
                || text.distance + 12. >= trim[1] / 2.
            {
                return Err(Error::Invalid(format!(
                    "Headers & Footers: reduce {name} edge distance to below {:.1} pt and keep it at or above zero.",
                    (trim[1] / 2. - 12.).max(0.)
                )));
            }
        }
        Ok(())
    }
    pub fn number(&self, index: usize, total: usize) -> String {
        let n = self.number_start + index as u32;
        match self.number_style {
            NumberStyle::Decimal => n.to_string(),
            NumberStyle::PageTotal => {
                format!("Page {n} of {}", self.number_start + total as u32 - 1)
            }
            NumberStyle::Letters => {
                let mut n = n;
                let mut s = String::new();
                while n > 0 {
                    n -= 1;
                    s.insert(0, (b'A' + (n % 26) as u8) as char);
                    n /= 26;
                }
                s
            }
            NumberStyle::Roman => {
                let mut n = n;
                let mut s = String::new();
                for (v, t) in [
                    (1000, "M"),
                    (900, "CM"),
                    (500, "D"),
                    (400, "CD"),
                    (100, "C"),
                    (90, "XC"),
                    (50, "L"),
                    (40, "XL"),
                    (10, "X"),
                    (9, "IX"),
                    (5, "V"),
                    (4, "IV"),
                    (1, "I"),
                ] {
                    while n >= v {
                        s.push_str(t);
                        n -= v;
                    }
                }
                s
            }
        }
    }
}
impl Layout {
    pub fn page_size(&self, page: usize, fallback: [u32; 2]) -> [u32; 2] {
        self.pages
            .as_ref()
            .map_or(fallback, |p| p.setup.get(page).map_or(p.size, |s| s.size))
    }
    pub fn page_setup(&self, page: usize) -> Option<&PageSetup> {
        self.pages.as_ref()?.setup.get(page)
    }
    pub fn content_top(&self, page: usize) -> f32 {
        self.page_setup(page).map_or(self.top, |s| {
            self.top.max(s.pixel_margins()[0]) - s.content_shift[1]
        })
    }
    pub fn content_left(&self, page: usize) -> f32 {
        self.page_setup(page)
            .map_or(0., |s| s.pixel_margins()[2] - s.content_shift[0])
    }
    pub fn content_bottom(&self, page: usize) -> f32 {
        self.page_size(page, [0; 2])[1] as f32
            - self.page_setup(page).map_or(0., |s| s.pixel_margins()[1])
    }
    pub fn content_width(&self, page: usize, fallback: u32) -> f32 {
        self.page_size(page, [fallback, 0])[0] as f32
            - self.page_setup(page).map_or(0., |s| {
                let m = s.pixel_margins();
                m[2] + m[3]
            })
    }
}

impl Document {
    pub(super) fn edit_page_setup(
        &self,
        settings: &PageSetup,
        selected: &[usize],
        scale: bool,
        automatic: bool,
        distribute: bool,
    ) -> Result<Self> {
        settings.validate()?;
        let mut next = self.clone();
        if next.guide.is_none() {
            next.guide = Some(Layout {
                pages: None,
                template: None,
                top: 0.,
                sections: vec![],
            });
        }
        next.promote_features();
        // Reuse migration/base-image conversion and its locked-object safeguards.
        next = next.edit_pages(&Action::MovePage { page: 0, index: 0 })?;
        let old = next.guide.clone().unwrap();
        if selected.is_empty() || selected.iter().any(|i| *i >= old.page_count()) {
            return Err(Error::Invalid(
                "Choose existing pages to apply Page Setup".into(),
            ));
        }
        let mut layout = old.clone();
        let mut origins = old.section_origins();
        let p = layout.pages.as_mut().unwrap();
        if p.setup.is_empty() {
            p.setup = vec![PageSetup::from_pixels(p.size); p.count];
        }
        for &i in selected {
            p.setup[i] = settings.clone();
        }
        p.auto_overflow = automatic;
        let original_pages: BTreeMap<_, _> = next
            .annotations
            .iter()
            .map(|a| (a.id, next.object_page(a)))
            .collect();
        for page in 0..old.page_count() {
            let factor = if scale && selected.contains(&page) {
                layout.content_width(page, next.width) / old.content_width(page, next.width)
            } else {
                1.
            };
            let from = [
                old.page_offset(page)[0] + old.content_left(page),
                old.page_offset(page)[1] + old.content_top(page),
            ];
            let to = [
                layout.page_offset(page)[0] + layout.content_left(page),
                layout.page_offset(page)[1] + layout.content_top(page),
            ];
            let ids: BTreeSet<_> = old
                .sections
                .iter()
                .filter(|s| s.page == page)
                .flat_map(|s| s.objects.iter().copied())
                .collect();
            for a in &mut next.annotations {
                if ids.contains(&a.id)
                    || (!old.sections.iter().any(|s| s.objects.contains(&a.id))
                        && original_pages[&a.id] == page)
                {
                    let mut v = crate::groups::scaled(std::slice::from_ref(a), from, factor)?;
                    if !ids.contains(&a.id) {
                        v = crate::canvas_expansion::translated(
                            &v,
                            [to[0] - from[0], to[1] - from[1]],
                        )?;
                    }
                    *a = v.remove(0);
                }
            }
            for s in layout.sections.iter_mut().filter(|s| s.page == page) {
                s.height = (s.height * factor).max(48.);
                let origin = origins.get_mut(&s.id).unwrap();
                for axis in 0..2 {
                    origin[axis] = from[axis] + (origin[axis] - from[axis]) * factor;
                }
            }
        }
        if distribute {
            for s in &mut layout.sections {
                s.page = 0;
            }
        }
        self.finish_guide(next, layout, origins)
    }
}

impl Document {
    /// Generated page furniture participates in retained vector rendering, never the Objects tree.
    pub fn page_furniture(&self) -> Vec<Annotation> {
        let mut result = vec![];
        let Some(g) = &self.guide else {
            return result;
        };
        for page in 0..g.page_count() {
            let Some(s) = g.page_setup(page) else {
                continue;
            };
            let origin = g.page_offset(page);
            let k = (s.ppi / 72.) as f32;
            for (text, rect, align) in s.running_text(page, g.page_count()) {
                let mut a = crate::templates::text(
                    u64::MAX - result.len() as u64,
                    [
                        origin[0] + rect[0] as f32 * k,
                        origin[1] + rect[1] as f32 * k,
                        rect[2] as f32 * k,
                        rect[3] as f32 * k,
                    ],
                    &text,
                    11. * k,
                    false,
                );
                let t = a.text.as_mut().unwrap();
                t.padding = 0.;
                t.horizontal = match align {
                    Alignment::Left => crate::text::HorizontalAlignment::Left,
                    Alignment::Center => crate::text::HorizontalAlignment::Center,
                    Alignment::Right => crate::text::HorizontalAlignment::Right,
                };
                result.push(a);
            }
        }
        result
    }
}

impl PageSetup {
    pub fn running_text(&self, page: usize, count: usize) -> Vec<(String, [f64; 4], Alignment)> {
        let mut result = vec![];
        let trim = self.trim_points();
        let width = trim[0] - self.margins[2] - self.margins[3];
        for (header, text) in [(true, &self.header), (false, &self.footer)] {
            let numbered =
                self.numbers && self.number_in_header == header && (page != 0 || self.number_first);
            let y = if header {
                text.distance
            } else {
                trim[1] - text.distance - 14.
            };
            let has_text = text.enabled && !text.text.is_empty();
            if numbered && has_text && self.number_alignment == text.alignment {
                result.push((
                    format!("{}   {}", text.text, self.number(page, count)),
                    [self.margins[2], y, width, 16.],
                    text.alignment,
                ));
                continue;
            }
            let mut add = |content: String, align: Alignment| {
                let (x, w) = if numbered && has_text {
                    let lane = match align {
                        Alignment::Left => 0.,
                        Alignment::Center => 1.,
                        Alignment::Right => 2.,
                    };
                    (lane * width / 3., width / 3.)
                } else {
                    (0., width)
                };
                result.push((content, [self.margins[2] + x, y, w, 16.], align));
            };
            if has_text {
                add(text.text.clone(), text.alignment);
            }
            if numbered {
                add(self.number(page, count), self.number_alignment);
            }
        }
        result
    }
}
