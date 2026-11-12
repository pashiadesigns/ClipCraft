//! Document and command engine shared by the desktop editor and automation CLI.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use image::{ImageFormat, Rgba, RgbaImage};
use imageproc::{
    drawing::{BresenhamLineIter, draw_filled_rect_mut},
    rect::Rect,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Cursor, Write},
    path::Path,
};

pub const MAX_PIXELS: u64 = 25_000_000;
pub const MAX_PROJECT_VERSION: u32 = 31;
mod arrow;
pub mod bitmap;
pub mod callout;
pub mod canvas_behavior;
pub mod canvas_expansion;
mod canvas_size;
pub mod capture_options;
pub mod captured_cursor;
pub mod clipboard_payload;
pub mod crop;
pub mod display;
pub mod editor_keybinds;
pub mod ellipse;
pub mod export;
pub mod focus;
pub mod gif_studio;
pub mod group_presets;
pub mod groups;
pub mod guide_sections;
pub mod history;
pub mod icon_library;
pub mod image_frame;
pub mod image_transform;
pub mod keybinds;
pub mod magnified;
pub mod obscure;
pub mod picture_finish;
pub mod privacy;
pub mod project_file;
pub mod redaction_labels;
pub mod reusable_presets;
pub mod scroll_capture;
pub mod templates;
pub mod video;
pub use image_transform::ImageTransform;
#[cfg(test)]
mod eraser_tests;
mod rounded;
pub mod step_capture;
pub mod steps;
#[cfg(test)]
mod stroke_tests;
pub mod styles;
pub use canvas_size::{Placement, RasterImage};
#[cfg(test)]
mod callout_tests;
#[cfg(test)]
mod canvas_size_tests;
pub mod cut_out;
mod flatten_pages;
#[cfg(test)]
mod flatten_tests;
pub mod kit;
pub mod layout_selection;
pub mod line_style;
pub mod lock;
pub mod multi_selection;
pub mod object_state;
pub mod object_styles;
pub mod pack;
pub mod palettes;
pub mod pdf_import;
pub mod polygon;
pub mod quick_preferences;
#[cfg(test)]
mod redaction_tests;
pub mod resize;
pub mod resize_modifiers;
#[cfg(test)]
mod resize_tests;
#[cfg(test)]
mod revert_tests;
pub mod rotation;
pub mod saved_colors;
pub mod selection;
pub mod shadow;
mod shape_fill;
#[cfg(test)]
mod shape_text_tests;
pub mod snapping;
#[cfg(test)]
mod stacking_tests;
pub mod text;
#[cfg(test)]
mod text_fit_tests;
#[cfg(test)]
mod text_padding_tests;
pub mod tilt;
pub mod toolbar;
pub mod vector_import;
pub mod watermark;
pub mod zip;
const MAX_FILE_BYTES: u64 = 160_000_000;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Image(#[from] image::ImageError),
    #[error(transparent)]
    Base64(#[from] base64::DecodeError),
}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Image,
    Arrow,
    Line,
    Pen,
    Highlighter,
    Eraser,
    Step,
    Rectangle,
    Ellipse,
    Polygon,
    Redact,
    Blur,
    Pixelate,
    Text,
    Callout,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Annotation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus: Option<focus::Focus>,
    #[serde(default, skip_serializing_if = "text::is_false")]
    pub highlighter_rect: bool,
    /// Eraser only: no soft edge.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hard_edge: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<std::sync::Arc<bitmap::Bitmap>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_outline: Option<steps::Outline>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_style: Option<line_style::Style>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow: Option<shadow::Shadow>,
    /// Clockwise orientation around the local body center. Geometry stays editable.
    #[serde(default, skip_serializing_if = "rounded::is_zero")]
    pub rotation_degrees: f32,
    /// Reflect local content before rotation, without changing the source image or paths.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mirror_x: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<u64>,
    pub id: u64,
    pub kind: Kind,
    pub start: [f32; 2],
    pub end: [f32; 2],
    pub color: [u8; 4],
    pub width: u32,
    #[serde(default, skip_serializing_if = "rounded::is_zero")]
    pub corner_radius: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<[u8; 4]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub points: Vec<[f32; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<text::TextStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callout: Option<callout::CalloutStyle>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Document {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video: Option<video::editor::Project>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guide: Option<guide_sections::Layout>,
    pub version: u32,
    pub width: u32,
    pub height: u32,
    /// Original image is retained, including underneath editable redactions.
    pub source_png: String,
    /// Original pixel dimensions when output has been resized. The PNG stays
    /// unchanged so repeated resizes and Revert retain a recoverable source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_size: Option<[u32; 2]>,
    /// Current raster after canvas changes. Shared by undo snapshots; the
    /// original PNG remains untouched for Revert. Version 2 requires support.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raster: Option<std::sync::Arc<RasterImage>>,
    /// Base canvas before automatic object-driven margins: x, y, width, height.
    /// Absent in older projects; never infer it from transparent image pixels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_canvas_base: Option<[u32; 4]>,
    #[serde(
        default,
        skip_serializing_if = "canvas_behavior::CanvasBehavior::is_default"
    )]
    pub canvas_behavior: canvas_behavior::CanvasBehavior,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub object_states: object_state::ObjectStates,
    pub annotations: Vec<Annotation>,
    /// One of several linked guides made from one large PDF (approved beta 6
    /// tab 7); export can put the parts back into one file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<pdf_import::PartOf>,
}

/// Runs third-party parsing or drawing of untrusted input so that a panic
/// inside it becomes an ordinary error instead of ending the program
/// (ENG-009 hostile-file testing found such panics in resvg).
pub(crate) fn guarded<T>(message: &str, job: impl FnOnce() -> Result<T>) -> Result<T> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(job))
        .unwrap_or_else(|_| Err(Error::Invalid(message.to_owned())))
}

pub fn validate_dimensions(width: u32, height: u32) -> Result<()> {
    if width == 0
        || height == 0
        || width > 32768
        || height > 32768
        || u64::from(width) * u64::from(height) > MAX_PIXELS
    {
        return Err(Error::Invalid(
            "Image dimensions must be positive and at most 25 megapixels".into(),
        ));
    }
    Ok(())
}

impl Document {
    pub fn blank(width: u32, height: u32) -> Result<Self> {
        validate_dimensions(width, height)?;
        Ok(Self {
            video: None,
            version: MAX_PROJECT_VERSION,
            width,
            height,
            source_png: String::new(),
            source_size: None,
            raster: None,
            auto_canvas_base: None,
            canvas_behavior: Default::default(),
            guide: None,
            object_states: Default::default(),
            annotations: vec![],
            part: None,
        })
    }

    pub fn from_image(image: RgbaImage) -> Result<Self> {
        validate_dimensions(image.width(), image.height())?;
        let mut data = Cursor::new(Vec::new());
        image.write_to(&mut data, ImageFormat::Png)?;
        Ok(Self {
            video: None,
            version: MAX_PROJECT_VERSION,
            width: image.width(),
            height: image.height(),
            source_png: STANDARD.encode(data.into_inner()),
            source_size: None,
            raster: None,
            auto_canvas_base: None,
            canvas_behavior: Default::default(),
            guide: None,
            object_states: Default::default(),
            annotations: vec![],
            part: None,
        })
    }

    /// Captures start as one editable image on a transparent canvas. Keep the
    /// original pixels separately for Revert, just like imported raster projects.
    pub fn from_capture_image(image: RgbaImage) -> Result<Self> {
        use std::sync::Arc;
        let mut document = Self::from_image(image)?;
        let bitmap = bitmap::Bitmap::new(Arc::new(RasterImage {
            width: document.width,
            height: document.height,
            png: document.source_png.clone(),
        }))?;
        document.raster = Some(Arc::new(RasterImage::from_image(&RgbaImage::new(
            document.width,
            document.height,
        ))?));
        document.source_size = Some([document.width, document.height]);
        document.annotations.push(Arc::new(bitmap).annotation(1));
        document.promote_features();
        document.validate()?;
        Ok(document)
    }

    /// Create a colored canvas through the same portable image representation
    /// used by imports. Validate before allocating potentially large images.
    pub fn canvas(width: u32, height: u32, color: [u8; 4]) -> Result<Self> {
        validate_dimensions(width, height)?;
        if color == [255, 255, 255, 255] {
            Self::blank(width, height)
        } else {
            Self::from_image(RgbaImage::from_pixel(width, height, Rgba(color)))
        }
    }

