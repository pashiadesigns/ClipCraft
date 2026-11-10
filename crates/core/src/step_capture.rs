//! Bounded click/drag recording and editable guide assembly. No platform input.
use crate::{Annotation, Document, Error, Kind, RasterImage, Result, bitmap::Bitmap};
use image::RgbaImage;
use std::{path::Path, sync::Arc};

mod context;
pub use context::label as context_label;
pub use context::{Context, Header};

pub const MAX_STEPS: usize = 64;
const MARGIN: u32 = 24;
const HEADER: u32 = 52;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gesture {
    pub start: [u32; 2],
    pub end: [u32; 2],
    pub button: Button,
}
impl Gesture {
    pub fn is_drag(self) -> bool {
        self.start[0]
            .abs_diff(self.end[0])
            .max(self.start[1].abs_diff(self.end[1]))
            >= 4
    }
    fn label(self) -> &'static str {
        if self.is_drag() {
            "Drag"
        } else if self.button == Button::Right {
            "Right-click"
        } else {
            "Click"
        }
    }
    fn validate(self, size: [u32; 2]) -> Result<()> {
        if [self.start, self.end]
            .iter()
            .any(|p| p[0] >= size[0] || p[1] >= size[1])
        {
            return Err(Error::Invalid(
                "Step click is outside its screenshot".into(),
            ));
        }
        Ok(())
    }
}

/// Input is admitted only between a matching down/up pair inside the chosen
/// physical-pixel area. Pause resets a partial gesture; resume cannot revive it.
pub struct Tracker {
    origin: [i32; 2],
    size: [u32; 2],
    paused: bool,
    finished: bool,
    down: Option<([u32; 2], Button)>,
}
impl Tracker {
    pub fn new(origin: [i32; 2], size: [u32; 2]) -> Result<Self> {
        crate::validate_dimensions(size[0], size[1])?;
        Ok(Self {
            origin,
            size,
            paused: false,
            finished: false,
            down: None,
        })
    }
    fn local(&self, point: [i32; 2]) -> Option<[u32; 2]> {
        let p = [0, 1].map(|i| i64::from(point[i]) - i64::from(self.origin[i]));
        (p[0] >= 0 && p[1] >= 0 && p[0] < i64::from(self.size[0]) && p[1] < i64::from(self.size[1]))
            .then_some([p[0] as u32, p[1] as u32])
    }
    pub fn pause(&mut self, value: bool) {
        self.paused = value;
        self.down = None;
    }
    pub fn cancel_gesture(&mut self) {
        self.down = None;
    }
    pub fn finish(&mut self) {
        self.finished = true;
        self.down = None;
    }
    pub fn down(&mut self, point: [i32; 2], button: Button) {
        self.down = if self.paused || self.finished {
            None
        } else {
            self.local(point).map(|p| (p, button))
        };
    }
    pub fn up(&mut self, point: [i32; 2], button: Button) -> Option<Gesture> {
        let (start, previous) = self.down.take()?;
        if self.finished || self.paused || button != previous {
            return None;
        }
        Some(Gesture {
            start,
            end: self.local(point)?,
            button,
        })
    }
}

