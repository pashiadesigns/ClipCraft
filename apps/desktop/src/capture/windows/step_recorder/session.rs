use crate::capture::step_pipeline::Area;
use capture_core::step_capture::{Button, Gesture, Tracker};

#[derive(Default)]
pub(super) struct Session {
    tracker: Option<Tracker>,
    pub area: Option<Area>,
    paused: bool,
    finished: bool,
}
impl Session {
    pub fn down(&mut self, point: [i32; 2], button: Button, area: Area) -> Result<bool, String> {
        if self.paused || self.finished {
            return Ok(false);
        }
        let first = self.tracker.is_none();
        if first {
            self.tracker = Some(Tracker::new(area.origin, area.size).map_err(|e| e.to_string())?);
            self.area = Some(area);
        }
        self.tracker.as_mut().unwrap().down(point, button);
        Ok(first)
    }
    pub fn up(&mut self, point: [i32; 2], button: Button) -> Option<Gesture> {
        self.tracker.as_mut()?.up(point, button)
    }
    pub fn cancel_gesture(&mut self) {
        if let Some(t) = &mut self.tracker {
            t.cancel_gesture();
        }
    }
    pub fn pause(&mut self, paused: bool) {
        self.paused = paused;
        if let Some(t) = &mut self.tracker {
            t.pause(paused);
        }
    }
    pub fn finish(&mut self) {
        self.finished = true;
        if let Some(t) = &mut self.tracker {
            t.finish();
        }
    }
}

pub(super) fn controller_position(work: [i32; 4]) -> [i32; 2] {
    [work[0] + 12, (work[3] - 52 - 12).max(work[1])]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn first_click_selects_monitor_until_restart_and_pause_does_not_select() {
        let left = Area {
            origin: [-1920, -400],
            size: [1920, 1080],
        };
        let right = Area {
            origin: [0, 0],
            size: [2560, 1440],
        };
        let mut s = Session::default();
        s.pause(true);
        assert!(!s.down([50, 50], Button::Left, right).unwrap());
        assert_eq!(s.area, None);
        s.pause(false);
        assert!(s.down([-1900, -380], Button::Left, left).unwrap());
        assert_eq!(s.up([-1900, -380], Button::Left).unwrap().start, [20, 20]);
        assert!(!s.down([50, 50], Button::Left, right).unwrap());
        assert!(s.up([50, 50], Button::Left).is_none());
        assert_eq!(s.area, Some(left));
        s.down([-1800, -300], Button::Left, left).unwrap();
        s.pause(true);
        s.pause(false);
        assert!(s.up([-1800, -300], Button::Left).is_none());
        s = Session::default();
        assert!(s.down([50, 50], Button::Right, right).unwrap());
        assert_eq!(s.up([50, 50], Button::Right).unwrap().button, Button::Right);
        s.finish();
        assert!(!s.down([-1900, -380], Button::Left, left).unwrap());
        assert_eq!(s.area, Some(right));
        assert_eq!(controller_position([-1920, -400, 0, 640]), [-1908, 576]);
    }
}