    pub fn open_image(path: &Path) -> Result<Self> {
        if fs::metadata(path)?.len() > MAX_FILE_BYTES {
            return Err(Error::Invalid("Image file is too large".into()));
        }
        // SVG and EPS open as finished images (EDIT-084); Insert keeps SVG sharp.
        match path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("svg") => {
                let text = fs::read_to_string(path)?;
                let size = vector_import::svg_size(&text)?;
                return Self::from_image(vector_import::render_svg(&text, size, None)?);
            }
            Some("eps") => return Self::from_image(vector_import::eps_image(&fs::read(path)?)?.0),
            _ => {}
        }
        let (width, height) = image::image_dimensions(path)?;
        validate_dimensions(width, height)?;
        Self::from_image(image::open(path)?.into_rgba8())
    }

    /// Current development format; old projects are intentionally unsupported.
    pub fn promote_features(&mut self) {
        self.version = MAX_PROJECT_VERSION;
    }

    pub fn validate(&self) -> Result<()> {
        if self.version != MAX_PROJECT_VERSION {
            return Err(Error::Invalid(
                "Unsupported development project format. Create a new project in this build."
                    .into(),
            ));
        }
        validate_dimensions(self.width, self.height)?;
        if let Some(tilt) = &self.canvas_behavior.perspective {
            if self.video.is_some() {
                return Err(Error::Invalid(
                    "Whole-image perspective is for images. Use perspective on image clips instead."
                        .into(),
                ));
            }
            tilt.validate()?;
        }
        if let Some(finish) = &self.canvas_behavior.finish {
            if self.video.is_some() {
                return Err(Error::Invalid(
                    "Filters and border for the whole picture are for images. Use them on clips instead."
                        .into(),
                ));
            }
            finish.validate()?;
        }
        for a in &self.annotations {
            if let Some(finish) = a.image.as_ref().and_then(|b| b.finish()) {
                finish.validate()?;
            }
        }

        if let Some(v) = &self.video {
            v.validate(self)?;
        }
        self.validate_object_states()?;
        self.validate_groups()?;
        if let Some(guide) = &self.guide {
            guide.validate(self)?;
        }
        if let Some(part) = &self.part {
            part.validate()?;
        }
        if let Some([width, height]) = self.source_size {
            validate_dimensions(width, height)?;
        }
        if let Some(raster) = &self.raster {
            if self.source_size.is_none() {
                return Err(Error::Invalid(
                    "Canvas raster requires original dimensions".into(),
                ));
            }
            validate_dimensions(raster.width, raster.height)?;
        }
        if let Some([x, y, width, height]) = self.auto_canvas_base {
            validate_dimensions(width, height)?;
            if x.checked_add(width).is_none_or(|end| end > self.width)
                || y.checked_add(height).is_none_or(|end| end > self.height)
            {
                return Err(Error::Invalid(
                    "Invalid automatic canvas base region".into(),
                ));
            }
        }
        if self.annotations.len() > 10000 {
            return Err(Error::Invalid(
                "This image has too many objects. Flatten or delete some first.".into(),
            ));
        }
        if self
            .annotations
            .iter()
            .map(|a| a.points.len())
            .sum::<usize>()
            > 100_000
        {
            return Err(Error::Invalid(
                "Too many freehand points in this document".into(),
            ));
        }
        let mut image_pixels = 0u64;
        for bitmap in self.annotations.iter().filter_map(|a| a.image.as_ref()) {
            let [w, h] = bitmap.size();
            validate_dimensions(w, h)?;
            image_pixels += u64::from(w) * u64::from(h);
            if image_pixels > MAX_PIXELS {
                return Err(Error::Invalid(
                    "Embedded images exceed the 25 megapixel document budget".into(),
                ));
            }
        }
        let mut ids = std::collections::HashSet::new();
        for annotation in &self.annotations {
            if let Some(focus) = annotation.focus {
                focus.validate(annotation.kind)?;
                if annotation.image.is_some()
                    || annotation.text.is_some()
                    || annotation.fill.is_some()
                {
                    return Err(Error::Invalid(
                        "Focus windows cannot contain text, images or a fill.".into(),
                    ));
                }
            }
            if annotation.highlighter_rect && (annotation.kind != Kind::Highlighter) {
                return Err(Error::Invalid(
                    "Rectangle highlighting requires a Highlighter".into(),
                ));
            }
            if let Some(style) = annotation.line_style {
                style.validate_for(annotation.kind)?;
            }
            if let Some(shadow) = annotation.shadow {
                shadow.validate()?;
                shadow::validate_patch(annotation)?;
            }

            match (annotation.kind, &annotation.image) {
                (_, Some(bitmap))
                    if (annotation.kind == Kind::Image) || (annotation.is_shape()) =>
                {
                    if annotation.is_shape()
                        && (annotation.text.is_some() || bitmap.is_cursor() || bitmap.is_icon())
                    {
                        return Err(Error::Invalid(
                            "Shape frames contain an image instead of text".into(),
                        ));
                    }
                    if bitmap.has_frame_opacity() && (!annotation.is_shape()) {
                        return Err(Error::Invalid(
                            "Separate image opacity requires a shape frame".into(),
                        ));
                    }
                    bitmap.validate_cursor()?;
                    bitmap.validate_view()?;

                    bitmap.pixels()?;
                    if annotation.start[0] == annotation.end[0]
                        || annotation.start[1] == annotation.end[1]
                    {
                        return Err(Error::Invalid(
                            "Image object must have nonzero dimensions".into(),
                        ));
                    }
                }
                (Kind::Image, _) | (_, Some(_)) => {
                    return Err(Error::Invalid(
                        "Embedded images require an Image object".into(),
                    ));
                }
                _ => {}
            }
            if let Some(outline) = &annotation.step_outline {
                outline.validate()?;
                if annotation.kind != Kind::Step {
                    return Err(Error::Invalid("Step outline requires a Step".into()));
                }
            }

            if !annotation.rotation_degrees.is_finite()
                || !(-180.0..180.0).contains(&annotation.rotation_degrees)
                || (annotation.rotation_degrees != 0.0 && (!annotation.can_rotate()))
                || (annotation.mirror_x && !annotation.can_rotate())
            {
                return Err(Error::Invalid(
                    "Object rotation requires a supported vector object".into(),
                ));
            }
            match (annotation.kind, &annotation.text) {
                (Kind::Text | Kind::Callout | Kind::Step, Some(style)) => style.validate()?,
                (Kind::Rectangle | Kind::Ellipse | Kind::Polygon, Some(style)) => {
                    style.validate()?
                }
                (Kind::Text | Kind::Callout | Kind::Step, None) => {
                    return Err(Error::Invalid("Text style is missing".into()));
                }
                (_, Some(_)) => {
                    return Err(Error::Invalid(
                        "This object type does not support text".into(),
                    ));
                }
                (_, None) => {}
            }
            match (annotation.kind, &annotation.callout) {
                (Kind::Callout, Some(style)) => style.validate(
                    annotation,
                    [self.width, self.height],
                    self.canvas_behavior.allow_overflow,
                )?,
                (Kind::Callout, None) => {
                    return Err(Error::Invalid("Callout style is missing".into()));
                }
                (_, Some(_)) => {
                    return Err(Error::Invalid(
                        "Only callout objects may carry callout styling".into(),
                    ));
                }
                (_, None) => {}
            }
            if !ids.insert(annotation.id) {
                return Err(Error::Invalid("Duplicate annotation id".into()));
            }
            if annotation.fill.is_some()
                && !matches!(
                    annotation.kind,
                    Kind::Rectangle | Kind::Ellipse | Kind::Polygon
                )
            {
                return Err(Error::Invalid(
                    "Only rectangles, ellipses and polygons may carry shape fill".into(),
                ));
            }
            if (annotation.width == 0 && annotation.focus.is_none()) || annotation.width > 32 {
                return Err(Error::Invalid("Stroke width must be 1–32".into()));
            }
            if !annotation.corner_radius.is_finite()
                || annotation.corner_radius < 0.0
                || annotation.corner_radius > annotation.max_corner_radius()
            {
                return Err(Error::Invalid(
                    "Corner radius must fit a rectangle or callout body".into(),
                ));
            }
            let valid_path = match annotation.kind {
                Kind::Highlighter if annotation.highlighter_rect => annotation.points.is_empty(),
                Kind::Pen | Kind::Highlighter | Kind::Eraser => {
                    (2..=8192).contains(&annotation.points.len())
                }
                Kind::Polygon => (3..=64).contains(&annotation.points.len()),
                Kind::Blur | Kind::Pixelate => {
                    annotation.points.is_empty() || (3..=8192).contains(&annotation.points.len())
                }
                _ => annotation.points.is_empty(),
            };
            if !valid_path {
                return Err(Error::Invalid("Strokes need 2-8192 points; polygons need 3-64; other objects cannot carry paths".into()));
            }
            if self.canvas_behavior.allow_overflow {
                let (lo, hi) = annotation.bounds();
                if (0..2)
                    .any(|i| !lo[i].is_finite() || !hi[i].is_finite() || hi[i] - lo[i] > 32768.)
                {
                    return Err(Error::Invalid(
                        "Object dimensions exceed the supported editing range".into(),
                    ));
                }
            }
            let bleed = self
                .guide
                .as_ref()
                .and_then(|g| g.pages.as_ref())
                .map_or(0., |p| {
                    p.setup
                        .iter()
                        .map(|s| (s.bleed * s.ppi / 72.) as f32)
                        .fold(0., f32::max)
                });
            for point in [annotation.start, annotation.end]
                .iter()
                .chain(&annotation.points)
            {
                let overflow = self.canvas_behavior.allow_overflow;
                if !point.iter().all(|n| n.is_finite() && n.abs() <= 1_000_000.)
                    || (!overflow
                        && (point[0] < -bleed
                            || point[1] < -bleed
                            || point[0] > self.width as f32 + bleed
                            || point[1] > self.height as f32 + bleed))
                {
                    return Err(Error::Invalid(
                        "Annotation coordinates are outside the image".into(),
                    ));
                }
            }
        }
        if self
            .canvas_behavior
            .held
            .iter()
            .any(|id| !self.annotations.iter().any(|a| a.id == *id))
            || self.canvas_behavior.page_owners.iter().any(|(id, p)| {
                !self.annotations.iter().any(|a| a.id == *id)
                    || self.guide.as_ref().is_none_or(|g| *p >= g.page_count())
            })
        {
            return Err(Error::Invalid("Invalid pasteboard object ownership".into()));
        }
        Ok(())
    }

    pub fn base_image(&self) -> Result<RgbaImage> {
        self.validate()?;
        let mode = self.canvas_behavior.resampling;
        if let Some(raster) = &self.raster {
            return raster.render_with(self.width, self.height, mode);
        }
        if self.source_png.is_empty() {
            return Ok(RgbaImage::from_pixel(
                self.width,
                self.height,
                Rgba([255, 255, 255, 255]),
            ));
        }
        let bytes = STANDARD.decode(&self.source_png)?;
        let reader = image::ImageReader::with_format(Cursor::new(&bytes), ImageFormat::Png);
        let dimensions = reader.into_dimensions()?;
        let [source_width, source_height] = self.source_size.unwrap_or([self.width, self.height]);
        if dimensions != (source_width, source_height) {
            return Err(Error::Invalid(
                "Embedded image dimensions do not match the project".into(),
            ));
        }
        let original = image::load_from_memory_with_format(&bytes, ImageFormat::Png)?.into_rgba8();
        if dimensions == (self.width, self.height) {
            Ok(original)
        } else {
            Ok(resize::resample_with(
                original,
                self.width,
                self.height,
                mode,
            ))
        }
    }

    /// The finished image, including a whole-image perspective (FX-003).
    pub fn render(&self) -> Result<RgbaImage> {
        let mut flat = self.render_flat()?;
        if let Some(finish) = &self.canvas_behavior.finish {
            let frame = [0., 0., flat.width() as f32, flat.height() as f32];
            finish.apply(&mut flat, frame, 1.);
        }
        match &self.canvas_behavior.perspective {
            Some(tilt) => tilt::apply(&flat, tilt),
            None => Ok(flat),
        }
    }
    /// The finished image before any whole-image perspective.
    pub fn render_flat(&self) -> Result<RgbaImage> {
        let mut image = self.base_image()?;
        if (!self.canvas_behavior.page_owners.is_empty()
            || self.annotations.iter().any(|a| a.focus.is_some()))
            && let Some(g) = &self.guide
        {
            let furniture = self.page_furniture();
            for page in 0..g.page_count() {
                let [x, y, w, h] = self.page_rect(page).map(|n| n as u32);
                let mut pixels = image::imageops::crop_imm(&image, x, y, w, h).to_image();
                let mut objects: Vec<_> = self
                    .annotations
                    .iter()
                    .filter(|a| self.object_visible(a.id) && self.object_page(a) == page)
                    .cloned()
                    .collect();
                objects.extend(
                    furniture
                        .iter()
                        .filter(|a| g.page_at(a.start) == page)
                        .cloned(),
                );
                let objects =
                    crate::canvas_expansion::translated(&objects, [-(x as f32), -(y as f32)])?;
                let backdrop = focus::Backdrop::new(&objects);
                for a in &objects {
                    if !backdrop.render(&mut pixels, a) {
                        render_annotation(&mut pixels, a);
                    }
                }
                image::imageops::replace(&mut image, &pixels, x as i64, y as i64);
            }
            return Ok(image);
        }
        let mut objects: Vec<Annotation> = self
            .annotations
            .iter()
            .filter(|a| self.object_visible(a.id))
            .cloned()
            .collect();
        objects.extend(self.page_furniture());
        // Overlapping Focus effects share one backdrop (BJ).
        let backdrop = focus::Backdrop::new(&objects);
        for a in &objects {
            if !backdrop.render(&mut image, a) {
                render_annotation(&mut image, a);
            }
        }
        Ok(image)
    }

    pub fn load(path: &Path) -> Result<Self> {
        if fs::metadata(path)?.len() > MAX_FILE_BYTES {
            return Err(Error::Invalid("Project file is too large".into()));
        }
        let bytes = fs::read(path)?;
        let mut doc: Self = serde_json::from_slice(&bytes).map_err(|e| {
            if crate::project_file::is_video_project(path) && e.is_data() {Error::Invalid(format!("Unsupported or invalid development video project. Create a new video project in this build. {e}"))} else {Error::Json(e)}
        })?;
        video::project::resolve(&mut doc, path)?;
        doc.base_image()?;
        if doc.raster.is_some() {
            let mut original = doc.clone();
            original.raster = None;
            original.auto_canvas_base = None;
            original.annotations.clear();
            original.object_states.clear();
            // This temporary document validates only the original raster.
            // Section references belong to the current editable document.
            original.guide = None;
            original.video = None;
            original.canvas_behavior = Default::default();
            let [width, height] = original.source_size.take().unwrap();
            original.width = width;
            original.height = height;
            original.base_image()?;
        }
        Ok(doc)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let portable = video::project::portable(self, path)?;
        let bytes = serde_json::to_vec(&portable)?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(Error::Invalid("This project is over 160 MB. Use Save As › Pack project, or export a flattened PNG.".into()));
        }
        atomic_write(path, &bytes)
    }

    pub fn export_png(&self, path: &Path) -> Result<()> {
        let mut bytes = Cursor::new(Vec::new());
        self.render()?.write_to(&mut bytes, ImageFormat::Png)?;
        atomic_write(path, &bytes.into_inner())
    }
}

