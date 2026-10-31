//! Optional recording context. Captions and crops never change source pixels.
use super::*;
#[derive(Clone, Debug, Default)]
pub struct Context {
    /// Local source-pixel bounds (left, top, right, bottom), usually a window.
    pub bounds: Option<[i32; 4]>,
    pub name: Option<String>,
}
#[derive(Clone, Debug)]
pub struct Header {
    pub title: String,
    pub author: Option<String>,
    pub date: String,
    pub seconds: u64,
}
impl Default for Header {
    fn default() -> Self {
        Self {
            title: "Step guide".into(),
            author: None,
            date: String::new(),
            seconds: 0,
        }
    }
}
/// Untrusted accessibility/OCR strings are bounded, flattened and validated
/// against the portable font. Unsupported names fall back to the gesture label.
pub fn label(name: &str) -> Option<String> {
    let name: String = name
        .split_whitespace()
        .flat_map(|s| std::iter::once(' ').chain(s.chars()))
        .skip(1)
        .take(96)
        .collect();
    (!name.is_empty()
        && crate::text::TextStyle {
            content: name.clone(),
            ..Default::default()
        }
        .validate()
        .is_ok())
    .then_some(name)
}
impl Context {
    pub fn view(&self, size: [u32; 2], gesture: Gesture) -> crate::image_frame::ImageView {
        let [l, t, r, b] = self
            .bounds
            .filter(|[l, t, r, b]| {
                r > l && b > t && *r > 0 && *b > 0 && *l < size[0] as i32 && *t < size[1] as i32
            })
            .unwrap_or([0, 0, size[0] as i32, size[1] as i32]);
        let mut lo = [l.max(0), t.max(0)];
        let mut hi = [r.min(size[0] as i32), b.min(size[1] as i32)];
        // Keep both ends of a drag, the cursor bitmap and some surrounding UI.
        for axis in 0..2 {
            lo[axis] = lo[axis]
                .min(gesture.start[axis].min(gesture.end[axis]) as i32 - 40)
                .max(0);
            hi[axis] = hi[axis]
                .max(gesture.start[axis].max(gesture.end[axis]) as i32 + 41)
                .min(size[axis] as i32);
            let minimum = [160, 96][axis].min(size[axis] as i32);
            if hi[axis] - lo[axis] < minimum {
                let mid = (lo[axis] + hi[axis]) / 2;
                lo[axis] = (mid - minimum / 2).clamp(0, size[axis] as i32 - minimum);
                hi[axis] = lo[axis] + minimum;
            }
        }
        // Start with a compact view around the actual interaction. Preserve both
        // ends of drags and all original pixels for later reveal/zoom-out.
        for axis in 0..2 {
            let span = gesture.start[axis].abs_diff(gesture.end[axis]) as i32 + 82;
            let cap = if axis == 0 {
                960
            } else {
                ((hi[0] - lo[0]) / 2).clamp(96, 480)
            };
            let extent = (hi[axis] - lo[axis]).min(cap.max(span));
            let center = ((gesture.start[axis] + gesture.end[axis]) / 2) as i32;
            lo[axis] = (center - extent / 2).clamp(lo[axis], hi[axis] - extent);
            hi[axis] = lo[axis] + extent;
        }
        crate::image_frame::ImageView {
            rect: [
                lo[0] as f32 / size[0] as f32,
                lo[1] as f32 / size[1] as f32,
                (hi[0] - lo[0]) as f32 / size[0] as f32,
                (hi[1] - lo[1]) as f32 / size[1] as f32,
            ],
        }
    }
}
impl Guide {
    pub fn set_header(&mut self, header: Header) -> Result<()> {
        for text in [&header.title, &header.date]
            .into_iter()
            .chain(header.author.iter())
        {
            crate::text::TextStyle {
                content: text.clone(),
                ..Default::default()
            }
            .validate()?;
        }
        self.header = Some(header);
        Ok(())
    }
    pub fn set_name(&mut self, index: usize, name: &str) -> bool {
        let Some(step) = self.steps.get_mut(index) else {
            return false;
        };
        let Some(name) = label(name) else {
            return false;
        };
        step.context.name = Some(name);
        true
    }
    pub(super) fn displayed(&self, step: &Step) -> [u32; 2] {
        let [_, _, w, h] = step.view(self.size).rect;
        let width = self.size[0].min(1000);
        [
            width,
            ((self.size[1] as f64 * h as f64) / (self.size[0] as f64 * w as f64) * width as f64)
                .round()
                .max(1.) as u32,
        ]
    }
    pub(super) fn header_objects(&self, canvas: [u32; 2]) -> Result<Vec<Annotation>> {
        let Some(h) = &self.header else {
            return Ok(vec![]);
        };
        let mut parts = vec![
            format!(
                "{} {}",
                self.len(),
                if self.len() == 1 { "step" } else { "steps" }
            ),
            format!("{}:{:02}", h.seconds / 60, h.seconds % 60),
        ];
        if let Some(author) = &h.author
            && !author.trim().is_empty()
        {
            parts.insert(0, author.clone());
        }
        if !h.date.is_empty() {
            parts.push(h.date.clone());
        }
        let template =
            crate::steps::marker(1000, [40., 40.], 32, [35, 45, 55, 255], (1, false), canvas)?;
        let mut title = template.clone();
        title.kind = Kind::Text;
        title.fill = None;
        title.start = [24., 24.];
        title.end = [(canvas[0] - 24) as f32, 72.];
        title.text = Some(crate::text::TextStyle {
            content: h.title.clone(),
            size_points: 32.,
            bold: true,
            horizontal: crate::text::HorizontalAlignment::Left,
            ..Default::default()
        });
        let mut summary = title.clone();
        summary.id = 1001;
        summary.start[1] = 78.;
        summary.end[1] = 122.;
        summary.text = Some(crate::text::TextStyle {
            content: parts.join(" | "),
            size_points: 16.,
            horizontal: crate::text::HorizontalAlignment::Left,
            ..Default::default()
        });
        Ok(vec![title, summary])
    }
}
impl Step {
    pub(super) fn view(&self, size: [u32; 2]) -> crate::image_frame::ImageView {
        let mut context = self.context.clone();
        if let Some(bounds) = &mut context.bounds
            && let Some(cursor) = &self.cursor
        {
            let (lo, hi) = cursor.bounds();
            bounds[0] = bounds[0].min(lo[0].floor() as i32);
            bounds[1] = bounds[1].min(lo[1].floor() as i32);
            bounds[2] = bounds[2].max(hi[0].ceil() as i32);
            bounds[3] = bounds[3].max(hi[1].ceil() as i32);
        }
        context.view(size, self.gesture)
    }
}

#[cfg(test)]
mod tests;
