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
pub const MAX_PROJECT_VERSION: u32 = 21;
mod arrow;
pub mod bitmap;
pub mod callout;
pub mod canvas_expansion;
mod canvas_size;
pub mod capture_options;
pub mod captured_cursor;
pub mod clipboard_payload;
mod crop;
pub mod display;
pub mod editor_keybinds;
pub mod ellipse;
pub mod export;
mod flood_fill;
pub mod groups;
pub mod guide_sections;
pub mod history;
pub mod image_frame;
mod image_transform;
pub mod keybinds;
mod obscure;
pub mod stamps;
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
#[cfg(test)]
mod flatten_tests;
pub mod layout_selection;
pub mod line_style;
pub mod multi_selection;
pub mod object_state;
pub mod object_styles;
pub mod pixel_mask;
pub mod pixels;
pub mod polygon;
pub mod quick_preferences;
#[cfg(test)]
mod redaction_tests;
mod resize;
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
pub mod simplify;
pub mod snapping;
#[cfg(test)]
mod stacking_tests;
pub mod text;
#[cfg(test)]
mod text_fit_tests;
#[cfg(test)]
mod text_padding_tests;
pub mod toolbar;
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
    Simplify,
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
    /// Invisible bounds of a live automatic Simplify group.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub analysis: Option<Box<simplify::Analysis>>,
    #[serde(default, skip_serializing_if = "text::is_false")]
    pub highlighter_rect: bool,
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
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub object_states: object_state::ObjectStates,
    pub annotations: Vec<Annotation>,
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
            version: 1,
            width,
            height,
            source_png: String::new(),
            source_size: None,
            raster: None,
            auto_canvas_base: None,
            guide: None,
            object_states: Default::default(),
            annotations: vec![],
        })
    }

    pub fn from_image(image: RgbaImage) -> Result<Self> {
        validate_dimensions(image.width(), image.height())?;
        let mut data = Cursor::new(Vec::new());
        image.write_to(&mut data, ImageFormat::Png)?;
        Ok(Self {
            version: 1,
            width: image.width(),
            height: image.height(),
            source_png: STANDARD.encode(data.into_inner()),
            source_size: None,
            raster: None,
            auto_canvas_base: None,
            guide: None,
            object_states: Default::default(),
            annotations: vec![],
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
        let (width, height) = image::image_dimensions(path)?;
        validate_dimensions(width, height)?;
        Self::from_image(image::open(path)?.into_rgba8())
    }

    /// Promote documents that use newer editable annotation properties.
    /// Old applications reject these projects instead of silently losing rules.
    pub fn promote_features(&mut self) {
        if self.annotations.iter().any(|a| {
            a.text
                .as_ref()
                .is_some_and(|t| matches!(t.font, text::TextFont::System(_)))
        }) {
            self.version = self.version.max(21);
        }
        if self.annotations.iter().any(|a| a.analysis.is_some()) {
            self.version = self.version.max(20);
        }
        if self
            .annotations
            .iter()
            .any(|a| a.image.as_ref().is_some_and(|b| b.focus().is_some()))
        {
            self.version = self.version.max(19);
        }
        if self
            .annotations
            .iter()
            .any(|a| a.text.as_ref().is_some_and(|s| s.typography.is_some()))
        {
            self.version = self.version.max(18);
        }
        if self.annotations.iter().any(|a| a.highlighter_rect) {
            self.version = self.version.max(17);
        }
        if self.annotations.iter().any(|a| a.group.is_some()) {
            self.version = self.version.max(16);
        }
        if self.annotations.iter().any(|a| a.line_style.is_some()) {
            self.version = self.version.max(15);
        }
        if self.annotations.iter().any(|a| a.shadow.is_some()) {
            self.version = self.version.max(14);
        }
        if self.annotations.iter().any(|a| a.kind == Kind::Simplify) {
            self.version = self.version.max(13);
        }
        if self
            .annotations
            .iter()
            .any(|a| a.text.as_ref().is_some_and(|s| s.outline.is_some()))
        {
            self.version = self.version.max(12);
        }
        if self.guide.is_some() {
            self.version = self.version.max(11);
        }
        if self.annotations.iter().any(|a| {
            a.kind == Kind::Image
                && (a.corner_radius > 0. || a.image.as_ref().is_some_and(|b| b.has_view()))
        }) {
            self.version = self.version.max(10);
        }
        if self.annotations.iter().any(|a| a.image.is_some()) {
            self.version = self.version.max(
                if self
                    .annotations
                    .iter()
                    .any(|a| a.image.as_ref().is_some_and(|b| b.is_cursor()))
                {
                    9
                } else {
                    8
                },
            );
        }
        if !self.object_states.is_empty() {
            self.version = self.version.max(7);
        }
        if self.annotations.iter().any(|a| a.step_outline.is_some()) {
            self.version = self.version.max(6);
        }
        if self
            .annotations
            .iter()
            .any(|a| a.text.as_ref().is_some_and(text::TextStyle::decorated))
        {
            self.version = self.version.max(5);
        }
    }

    pub fn validate(&self) -> Result<()> {
        if !(1..=MAX_PROJECT_VERSION).contains(&self.version) {
            return Err(Error::Invalid("Unsupported project version".into()));
        }
        validate_dimensions(self.width, self.height)?;
        if self.version < 21
            && self.annotations.iter().any(|a| {
                a.text
                    .as_ref()
                    .is_some_and(|t| matches!(t.font, text::TextFont::System(_)))
            })
        {
            return Err(Error::Invalid(
                "Installed fonts require project version 21".into(),
            ));
        }
        self.validate_object_states()?;
        self.validate_groups()?;
        if let Some(guide) = &self.guide {
            guide.validate(self)?;
        }
        if let Some([width, height]) = self.source_size {
            validate_dimensions(width, height)?;
        }
        if let Some(raster) = &self.raster {
            if self.version < 2 || self.source_size.is_none() {
                return Err(Error::Invalid(
                    "Canvas raster requires project version 2 and original dimensions".into(),
                ));
            }
            validate_dimensions(raster.width, raster.height)?;
        }
        if let Some([x, y, width, height]) = self.auto_canvas_base {
            validate_dimensions(width, height)?;
            if self.version < 4
                || x.checked_add(width).is_none_or(|end| end > self.width)
                || y.checked_add(height).is_none_or(|end| end > self.height)
            {
                return Err(Error::Invalid(
                    "Invalid automatic canvas base region".into(),
                ));
            }
        }
        if self.annotations.len() > 10000 {
            return Err(Error::Invalid("Too many annotations".into()));
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
            if let Some(analysis) = &annotation.analysis
                && (self.version < 20
                    || annotation.kind != Kind::Simplify
                    || annotation.group.is_none()
                    || annotation.rotation_degrees != 0.
                    || analysis.detail > 100
                    || analysis.colors.len() > 400)
            {
                return Err(Error::Invalid("Invalid live Simplify analysis area".into()));
            }
            if self.version < 18
                && annotation
                    .text
                    .as_ref()
                    .is_some_and(|s| s.typography.is_some())
            {
                return Err(Error::Invalid(
                    "Extended typography requires project version 18".into(),
                ));
            }
            if annotation.highlighter_rect
                && (self.version < 17 || annotation.kind != Kind::Highlighter)
            {
                return Err(Error::Invalid(
                    "Rectangle highlighting requires a Highlighter and project version 17".into(),
                ));
            }
            if let Some(style) = annotation.line_style {
                style.validate()?;
                if self.version < 15 || !matches!(annotation.kind, Kind::Arrow | Kind::Line) {
                    return Err(Error::Invalid(
                        "Line settings require a Line or Arrow and project version 15".into(),
                    ));
                }
            }
            if let Some(shadow) = annotation.shadow {
                shadow.validate()?;
                shadow::validate_patch(annotation)?;
                if self.version < 14 {
                    return Err(Error::Invalid(
                        "Object shadows require project version 14".into(),
                    ));
                }
            }
            if annotation.kind == Kind::Simplify && self.version < 13 {
                return Err(Error::Invalid(
                    "Simplify objects require project version 13".into(),
                ));
            }
            if self.version < 12
                && annotation
                    .text
                    .as_ref()
                    .is_some_and(|s| s.outline.is_some())
            {
                return Err(Error::Invalid(
                    "Text outlines require project version 12".into(),
                ));
            }
            match (annotation.kind, &annotation.image) {
                (Kind::Image, Some(bitmap)) if self.version >= 8 => {
                    bitmap.validate_cursor()?;
                    bitmap.validate_view()?;
                    if bitmap.focus().is_some() && self.version < 19 {
                        return Err(Error::Invalid(
                            "Image focus requires project version 19".into(),
                        ));
                    }
                    if (bitmap.has_view() || annotation.corner_radius > 0.) && self.version < 10 {
                        return Err(Error::Invalid(
                            "Image framing requires project version 10".into(),
                        ));
                    }
                    if bitmap.is_cursor() && self.version < 9 {
                        return Err(Error::Invalid(
                            "Editable cursors require project version 9".into(),
                        ));
                    }
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
                        "Embedded images require an Image object and project version 8".into(),
                    ));
                }
                _ => {}
            }
            if let Some(outline) = &annotation.step_outline {
                outline.validate()?;
                if self.version < 6 || annotation.kind != Kind::Step {
                    return Err(Error::Invalid(
                        "Step outline requires a Step and project version 6".into(),
                    ));
                }
            }
            if self.version < 5
                && annotation
                    .text
                    .as_ref()
                    .is_some_and(text::TextStyle::decorated)
            {
                return Err(Error::Invalid(
                    "Text decoration requires project version 5".into(),
                ));
            }
            if !annotation.rotation_degrees.is_finite()
                || !(-180.0..180.0).contains(&annotation.rotation_degrees)
                || (annotation.rotation_degrees != 0.0
                    && (self.version < 3 || !annotation.can_rotate()))
            {
                return Err(Error::Invalid(
                    "Object rotation requires a supported vector object and project version 3"
                        .into(),
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
                (Kind::Callout, Some(style)) => {
                    style.validate(annotation, [self.width, self.height])?
                }
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
            if annotation.width == 0 || annotation.width > 32 {
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
                _ => annotation.points.is_empty(),
            };
            if !valid_path {
                return Err(Error::Invalid("Strokes need 2-8192 points; polygons need 3-64; other objects cannot carry paths".into()));
            }
            for point in [annotation.start, annotation.end]
                .iter()
                .chain(&annotation.points)
            {
                if !point.iter().all(|n| n.is_finite())
                    || point[0] < 0.0
                    || point[1] < 0.0
                    || point[0] > self.width as f32
                    || point[1] > self.height as f32
                {
                    return Err(Error::Invalid(
                        "Annotation coordinates are outside the image".into(),
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn base_image(&self) -> Result<RgbaImage> {
        self.validate()?;
        if let Some(raster) = &self.raster {
            return raster.render(self.width, self.height);
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
            Ok(resize::resample(original, self.width, self.height))
        }
    }

    pub fn render(&self) -> Result<RgbaImage> {
        let mut image = self.base_image()?;
        for annotation in self
            .annotations
            .iter()
            .filter(|a| self.object_visible(a.id))
        {
            render_annotation(&mut image, annotation);
        }
        Ok(image)
    }

    pub fn load(path: &Path) -> Result<Self> {
        if fs::metadata(path)?.len() > MAX_FILE_BYTES {
            return Err(Error::Invalid("Project file is too large".into()));
        }
        let doc: Self = serde_json::from_slice(&fs::read(path)?)?;
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
            let [width, height] = original.source_size.take().unwrap();
            original.width = width;
            original.height = height;
            original.base_image()?;
        }
        Ok(doc)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(Error::Invalid("The editable project exceeds the supported 160 MB file size. Export a flattened PNG instead.".into()));
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
    if a.analysis.is_some() {
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
    if a.analysis.is_some() {
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
        Kind::Image | Kind::Simplify => display::render_rotated(image, a),
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
    Guide {
        action: guide_sections::Action,
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
    ReplaceSimplifyGroup {
        group: u64,
        annotations: Vec<Annotation>,
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
    /// Fill a connected region of the complete composite, preserving the
    /// original source and flattening editable objects in one Undo operation.
    FloodFill {
        x: u32,
        y: u32,
        color: [u8; 4],
        tolerance: u8,
    },
    /// Clear a rectangular pixel region of the rendered image with one Undo.
    ClearPixels {
        rect: pixels::PixelRect,
    },
    ClearMaskedPixels {
        mask: pixel_mask::PixelMask,
    },
    /// Lift a masked composite into an editable image, clearing its original
    /// pixels in the same transaction. The embedded source is preserved.
    LiftMaskedPixels {
        mask: pixel_mask::PixelMask,
    },
    /// Remove a complete horizontal/vertical strip and join its remaining sides.
    CutOut {
        axis: cut_out::CutAxis,
        start: u32,
        end: u32,
    },
    /// Transform the complete rendered image in one reversible operation.
    TransformImage {
        transform: ImageTransform,
    },
    /// Crop the complete composite in one reversible operation.
    CropImage {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },
    ResizeImage {
        width: u32,
        height: u32,
    },
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
        self.guide == document.guide
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
    undo: Vec<EditState>,
    redo: Vec<EditState>,
}

impl Session {
    pub fn new(document: Document) -> Result<Self> {
        document.validate()?;
        Ok(Self {
            saved_state: Some(EditState::from_document(&document)),
            document,
            undo: vec![],
            redo: vec![],
        })
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
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
                    Command::CutOut { axis, start, end } => {
                        self.document.cut_out(axis, start, end)?;
                    }
                    Command::ClearPixels { rect } => {
                        if !self.document.clear_pixels(rect)? {
                            return Ok(());
                        }
                    }
                    Command::ClearMaskedPixels { mask } => {
                        if !self.document.edit_masked_pixels(&mask, false)? {
                            return Ok(());
                        }
                    }
                    Command::LiftMaskedPixels { mask } => {
                        if !self.document.edit_masked_pixels(&mask, true)? {
                            return Ok(());
                        }
                    }
                    Command::FloodFill {
                        x,
                        y,
                        color,
                        tolerance,
                    } => {
                        if !self.document.flood_fill(x, y, color, tolerance)? {
                            return Ok(());
                        }
                    }
                    Command::CropImage {
                        x,
                        y,
                        width,
                        height,
                    } => {
                        if !self.document.crop_image(x, y, width, height)? {
                            return Ok(());
                        }
                    }
                    Command::TransformImage { transform } => {
                        if !self.document.transform_image(transform)? {
                            return Ok(());
                        }
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
                        self.document.version = 2;
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
                    Command::ResizeImage { width, height } => {
                        if (width, height) == (self.document.width, self.document.height) {
                            return Ok(());
                        }
                        let annotations = self.document.resized_annotations(width, height)?;
                        if let Some(guide) = &mut self.document.guide {
                            let scale = height as f32 / self.document.height as f32;
                            guide.top *= scale;
                            for section in &mut guide.sections {
                                section.height *= scale;
                            }
                        }
                        let original_size = self
                            .document
                            .source_size
                            .unwrap_or([self.document.width, self.document.height]);
                        self.document.source_size = ([width, height] != original_size
                            || self.document.raster.is_some())
                        .then_some(original_size);
                        self.document.width = width;
                        self.document.height = height;
                        self.document.annotations = annotations;
                        self.document.auto_canvas_base = None;
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
                            self.document.version = 2;
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
                        self.document.version = 1;
                        if let Some([width, height]) = self.document.source_size.take() {
                            self.document.width = width;
                            self.document.height = height;
                        }
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
                    Command::ReplaceSimplifyGroup { group, annotations } => {
                        self.document.replace_simplify_group(group, annotations)?;
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
                self.document.prune_groups();
                self.document.reconcile_guide();
                self.document.prune_object_states();
                self.document.promote_features();
                if let Err(error) = self
                    .document
                    .validate()
                    .and_then(|_| self.document.fit_automatic_canvas())
                {
                    previous.restore(&mut self.document);
                    return Err(error);
                }
                self.undo.push(previous);
                while self.undo.len() > 200 || raster_history_bytes(&self.undo) > 256 * 1024 * 1024
                {
                    self.undo.remove(0);
                }
                self.redo.clear();
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
            analysis: None,
            highlighter_rect: false,
            group: None,
            rotation_degrees: 0.0,
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
