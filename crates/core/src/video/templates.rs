//! Editable video starters; the background is a local image, not a dummy movie.
use super::editor::{Animation, MediaKind, Project, Source, Timing};
use super::links::Layer;
use crate::{Document, Kind, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Template {
    Blank,
    Tutorial,
    Walkthrough,
    Announcement,
}
impl Template {
    pub const ALL: [Self; 4] = [
        Self::Blank,
        Self::Tutorial,
        Self::Walkthrough,
        Self::Announcement,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Blank => "Blank video",
            Self::Tutorial => "Tutorial",
            Self::Walkthrough => "Product walkthrough",
            Self::Announcement => "Announcement",
        }
    }
    pub fn description(self) -> &'static str {
        match self {
            Self::Blank => "Start with an empty timeline",
            Self::Tutorial => "Title, three steps and closing card",
            Self::Walkthrough => "Opening title and feature captions",
            Self::Announcement => "Headline, message and closing action",
        }
    }
}
pub fn build(
    path: String,
    size: [u32; 2],
    duration: u64,
    fps: u32,
    template: Template,
) -> Result<Document> {
    let color = if template == Template::Blank {
        [255; 4]
    } else {
        [27, 36, 42, 255]
    };
    let mut d = Document::from_image(image::RgbaImage::from_pixel(
        size[0],
        size[1],
        image::Rgba(color),
    ))?;
    let mut p = Project::new(path.clone(), size, duration, fps);
    p.segments[0].label = "Background".into();
    p.segments[0].source = Some(Source {
        path,
        size,
        duration: 14_400_000,
        kind: MediaKind::Image,
    });

    if template != Template::Blank {
        let track = p.add_track();
        p.track_settings_mut(Layer::Annotation(track)).name = "Titles & instructions".into();
        let labels: &[&str] = match template {
            Template::Tutorial => &[
                "How to get started",
                "1. Introduce the task",
                "2. Show the action",
                "3. Explain the result",
                "You're ready!",
            ],
            Template::Walkthrough => &[
                "Meet your product",
                "Highlight the first feature",
                "Show what makes it useful",
                "Try it for yourself",
            ],
            Template::Announcement => {
                &["Something new is here", "Share your message", "Learn more"]
            }
            Template::Blank => &[],
        };
        for (i, label) in labels.iter().enumerate() {
            let id = i as u64 + 1;
            let mut a = crate::steps::marker(id, [64., 64.], 32, [255; 4], (1, false), size)?;
            a.kind = Kind::Rectangle;
            a.step_outline = None;
            a.width = 1;
            a.fill = None;
            a.color = [0; 4];
            a.start = [size[0] as f32 * 0.1, size[1] as f32 * 0.36];
            a.end = [size[0] as f32 * 0.9, size[1] as f32 * 0.64];
            a.text = Some(crate::text::TextStyle {
                content: (*label).into(),
                size_points: (size[1] as f32 * 0.065).clamp(18., 60.),
                color: Some([255; 4]),
                ..Default::default()
            });
            let start = duration * i as u64 / labels.len() as u64;
            let end = duration * (i + 1) as u64 / labels.len() as u64;
            let mut t = Timing::new(track, start, end);
            t.label = (*label).into();
            t.entrance = Animation::Fade;
            t.exit = Animation::Fade;
            t.entrance_ms = 300;
            t.exit_ms = 300;
            t.clamp_animations();
            p.place(id, t);
            d.annotations.push(a);
        }
    }
    p.cursor = 0;
    d.video = Some(p);
    d.promote_features();
    d.validate()?;
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn starters_are_editable_and_timing_covers_project() {
        for t in Template::ALL {
            let d = build("C:/starter.png".into(), [640, 360], 15000, 30, t).unwrap();
            assert_eq!(d.video.as_ref().unwrap().duration(), 15000);
            let copy: Document = serde_json::from_slice(&serde_json::to_vec(&d).unwrap()).unwrap();
            copy.validate().unwrap();
            assert_eq!(copy.video, d.video);
        }
    }
}