pub struct Frame {
    pub image: RgbaImage,
    pub cursor: Option<Annotation>,
    pub context: Context,
}
impl From<RgbaImage> for Frame {
    fn from(image: RgbaImage) -> Self {
        Self {
            image,
            cursor: None,
            context: Context::default(),
        }
    }
}
struct Step {
    instruction: Option<String>,
    bytes: usize,
    context: Context,
    cursor: Option<Annotation>,
    image: Arc<Bitmap>,
    gesture: Gesture,
}
pub struct Guide {
    header: Option<Header>,
    size: [u32; 2],
    steps: Vec<Step>,
    png_bytes: usize,
}
impl Guide {
    pub fn set_instruction(&mut self, index: usize, instruction: &str) -> bool {
        let Some(step) = self.steps.get_mut(index) else {
            return false;
        };
        let text: String = instruction
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(512)
            .collect();
        if text.is_empty()
            || (crate::text::TextStyle {
                content: text.clone(),
                ..Default::default()
            })
            .validate()
            .is_err()
        {
            return false;
        }
        step.instruction = Some(text);
        true
    }
    pub fn new(size: [u32; 2]) -> Result<Self> {
        crate::validate_dimensions(size[0], size[1])?;
        Ok(Self {
            size,
            steps: Vec::new(),
            png_bytes: 0,
            header: None,
        })
    }
    pub fn len(&self) -> usize {
        self.steps.len()
    }
    /// Removes the captured frame and cursor; the next step reuses its number.
    pub fn undo_step(&mut self) -> bool {
        if self.steps.pop().is_none() {
            return false;
        }
        self.png_bytes = self.steps.iter().map(|s| s.bytes).sum();
        true
    }
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
    fn layout(&self, count: usize) -> Result<([u32; 2], [u32; 2])> {
        let scale = (1000.0 / self.size[0] as f64).min(1.0);
        let displayed = self
            .size
            .map(|n| (n as f64 * scale).round().max(1.0) as u32);
        let canvas = [
            displayed[0].max(320) + MARGIN * 2,
            (displayed[1] + HEADER + MARGIN) * count as u32 + MARGIN,
        ];
        crate::validate_dimensions(canvas[0], canvas[1])?;
        Ok((canvas, displayed))
    }
    pub fn push(&mut self, frame: impl Into<Frame>, gesture: Gesture) -> Result<()> {
        let Frame {
            image,
            cursor,
            context,
        } = frame.into();
        if let Some(a) = &cursor {
            let mut d = Document::blank(self.size[0], self.size[1])?;
            d.annotations.push(a.clone());
            d.promote_features();
            d.validate()?;
            if a.image.as_ref().is_none_or(|b| !b.is_cursor()) {
                return Err(Error::Invalid("Invalid step cursor".into()));
            }
        }
        let next = self.len() + 1;
        if next > MAX_STEPS
            || next as u64 * self.size[0] as u64 * self.size[1] as u64
                + self
                    .steps
                    .iter()
                    .filter_map(|s| s.cursor.as_ref())
                    .chain(cursor.iter())
                    .map(|a| {
                        a.image.as_ref().map_or(0, |b| {
                            let [w, h] = b.size();
                            w as u64 * h as u64
                        })
                    })
                    .sum::<u64>()
                > crate::MAX_PIXELS
        {
            return Err(Error::Invalid("Step guide reached its 64-step or 25-megapixel source limit; recorded steps are preserved".into()));
        }
        if [image.width(), image.height()] != self.size {
            return Err(Error::Invalid("Step screenshot dimensions changed".into()));
        }
        gesture.validate(self.size)?;
        self.layout(next)?;
        let raster = RasterImage::from_image(&image)?;
        // Base64 + JSON must fit the portable document limit; reserve metadata.
        let bytes = raster.png.len()
            + cursor
                .as_ref()
                .and_then(|a| a.image.as_ref())
                .map_or(0, |b| b.memory_bytes());
        if (self.png_bytes + bytes) as u64 * 4 / 3 > crate::MAX_FILE_BYTES - 1_000_000 {
            return Err(Error::Invalid(
                "Step guide reached its portable file limit; recorded steps are preserved".into(),
            ));
        }
        let bitmap = Arc::new(Bitmap::new(Arc::new(raster))?);
        let step = Step {
            instruction: None,
            bytes,
            context,
            image: bitmap,
            cursor,
            gesture,
        };
        let height = self
            .steps
            .iter()
            .map(|s| self.displayed(s)[1] + HEADER + MARGIN)
            .sum::<u32>()
            + self.displayed(&step)[1]
            + HEADER
            + MARGIN
            + MARGIN
            + if self.header.is_some() { 112 } else { 0 };
        crate::validate_dimensions(self.size[0].clamp(320, 1000) + MARGIN * 2, height)?;
        self.steps.push(step);
        self.png_bytes += bytes;
        Ok(())
    }
    /// Frames keep original pixels; display size, numbering and captions are
    /// independent editable annotations with persistent section ownership.
    pub fn document(&self) -> Result<Document> {
        if self.is_empty() {
            return Err(Error::Invalid("No steps have been captured".into()));
        }
        let (mut canvas, _) = self.layout(self.len())?;
        canvas[1] = self
            .steps
            .iter()
            .map(|s| self.displayed(s)[1] + HEADER + MARGIN)
            .sum::<u32>()
            + MARGIN
            + if self.header.is_some() { 112 } else { 0 };
        crate::validate_dimensions(canvas[0], canvas[1])?;
        let mut d = Document::blank(canvas[0], canvas[1])?;
        let mut sections = Vec::new();
        if self.header.is_some() {
            d.annotations = self.header_objects(canvas)?;
            sections.push(crate::guide_sections::Section {
                page: 0,
                id: 1000,
                kind: crate::guide_sections::SectionKind::Header,
                height: 112.,
                objects: vec![1000, 1001],
                marker: None,
            });
        }
        let mut top = MARGIN + if self.header.is_some() { 112 } else { 0 };
        for (index, step) in self.steps.iter().enumerate() {
            let first_object = d.annotations.len();
            let id = index as u64 * 3 + 1;
            let displayed = self.displayed(step);
            let view = step.view(self.size);
            let mut image = Arc::new(step.image.with_view(Some(view))?.with_focus([
                step.gesture.end[0] as f32 / self.size[0] as f32,
                step.gesture.end[1] as f32 / self.size[1] as f32,
            ])?)
            .annotation(id);
            let [vx, vy, vw, vh] = view.rect;
            let map_point = |point: [f32; 2]| {
                [
                    MARGIN as f32
                        + (point[0] - vx * self.size[0] as f32) * displayed[0] as f32
                            / (vw * self.size[0] as f32),
                    (top + HEADER) as f32
                        + (point[1] - vy * self.size[1] as f32) * displayed[1] as f32
                            / (vh * self.size[1] as f32),
                ]
            };
            image.start = [MARGIN as f32, (top + HEADER) as f32];
            image.end = [
                (MARGIN + displayed[0]) as f32,
                (top + HEADER + displayed[1]) as f32,
            ];
            d.annotations.push(image);
            if let Some(cursor) = &step.cursor {
                let mut cursor = cursor.clone();
                cursor.id = MAX_STEPS as u64 * 3 + index as u64 + 1;
                cursor.group = Some(index as u64 + 1);
                for point in [&mut cursor.start, &mut cursor.end] {
                    *point = map_point(*point);
                }
                d.annotations.push(cursor);
            }
            d.annotations.push(Annotation {
                id: id + 1,
                kind: Kind::Text,
                start: [(MARGIN + 44) as f32, top as f32],
                end: [(canvas[0] - MARGIN) as f32, (top + HEADER - 8) as f32],
                color: [35, 45, 55, 255],
                width: 1,
                corner_radius: 0.,
                fill: None,
                points: vec![],
                callout: None,

                focus: None,

                highlighter_rect: false,
                hard_edge: false,
                group: None,
                rotation_degrees: 0.,
                mirror_x: false,
                step_outline: None,
                line_style: None,
                shadow: None,
                image: None,
                text: Some(crate::text::TextStyle {
                    content: step.instruction.clone().unwrap_or_else(|| {
                        step.context
                            .name
                            .as_deref()
                            .and_then(context::label)
                            .map_or_else(
                                || step.gesture.label().to_owned(),
                                |n| format!("{} \"{}\"", step.gesture.label(), n),
                            )
                    }),
                    size_points: 22.,
                    bold: true,
                    horizontal: crate::text::HorizontalAlignment::Left,
                    vertical: crate::text::VerticalAlignment::Middle,
                    ..Default::default()
                }),
            });
            let point = step.gesture.end;
            let center = map_point([point[0] as f32 + 0.5, point[1] as f32 + 0.5]);
            d.annotations.push(crate::steps::marker(
                id + 2,
                [(MARGIN + 16) as f32, (top + 22) as f32],
                32,
                [0, 139, 185, 255],
                ((index + 1) as u32, false),
                canvas,
            )?);
            // An editable click highlight remains separate from source pixels.
            let radius = 18.0f32
                .min(displayed[0] as f32 * 0.5)
                .min(displayed[1] as f32 * 0.5);
            // Near frame edges, shrink the circle rather than moving its center
            // away from the recorded pixel. Its source screenshot is untouched.
            let r = radius
                .min(center[0] - MARGIN as f32)
                .min((MARGIN + displayed[0]) as f32 - center[0])
                .min(center[1] - (top + HEADER) as f32)
                .min((top + HEADER + displayed[1]) as f32 - center[1])
                .max(0.25);
            let circle = Annotation {
                focus: None,
                highlighter_rect: false,
                hard_edge: false,
                id: MAX_STEPS as u64 * 4 + index as u64 + 1,
                kind: Kind::Ellipse,
                start: [center[0] - r, center[1] - r],
                end: [center[0] + r, center[1] + r],
                color: [220, 190, 0, 255],
                width: 1,
                corner_radius: 0.,
                fill: Some([255, 235, 0, 65]),
                points: vec![],
                callout: None,
                group: step.cursor.as_ref().map(|_| index as u64 + 1),
                rotation_degrees: 0.,
                mirror_x: false,
                step_outline: None,
                line_style: None,
                shadow: None,
                image: None,
                text: None,
            };
            d.annotations.insert(first_object + 1, circle);
            sections.push(crate::guide_sections::Section {
                page: 0,
                id: index as u64 + 1,
                kind: crate::guide_sections::SectionKind::Image,
                height: (displayed[1] + HEADER + MARGIN) as f32,
                objects: d.annotations[first_object..].iter().map(|a| a.id).collect(),
                marker: Some(id + 2),
            });
            top += displayed[1] + HEADER + MARGIN;
        }
        d.guide = Some(crate::guide_sections::Layout {
            pages: None,
            template: None,
            top: MARGIN as f32,
            sections,
        });
        d.promote_features();
        d.validate()?;
        Ok(d)
    }
    /// The latest complete guide is atomically recoverable between clicks.
    pub fn checkpoint(&self, path: &Path) -> Result<()> {
        self.document()?.save(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn gesture() -> Gesture {
        Gesture {
            start: [40, 30],
            end: [40, 30],
            button: Button::Left,
        }
    }
    #[test]
    fn tracker_orders_matching_clicks_drags_and_discards_paused_partial_input() {
        let mut t = Tracker::new([-100, 20], [100, 80]).unwrap();
        assert!(t.up([-60, 50], Button::Left).is_none());
        t.down([-60, 50], Button::Left);
        assert_eq!(t.up([-60, 50], Button::Left), Some(gesture()));
        t.down([-60, 50], Button::Left);
        let drag = t.up([-50, 60], Button::Left).unwrap();
        assert!(drag.is_drag());
        t.down([-60, 50], Button::Left);
        t.pause(true);
        t.pause(false);
        assert!(t.up([-60, 50], Button::Left).is_none());
        t.down([-101, 50], Button::Left);
        assert!(t.up([-60, 50], Button::Left).is_none());
        t.down([-60, 50], Button::Left);
        assert!(t.up([-60, 50], Button::Right).is_none());
        t.down([-60, 50], Button::Right);
        assert!(t.up([0, 50], Button::Right).is_none());
        t.finish();
        t.pause(false);
        t.down([-60, 50], Button::Left);
        assert!(t.up([-60, 50], Button::Left).is_none());
    }
    #[test]
    fn guide_preserves_original_pixels_order_numbering_and_editable_persistence() {
        let mut guide = Guide::new([120, 80]).unwrap();
        for color in [[220, 20, 40, 255], [20, 180, 220, 255]] {
            guide
                .push(
                    RgbaImage::from_pixel(120, 80, image::Rgba(color)),
                    gesture(),
                )
                .unwrap();
        }
        let document = guide.document().unwrap();
        assert_eq!(document.version, crate::MAX_PROJECT_VERSION);
        assert_eq!(document.annotations.len(), 8);
        assert_eq!(document.annotations[3].text.as_ref().unwrap().content, "1");
        assert_eq!(document.annotations[7].text.as_ref().unwrap().content, "2");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("guide.capture.json");
        guide.checkpoint(&path).unwrap();
        let reopened = Document::load(&path).unwrap();
        assert_eq!(reopened.annotations, document.annotations);
        let mut session = crate::Session::new(reopened).unwrap();
        let before = session.document.render().unwrap();
        session.execute(crate::Command::Remove { id: 1 }).unwrap();
        assert_ne!(session.document.render().unwrap(), before);
        session.execute(crate::Command::Undo).unwrap();
        assert_eq!(session.document.render().unwrap(), before);
        assert_eq!(
            session.document.annotations[0]
                .image
                .as_ref()
                .unwrap()
                .pixels()
                .unwrap()
                .get_pixel(0, 0)
                .0,
            [220, 20, 40, 255]
        );
        if let Some(root) = std::env::var_os("CLIPCRAFT_STEP_FIXTURE_DIR") {
            let root = std::path::PathBuf::from(root);
            std::fs::create_dir_all(&root).unwrap();
            document.save(&root.join("guide.capture.json")).unwrap();
            before.save(root.join("guide.png")).unwrap();
        }
    }
    #[test]
    fn guide_rejects_invalid_frames_without_replacing_prior_checkpoint() {
        let mut guide = Guide::new([120, 80]).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("guide.capture.json");
        assert!(guide.checkpoint(&path).is_err());
        assert!(!path.exists());
        guide.push(RgbaImage::new(120, 80), gesture()).unwrap();
        guide.checkpoint(&path).unwrap();
        let old = std::fs::read(&path).unwrap();
        assert!(guide.push(RgbaImage::new(121, 80), gesture()).is_err());
        assert!(
            guide
                .push(
                    RgbaImage::new(120, 80),
                    Gesture {
                        end: [120, 0],
                        ..gesture()
                    }
                )
                .is_err()
        );
        assert_eq!(guide.len(), 1);
        guide.checkpoint(&path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), old);
    }
    #[test]
    fn step_and_geometry_limits_preserve_every_accepted_frame() {
        let mut guide = Guide::new([64, 64]).unwrap();
        for _ in 0..MAX_STEPS {
            guide.push(RgbaImage::new(64, 64), gesture()).unwrap();
        }
        assert!(guide.push(RgbaImage::new(64, 64), gesture()).is_err());
        assert_eq!(guide.len(), MAX_STEPS);
        assert_eq!(guide.document().unwrap().annotations.len(), MAX_STEPS * 4);
        let tall = Guide::new([1, 32768]).unwrap();
        assert!(tall.layout(1).is_err());
    }
}
