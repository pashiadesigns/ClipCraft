//! Webcam picture-in-picture for screen recordings (AS13, approved mockup
//! `artifacts/bg-proposal`). The webcam is recorded as its own clip and placed
//! on its own video track as an editable framed clip.
use super::editor::{MediaKind, Project, Segment, Source, TrackKind};
use super::links::Layer;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Shape {
    #[default]
    Circle,
    Rounded,
    Square,
}
impl Shape {
    pub const ALL: [Self; 3] = [Self::Circle, Self::Rounded, Self::Square];
    pub fn label(self) -> &'static str {
        match self {
            Self::Circle => "Circle",
            Self::Rounded => "Rounded",
            Self::Square => "Square",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Size {
    Small,
    #[default]
    Medium,
    Large,
}
impl Size {
    pub const ALL: [Self; 3] = [Self::Small, Self::Medium, Self::Large];
    pub fn label(self) -> &'static str {
        match self {
            Self::Small => "Small",
            Self::Medium => "Medium",
            Self::Large => "Large",
        }
    }
    /// Side of the webcam frame as a share of the recording's height.
    fn share(self) -> f32 {
        match self {
            Self::Small => 0.2,
            Self::Medium => 0.28,
            Self::Large => 0.38,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    #[default]
    BottomRight,
}
impl Corner {
    pub const ALL: [Self; 4] = [
        Self::TopLeft,
        Self::TopRight,
        Self::BottomLeft,
        Self::BottomRight,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::TopLeft => "Top left",
            Self::TopRight => "Top right",
            Self::BottomLeft => "Bottom left",
            Self::BottomRight => "Bottom right",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Webcam {
    pub enabled: bool,
    /// Windows camera id; empty uses the first camera.
    pub device: String,
    pub shape: Shape,
    pub size: Size,
    pub corner: Corner,
    /// Keep the webcam above the zoom tracks so click zooms don't enlarge it.
    pub steady: bool,
}
impl Default for Webcam {
    fn default() -> Self {
        Self {
            enabled: false,
            device: String::new(),
            shape: Shape::default(),
            size: Size::default(),
            corner: Corner::default(),
            steady: true,
        }
    }
}
impl Webcam {
    /// The webcam's frame in output coordinates normalized to 0..1, for a
    /// recording of `output` pixels. The frame is square on screen.
    pub fn rect(&self, output: [u32; 2]) -> [f32; 4] {
        let [w, h] = output.map(|v| v.max(1) as f32);
        let side = (h * self.size.share()).min(w * 0.9);
        let margin = h * 0.03;
        let x = match self.corner {
            Corner::TopLeft | Corner::BottomLeft => margin,
            Corner::TopRight | Corner::BottomRight => w - side - margin,
        };
        let y = match self.corner {
            Corner::TopLeft | Corner::TopRight => margin,
            Corner::BottomLeft | Corner::BottomRight => h - side - margin,
        };
        [x / w, y / h, side / w, side / h]
    }
    pub fn frame(&self) -> super::frame::Frame {
        super::frame::Frame {
            shape: if self.shape == Shape::Circle {
                crate::Kind::Ellipse
            } else {
                crate::Kind::Rectangle
            },
            radius: if self.shape == Shape::Rounded {
                24.
            } else {
                0.
            },
            fill: true,
            outline: [255; 4],
            width: 3,
            ..Default::default()
        }
    }
    /// Adds the recorded webcam clip (`path`, `size` pixels, `duration` ms) to
    /// `project` on its own Webcam video track and returns that track. With
    /// [`Webcam::steady`] the track sits above every zoom track; otherwise it
    /// sits just below them, so zooms enlarge it with the screen.
    pub fn place(&self, project: &mut Project, path: String, size: [u32; 2], duration: u64) -> u64 {
        let length = duration.min(project.duration().max(1));
        let output = [project.crop[2], project.crop[3]];
        let track = project.add_typed_track(TrackKind::Video);
        let id = project.next_media_id();
        let mut clip = Segment::new(
            id,
            0,
            track,
            Some(Source {
                path,
                size,
                duration,
                kind: MediaKind::Video,
            }),
            length,
        );
        clip.label = "Webcam".into();
        clip.rect = self.rect(output);
        clip.frame = Some(self.frame());
        clip.audio.clear();
        project.segments.push(clip);
        if self.steady {
            // Pinned zoom tracks always sort to the top; unpin them so the webcam
            // can sit above them and stay put while they zoom.
            for z in &mut project.zooms {
                z.pinned = false;
            }
        }
        let mut order = project.layers();
        order.retain(|l| *l != Layer::Media(track));
        let at = if self.steady {
            0
        } else {
            order
                .iter()
                .rposition(|l| matches!(l, Layer::Zoom(_)))
                .map_or(0, |i| i + 1)
        };
        order.insert(at, Layer::Media(track));
        project.layer_order = order;
        track
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn webcam_sits_in_its_corner_on_its_own_track_above_or_below_zooms() {
        let mut p = Project::new("screen.mp4".into(), [1920, 1080], 10_000, 30);
        p.zooms.push(super::super::zoom::Zoom::new(1, 1000, 3000));
        p.layer_order = p.layers();
        let cam = Webcam {
            enabled: true,
            ..Default::default()
        };
        let r = cam.rect([1920, 1080]);
        // Bottom right, square on screen, inside the frame.
        assert!((r[2] * 1920. - r[3] * 1080.).abs() < 0.5);
        assert!(r[0] + r[2] < 1. && r[1] + r[3] < 1. && r[0] > 0.5 && r[1] > 0.5);
        let track = cam.place(&mut p, "cam.mp4".into(), [1280, 720], 9_000);
        let layers = p.layers();
        assert_eq!(
            layers[0],
            Layer::Media(track),
            "steady webcam is above the zooms"
        );
        let clip = p.segments.last().unwrap();
        assert_eq!((clip.label.as_str(), clip.duration()), ("Webcam", 9_000));
        assert_eq!(clip.frame.as_ref().unwrap().shape, crate::Kind::Ellipse);
        let mut q = Project::new("screen.mp4".into(), [1920, 1080], 10_000, 30);
        q.zooms.push(super::super::zoom::Zoom::new(1, 1000, 3000));
        q.layer_order = q.layers();
        let zoomed = Webcam {
            steady: false,
            corner: Corner::TopLeft,
            ..cam
        };
        let track = zoomed.place(&mut q, "cam.mp4".into(), [1280, 720], 12_000);
        let layers = q.layers();
        let at = layers
            .iter()
            .position(|l| *l == Layer::Media(track))
            .unwrap();
        let zoom = layers
            .iter()
            .position(|l| matches!(l, Layer::Zoom(_)))
            .unwrap();
        assert!(at > zoom, "a webcam that zooms sits below the zoom track");
        // The clip never runs past the recording.
        assert_eq!(q.segments.last().unwrap().duration(), 10_000);
        assert!(zoomed.rect([1920, 1080])[0] < 0.1);
    }

    /// BH5: a Step by Step recording with the webcam must still be a valid project.
    #[test]
    fn step_by_step_recording_with_webcam_is_valid() {
        use super::super::instruction::{Event, Options, apply};
        let click = |at: u64, x: f32| Event {
            at,
            point: [x, 180.],
            button: 0,
            dragging: false,
            release: Some(at + 80),
            caption: String::new(),
        };
        use super::super::motion_path::{Node, Path, Purpose, Reduction};
        let sprite =
            std::sync::Arc::new(
                crate::bitmap::Bitmap::transient(std::sync::Arc::new(
                    image::RgbaImage::from_pixel(12, 18, image::Rgba([0, 0, 0, 255])),
                ))
                .unwrap(),
            );
        let path = Path {
            purpose: Purpose::Cursor,
            reference: [100., 180., 12., 18.],
            nodes: (0..=120)
                .map(|i| Node {
                    at: i * 100,
                    rect: [100. + i as f32 * 3., 180., 12., 18.],
                    visible: true,
                    sprite: 0,
                    confidence: 1.,
                })
                .collect(),
            sprites: vec![sprite],
            smooth: false,
            enabled: true,
            offset_ms: 0,
            reviewed: true,
            reduction: Reduction::Off,
            inputs: vec![],
            recorded: None,
        };
        let events = [click(1000, 100.), click(2500, 500.), click(9000, 300.)];
        for steady in [true, false] {
            for duration in [12_000, 11_400, 12_600] {
                let mut doc = crate::Document::blank(1920, 1080).unwrap();
                doc.video = Some(Project::new(
                    "C:\\rec\\screen.mp4".into(),
                    [1920, 1080],
                    12_000,
                    30,
                ));
                let options = Options {
                    enabled: true,
                    captions: true,
                    zoom_follow_cursor: true,
                    ..Default::default()
                };
                apply(&mut doc, Some(path.clone()), &events, &options).unwrap();
                let cam = Webcam {
                    enabled: true,
                    steady,
                    ..Default::default()
                };
                cam.place(
                    doc.video.as_mut().unwrap(),
                    "cam.mp4".into(),
                    [1280, 720],
                    duration,
                );
                doc.validate()
                    .unwrap_or_else(|e| panic!("steady {steady}, webcam {duration} ms: {e}"));
                // The editor rebuilds following zooms on every edit.
                let mut p = doc.video.clone().unwrap();
                p.refollow_zooms([1920, 1080]);
                doc.video = Some(p);
                doc.validate()
                    .unwrap_or_else(|e| panic!("after refollow, steady {steady}: {e}"));
            }
        }
    }
}