fn line(image: &mut RgbaImage, start: [f32; 2], end: [f32; 2], width: u32, color: Rgba<u8>) {
    line_offset(image, start, end, width, color, [0, 0]);
}
fn line_offset<I: image::GenericImage>(
    image: &mut I,
    start: [f32; 2],
    end: [f32; 2],
    width: u32,
    color: I::Pixel,
    translation: [i32; 2],
) {
    let origin = [
        start[0].min(end[0]).floor() - width as f32,
        start[1].min(end[1]).floor() - width as f32,
    ];
    let start = [start[0] - origin[0], start[1] - origin[1]];
    let end = [end[0] - origin[0], end[1] - origin[1]];
    let dx = end[0] - start[0];
    let dy = end[1] - start[1];
    let distance = dx.hypot(dy).max(1.0);
    for i in 0..width {
        let offset = i as f32 - (width as f32 - 1.0) * 0.5;
        let ox = -dy / distance * offset;
        let oy = dx / distance * offset;
        for (x, y) in
            BresenhamLineIter::new((start[0] + ox, start[1] + oy), (end[0] + ox, end[1] + oy))
        {
            let x = x + origin[0] as i32 + translation[0];
            let y = y + origin[1] as i32 + translation[1];
            if x >= 0 && y >= 0 && (x as u32) < image.width() && (y as u32) < image.height() {
                image.put_pixel(x as u32, y as u32, color);
            }
        }
    }
}

