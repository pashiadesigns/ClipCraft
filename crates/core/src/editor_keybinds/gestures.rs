use crate::{Error, Result};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Gesture {
    CanvasZoom,
    TimelineZoom,
    LibraryZoom,
    StrokeSize,
    DuplicateDrag,
}
impl Gesture {
    pub const ALL: [Self; 5] = [
        Self::CanvasZoom,
        Self::TimelineZoom,
        Self::LibraryZoom,
        Self::StrokeSize,
        Self::DuplicateDrag,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::CanvasZoom => "Zoom canvas · mouse wheel",
            Self::TimelineZoom => "Zoom timeline · mouse wheel",
            Self::LibraryZoom => "Zoom Library thumbnails · mouse wheel",
            Self::StrokeSize => "Change stroke size · mouse wheel",
            Self::DuplicateDrag => "Duplicate object / timeline clip · drag",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Gestures {
    pub canvas_zoom: Option<u8>,
    pub timeline_zoom: Option<u8>,
    pub library_zoom: Option<u8>,
    pub stroke_size: Option<u8>,
    pub duplicate_drag: Option<u8>,
}
impl Default for Gestures {
    fn default() -> Self {
        Self {
            canvas_zoom: Some(2),
            timeline_zoom: Some(2),
            library_zoom: Some(2),
            stroke_size: Some(1),
            duplicate_drag: Some(1),
        }
    }
}
impl Gestures {
    pub fn get(&self, g: Gesture) -> Option<u8> {
        match g {
            Gesture::CanvasZoom => self.canvas_zoom,
            Gesture::TimelineZoom => self.timeline_zoom,
            Gesture::LibraryZoom => self.library_zoom,
            Gesture::StrokeSize => self.stroke_size,
            Gesture::DuplicateDrag => self.duplicate_drag,
        }
    }
    pub fn set(&mut self, g: Gesture, v: Option<u8>) {
        *match g {
            Gesture::CanvasZoom => &mut self.canvas_zoom,
            Gesture::TimelineZoom => &mut self.timeline_zoom,
            Gesture::LibraryZoom => &mut self.library_zoom,
            Gesture::StrokeSize => &mut self.stroke_size,
            Gesture::DuplicateDrag => &mut self.duplicate_drag,
        } = v;
    }
    pub fn validate(&self) -> Result<()> {
        if Gesture::ALL
            .into_iter()
            .any(|g| self.get(g).is_some_and(|v| !(1..=7).contains(&v)))
        {
            return Err(Error::Invalid(
                "Choose Ctrl, Alt, Shift, or a combination for mouse gestures.".into(),
            ));
        }
        if self.canvas_zoom.is_some() && self.canvas_zoom == self.stroke_size {
            return Err(Error::Invalid(
                "Canvas zoom and stroke size share the canvas. Choose different modifiers.".into(),
            ));
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gestures_validate_shared_surface_conflicts_and_persist() {
        let mut g = Gestures::default();
        g.validate().unwrap();
        assert_eq!(g.library_zoom, Some(2));
        g.stroke_size = Some(2);
        assert!(g.validate().is_err());
        g.stroke_size = None;
        g.validate().unwrap();
        g.duplicate_drag = Some(8);
        assert!(g.validate().is_err());
        g.duplicate_drag = Some(4);
        assert_eq!(
            g,
            serde_json::from_slice::<Gestures>(&serde_json::to_vec(&g).unwrap()).unwrap()
        );
    }
}