fn render_annotation(image: &mut RgbaImage, a: &Annotation) {
    if a.focus.is_some() {
        focus::render(image, a);
        return;
    }
    let patch = shadow::patch(a);
    if shadow::above_pixel_effect(a.kind) {
        render_annotation_ink(image, a);
        if let Some(patch) = patch {
            patch.paint(image);
        }
    } else {
        if let Some(patch) = patch {
            patch.paint(image);
        }
        render_annotation_ink(image, a);
    }
}

fn render_annotation_ink(image: &mut RgbaImage, a: &Annotation) {
    if a.focus.is_some() {
        focus::render(image, a);
        return;
    }
    if a.is_shape() && a.image.is_some() {
        display::render_rotated(image, a);
        return;
    }

    // Ordinary annotations share the viewport's continuous geometry, pixel-center
    // convention and coverage rasterizer. Pixel-reading effects retain their
    // native-resolution CPU processing and opaque redaction stays opaque.
    if a.rotation_degrees != 0.0
        || !matches!(
            a.kind,
            Kind::Blur | Kind::Pixelate | Kind::Eraser | Kind::Redact
        )
    {
        display::render_rotated(image, a);
        return;
    }
    shape_fill::render(image, a);
    let x0 = a.start[0].min(a.end[0]);
    let y0 = a.start[1].min(a.end[1]);
    let x1 = a.start[0].max(a.end[0]);
    let y1 = a.start[1].max(a.end[1]);
    let color = Rgba(a.color);
    match a.kind {
        Kind::Image => display::render_rotated(image, a),
        Kind::Text => text::render(image, a),
        Kind::Callout => callout::render(image, a),
        Kind::Step => steps::render(image, a),
        Kind::Ellipse => ellipse::render(image, a),
        Kind::Polygon => polygon::render(image, a),
        Kind::Blur | Kind::Pixelate => obscure::render(image, a),
        Kind::Redact => {
            let width = (x1.ceil() as u32).saturating_sub(x0.floor() as u32).max(1);
            let height = (y1.ceil() as u32).saturating_sub(y0.floor() as u32).max(1);
            draw_filled_rect_mut(
                image,
                Rect::at(x0.floor() as i32, y0.floor() as i32).of_size(width, height),
                Rgba([0, 0, 0, 255]),
            );
        }
        Kind::Rectangle if a.corner_radius > 0.0 || a.fill.is_some_and(|fill| fill[3] != 0) => {
            rounded::render_rectangle(image, a)
        }
        Kind::Rectangle => {
            line(image, [x0, y0], [x1, y0], a.width, color);
            line(image, [x1, y0], [x1, y1], a.width, color);
            line(image, [x1, y1], [x0, y1], a.width, color);
            line(image, [x0, y1], [x0, y0], a.width, color);
        }
        Kind::Arrow | Kind::Line | Kind::Pen | Kind::Highlighter | Kind::Eraser => {
            arrow::render(image, a)
        }
    }
    if matches!(a.kind, Kind::Rectangle | Kind::Ellipse | Kind::Polygon) {
        text::render(image, a);
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|e| Error::Io(e.error))?;
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Command {
    /// Atomic timeline composition edit, including editable generated objects.
    VideoComposition {
        project: video::editor::Project,
        annotations: Vec<Annotation>,
    },
    /// Place objects with session motion defaults as one undoable action.
    AddTimed {
        annotations: Vec<Annotation>,
        motion: std::collections::BTreeMap<u64, reusable_presets::Motion>,
    },
    VideoDeleteTrack {
        layer: video::links::Layer,
    },
    VideoDeleteItems {
        items: std::collections::BTreeSet<video::editing::Item>,
    },
    Video {
        project: video::editor::Project,
    },
    VideoDuplicate {
        id: u64,
        timing: Option<video::editor::Timing>,
    },
    VideoResize {
        width: u32,
        height: u32,
    },
    VideoRemove {
        start: u64,
        end: u64,
    },
    Guide {
        action: guide_sections::Action,
    },
    /// Insert › PDF pages into the open guide before page `at` (approved
    /// beta 6 tab 6), as one Undo step.
    InsertPdfPages {
        pages: Vec<(std::sync::Arc<bitmap::Bitmap>, [f64; 2])>,
        at: usize,
        fit: bool,
    },
    SetObjectsVisible {
        ids: Vec<u64>,
        visible: bool,
    },
    SetObjectsLocked {
        ids: Vec<u64>,
        locked: bool,
    },
    /// Complete back-to-front layer order, committed once on row drop.
    ReorderObjects {
        ids: Vec<u64>,
    },
    PasteObjects {
        payload: clipboard_payload::ObjectPayload,
        center: [f32; 2],
    },
    RotateAnnotation {
        id: u64,
        degrees: f32,
    },
    Add {
        annotation: Annotation,
    },
    /// Atomic addition of independent editable objects (for duplication/paste).
    AddMany {
        annotations: Vec<Annotation>,
    },
    Update {
        annotation: Annotation,
    },
    /// Atomic object update for group transforms and batch formatting.
    UpdateMany {
        annotations: Vec<Annotation>,
    },
    /// Move/duplicate objects and pad the canvas as one reversible edit.
    MoveAndExpand {
        annotations: Vec<Annotation>,
        #[serde(default)]
        duplicate: bool,
    },
    LayoutSelection {
        ids: Vec<u64>,
        action: layout_selection::LayoutAction,
    },
    AlignToFirst {
        ids: Vec<u64>,
        action: layout_selection::LayoutAction,
    },
    Remove {
        id: u64,
    },
    RemoveMany {
        ids: Vec<u64>,
    },
    GroupObjects {
        ids: Vec<u64>,
    },
    UngroupObjects {
        ids: Vec<u64>,
    },
    Arrange {
        id: u64,
        order: StackOrder,
    },
    ArrangeMany {
        ids: Vec<u64>,
        order: StackOrder,
    },
    /// Restore the embedded source image by removing editable annotations.
    /// The original source pixels are never rewritten by this command.
    RevertOriginal,
    FlattenAll,
    FlattenPage {
        page: usize,
    },
    /// Remove a complete horizontal/vertical strip and join its remaining sides.
    CutOut {
        axis: cut_out::CutAxis,
        start: u32,
        end: u32,
        #[serde(default)]
        edge: cut_out::EdgeShape,
    },
    CutOutPages {
        axis: cut_out::CutAxis,
        start: u32,
        end: u32,
        #[serde(default)]
        edge: cut_out::EdgeShape,
        page: usize,
        all_pages: bool,
    },
    /// Transform the complete rendered image in one reversible operation.
    TransformImage {
        transform: ImageTransform,
    },
    SetCanvasExpansion {
        enabled: bool,
    },
    /// Whole-image perspective (FX-003); `None` lays the image flat.
    /// Filters, border and edge for the whole picture.
    SetFinish {
        finish: Option<picture_finish::Finish>,
    },
    SetPerspective {
        tilt: Option<tilt::Tilt>,
    },
    /// Crop the canvas in one reversible operation.
    CropImage {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        #[serde(default)]
        mode: crop::CropMode,
        #[serde(default)]
        page: usize,
    },
    ResizeImage {
        width: u32,
        height: u32,
    },
    /// Resize Image with a chosen resampling method (EDIT-028).
    ResizeImageResampled {
        width: u32,
        height: u32,
        resampling: resize::Resampling,
    },
    /// Rotate the whole picture by any angle (EDIT-038); objects rotate with
    /// it and stay editable. `grow` enlarges the canvas so nothing is cut off;
    /// `corners` fills the new corners (None is transparent).
    RotateImage {
        degrees: f32,
        grow: bool,
        corners: Option<[u8; 4]>,
    },
    /// Bring back the picture as it was before Rotate by angle, with every
    /// object turned back (BY3).
    ResetRotation,
    /// Flatten at the current size and resize in one reversible operation.
    ResizeImageFlattened {
        width: u32,
        height: u32,
    },
    ResizeCanvas {
        width: u32,
        height: u32,
        placement: Placement,
    },
    Undo,
    Redo,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StackOrder {
    BringForward,
    BringToFront,
    SendBackward,
    SendToBack,
}

impl Document {
    fn stack_target(&self, id: u64, order: StackOrder) -> Option<(usize, usize)> {
        let index = self
            .annotations
            .iter()
            .position(|annotation| annotation.id == id)?;
        let last = self.annotations.len() - 1;
        let target = match order {
            StackOrder::BringForward => (index + 1).min(last),
            StackOrder::BringToFront => last,
            StackOrder::SendBackward => index.saturating_sub(1),
            StackOrder::SendToBack => 0,
        };
        Some((index, target))
    }

    pub fn can_arrange(&self, id: u64, order: StackOrder) -> bool {
        self.stack_target(id, order)
            .is_some_and(|(index, target)| index != target)
    }
}

/// Undo snapshots share the unchanged source PNG in Document. Only editable
/// state is retained, avoiding a large encoded-image copy for every command.
struct EditState {
    video: Option<video::editor::Project>,
    canvas_behavior: canvas_behavior::CanvasBehavior,
    guide: Option<guide_sections::Layout>,
    object_states: object_state::ObjectStates,
    version: u32,
    width: u32,
    height: u32,
    source_size: Option<[u32; 2]>,
    raster: Option<std::sync::Arc<RasterImage>>,
    auto_canvas_base: Option<[u32; 4]>,
    annotations: Vec<Annotation>,
}
impl EditState {
    fn from_document(document: &Document) -> Self {
        Self {
            video: document.video.clone(),
            canvas_behavior: document.canvas_behavior.clone(),
            guide: document.guide.clone(),
            object_states: document.object_states.clone(),
            version: document.version,
            width: document.width,
            height: document.height,
            source_size: document.source_size,
            raster: document.raster.clone(),
            auto_canvas_base: document.auto_canvas_base,
            annotations: document.annotations.clone(),
        }
    }
    fn matches(&self, document: &Document) -> bool {
        (match (&self.video, &document.video) {
            (Some(a), Some(b)) => a.same_edit(b),
            (None, None) => true,
            _ => false,
        }) && self.canvas_behavior == document.canvas_behavior
            && self.guide == document.guide
            && self.object_states == document.object_states
            && self.version == document.version
            && self.width == document.width
            && self.height == document.height
            && self.source_size == document.source_size
            && self.auto_canvas_base == document.auto_canvas_base
            && self.raster == document.raster
            && self.annotations == document.annotations
    }
    fn restore(self, document: &mut Document) -> Self {
        Self {
            video: std::mem::replace(&mut document.video, self.video),
            canvas_behavior: std::mem::replace(&mut document.canvas_behavior, self.canvas_behavior),
            guide: std::mem::replace(&mut document.guide, self.guide),
            object_states: std::mem::replace(&mut document.object_states, self.object_states),
            version: std::mem::replace(&mut document.version, self.version),
            width: std::mem::replace(&mut document.width, self.width),
            height: std::mem::replace(&mut document.height, self.height),
            source_size: std::mem::replace(&mut document.source_size, self.source_size),
            raster: std::mem::replace(&mut document.raster, self.raster),
            auto_canvas_base: std::mem::replace(
                &mut document.auto_canvas_base,
                self.auto_canvas_base,
            ),
            annotations: std::mem::replace(&mut document.annotations, self.annotations),
        }
    }
}

pub struct Session {
    pub document: Document,
    saved_state: Option<EditState>,
    gesture: Option<EditState>,
    undo: Vec<EditState>,
    redo: Vec<EditState>,
}

impl Session {
    /// Retain dependencies referenced by live edits, including Undo and Redo.
    pub fn retained_media(&self) -> Vec<std::path::PathBuf> {
        self.document
            .video
            .iter()
            .chain(
                self.undo
                    .iter()
                    .chain(&self.redo)
                    .chain(self.gesture.iter())
                    .chain(self.saved_state.iter())
                    .filter_map(|s| s.video.as_ref()),
            )
            .flat_map(|v| v.media_paths())
            .map(std::path::PathBuf::from)
            .collect()
    }
    pub fn new(document: Document) -> Result<Self> {
        document.validate()?;
        Ok(Self {
            saved_state: Some(EditState::from_document(&document)),
            gesture: None,
            document,
            undo: vec![],
            redo: vec![],
        })
    }
    /// One pointer gesture may preview many validated edits but creates one Undo entry.
    pub fn begin_gesture(&mut self) {
        if self.gesture.is_none() {
            self.gesture = Some(EditState::from_document(&self.document));
        }
    }
    pub fn finish_gesture(&mut self) {
        if let Some(before) = self.gesture.take()
            && !before.matches(&self.document)
        {
            self.undo.push(before);
            self.trim_history();
            self.redo.clear();
        }
    }
    pub fn cancel_edit_gesture(&mut self) {
        if let Some(before) = self.gesture.take() {
            before.restore(&mut self.document);
        }
    }
    fn trim_history(&mut self) {
        while self.undo.len() > 200 || raster_history_bytes(&self.undo) > 256 * 1024 * 1024 {
            self.undo.remove(0);
        }
    }
    pub fn can_undo(&self) -> bool {
        self.gesture
            .as_ref()
            .is_some_and(|s| !s.matches(&self.document))
            || !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    /// A new capture/import has no saved state, even before its first annotation.
    pub fn unsaved(document: Document) -> Result<Self> {
        let mut session = Self::new(document)?;
        session.saved_state = None;
        Ok(session)
    }
    /// Whether output dimensions or annotations differ from the saved state.
    pub fn is_modified(&self) -> bool {
        !self
            .saved_state
            .as_ref()
            .is_some_and(|state| state.matches(&self.document))
    }
    /// After a project's media folder is renamed, points the document and
    /// every Undo state at the moved files, so Undo keeps working (FILE-007).
    pub fn relocate_media(&mut self, from: &Path, to: &Path) {
        project_file::relocate(&mut self.document.video, from, to);
        for state in self
            .undo
            .iter_mut()
            .chain(self.redo.iter_mut())
            .chain(self.saved_state.iter_mut())
            .chain(self.gesture.iter_mut())
        {
            project_file::relocate(&mut state.video, from, to);
        }
    }
    pub fn save(&mut self, path: &Path) -> Result<()> {
        self.document.save(path)?;
        self.saved_state = Some(EditState::from_document(&self.document));
        Ok(())
    }
    pub fn next_id(&self) -> u64 {
        self.document
            .annotations
            .iter()
            .map(|a| a.id)
            .max()
            .unwrap_or(0)
            .saturating_add(1)
    }
    pub fn execute(&mut self, command: Command) -> Result<()> {
        if matches!(command, Command::Undo | Command::Redo) {
            self.finish_gesture();
        }
        if self.document.video.is_some()
            && matches!(
                command,
                Command::Guide { .. }
                    | Command::RevertOriginal
                    | Command::FlattenAll
                    | Command::FlattenPage { .. }
                    | Command::CutOut { .. }
                    | Command::CutOutPages { .. }
                    | Command::TransformImage { .. }
                    | Command::SetCanvasExpansion { .. }
                    | Command::SetPerspective { .. }
                    | Command::SetFinish { .. }
                    | Command::CropImage { .. }
                    | Command::ResizeImage { .. }
                    | Command::ResizeImageResampled { .. }
                    | Command::RotateImage { .. }
                    | Command::ResetRotation
                    | Command::ResizeImageFlattened { .. }
                    | Command::ResizeCanvas { .. }
            )
        {
            return Err(Error::Invalid("Use the video timeline to trim, or Advanced video setup to change the frame. Image canvas commands do not alter a video clip.".into()));
        }
        self.document.protect_locked_objects(&command)?;
        let canvas = [self.document.width, self.document.height];
        let command = match command {
            Command::Add { annotation } => Command::Add {
                annotation: annotation.fit_text_shape(canvas)?,
            },
            Command::Update { annotation } => Command::Update {
                annotation: annotation.fit_text_shape(canvas)?,
            },
            Command::AddMany { annotations } => Command::AddMany {
                annotations: annotations
                    .iter()
                    .map(|a| a.fit_text_shape(canvas))
                    .collect::<Result<_>>()?,
            },
            Command::UpdateMany { annotations } => Command::UpdateMany {
                annotations: annotations
                    .iter()
                    .map(|a| a.fit_text_shape(canvas))
                    .collect::<Result<_>>()?,
            },
            other => other,
        };
        match command {
            Command::Undo => {
                if let Some(previous) = self.undo.pop() {
                    self.redo.push(previous.restore(&mut self.document));
                }
            }
            Command::Redo => {
                if let Some(next) = self.redo.pop() {
                    self.undo.push(next.restore(&mut self.document));
                }
            }
            command => {
                let previous = EditState::from_document(&self.document);
                match command {
                    Command::VideoDeleteItems { items } => {
                        let v = self
                            .document
                            .video
                            .as_mut()
                            .ok_or_else(|| Error::Invalid("Open a video first.".into()))?;
                        let expanded = v.linked_items(&items);
                        if expanded.iter().any(|item| matches!(item, video::editing::Item::Annotation(id) if self.document.object_states.get(id).is_some_and(|s| s.locked))) {
                            return Err(Error::Invalid("These objects are locked. Unlock them to delete them.".into()));
                        }
                        let removed = v.remove_items(&items);
                        self.document
                            .annotations
                            .retain(|a| !removed.contains(&a.id));
                    }
                    Command::VideoDeleteTrack { layer } => {
                        let v = self
                            .document
                            .video
                            .as_mut()
                            .ok_or_else(|| Error::Invalid("Open a video first.".into()))?;
                        let removed = v.remove_layer(layer);
                        self.document
                            .annotations
                            .retain(|a| !removed.contains(&a.id));
                    }
                    Command::VideoComposition {
                        project,
                        annotations,
                    } => {
                        self.document.video = Some(project);
                        self.document.annotations = annotations;
                        self.document.prune_object_states();
                    }
                    Command::Video { project } => {
                        self.document.video = Some(project);
                    }
                    Command::VideoDuplicate { id, timing } => {
                        if self.document.object_state(id).locked {
                            return Err(Error::Invalid(
                                "This object is locked. Unlock it to duplicate it.".into(),
                            ));
                        }
                        let original = self
                            .document
                            .video
                            .as_ref()
                            .and_then(|v| v.objects.get(&id))
                            .cloned()
                            .ok_or_else(|| {
                                Error::Invalid("Select a video object to duplicate.".into())
                            })?;
                        let copy = self.document.duplicate_selection(&[id])?.remove(0);
                        self.document
                            .video
                            .as_mut()
                            .unwrap()
                            .place(copy.id, timing.unwrap_or(original));
                        self.document.annotations.push(copy);
                    }
                    Command::VideoResize { width, height } => {
                        if self.document.video.is_none() {
                            return Err(Error::Invalid("Open a video first.".into()));
                        }
                        self.document.source_size = Some(
                            self.document
                                .source_size
                                .unwrap_or([self.document.width, self.document.height]),
                        );
                        self.document.width = width;
                        self.document.height = height;
                    }
                    Command::VideoRemove { start, end } => {
                        let removed = self
                            .document
                            .video
                            .as_mut()
                            .ok_or_else(|| Error::Invalid("Open a video first.".into()))?
                            .remove_range(start, end)?;
                        self.document
                            .annotations
                            .retain(|a| !removed.contains(&a.id));
                    }
                    Command::Guide { action } => {
                        self.document = self.document.edit_guide(&action)?;
                        if previous.matches(&self.document) {
                            return Ok(());
                        }
                    }
                    Command::SetObjectsVisible { ids, visible } => {
                        if !self
                            .document
                            .set_object_states(&ids, Some(!visible), None)?
                        {
                            return Ok(());
                        }
                    }
                    Command::SetObjectsLocked { ids, locked } => {
                        if !self.document.set_object_states(&ids, None, Some(locked))? {
                            return Ok(());
                        }
                    }
                    Command::ReorderObjects { ids } => {
                        if !self.document.reorder_objects(&ids)? {
                            return Ok(());
                        }
                    }
                    Command::PasteObjects { payload, center } => {
                        self.document.paste_objects(payload, center)?;
                    }
                    Command::RotateAnnotation { id, degrees } => {
                        if !self.document.rotate_annotation(id, degrees)? {
                            return Ok(());
                        }
                    }
                    Command::CutOut {
                        axis,
                        start,
                        end,
                        edge,
                    } => {
                        self.document.cut_out(axis, start, end, edge)?;
                    }
                    Command::CutOutPages {
                        axis,
                        start,
                        end,
                        edge,
                        page,
                        all_pages,
                    } => {
                        let mut next = self.document.clone();
                        next.cut_out_pages(axis, start, end, edge, page, all_pages)?;
                        self.document = next;
                    }

                    Command::SetCanvasExpansion { enabled } => {
                        if !self.document.set_canvas_expansion(enabled) {
                            return Ok(());
                        }
                    }
                    Command::SetFinish { finish } => {
                        if self.document.canvas_behavior.finish == finish {
                            return Ok(());
                        }
                        if let Some(f) = &finish {
                            f.validate()?;
                        }
                        self.document.canvas_behavior.finish = finish;
                    }
                    Command::SetPerspective { tilt } => {
                        if self.document.canvas_behavior.perspective == tilt {
                            return Ok(());
                        }
                        if let Some(t) = &tilt {
                            t.validate()?;
                        }
                        self.document.canvas_behavior.perspective = tilt;
                    }
                    Command::CropImage {
                        x,
                        y,
                        width,
                        height,
                        mode,
                        page,
                    } => {
                        let mut cropped = self.document.clone();
                        if !cropped.crop_image([x, y, width, height], mode, page)? {
                            return Ok(());
                        }
                        self.document = cropped;
                    }
                    Command::TransformImage { transform } => {
                        if !self.document.transform_image(transform)? {
                            return Ok(());
                        }
                    }
                    Command::FlattenPage { page } => {
                        self.document = self.document.flatten_pages(Some(page))?;
                    }
                    Command::FlattenAll if self.document.guide.is_some() => {
                        self.document = self.document.flatten_pages(None)?;
                    }
                    Command::FlattenAll => {
                        if self.document.annotations.is_empty() && self.document.guide.is_none() {
                            return Ok(());
                        }
                        let raster =
                            std::sync::Arc::new(RasterImage::from_image(&self.document.render()?)?);
                        self.document.source_size = Some(
                            self.document
                                .source_size
                                .unwrap_or([self.document.width, self.document.height]),
                        );
                        self.document.raster = Some(raster);
                        self.document.auto_canvas_base = None;
                        self.document.version = MAX_PROJECT_VERSION;
                        self.document.annotations.clear();
                        self.document.guide = None;
                    }
                    Command::MoveAndExpand {
                        annotations,
                        duplicate,
                    } => {
                        let plan = self.document.plan_expanding_move(&annotations, duplicate)?;
                        if plan.dimensions == [self.document.width, self.document.height]
                            && plan.offset == [0, 0]
                            && plan.annotations == self.document.annotations
                        {
                            return Ok(());
                        }
                        self.document = plan.apply(&self.document)?;
                    }
                    Command::ResizeCanvas {
                        width,
                        height,
                        placement,
                    } => {
                        if (width, height) == (self.document.width, self.document.height) {
                            self.document.canvas_annotations(width, height, placement)?;
                            return Ok(());
                        }
                        self.document.resize_canvas(width, height, placement)?;
                    }
                    Command::InsertPdfPages { pages, at, fit } => {
                        let pages: Vec<_> =
                            pages.iter().map(|(b, p)| ((**b).clone(), *p)).collect();
                        self.document = pdf_import::insert_pages(&self.document, &pages, at, fit)?;
                    }
                    Command::RotateImage {
                        degrees,
                        grow,
                        corners,
                    } => {
                        if !self.document.rotate_image(degrees, grow, corners)? {
                            return Ok(());
                        }
                    }
                    Command::ResetRotation => {
                        if !self.document.reset_rotation()? {
                            return Ok(());
                        }
                    }
                    Command::ResizeImageResampled {
                        width,
                        height,
                        resampling,
                    } => {
                        let same_size =
                            (width, height) == (self.document.width, self.document.height);
                        if same_size && self.document.canvas_behavior.resampling == resampling {
                            return Ok(());
                        }
                        self.document.canvas_behavior.resampling = resampling;
                        if !same_size {
                            self.document.resize_image_to(width, height)?;
                        }
                    }
                    Command::ResizeImage { width, height } => {
                        if (width, height) == (self.document.width, self.document.height) {
                            return Ok(());
                        }
                        self.document.resize_image_to(width, height)?;
                    }
                    Command::ResizeImageFlattened { width, height } => {
                        validate_dimensions(width, height)?;
                        if (width, height) == (self.document.width, self.document.height) {
                            return Ok(());
                        }
                        // Finish every fallible operation before modifying the document.
                        let raster = if self.document.annotations.is_empty() {
                            self.document.raster.clone()
                        } else {
                            Some(std::sync::Arc::new(RasterImage::from_image(
                                &self.document.render()?,
                            )?))
                        };
                        let original_size = self
                            .document
                            .source_size
                            .unwrap_or([self.document.width, self.document.height]);
                        self.document.source_size = ([width, height] != original_size
                            || raster.is_some())
                        .then_some(original_size);
                        if raster.is_some() {
                            self.document.version = MAX_PROJECT_VERSION;
                        }
                        self.document.raster = raster;
                        self.document.auto_canvas_base = None;
                        self.document.width = width;
                        self.document.height = height;
                        self.document.annotations.clear();
                    }
                    Command::RevertOriginal => {
                        if !self.document.can_revert() {
                            // A no-op must not consume Undo or discard Redo.
                            return Ok(());
                        }
                        self.document.guide = None;
                        self.document.annotations.clear();
                        self.document.raster = None;
                        self.document.auto_canvas_base = None;
                        self.document.version = MAX_PROJECT_VERSION;
                        if let Some([width, height]) = self.document.source_size.take() {
                            self.document.width = width;
                            self.document.height = height;
                        }
                    }
                    Command::AddTimed {
                        annotations,
                        motion,
                    } => {
                        let mut next = self.document.clone();
                        let annotations = annotations
                            .iter()
                            .map(|a| a.fit_text_shape(canvas))
                            .collect::<Result<Vec<_>>>()?;
                        let plan = next.plan_expanding_move(&annotations, true)?;
                        next = plan.apply(&next)?;
                        let v = next
                            .video
                            .as_mut()
                            .ok_or_else(|| Error::Invalid("Open a video first.".into()))?;
                        v.sync(&next.annotations);
                        for a in &annotations {
                            if let Some(motion) = motion.get(&a.id) {
                                motion.apply(a, v.objects.get_mut(&a.id).unwrap())?;
                            }
                        }
                        next.validate()?;
                        self.document = next;
                    }
                    Command::Add { annotation } => self.document.annotations.push(annotation),
                    Command::AddMany { annotations } => {
                        if annotations.is_empty() {
                            return Ok(());
                        }
                        self.document.annotations.extend(annotations);
                    }
                    Command::Update { annotation } => {
                        let target = self
                            .document
                            .annotations
                            .iter_mut()
                            .find(|a| a.id == annotation.id)
                            .ok_or_else(|| Error::Invalid("Annotation does not exist".into()))?;
                        *target = annotation;
                    }
                    Command::UpdateMany { annotations } => {
                        if !self.document.update_many(annotations)? {
                            return Ok(());
                        }
                    }
                    Command::LayoutSelection { ids, action } => {
                        let annotations = self.document.layout_selection(&ids, action)?;
                        if !self.document.update_many(annotations)? {
                            return Ok(());
                        }
                    }
                    Command::AlignToFirst { ids, action } => {
                        let annotations = self.document.align_to_first(&ids, action)?;
                        if annotations
                            .iter()
                            .all(|a| self.document.annotations.iter().any(|b| a == b))
                        {
                            return Ok(());
                        }
                        self.document = self
                            .document
                            .plan_expanding_move(&annotations, false)?
                            .apply(&self.document)?;
                    }
                    Command::Remove { id } => {
                        let index = self
                            .document
                            .annotations
                            .iter()
                            .position(|a| a.id == id)
                            .ok_or_else(|| Error::Invalid("Annotation does not exist".into()))?;
                        self.document.annotations.remove(index);
                    }
                    Command::ArrangeMany { ids, order } => {
                        let ids = self.document.arranged_groups(&ids, order)?;
                        if !self.document.reorder_objects(&ids)? {
                            return Ok(());
                        }
                    }
                    Command::GroupObjects { ids } => {
                        if !self.document.group_objects(&ids, false)? {
                            return Ok(());
                        }
                    }

                    Command::UngroupObjects { ids } => {
                        if !self.document.group_objects(&ids, true)? {
                            return Ok(());
                        }
                    }
                    Command::RemoveMany { ids } => {
                        if ids.is_empty() {
                            return Ok(());
                        }
                        let unique: std::collections::BTreeSet<_> = ids.iter().copied().collect();
                        if unique.len() != ids.len()
                            || unique
                                .iter()
                                .any(|id| !self.document.annotations.iter().any(|a| a.id == *id))
                        {
                            return Err(Error::Invalid("Invalid object selection".into()));
                        }
                        self.document
                            .annotations
                            .retain(|a| !unique.contains(&a.id));
                    }
                    Command::Arrange { id, order } => {
                        let (index, target) = self
                            .document
                            .stack_target(id, order)
                            .ok_or_else(|| Error::Invalid("Annotation does not exist".into()))?;
                        if index == target {
                            return Ok(());
                        }
                        let annotation = self.document.annotations.remove(index);
                        self.document.annotations.insert(target, annotation);
                    }
                    _ => {}
                }
                if let Some(v) = self.document.video.as_mut() {
                    v.sync(&self.document.annotations);
                }
                if let Some((old, new)) = previous.video.as_ref().zip(self.document.video.as_mut())
                {
                    let changed = previous.width != self.document.width
                        || previous.height != self.document.height
                        || previous.annotations != self.document.annotations
                        || previous.object_states != self.document.object_states
                        || old.track_settings != new.track_settings
                        || old.source != new.source
                        || old.segments != new.segments
                        || old.zooms != new.zooms
                        || old.layer_order != new.layer_order
                        || old.crop != new.crop
                        || old.fill != new.fill
                        || old.background != new.background
                        || old.objects.iter().any(|(id, t)| {
                            new.objects.get(id).is_none_or(|n| {
                                (t.start, t.end, t.track) != (n.start, n.end, n.track)
                                    || reusable_presets::Motion::from(t)
                                        != reusable_presets::Motion::from(n)
                                    || t.keyframes != n.keyframes
                                    || t.motion_path
                                        .as_ref()
                                        .map(|p| (&p.nodes, p.enabled, p.smooth, p.offset_ms))
                                        != n.motion_path
                                            .as_ref()
                                            .map(|p| (&p.nodes, p.enabled, p.smooth, p.offset_ms))
                            })
                        });
                    if changed {
                        for (id, t) in &mut new.objects {
                            if old.objects.contains_key(id)
                                && let Some(path) = &mut t.motion_path
                                && path.purpose == video::motion_path::Purpose::Redaction
                            {
                                std::sync::Arc::make_mut(path).reviewed = false;
                            }
                        }
                    }
                }

                self.document.prune_groups();
                self.document.reconcile_guide();
                self.document.prune_object_states();
                self.document.prune_canvas_behavior();
                self.document.promote_features();
                if let Err(error) = self
                    .document
                    .validate()
                    .and_then(|_| self.document.fit_automatic_canvas())
                {
                    previous.restore(&mut self.document);
                    return Err(error);
                }
                if self.gesture.is_none() {
                    self.undo.push(previous);
                    self.trim_history();
                    self.redo.clear();
                }
            }
        }
        Ok(())
    }
}

fn raster_history_bytes(states: &[EditState]) -> usize {
    let mut seen = std::collections::HashSet::new();
    let raster: usize = states
        .iter()
        .filter_map(|state| state.raster.as_ref())
        .filter(|raster| seen.insert(std::sync::Arc::as_ptr(raster)))
        .map(|raster| raster.png.len())
        .sum();
    let mut images = std::collections::HashSet::new();
    raster
        + states
            .iter()
            .flat_map(|state| &state.annotations)
            .filter_map(|a| a.image.as_ref())
            .filter(|bitmap| images.insert(std::sync::Arc::as_ptr(bitmap)))
            .map(|bitmap| bitmap.memory_bytes())
            .sum::<usize>()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn new_capture_stays_unsaved_through_undo_export_and_failed_save() {
        let image = RgbaImage::from_pixel(12, 10, Rgba([25, 60, 120, 255]));
        let mut session = Session::unsaved(Document::from_image(image.clone()).unwrap()).unwrap();
        let temp = tempfile::tempdir().unwrap();
        assert!(session.is_modified());
        session
            .execute(Command::Add {
                annotation: annotation(1),
            })
            .unwrap();
        session.execute(Command::Undo).unwrap();
        assert!(session.is_modified());
        session
            .document
            .export_png(&temp.path().join("capture.png"))
            .unwrap();
        assert!(session.is_modified());
        assert!(
            session
                .save(&temp.path().join("missing/capture.json"))
                .is_err()
        );
        assert!(session.is_modified());
        let path = temp.path().join("capture.json");
        session.save(&path).unwrap();
        assert!(!session.is_modified());
        assert_eq!(Document::load(&path).unwrap().render().unwrap(), image);
    }
    fn annotation(id: u64) -> Annotation {
        Annotation {
            image: None,
            step_outline: None,
            line_style: None,
            shadow: None,

            focus: None,

            highlighter_rect: false,
            hard_edge: false,
            group: None,
            rotation_degrees: 0.0,
            mirror_x: false,
            id,
            kind: Kind::Redact,
            start: [2.0, 3.0],
            end: [8.0, 9.0],
            color: [255, 0, 0, 10],
            width: 2,
            text: None,
            corner_radius: 0.0,
            fill: None,
            points: Vec::new(),
            callout: None,
        }
    }
    #[test]
    fn redaction_is_opaque_and_only_changes_selected_pixels() {
        let mut session = Session::new(Document::blank(10, 10).unwrap()).unwrap();
        session
            .execute(Command::Add {
                annotation: annotation(1),
            })
            .unwrap();
        let image = session.document.render().unwrap();
        assert_eq!(image.get_pixel(4, 5).0, [0, 0, 0, 255]);
        assert_eq!(image.get_pixel(0, 0).0, [255, 255, 255, 255]);
        assert_eq!(
            session.document.base_image().unwrap().get_pixel(4, 5).0,
            [255, 255, 255, 255]
        );
    }
    #[test]
    fn colored_and_transparent_canvases_survive_save_reopen_and_export() {
        let temp = tempfile::tempdir().unwrap();
        for color in [[255; 4], [18, 52, 171, 128], [0; 4]] {
            let document = Document::canvas(73, 41, color).unwrap();
            let mut session = Session::unsaved(document).unwrap();
            assert!(session.is_modified());
            let project = temp.path().join("canvas.json");
            let png = temp.path().join("canvas.png");
            session.save(&project).unwrap();
            assert!(!session.is_modified());
            let reopened = Document::load(&project).unwrap();
            assert_eq!((reopened.width, reopened.height), (73, 41));
            reopened.export_png(&png).unwrap();
            let image = image::open(&png).unwrap().into_rgba8();
            assert!(image.pixels().all(|p| p.0 == color));
            session
                .execute(Command::Add {
                    annotation: annotation(1),
                })
                .unwrap();
            session.execute(Command::Undo).unwrap();
            assert!(!session.is_modified());
            assert_eq!(session.document.render().unwrap(), image);
        }
    }
    #[test]
    fn new_canvas_validates_before_allocating() {
        for (width, height) in [(0, 1), (1, 0), (u32::MAX, 2), (32769, 1), (5001, 5000)] {
            assert!(Document::canvas(width, height, [0; 4]).is_err());
        }
    }
    #[test]
    fn undo_redo_and_divergent_edits() {
        let mut s = Session::new(Document::blank(10, 10).unwrap()).unwrap();
        s.execute(Command::Add {
            annotation: annotation(1),
        })
        .unwrap();
        s.execute(Command::Undo).unwrap();
        assert!(s.document.annotations.is_empty());
        s.execute(Command::Redo).unwrap();
        assert_eq!(s.document.annotations.len(), 1);
        s.execute(Command::Undo).unwrap();
        s.execute(Command::Add {
            annotation: annotation(2),
        })
        .unwrap();
        assert!(!s.can_redo());
        s.execute(Command::Redo).unwrap();
        assert_eq!(s.document.annotations[0].id, 2);
    }
    #[test]
    fn invalid_command_rolls_back_and_preserves_redo() {
        let mut s = Session::new(Document::blank(10, 10).unwrap()).unwrap();
        s.execute(Command::Add {
            annotation: annotation(1),
        })
        .unwrap();
        s.execute(Command::Undo).unwrap();
        let mut bad = annotation(2);
        bad.end = [f32::NAN, 5.0];
        assert!(s.execute(Command::Add { annotation: bad }).is_err());
        assert!(s.document.annotations.is_empty());
        assert!(s.can_redo());
    }
    #[test]
    fn portable_project_round_trip_preserves_render_and_editability() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("roundtrip.capture.json");
        let source = RgbaImage::from_pixel(10, 10, Rgba([30, 90, 140, 255]));
        let mut s = Session::new(Document::from_image(source).unwrap()).unwrap();
        s.execute(Command::Add {
            annotation: annotation(1),
        })
        .unwrap();
        s.document.save(&path).unwrap();
        let loaded = Document::load(&path).unwrap();
        assert_eq!(loaded.render().unwrap(), s.document.render().unwrap());
        let mut reopened = Session::new(loaded).unwrap();
        reopened.execute(Command::Remove { id: 1 }).unwrap();
        assert_eq!(
            reopened.document.render().unwrap().get_pixel(4, 5).0,
            [30, 90, 140, 255]
        );
        reopened.document.save(&path).unwrap();
        assert!(Document::load(&path).unwrap().annotations.is_empty());
    }
    #[test]
    fn rejects_invalid_dimensions_versions_and_duplicate_ids() {
        assert!(Document::blank(0, 1).is_err());
        assert!(Document::blank(10000, 10000).is_err());
        let mut d = Document::blank(10, 10).unwrap();
        d.version = 999;
        assert!(d.validate().is_err());
        d.version = 1;
        d.annotations = vec![annotation(1), annotation(1)];
        assert!(d.validate().is_err());
    }

    #[test]
    fn saved_state_tracks_undo_redo_and_divergent_edits() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("saved.capture.json");
        let mut session = Session::new(Document::blank(10, 10).unwrap()).unwrap();
        session.execute(Command::Undo).unwrap();
        assert!(!session.is_modified());
        session
            .execute(Command::Add {
                annotation: annotation(1),
            })
            .unwrap();
        assert!(session.is_modified());
        session.save(&path).unwrap();
        assert!(!session.is_modified());
        session.execute(Command::Undo).unwrap();
        assert!(session.is_modified());
        session.execute(Command::Redo).unwrap();
        assert!(!session.is_modified());
        session.execute(Command::Undo).unwrap();
        session
            .execute(Command::Add {
                annotation: annotation(2),
            })
            .unwrap();
        assert!(session.is_modified());
        assert_eq!(Document::load(&path).unwrap().annotations[0].id, 1);
    }

    #[test]
    fn failed_save_and_png_export_do_not_clear_unsaved_edits() {
        let temp = tempfile::tempdir().unwrap();
        let mut session = Session::new(Document::blank(10, 10).unwrap()).unwrap();
        session
            .execute(Command::Add {
                annotation: annotation(1),
            })
            .unwrap();
        assert!(
            session
                .save(&temp.path().join("missing").join("save.json"))
                .is_err()
        );
        assert!(session.is_modified());
        session
            .document
            .export_png(&temp.path().join("shared.png"))
            .unwrap();
        assert!(session.is_modified());
        session.execute(Command::Undo).unwrap();
        assert!(!session.is_modified());
    }
}
pub mod angle_snap;

#[cfg(test)]
mod capture_refinement_tests;
